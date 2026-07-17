use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, bail};
use base64::Engine;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc};

use crate::ai::{AiPublicSettings, AiThemeService, GenerateRequest};
use crate::asset_server::AssetServer;
use crate::bridge::{BridgeHandle, RpcHandler};
use crate::storage::{AppPaths, SettingsStore, ThemeStore};
use crate::theme::ThemeManifest;
use crate::theme_package::ThemePackageManager;

const INSTANCE_PORT: u16 = 47631;

#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    pub app_path: Option<PathBuf>,
    pub debug_port: Option<u16>,
    pub open_panel: bool,
}

pub struct ThemeLauncher {
    settings: SettingsStore,
    themes: ThemeStore,
    instance_guard: InstanceGuard,
}

enum InstanceGuard {
    Primary(tokio::net::TcpListener),
}

enum InstanceResult {
    Primary(InstanceGuard),
    Forwarded,
}

impl ThemeLauncher {
    pub async fn run(mut options: LaunchOptions) -> anyhow::Result<()> {
        let instance_guard = match InstanceGuard::acquire().await? {
            InstanceResult::Primary(guard) => guard,
            InstanceResult::Forwarded => return Ok(()),
        };
        let paths = AppPaths::discover()?;
        paths.ensure()?;
        crate::diagnostic::init(paths.logs.join("theme-inject.log"));
        crate::diagnostic::log(
            "launcher.start",
            json!({ "options": format!("{options:?}") }),
        );
        if !paths.settings.exists() {
            options.open_panel = true;
        }
        let themes = ThemeStore::new(paths.clone());
        themes.ensure_builtins()?;
        themes.cleanup_staging()?;
        let settings = SettingsStore::new(paths.settings.clone());
        let launcher = Self {
            settings,
            themes,
            instance_guard,
        };
        launcher.run_primary(options).await
    }

    async fn run_primary(self, options: LaunchOptions) -> anyhow::Result<()> {
        crate::diagnostic::log("launcher.select_debug_port", json!({}));
        let debug_port = match options.debug_port {
            Some(port) if crate::cdp::list_targets(port).await.is_ok() => port,
            Some(port) => {
                if crate::windows_app::codex_is_running() {
                    crate::windows_dialog::show_info(
                        "Theme Inject",
                        "Codex 已经运行，但指定 CDP 端口不可用。请关闭全部 Codex 窗口后重新启动 Theme Inject。",
                    );
                    return Ok(());
                }
                port
            }
            None => {
                if let Some(port) = crate::windows_app::running_debug_port().await
                    && crate::cdp::list_targets(port).await.is_ok()
                {
                    crate::diagnostic::log(
                        "launcher.recover_existing_cdp",
                        json!({ "debugPort": port }),
                    );
                    port
                } else if crate::windows_app::codex_is_running() {
                    crate::windows_dialog::show_info(
                        "Theme Inject",
                        "Codex 已经运行且没有 Theme Inject 的 CDP 会话。请先关闭全部 Codex 窗口，再重新启动 Theme Inject。",
                    );
                    return Ok(());
                } else {
                    available_port()?
                }
            }
        };

        crate::diagnostic::log("launcher.discover_app", json!({ "debugPort": debug_port }));
        let app = crate::windows_app::discover(options.app_path.as_deref())?;
        crate::diagnostic::log(
            "launcher.app_discovered",
            json!({ "app": format!("{app:?}"), "version": app.version() }),
        );
        let asset_server = AssetServer::start(self.themes.paths.clone()).await?;
        crate::diagnostic::log(
            "launcher.asset_server_ready",
            json!({ "port": asset_server.port }),
        );
        let nonce = uuid::Uuid::new_v4().to_string();
        if crate::cdp::list_targets(debug_port).await.is_err() {
            match tokio::time::timeout(Duration::from_secs(8), app.launch(debug_port)).await {
                Ok(Ok(process_id)) => crate::diagnostic::log(
                    "launcher.codex_launched",
                    json!({ "processId": process_id, "debugPort": debug_port }),
                ),
                Ok(Err(error)) => return Err(error),
                Err(_) => crate::diagnostic::log(
                    "launcher.activation_delayed",
                    json!({ "debugPort": debug_port, "message": "MSIX 激活调用未及时返回，继续等待 CDP" }),
                ),
            }
        } else {
            crate::diagnostic::log(
                "launcher.attach_existing",
                json!({ "debugPort": debug_port }),
            );
        }
        let mut app_settings = self.settings.load()?;
        app_settings.last_codex_version = app.version().to_string();
        self.settings.save(&app_settings)?;
        let reinject_requested = Arc::new(AtomicBool::new(false));
        let ai = AiThemeService::new(
            self.themes.paths.clone(),
            asset_server.base_url(),
            asset_server.token.clone(),
        )?;
        let service = Arc::new(ThemeService {
            settings: self.settings.clone(),
            themes: self.themes.clone(),
            ai,
            asset_base: asset_server.base_url(),
            asset_token: asset_server.token.clone(),
            state_lock: Mutex::new(()),
            reinject_requested: reinject_requested.clone(),
        });
        let handler = rpc_handler(service);
        let script = runtime_script()?;
        let (mut websocket_url, mut bridge) = install_initial_runtime(
            debug_port,
            &nonce,
            &script,
            handler.clone(),
            Duration::from_secs(3 * 60),
        )
        .await?;
        let runtime = RuntimeInstall {
            nonce,
            script,
            handler,
            reinject_requested,
        };
        crate::diagnostic::log(
            "launcher.bridge_ready",
            json!({ "debugPort": debug_port, "openPanel": options.open_panel }),
        );
        if options.open_panel {
            crate::watchdog::open_panel(&websocket_url).await?;
        }

        let cleanup_themes = self.themes.clone();
        let command_rx = self.instance_guard.start_control_listener();
        Self::event_loop(
            debug_port,
            &runtime,
            &mut websocket_url,
            &mut bridge,
            command_rx,
        )
        .await;
        bridge.shutdown(&websocket_url).await;
        asset_server.shutdown().await;
        cleanup_themes.cleanup_staging()?;
        Ok(())
    }

