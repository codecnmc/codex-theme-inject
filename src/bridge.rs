use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::task::JoinSet;

use crate::cdp::CdpSession;

pub const BINDING_NAME: &str = "themeInjectV1";
pub const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: usize = 24 * 1024 * 1024;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const DATA_URL_REQUEST_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const FILE_REQUEST_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const GENERATION_REQUEST_TIMEOUT: Duration = Duration::from_secs(25 * 60);
const MAX_CONCURRENT_REQUESTS: usize = 8;

pub type RpcFuture = Pin<Box<dyn Future<Output = anyhow::Result<Value>> + Send>>;
pub type RpcHandler = Arc<dyn Fn(String, Value) -> RpcFuture + Send + Sync>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcRequest {
    version: u32,
    id: String,
    nonce: String,
    method: String,
    #[serde(default)]
    params: Value,
}

struct BindingRequest {
    request: RpcRequest,
    execution_context_id: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcError {
    code: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

pub struct BridgeHandle {
    pub script_identifier: String,
    pub(crate) shutdown: Option<oneshot::Sender<()>>,
    pub(crate) task: tokio::task::JoinHandle<()>,
}

impl BridgeHandle {
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    pub async fn shutdown(mut self, websocket_url: &str) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = self.task.await;
        if !self.script_identifier.is_empty()
            && let Ok(mut session) = CdpSession::connect(websocket_url).await
        {
            let _ = session
                .command(
                    "Page.removeScriptToEvaluateOnNewDocument",
                    json!({ "identifier": self.script_identifier }),
                )
                .await;
            let _ = session
                .command("Runtime.removeBinding", json!({ "name": BINDING_NAME }))
                .await;
        }
    }
}

pub async fn install(
    websocket_url: &str,
    nonce: &str,
    runtime_script: &str,
    handler: RpcHandler,
) -> anyhow::Result<BridgeHandle> {
    let mut session = CdpSession::connect(websocket_url).await?;
    session.command("Runtime.enable", json!({})).await?;
    let _ = session
        .command("Runtime.removeBinding", json!({ "name": BINDING_NAME }))
        .await;
    session
        .command("Runtime.addBinding", json!({ "name": BINDING_NAME }))
        .await?;

    let bundle = build_bundle(nonce, runtime_script)?;
    let response = session
        .command(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": bundle }),
        )
        .await?;
    let identifier = response
        .pointer("/result/identifier")
        .and_then(Value::as_str)
        .context("CDP 未返回 new-document script identifier")?
        .to_string();
    if let Err(error) = session
        .command(
            "Runtime.evaluate",
            json!({ "expression": bundle, "allowUnsafeEvalBlockedByCSP": true }),
        )
        .await
    {
        let _ = session
            .command(
                "Page.removeScriptToEvaluateOnNewDocument",
                json!({ "identifier": identifier }),
            )
            .await;
        let _ = session
            .command("Runtime.removeBinding", json!({ "name": BINDING_NAME }))
            .await;
        return Err(error).context("执行主题运行时失败，已回滚 Bridge 安装");
    }

    let expected_nonce = nonce.to_string();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
        let generation_permit = Arc::new(Semaphore::new(1));
        let (response_tx, mut response_rx) = mpsc::channel(MAX_CONCURRENT_REQUESTS);
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                Some(response) = response_rx.recv() => {
                    if let Err(error) = resolve_response(&mut session, response).await {
                        crate::diagnostic::log("bridge.response_failed", json!({ "message": format!("{error:#}") }));
                    }
                }
                joined = tasks.join_next(), if !tasks.is_empty() => {
                    if let Some(Err(error)) = joined {
                        crate::diagnostic::log("bridge.request_panicked", json!({ "message": error.to_string() }));
                    }
                }
                event = session.next_event() => {
                    let Ok(Some(event)) = event else { break };
                    if event.get("method").and_then(Value::as_str) != Some("Runtime.bindingCalled") {
                        continue;
                    }
                    let binding = match binding_request(event) {
                        Ok(binding) => binding,
                        Err(error) => {
                            crate::diagnostic::log("bridge.request_failed", json!({ "message": format!("{error:#}") }));
                            continue;
                        }
                    };
                    let request = binding.request;
                    let execution_context_id = binding.execution_context_id;
                    if let Some(response) = rejected_request(&request, &expected_nonce) {
                        let pending = PendingResponse {
                            id: request.id,
                            execution_context_id,
                            response,
                        };
                        if let Err(error) = resolve_response(&mut session, pending).await {
                            crate::diagnostic::log("bridge.response_failed", json!({ "message": format!("{error:#}") }));
                        }
                        continue;
                    }
                    let request_permit = if uses_general_capacity(&request.method) {
                        let Ok(permit) = permits.clone().try_acquire_owned() else {
                            let pending = PendingResponse {
                                id: request.id,
                                execution_context_id,
                                response: failed("busy", "请求过多，请稍后重试"),
                            };
                            if let Err(error) = resolve_response(&mut session, pending).await {
                                crate::diagnostic::log("bridge.response_failed", json!({ "message": format!("{error:#}") }));
                            }
                            continue;
                        };
                        Some(permit)
                    } else {
                        None
                    };
                    let generation_permit = if matches!(request.method.as_str(), "ai.generate" | "ai.resource.generate") {
                        match generation_permit.clone().try_acquire_owned() {
                            Ok(permit) => Some(permit),
                            Err(_) => {
                                let pending = PendingResponse {
                                    id: request.id,
                                    execution_context_id,
                                    response: failed("busy", "已有 AI 主题生成任务正在运行"),
                                };
                                if let Err(error) = resolve_response(&mut session, pending).await {
                                    crate::diagnostic::log("bridge.response_failed", json!({ "message": format!("{error:#}") }));
                                }
                                continue;
                            }
                        }
                    } else {
                        None
                    };
                    spawn_handler_request(
                        &mut tasks,
                        response_tx.clone(),
                        handler.clone(),
                        request,
                        execution_context_id,
                        request_permit,
                        generation_permit,
                    );
                }
            }
        }
        tasks.detach_all();
        while tasks.join_next().await.is_some() {}
    });
    Ok(BridgeHandle {
        script_identifier: identifier,
        shutdown: Some(shutdown_tx),
        task,
    })
}

