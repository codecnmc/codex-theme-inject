use serde_json::Value;

pub async fn healthy(websocket_url: &str) -> bool {
    let expression = r#"
(() => Boolean(
  window.__themeInjectRuntime?.version === 2 &&
  document.getElementById("theme-inject-style") &&
  document.getElementById("theme-inject-backdrop") &&
  typeof window.themeInject?.call === "function"
))()
"#;
    crate::cdp::evaluate(websocket_url, expression, false)
        .await
        .ok()
        .and_then(|value| {
            value
                .pointer("/result/result/value")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

pub async fn open_panel(websocket_url: &str) -> anyhow::Result<()> {
    crate::cdp::evaluate(
        websocket_url,
        "window.__themeInjectRuntime?.openPanel?.()",
        false,
    )
    .await?;
    Ok(())
}