    async fn event_loop(
        debug_port: u16,
        runtime: &RuntimeInstall,
        websocket_url: &mut String,
        bridge: &mut BridgeHandle,
        mut command_rx: mpsc::UnboundedReceiver<String>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut process_missing_ticks = 0u8;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if !crate::windows_app::codex_is_running() {
                        process_missing_ticks += 1;
                        if process_missing_ticks >= 2 { break; }
                    } else {
                        process_missing_ticks = 0;
                    }
                    let target = crate::cdp::list_targets(debug_port).await.ok().and_then(|targets| crate::cdp::pick_codex_target(&targets).ok());
                    let next_url = target.and_then(|target| target.websocket_url);
                    let requested = runtime.reinject_requested.swap(false, Ordering::SeqCst);
                    let requires_reinstall = requested || bridge.is_finished() || match next_url.as_deref() {
                        Some(next_url) if next_url == websocket_url => !crate::watchdog::healthy(websocket_url).await,
                        Some(_) => true,
                        None => false,
                    };
                    if requires_reinstall && let Some(next_url) = next_url {
                        let placeholder = std::mem::replace(bridge, inert_bridge());
                        placeholder.shutdown(websocket_url).await;
                        match crate::bridge::install(&next_url, &runtime.nonce, &runtime.script, runtime.handler.clone()).await {
                            Ok(next_bridge) => {
                                *bridge = next_bridge;
                                *websocket_url = next_url;
                                if requested {
                                    let _ = crate::watchdog::open_panel(websocket_url).await;
                                }
                                crate::diagnostic::log("watchdog.reinjected", json!({ "requested": requested }));
                            }
                                Err(error) => {
                                    runtime.reinject_requested.store(true, Ordering::SeqCst);
                                    crate::diagnostic::log("watchdog.reinject_failed", json!({ "message": format!("{error:#}") }));
                                }
                        }
                    } else if requested && next_url.is_none() {
                        runtime.reinject_requested.store(true, Ordering::SeqCst);
                    }
                }
                Some(command) = command_rx.recv() => {
                    if command.trim() == "open-panel" { let _ = crate::watchdog::open_panel(websocket_url).await; }
                }
                _ = tokio::signal::ctrl_c() => break,
            }
        }
    }
}

fn inert_bridge() -> BridgeHandle {
    let (shutdown, _) = tokio::sync::oneshot::channel();
    BridgeHandle::inert(shutdown)
}