fn build_bundle(nonce: &str, runtime_script: &str) -> anyhow::Result<String> {
    Ok(format!(
        "window.__THEME_INJECT_NONCE__ = {};\n{}",
        serde_json::to_string(nonce)?,
        runtime_script
    ))
}

fn binding_request(event: Value) -> anyhow::Result<BindingRequest> {
    let execution_context_id = event
        .pointer("/params/executionContextId")
        .and_then(Value::as_u64)
        .context("Binding 事件缺少 executionContextId")?;
    let payload = event
        .pointer("/params/payload")
        .and_then(Value::as_str)
        .context("Binding 事件缺少 payload")?;
    if payload.len() > MAX_REQUEST_BYTES {
        bail!("Binding 请求超过大小限制");
    }
    let request = serde_json::from_str(payload).context("Binding 请求 JSON 无效")?;
    Ok(BindingRequest {
        request,
        execution_context_id,
    })
}

fn rejected_request(request: &RpcRequest, expected_nonce: &str) -> Option<RpcResponse> {
    if request.version != PROTOCOL_VERSION {
        Some(failed("unsupported_version", "不支持的 Binding 协议版本"))
    } else if request.nonce != expected_nonce {
        Some(failed("unauthorized", "Binding nonce 无效"))
    } else if !valid_method(&request.method) {
        Some(failed("unknown_method", "未知的 Binding 方法"))
    } else {
        None
    }
}

fn spawn_handler_request(
    tasks: &mut JoinSet<()>,
    response_tx: mpsc::Sender<PendingResponse>,
    handler: RpcHandler,
    request: RpcRequest,
    execution_context_id: u64,
    request_permit: Option<OwnedSemaphorePermit>,
    generation_permit: Option<OwnedSemaphorePermit>,
) {
    tasks.spawn(async move {
        let _request_permit = request_permit;
        let _generation_permit = generation_permit;
        let response = run_handler_request(&handler, request, execution_context_id).await;
        let _ = response_tx.send(response).await;
    });
}

