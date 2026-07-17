use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, bail};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct CdpTarget {
    pub id: String,
    #[serde(rename = "type")]
    pub target_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    pub websocket_url: Option<String>,
}

pub async fn list_targets(port: u16) -> anyhow::Result<Vec<CdpTarget>> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()?;
    let mut errors = Vec::new();
    for url in [
        format!("http://127.0.0.1:{port}/json"),
        format!("http://[::1]:{port}/json"),
    ] {
        match client.get(&url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => return response.json().await.context("CDP target JSON 无效"),
                Err(error) => errors.push(error.to_string()),
            },
            Err(error) => errors.push(error.to_string()),
        }
    }
    bail!("无法查询 CDP target：{}", errors.join("; "))
}

pub fn pick_codex_target(targets: &[CdpTarget]) -> anyhow::Result<CdpTarget> {
    targets
        .iter()
        .filter(|target| {
            target.target_type == "page"
                && target
                    .websocket_url
                    .as_deref()
                    .is_some_and(|url| !url.is_empty())
        })
        .filter(|target| {
            let text = format!("{} {}", target.title, target.url).to_ascii_lowercase();
            let auxiliary = text.contains("avatar-overlay")
                || text.contains("hotkey-window")
                || text.contains("devtools://");
            !auxiliary
                && (text.contains("codex")
                    || text.contains("chatgpt")
                    || text.contains("app://-/index.html"))
        })
        .max_by_key(|target| {
            let text = format!("{} {}", target.title, target.url).to_ascii_lowercase();
            usize::from(target.url.trim_end_matches('/') == "app://-/index.html") * 16
                + usize::from(!target.url.contains("initialRoute")) * 8
                + usize::from(text.contains("codex")) * 4
                + usize::from(text.contains("chatgpt")) * 2
                + usize::from(text.contains("app://-/index.html"))
        })
        .cloned()
        .context("未找到可注入的 Codex 页面")
}

pub type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

pub struct CdpSession {
    socket: Socket,
    next_id: u64,
    queued_responses: HashMap<u64, Value>,
    queued_events: Vec<Value>,
}

impl CdpSession {
    pub async fn connect(websocket_url: &str) -> anyhow::Result<Self> {
        let (socket, _) = tokio::time::timeout(COMMAND_TIMEOUT, connect_async(websocket_url))
            .await
            .context("连接 CDP WebSocket 超时")??;
        Ok(Self {
            socket,
            next_id: 1,
            queued_responses: HashMap::new(),
            queued_events: Vec::new(),
        })
    }

    pub async fn command(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.socket
            .send(Message::Text(
                json!({ "id": id, "method": method, "params": params })
                    .to_string()
                    .into(),
            ))
            .await
            .with_context(|| format!("发送 CDP 命令 {method} 失败"))?;
        if let Some(response) = self.queued_responses.remove(&id) {
            return command_result(response, method);
        }
        tokio::time::timeout(COMMAND_TIMEOUT, async {
            loop {
                let value = self.next_value().await?.context("CDP WebSocket 已关闭")?;
                if value.get("id").and_then(Value::as_u64) == Some(id) {
                    return command_result(value, method);
                }
                if let Some(other_id) = value.get("id").and_then(Value::as_u64) {
                    self.queued_responses.insert(other_id, value);
                } else {
                    self.queued_events.push(value);
                }
            }
        })
        .await
        .with_context(|| format!("等待 CDP 命令 {method} 响应超时"))?
    }

    pub async fn next_event(&mut self) -> anyhow::Result<Option<Value>> {
        if !self.queued_events.is_empty() {
            return Ok(Some(self.queued_events.remove(0)));
        }
        loop {
            let Some(value) = self.next_value().await? else {
                return Ok(None);
            };
            if let Some(id) = value.get("id").and_then(Value::as_u64) {
                self.queued_responses.insert(id, value);
            } else {
                return Ok(Some(value));
            }
        }
    }

    async fn next_value(&mut self) -> anyhow::Result<Option<Value>> {
        let Some(message) = self.socket.next().await else {
            return Ok(None);
        };
        let message = message.context("读取 CDP WebSocket 失败")?;
        let Message::Text(text) = message else {
            return Ok(Some(json!({})));
        };
        Ok(Some(
            serde_json::from_str(&text).context("CDP 消息不是 JSON")?,
        ))
    }
}

fn command_result(response: Value, method: &str) -> anyhow::Result<Value> {
    if let Some(error) = response.get("error") {
        bail!("CDP {method} 返回错误：{error}");
    }
    Ok(response)
}

pub async fn evaluate(
    websocket_url: &str,
    expression: &str,
    await_promise: bool,
) -> anyhow::Result<Value> {
    let mut session = CdpSession::connect(websocket_url).await?;
    session
        .command(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "awaitPromise": await_promise,
                "returnByValue": true,
                "allowUnsafeEvalBlockedByCSP": true
            }),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_primary_codex_page() {
        let targets = vec![
            CdpTarget {
                id: "overlay".into(),
                target_type: "page".into(),
                title: "Codex".into(),
                url: "app://-/index.html?initialRoute=/avatar-overlay".into(),
                websocket_url: Some("ws://overlay".into()),
            },
            CdpTarget {
                id: "hotkey".into(),
                target_type: "page".into(),
                title: "Codex".into(),
                url: "app://-/index.html?initialRoute=%2Fhotkey-window".into(),
                websocket_url: Some("ws://hotkey".into()),
            },
            CdpTarget {
                id: "main".into(),
                target_type: "page".into(),
                title: "Codex".into(),
                url: "app://-/index.html".into(),
                websocket_url: Some("ws://main".into()),
            },
        ];
        assert_eq!(pick_codex_target(&targets).unwrap().id, "main");
    }
}