async fn install_initial_runtime(
    debug_port: u16,
    nonce: &str,
    script: &str,
    handler: RpcHandler,
    timeout: Duration,
) -> anyhow::Result<(String, BridgeHandle)> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_error = None;
    let mut attempt = 0u32;

    while tokio::time::Instant::now() < deadline {
        let target = match crate::cdp::list_targets(debug_port)
            .await
            .and_then(|targets| crate::cdp::pick_codex_target(&targets))
        {
            Ok(target) => target,
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(300)).await;
                continue;
            }
        };
        let Some(websocket_url) = target.websocket_url.clone() else {
            last_error = Some(anyhow::anyhow!("Codex target 缺少 WebSocket URL"));
            tokio::time::sleep(Duration::from_millis(300)).await;
            continue;
        };

        attempt += 1;
        crate::diagnostic::log(
            "launcher.target_ready",
            json!({
                "attempt": attempt,
                "targetId": target.id,
                "title": target.title,
                "url": target.url
            }),
        );
        match crate::bridge::install(&websocket_url, nonce, script, handler.clone()).await {
            Ok(bridge) => {
                let health_deadline = std::cmp::min(
                    deadline,
                    tokio::time::Instant::now() + Duration::from_secs(12),
                );
                while tokio::time::Instant::now() < health_deadline {
                    if crate::watchdog::healthy(&websocket_url).await {
                        return Ok((websocket_url, bridge));
                    }
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
                last_error = Some(anyhow::anyhow!("主题运行时安装后未通过健康检查"));
                crate::diagnostic::log(
                    "launcher.initial_inject_unhealthy",
                    json!({ "attempt": attempt, "targetId": target.id }),
                );
                bridge.shutdown(&websocket_url).await;
            }
            Err(error) => {
                crate::diagnostic::log(
                    "launcher.initial_inject_failed",
                    json!({ "attempt": attempt, "message": format!("{error:#}") }),
                );
                last_error = Some(error);
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("等待 Codex 首次注入超时")))
        .context("首次启动时无法安装主题运行时")
}

impl InstanceGuard {
    async fn acquire() -> anyhow::Result<InstanceResult> {
        match tokio::net::TcpListener::bind(("127.0.0.1", INSTANCE_PORT)).await {
            Ok(listener) => Ok(InstanceResult::Primary(Self::Primary(listener))),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                if let Ok(mut stream) =
                    tokio::net::TcpStream::connect(("127.0.0.1", INSTANCE_PORT)).await
                {
                    stream.write_all(b"open-panel\n").await?;
                }
                Ok(InstanceResult::Forwarded)
            }
            Err(error) => Err(error).context("无法获取 Theme Inject 单实例端口"),
        }
    }

    fn start_control_listener(self) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        let Self::Primary(listener) = self;
        tokio::spawn(async move {
            while let Ok((mut stream, address)) = listener.accept().await {
                if !address.ip().is_loopback() {
                    continue;
                }
                let mut buffer = [0u8; 128];
                if let Ok(size) = stream.read(&mut buffer).await {
                    let _ = tx.send(String::from_utf8_lossy(&buffer[..size]).to_string());
                }
            }
        });
        rx
    }
}

impl BridgeHandle {
    pub(crate) fn inert(shutdown: tokio::sync::oneshot::Sender<()>) -> Self {
        Self {
            script_identifier: String::new(),
            shutdown: Some(shutdown),
            task: tokio::spawn(async {}),
        }
    }
}

struct ThemeService {
    settings: SettingsStore,
    themes: ThemeStore,
    ai: AiThemeService,
    asset_base: String,
    asset_token: String,
    state_lock: Mutex<()>,
    reinject_requested: Arc<AtomicBool>,
}

struct RuntimeInstall {
    nonce: String,
    script: String,
    handler: RpcHandler,
    reinject_requested: Arc<AtomicBool>,
}

fn rpc_handler(service: Arc<ThemeService>) -> RpcHandler {
    Arc::new(move |method, params| {
        let service = service.clone();
        Box::pin(async move { service.call(&method, params).await })
    })
}