async fn run_handler_request(
    handler: &RpcHandler,
    request: RpcRequest,
    execution_context_id: u64,
) -> PendingResponse {
    let timeout = request_timeout(&request.method);
    let response =
        match tokio::time::timeout(timeout, handler(request.method, request.params)).await {
            Ok(Ok(result)) => RpcResponse {
                ok: true,
                result: Some(result),
                error: None,
            },
            Ok(Err(error)) => failed("operation_failed", &error.to_string()),
            Err(_) => failed("timeout", "操作超时"),
        };
    PendingResponse {
        id: request.id,
        execution_context_id,
        response,
    }
}

async fn resolve_response(
    session: &mut CdpSession,
    pending: PendingResponse,
) -> anyhow::Result<()> {
    let expression = format!(
        "window.__themeInjectResolve({}, {})",
        serde_json::to_string(&pending.id)?,
        serde_json::to_string(&pending.response)?
    );
    session
        .command(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "contextId": pending.execution_context_id,
                "allowUnsafeEvalBlockedByCSP": true
            }),
        )
        .await?;
    Ok(())
}

struct PendingResponse {
    id: String,
    execution_context_id: u64,
    response: RpcResponse,
}

fn request_timeout(method: &str) -> Duration {
    match method {
        "ai.generate" | "ai.resource.generate" => GENERATION_REQUEST_TIMEOUT,
        "theme.package.import"
        | "theme.package.export"
        | "theme.background.import"
        | "theme.asset.import"
        | "ai.reference.import"
        | "ai.reference.upload" => FILE_REQUEST_TIMEOUT,
        "theme.background.data"
        | "theme.preview.data"
        | "theme.preview.thumbnail"
        | "theme.thumbnail" => DATA_URL_REQUEST_TIMEOUT,
        _ => DEFAULT_REQUEST_TIMEOUT,
    }
}

fn uses_general_capacity(method: &str) -> bool {
    !matches!(method, "theme.preview.cancel" | "ai.generation.cancel")
}

fn failed(code: &str, message: &str) -> RpcResponse {
    RpcResponse {
        ok: false,
        result: None,
        error: Some(RpcError {
            code: code.into(),
            message: message.into(),
        }),
    }
}