impl ThemeService {
    async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let _guard = if method == "ai.generate" {
            None
        } else {
            Some(self.state_lock.lock().await)
        };
        let packages = ThemePackageManager::new(self.themes.clone());
        match method {
            "theme.health" => Ok(json!({ "status": "ok", "version": 1 })),
            "ai.settings.get" => Ok(serde_json::to_value(self.ai.public_settings()?)?),
            "ai.log.get" => Ok(serde_json::to_value(self.ai.request_log())?),
            "ai.progress.get" => Ok(serde_json::to_value(self.ai.generation_progress())?),
            "ai.generation.restore" => Ok(serde_json::to_value(self.ai.restore_checkpoint()?)?),
            "ai.settings.save" => {
                let settings: AiPublicSettings = serde_json::from_value(
                    params
                        .get("settings")
                        .cloned()
                        .context("ai.settings.save 缺少 settings")?,
                )
                .context("AI 设置格式无效")?;
                let api_key = params.get("apiKey").and_then(Value::as_str);
                let image_api_key = params.get("imageApiKey").and_then(Value::as_str);
                let clear_api_key = params
                    .get("clearApiKey")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let clear_image_api_key = params
                    .get("clearImageApiKey")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                Ok(serde_json::to_value(self.ai.save_settings(
                    settings,
                    api_key,
                    image_api_key,
                    clear_api_key,
                    clear_image_api_key,
                )?)?)
            }
            "ai.reference.import" => {
                let imported = packages.import_image_staging_many("ai-reference").await?;
                let references = imported
                    .into_iter()
                    .map(|(session, path)| -> anyhow::Result<Value> {
                        Ok(json!({ "session": session, "path": path }))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(json!({ "references": references }))
            }
            "ai.reference.upload" => {
                let data_url = string_param(&params, "dataUrl")?;
                let (mime, encoded) = data_url
                    .strip_prefix("data:")
                    .and_then(|value| value.split_once(";base64,"))
                    .context("粘贴图片数据格式无效")?;
                let extension = match mime {
                    "image/png" => "png",
                    "image/jpeg" => "jpg",
                    "image/webp" => "webp",
                    "image/gif" => "gif",
                    "image/bmp" => "bmp",
                    _ => bail!("不支持的粘贴图片格式"),
                };
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .context("粘贴图片无法解码")?;
                let (session, path) =
                    packages.stage_image_bytes("ai-reference", extension, &bytes)?;
                Ok(json!({ "session": session, "path": path }))
            }
            "ai.generate" => {
                let request: GenerateRequest =
                    serde_json::from_value(params).context("AI 生成参数格式无效")?;
                Ok(serde_json::to_value(self.ai.generate(request).await?)?)
            }
            "ai.generation.save" => self.save_generated_theme(params),
            "theme.state.get" => self.state_value(),
            "theme.package.list" => Ok(serde_json::to_value(self.themes.list()?)?),
            "theme.package.read" => {
                let theme = self.themes.load(string_param(&params, "id")?)?;
                let trusted = packages.custom_css_trusted(&theme)?;
                let css = self.themes.custom_css(&theme)?;
                Ok(json!({ "theme": theme, "customCssTrusted": trusted, "customCss": css }))
            }
            "theme.package.create" => {
                let theme = packages.create_theme(
                    string_param(&params, "sourceId")?,
                    string_param(&params, "name")?,
                )?;
                Ok(serde_json::to_value(theme)?)
            }
            "theme.package.import" => Ok(serde_json::to_value(packages.import_dialog().await?)?),
            "theme.package.export" => {
                packages
                    .export_dialog(string_param(&params, "id")?.to_string())
                    .await?;
                Ok(json!({ "status": "ok" }))
            }
            "theme.package.clone" => Ok(serde_json::to_value(
                packages.clone_theme(string_param(&params, "id")?)?,
            )?),
            "theme.package.delete" => {
                let id = string_param(&params, "id")?;
                let mut settings = self.settings.load()?;
                if settings.active_theme_id == id {
                    settings.active_theme_id = "builtin.neutral-dark".into();
                    self.settings.save(&settings)?;
                }
                packages.delete_theme(id, &settings.active_theme_id)?;
                self.state_value()
            }
            "theme.background.import" => {
                let id = string_param(&params, "id")?;
                let theme = self.themes.load(id)?;
                let purpose = params
                    .get("purpose")
                    .and_then(Value::as_str)
                    .unwrap_or("background");
                let (session, path) = packages.import_image_staging(purpose).await?;
                let data_url = packages.background_data_url(id, &path, Some(&session))?;
                Ok(
                    json!({ "path": path, "theme": theme, "stagingSession": session, "previewUrl": data_url }),
                )
            }
            "theme.background.data" => Ok(json!({
                "url": packages.background_data_url(
                    string_param(&params, "id")?,
                    string_param(&params, "path")?,
                    None,
                )?
            })),
            "theme.preview.data" => Ok(json!({
                "url": packages.background_data_url(
                    "builtin.neutral-dark",
                    string_param(&params, "path")?,
                    Some(string_param(&params, "session")?),
                )?
            })),
            "theme.preview.thumbnail" => Ok(json!({
                "url": packages.staging_thumbnail_data_url(
                    string_param(&params, "session")?,
                    string_param(&params, "path")?,
                )?
            })),
            "theme.asset.import" => {
                let id = string_param(&params, "id")?;
                let theme = self.themes.load(id)?;
                let purpose = string_param(&params, "purpose")?;
                let (session, path) = packages.import_image_staging(purpose).await?;
                let data_url = packages.background_data_url(id, &path, Some(&session))?;
                Ok(json!({
                    "path": path,
                    "theme": theme,
                    "stagingSession": session,
                    "previewUrl": data_url
                }))
            }
            "theme.preview.cancel" => {
                let session = string_param(&params, "session")?;
                if !self.ai.checkpoint_has_session(session) {
                    packages.cancel_staging(session)?;
                }
                Ok(json!({ "status": "ok" }))
            }
            "theme.activate" => {
                let id = string_param(&params, "id")?;
                let theme = self.themes.load(id)?;
                let mut settings = self.settings.load()?;
                settings.active_theme_id = theme.id;
                self.settings.save(&settings)?;
                self.state_value()
            }
            "theme.apply" => self.apply_theme(params),
            "theme.custom_css.trust" => {
                let theme = self.themes.load(string_param(&params, "id")?)?;
                packages.trust_custom_css(&theme)?;
                Ok(json!({ "trusted": true }))
            }
            "theme.custom_css.revoke" => {
                packages.revoke_custom_css(string_param(&params, "id")?)?;
                Ok(json!({ "trusted": false }))
            }
            "runtime.reinject" => {
                self.reinject_requested.store(true, Ordering::SeqCst);
                Ok(json!({ "status": "scheduled" }))
            }
            "diagnostics.report" => {
                crate::diagnostic::log("renderer.report", params);
                Ok(json!({ "status": "recorded" }))
            }
            _ => bail!("未知 RPC 方法：{method}"),
        }
    }

    fn state_value(&self) -> anyhow::Result<Value> {
        let settings = self.settings.load()?;
        let active = self
            .themes
            .load(&settings.active_theme_id)
            .or_else(|_| self.themes.load("builtin.neutral-dark"))?;
        let manager = ThemePackageManager::new(self.themes.clone());
        let custom_css_trusted = manager.custom_css_trusted(&active)?;
        let custom_css = self.themes.custom_css(&active)?;
        Ok(json!({
            "settings": settings,
            "activeTheme": active,
            "themes": self.themes.list()?,
            "assetBase": self.asset_base,
            "assetToken": self.asset_token,
            "customCssTrusted": custom_css_trusted,
            "customCss": custom_css,
            "ai": self.ai.public_settings()?
        }))
    }

    fn apply_theme(&self, params: Value) -> anyhow::Result<Value> {
        let mut theme = ThemeManifest::from_value_migrated(
            params
                .get("theme")
                .cloned()
                .context("theme.apply 缺少 theme")?,
        )?;
        let custom_css_requested = params
            .get("customCss")
            .and_then(Value::as_str)
            .is_some_and(|css| !css.trim().is_empty());
        if theme.id.starts_with("builtin.") {
            let original = self.themes.load(&theme.id)?;
            if original != theme || custom_css_requested {
                theme.id = format!("local.{}", uuid::Uuid::new_v4().simple());
                theme.name = format!("{} 自定义", original.name);
                theme.version = "0.1.0".into();
                theme.author = "Local user".into();
            }
        }
        let asset_paths = theme.asset_paths();
        if let Some(assets) = params.get("stagingAssets").and_then(Value::as_array) {
            if assets.len() > 64 {
                bail!("单次应用的预览资源不能超过 64 个");
            }
            for asset in assets {
                let session = string_param(asset, "session")?;
                let relative = string_param(asset, "path")?;
                if !asset_paths.contains(&relative) {
                    bail!("预览资源未被当前主题引用");
                }
                let packages = ThemePackageManager::new(self.themes.clone());
                if self.ai.checkpoint_has_session(session) {
                    packages.promote_image_preserving_staging(session, relative, &theme.id)?;
                } else {
                    packages.promote_image(session, relative, &theme.id)?;
                }
            }
        } else if let Some(session) = params.get("stagingSession").and_then(Value::as_str) {
            let relative = asset_paths
                .first()
                .context("预览会话没有对应的主题图片资源")?;
            ThemePackageManager::new(self.themes.clone())
                .promote_image(session, relative, &theme.id)?;
        }
        self.themes.save(&theme)?;
        if let Some(css) = params.get("customCss").and_then(Value::as_str) {
            self.themes.save_custom_css(&mut theme, css)?;
            let packages = ThemePackageManager::new(self.themes.clone());
            packages.revoke_custom_css(&theme.id)?;
            if params
                .get("enableCustomCss")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && !css.trim().is_empty()
            {
                packages.trust_custom_css(&theme)?;
            }
        }
        let mut settings = self.settings.load()?;
        settings.active_theme_id = theme.id.clone();
        self.settings.save(&settings)?;
        self.ai.clear_checkpoint_for_theme(&theme.id)?;
        Ok(json!({ "theme": theme, "state": self.state_value()? }))
    }

    fn save_generated_theme(&self, params: Value) -> anyhow::Result<Value> {
        let name = string_param(&params, "name")?.trim();
        if name.is_empty() || name.chars().count() > 80 {
            bail!("新主题名称不能为空且不能超过 80 个字符");
        }
        let mut theme = ThemeManifest::from_value_migrated(
            params
                .get("theme")
                .cloned()
                .context("ai.generation.save 缺少 theme")?,
        )?;
        theme.id = format!("local.{}", uuid::Uuid::new_v4().simple());
        theme.name = name.to_string();
        theme.version = "0.1.0".into();
        theme.author = "Local user".into();
        theme.validate()?;
        let asset_paths = theme.asset_paths();
        let assets = params
            .get("stagingAssets")
            .and_then(Value::as_array)
            .context("ai.generation.save 缺少 stagingAssets")?;
        if assets.len() > 64 {
            bail!("单次保存的预览资源不能超过 64 个");
        }
        let packages = ThemePackageManager::new(self.themes.clone());
        for relative in &asset_paths {
            if !assets
                .iter()
                .any(|asset| asset.get("path").and_then(Value::as_str) == Some(relative))
            {
                bail!("生成主题缺少预览资源：{relative}");
            }
        }
        let save_result = (|| -> anyhow::Result<()> {
            for asset in assets {
                let session = string_param(asset, "session")?;
                let relative = string_param(asset, "path")?;
                if !asset_paths.contains(&relative) {
                    bail!("预览资源未被当前主题引用");
                }
                packages.promote_image_preserving_staging(session, relative, &theme.id)?;
            }
            self.themes.save(&theme)?;
            let mut settings = self.settings.load()?;
            settings.active_theme_id = theme.id.clone();
            self.settings.save(&settings)
        })();
        if let Err(error) = save_result {
            let _ = std::fs::remove_dir_all(self.themes.paths.themes.join(&theme.id));
            return Err(error);
        }
        self.ai.clear_checkpoint()?;
        for asset in assets {
            let _ = packages.cancel_staging(string_param(asset, "session")?);
        }
        Ok(json!({ "theme": theme, "state": self.state_value()? }))
    }
}

fn string_param<'a>(params: &'a Value, key: &str) -> anyhow::Result<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("缺少参数 {key}"))
}

fn available_port() -> anyhow::Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

fn runtime_script() -> anyhow::Result<String> {
    let script = include_str!("../assets/theme-runtime.js");
    let css = include_str!("../assets/theme-panel.css");
    Ok(script.replace(
        "__THEME_INJECT_PANEL_CSS_JSON__",
        &serde_json::to_string(css)?,
    ))
}