fn valid_method(method: &str) -> bool {
    matches!(
        method,
        "theme.health"
            | "theme.window.chrome"
            | "ai.settings.get"
            | "ai.settings.save"
            | "ai.log.get"
            | "ai.progress.get"
            | "ai.generation.cancel"
            | "ai.generation.restore"
            | "ai.generation.save"
            | "ai.reference.import"
            | "ai.reference.upload"
            | "ai.generate"
            | "ai.resource.generate"
            | "theme.state.get"
            | "app.trigger.save"
            | "app.trigger.icon.import"
            | "app.trigger.icon.reset"
            | "theme.package.list"
            | "theme.package.read"
            | "theme.package.create"
            | "theme.package.metadata"
            | "theme.package.import"
            | "theme.package.export"
            | "theme.package.clone"
            | "theme.package.delete"
            | "theme.background.import"
            | "theme.background.data"
            | "theme.preview.data"
            | "theme.preview.thumbnail"
            | "theme.thumbnail"
            | "theme.asset.import"
            | "theme.preview.cancel"
            | "theme.activate"
            | "theme.apply"
            | "theme.custom_css.trust"
            | "theme.custom_css.revoke"
            | "runtime.reinject"
            | "diagnostics.report"
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use tokio::sync::Notify;

    use super::*;

    #[test]
    fn method_allowlist_is_closed() {
        assert!(valid_method("theme.apply"));
        assert!(valid_method("theme.window.chrome"));
        assert!(valid_method("ai.generate"));
        assert!(valid_method("ai.resource.generate"));
        assert!(valid_method("app.trigger.save"));
        assert!(valid_method("app.trigger.icon.import"));
        assert!(valid_method("theme.package.metadata"));
        assert!(valid_method("ai.progress.get"));
        assert!(valid_method("ai.generation.cancel"));
        assert!(valid_method("ai.generation.restore"));
        assert!(valid_method("ai.generation.save"));
        assert!(valid_method("ai.reference.upload"));
        assert!(valid_method("theme.preview.data"));
        assert!(valid_method("theme.preview.thumbnail"));
        assert!(!valid_method("filesystem.delete"));
    }

    #[test]
    fn bundle_serializes_nonce() {
        let bundle = build_bundle("quote\"nonce", "void 0").unwrap();
        assert!(bundle.contains("quote\\\"nonce"));
    }

    #[test]
    fn binding_request_preserves_execution_context() {
        let event = json!({
            "params": {
                "executionContextId": 42,
                "payload": serde_json::to_string(&json!({
                    "version": PROTOCOL_VERSION,
                    "id": "request",
                    "nonce": "nonce",
                    "method": "theme.health",
                    "params": {}
                })).unwrap()
            }
        });
        let binding = binding_request(event).unwrap();
        assert_eq!(binding.execution_context_id, 42);
        assert_eq!(binding.request.id, "request");
    }

    #[test]
    fn data_url_requests_have_extended_timeout() {
        assert_eq!(
            request_timeout("theme.preview.data"),
            DATA_URL_REQUEST_TIMEOUT
        );
        assert_eq!(
            request_timeout("theme.preview.thumbnail"),
            DATA_URL_REQUEST_TIMEOUT
        );
        assert_eq!(
            request_timeout("theme.background.data"),
            DATA_URL_REQUEST_TIMEOUT
        );
        assert!(DATA_URL_REQUEST_TIMEOUT > request_timeout("theme.health"));
    }

    #[test]
    fn cleanup_and_generation_cancel_do_not_consume_general_capacity() {
        let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
        let held = (0..MAX_CONCURRENT_REQUESTS)
            .map(|_| permits.clone().try_acquire_owned().unwrap())
            .collect::<Vec<_>>();
        assert!(permits.clone().try_acquire_owned().is_err());
        assert!(!uses_general_capacity("theme.preview.cancel"));
        assert!(!uses_general_capacity("ai.generation.cancel"));
        assert!(uses_general_capacity("theme.background.data"));
        drop(held);
    }

    #[tokio::test]
    async fn progress_can_finish_while_generation_is_running() {
        let generation_started = Arc::new(Notify::new());
        let release_generation = Arc::new(Notify::new());
        let generation_running = Arc::new(AtomicBool::new(false));
        let handler: RpcHandler = Arc::new({
            let generation_started = generation_started.clone();
            let release_generation = release_generation.clone();
            let generation_running = generation_running.clone();
            move |method, _| {
                let generation_started = generation_started.clone();
                let release_generation = release_generation.clone();
                let generation_running = generation_running.clone();
                Box::pin(async move {
                    if method == "ai.generate" {
                        generation_running.store(true, Ordering::SeqCst);
                        generation_started.notify_one();
                        release_generation.notified().await;
                        generation_running.store(false, Ordering::SeqCst);
                    }
                    Ok(json!({ "method": method }))
                })
            }
        });
        let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
        let (response_tx, mut response_rx) = mpsc::channel(MAX_CONCURRENT_REQUESTS);
        let mut tasks = JoinSet::new();
        spawn_handler_request(
            &mut tasks,
            response_tx.clone(),
            handler.clone(),
            request("generation", "ai.generate"),
            17,
            Some(permits.clone().try_acquire_owned().unwrap()),
            None,
        );
        generation_started.notified().await;
        spawn_handler_request(
            &mut tasks,
            response_tx,
            handler,
            request("progress", "ai.progress.get"),
            17,
            Some(permits.try_acquire_owned().unwrap()),
            None,
        );

        let progress = tokio::time::timeout(Duration::from_secs(1), response_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(progress.id, "progress");
        assert!(progress.response.ok);
        assert!(generation_running.load(Ordering::SeqCst));

        release_generation.notify_one();
        let generation = tokio::time::timeout(Duration::from_secs(1), response_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(generation.id, "generation");
        while tasks.join_next().await.is_some() {}
    }

    fn request(id: &str, method: &str) -> RpcRequest {
        RpcRequest {
            version: PROTOCOL_VERSION,
            id: id.into(),
            nonce: "nonce".into(),
            method: method.into(),
            params: json!({}),
        }
    }
}
