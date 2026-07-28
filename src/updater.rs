use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use futures_util::StreamExt;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::storage::{AppPaths, SettingsStore};

const REPOSITORY: &str = "codecnmc/codex-theme-inject";
const RELEASE_API: &str =
    "https://api.github.com/repos/codecnmc/codex-theme-inject/releases/latest";
const RELEASE_DOWNLOAD_PREFIX: &str =
    "https://github.com/codecnmc/codex-theme-inject/releases/download/";
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 16 * 1024;
const MAX_EXECUTABLE_BYTES: u64 = 128 * 1024 * 1024;
const AUTO_CHECK_INTERVAL: u64 = 24 * 60 * 60;
const INTERNAL_UPDATE_HELPER: &str = "--internal-update-helper";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub version: String,
    pub tag: String,
    pub name: String,
    pub notes: String,
    pub published_at: String,
    pub page_url: String,
    zip_url: String,
    checksum_url: String,
    zip_size: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub phase: String,
    pub latest: Option<ReleaseInfo>,
    pub checked_at: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
    pub install_supported: bool,
    pub install_reason: Option<String>,
}

impl UpdateStatus {
    fn initial(paths: &AppPaths, settings: &SettingsStore) -> Self {
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        let (install_supported, install_reason) = install_capability();
        let saved = settings.load().unwrap_or_default();
        let downloaded = saved.downloaded_update_version;
        let prepared = prepared_executable(paths, &downloaded);
        let ready = !downloaded.is_empty()
            && newer_than_current(&downloaded).unwrap_or(false)
            && prepared.is_file();
        let latest = ready.then(|| ReleaseInfo {
            version: downloaded.clone(),
            tag: format!("v{downloaded}"),
            name: format!("Theme Inject v{downloaded}"),
            notes: String::new(),
            published_at: String::new(),
            page_url: format!("https://github.com/{REPOSITORY}/releases/tag/v{downloaded}"),
            zip_url: String::new(),
            checksum_url: String::new(),
            zip_size: 0,
        });
        Self {
            current_version,
            phase: if ready { "ready" } else { "idle" }.into(),
            latest,
            checked_at: saved.last_update_check_at,
            downloaded_bytes: 0,
            total_bytes: 0,
            error: None,
            install_supported,
            install_reason,
        }
    }
}

#[derive(Clone)]
pub struct UpdateManager {
    paths: AppPaths,
    settings: SettingsStore,
    settings_lock: Arc<Mutex<()>>,
    state: Arc<Mutex<UpdateStatus>>,
    cancel: Arc<AtomicBool>,
    download_running: Arc<AtomicBool>,
    restart_requested: Arc<AtomicBool>,
    relaunch_args: Vec<String>,
    client: reqwest::Client,
}

impl UpdateManager {
    pub fn new(
        paths: AppPaths,
        settings: SettingsStore,
        settings_lock: Arc<Mutex<()>>,
        restart_requested: Arc<AtomicBool>,
        relaunch_args: Vec<String>,
    ) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("更新下载重定向次数过多");
                }
                if is_allowed_host(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error("更新下载被重定向到不受信任的主机")
                }
            }))
            .build()?;
        let state = UpdateStatus::initial(&paths, &settings);
        Ok(Self {
            paths,
            settings,
            settings_lock,
            state: Arc::new(Mutex::new(state)),
            cancel: Arc::new(AtomicBool::new(false)),
            download_running: Arc::new(AtomicBool::new(false)),
            restart_requested,
            relaunch_args,
            client,
        })
    }

    pub async fn status(&self) -> UpdateStatus {
        self.state.lock().await.clone()
    }

    pub async fn check_if_due(&self) {
        let due = self
            .settings
            .load()
            .map(|settings| {
                now_epoch().saturating_sub(settings.last_update_check_at) >= AUTO_CHECK_INTERVAL
            })
            .unwrap_or(true);
        if due && let Err(error) = self.check().await {
            crate::diagnostic::log(
                "updater.automatic_check_failed",
                serde_json::json!({ "message": format!("{error:#}") }),
            );
        }
    }

    pub async fn check(&self) -> anyhow::Result<UpdateStatus> {
        if self.download_running.load(Ordering::SeqCst) {
            bail!("更新包正在下载，请稍后再检查");
        }
        {
            let mut state = self.state.lock().await;
            if state.phase == "checking" {
                bail!("更新检查已在进行中");
            }
            state.phase = "checking".into();
            state.error = None;
        }
        let checked_at = now_epoch();
        let result = self.fetch_latest().await;
        self.store_check_time(checked_at).await?;
        match result {
            Ok(release) => {
                let available = newer_than_current(&release.version)?;
                let mut state = self.state.lock().await;
                state.checked_at = checked_at;
                state.phase = if available { "available" } else { "up-to-date" }.into();
                state.latest = Some(release);
                state.downloaded_bytes = 0;
                state.total_bytes = 0;
                state.error = None;
                Ok(state.clone())
            }
            Err(error) => {
                let message = format!("{error:#}");
                let mut state = self.state.lock().await;
                state.checked_at = checked_at;
                state.phase = "error".into();
                state.error = Some(message);
                Err(error)
            }
        }
    }

    pub async fn start_download(&self) -> anyhow::Result<UpdateStatus> {
        if self
            .download_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            bail!("更新包已在下载中");
        }
        let release_result = async {
            let mut state = self.state.lock().await;
            let release = state.latest.clone().context("请先检查更新")?;
            if !newer_than_current(&release.version)? {
                bail!("当前已经是最新版本");
            }
            if release.zip_url.is_empty() || release.checksum_url.is_empty() {
                bail!("缓存的更新信息不包含下载地址，请重新检查更新");
            }
            state.phase = "downloading".into();
            state.error = None;
            state.downloaded_bytes = 0;
            state.total_bytes = release.zip_size;
            Ok(release)
        }
        .await;
        let release = match release_result {
            Ok(release) => release,
            Err(error) => {
                self.download_running.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };
        self.cancel.store(false, Ordering::SeqCst);
        let manager = self.clone();
        tokio::spawn(async move {
            let result = manager.download_release(&release).await;
            manager.download_running.store(false, Ordering::SeqCst);
            match result {
                Ok(prepared) => {
                    if let Err(error) = manager.store_downloaded_version(&release.version).await {
                        manager.fail_download(error).await;
                        return;
                    }
                    let mut state = manager.state.lock().await;
                    state.phase = "ready".into();
                    state.downloaded_bytes = state.total_bytes;
                    state.error = None;
                    crate::diagnostic::log(
                        "updater.download_ready",
                        serde_json::json!({ "version": release.version, "path": prepared }),
                    );
                }
                Err(error) if manager.cancel.load(Ordering::SeqCst) => {
                    let mut state = manager.state.lock().await;
                    state.phase = "available".into();
                    state.downloaded_bytes = 0;
                    state.error = None;
                    crate::diagnostic::log("updater.download_cancelled", serde_json::json!({}));
                }
                Err(error) => manager.fail_download(error).await,
            }
        });
        Ok(self.status().await)
    }

    pub async fn cancel(&self) -> UpdateStatus {
        self.cancel.store(true, Ordering::SeqCst);
        self.status().await
    }

    pub async fn install(&self) -> anyhow::Result<UpdateStatus> {
        let (supported, reason) = install_capability();
        if !supported {
            bail!(
                "{}",
                reason.unwrap_or_else(|| "当前构建不支持自动安装".into())
            );
        }
        if self.download_running.load(Ordering::SeqCst) {
            bail!("更新包尚未下载完成");
        }
        let version = {
            let state = self.state.lock().await;
            if state.phase != "ready" {
                bail!("没有已校验、可安装的更新包");
            }
            state
                .latest
                .as_ref()
                .context("更新版本信息缺失")?
                .version
                .clone()
        };
        let prepared = prepared_executable(&self.paths, &version);
        if !prepared.is_file() {
            bail!("已校验的更新程序不存在，请重新下载");
        }
        schedule_install(&self.paths, &prepared, &self.relaunch_args)?;
        {
            let mut state = self.state.lock().await;
            state.phase = "installing".into();
        }
        self.restart_requested.store(true, Ordering::SeqCst);
        Ok(self.status().await)
    }

    async fn fetch_latest(&self) -> anyhow::Result<ReleaseInfo> {
        let response = self
            .client
            .get(RELEASE_API)
            .header(reqwest::header::USER_AGENT, "theme-inject-updater")
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .context("无法连接 GitHub 更新服务")?;
        if response.status() == reqwest::StatusCode::FORBIDDEN
            || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            bail!("GitHub 请求受限，请稍后重试");
        }
        let response = response
            .error_for_status()
            .context("GitHub Release 查询失败")?;
        let raw: GithubRelease = response
            .json()
            .await
            .context("GitHub Release 数据格式无效")?;
        parse_release(raw)
    }

    async fn download_release(&self, release: &ReleaseInfo) -> anyhow::Result<PathBuf> {
        let directory = update_directory(&self.paths, &release.version);
        fs::create_dir_all(&directory)?;
        let zip = directory.join(format!("theme-inject-windows-x64-v{}.zip", release.version));
        let checksum = zip.with_extension("zip.sha256");
        self.download_to(&release.zip_url, &zip, MAX_PACKAGE_BYTES, true)
            .await?;
        self.download_to(&release.checksum_url, &checksum, MAX_CHECKSUM_BYTES, false)
            .await?;
        if self.cancel.load(Ordering::SeqCst) {
            bail!("下载已取消");
        }
        verify_checksum(&zip, &checksum)?;
        let prepared = prepared_executable(&self.paths, &release.version);
        extract_executable(&zip, &prepared)?;
        Ok(prepared)
    }

    async fn download_to(
        &self,
        url: &str,
        destination: &Path,
        limit: u64,
        report_progress: bool,
    ) -> anyhow::Result<()> {
        validate_asset_url(url)?;
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, "theme-inject-updater")
            .timeout(Duration::from_secs(30 * 60))
            .send()
            .await
            .context("更新资产下载请求失败")?
            .error_for_status()
            .context("更新资产下载失败")?;
        if !is_allowed_host(response.url()) {
            bail!("更新资产被重定向到不受信任的主机");
        }
        if response
            .content_length()
            .is_some_and(|length| length > limit)
        {
            bail!("更新资产超过允许的大小");
        }
        let temporary = destination.with_extension("download");
        let mut file = File::create(&temporary)?;
        let mut received = 0u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if self.cancel.load(Ordering::SeqCst) {
                drop(file);
                let _ = fs::remove_file(&temporary);
                bail!("下载已取消");
            }
            let chunk = chunk.context("读取更新下载流失败")?;
            received = received.saturating_add(chunk.len() as u64);
            if received > limit {
                drop(file);
                let _ = fs::remove_file(&temporary);
                bail!("更新资产超过允许的大小");
            }
            file.write_all(&chunk)?;
            if report_progress {
                let mut state = self.state.lock().await;
                state.downloaded_bytes = received;
                if state.total_bytes == 0 {
                    state.total_bytes = received;
                }
            }
            tokio::task::yield_now().await;
        }
        file.flush()?;
        drop(file);
        if received == 0 {
            let _ = fs::remove_file(&temporary);
            bail!("更新资产内容为空");
        }
        if destination.exists() {
            fs::remove_file(destination)?;
        }
        fs::rename(temporary, destination)?;
        Ok(())
    }

    async fn store_check_time(&self, checked_at: u64) -> anyhow::Result<()> {
        let _guard = self.settings_lock.lock().await;
        let mut settings = self.settings.load()?;
        settings.last_update_check_at = checked_at;
        self.settings.save(&settings)
    }

    async fn store_downloaded_version(&self, version: &str) -> anyhow::Result<()> {
        let _guard = self.settings_lock.lock().await;
        let mut settings = self.settings.load()?;
        settings.downloaded_update_version = version.to_string();
        self.settings.save(&settings)
    }

    async fn fail_download(&self, error: anyhow::Error) {
        let message = format!("{error:#}");
        let mut state = self.state.lock().await;
        state.phase = "error".into();
        state.error = Some(message.clone());
        crate::diagnostic::log(
            "updater.download_failed",
            serde_json::json!({ "message": message }),
        );
    }
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn parse_release(raw: GithubRelease) -> anyhow::Result<ReleaseInfo> {
    if raw.draft || raw.prerelease {
        bail!("最新 Release 不是稳定正式版本");
    }
    let version_text = raw
        .tag_name
        .strip_prefix('v')
        .context("Release 标签必须使用 v{SemVer}")?;
    let version = Version::parse(version_text).context("Release 标签不是有效 SemVer")?;
    if !version.pre.is_empty() {
        bail!("稳定更新不接受预发布版本");
    }
    let zip_name = format!("theme-inject-windows-x64-v{version}.zip");
    let checksum_name = format!("{zip_name}.sha256");
    let zip = unique_asset(&raw.assets, &zip_name)?;
    let checksum = unique_asset(&raw.assets, &checksum_name)?;
    if zip.size == 0 || zip.size > MAX_PACKAGE_BYTES {
        bail!("Release 更新包大小无效");
    }
    if checksum.size == 0 || checksum.size > MAX_CHECKSUM_BYTES {
        bail!("Release 校验文件大小无效");
    }
    validate_asset_url(&zip.browser_download_url)?;
    validate_asset_url(&checksum.browser_download_url)?;
    let expected_page = format!("https://github.com/{REPOSITORY}/releases/");
    if !raw.html_url.starts_with(&expected_page) {
        bail!("Release 页面不属于固定更新仓库");
    }
    Ok(ReleaseInfo {
        version: version.to_string(),
        tag: raw.tag_name,
        name: raw
            .name
            .unwrap_or_else(|| format!("Theme Inject v{version}")),
        notes: raw.body.unwrap_or_default(),
        published_at: raw.published_at.unwrap_or_default(),
        page_url: raw.html_url,
        zip_url: zip.browser_download_url.clone(),
        checksum_url: checksum.browser_download_url.clone(),
        zip_size: zip.size,
    })
}

fn unique_asset<'a>(assets: &'a [GithubAsset], name: &str) -> anyhow::Result<&'a GithubAsset> {
    let mut matches = assets.iter().filter(|asset| asset.name == name);
    let asset = matches
        .next()
        .with_context(|| format!("Release 缺少 {name}"))?;
    if matches.next().is_some() {
        bail!("Release 包含重复资产 {name}");
    }
    Ok(asset)
}

fn validate_asset_url(value: &str) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(value).context("Release 资产地址无效")?;
    if url.scheme() != "https" || !value.starts_with(RELEASE_DOWNLOAD_PREFIX) {
        bail!("Release 资产不属于固定 GitHub 仓库");
    }
    Ok(())
}

fn is_allowed_host(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some("api.github.com")
                | Some("github.com")
                | Some("objects.githubusercontent.com")
                | Some("release-assets.githubusercontent.com")
        )
}

fn newer_than_current(candidate: &str) -> anyhow::Result<bool> {
    Ok(Version::parse(candidate).context("更新版本号无效")?
        > Version::parse(env!("CARGO_PKG_VERSION")).context("当前程序版本号无效")?)
}

fn verify_checksum(package: &Path, checksum_file: &Path) -> anyhow::Result<()> {
    let checksum = fs::read_to_string(checksum_file).context("无法读取 SHA-256 文件")?;
    let expected = checksum
        .split_whitespace()
        .next()
        .context("SHA-256 文件为空")?
        .to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|value| value.is_ascii_hexdigit()) {
        bail!("SHA-256 文件格式无效");
    }
    let mut file = File::open(package)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        bail!("更新包 SHA-256 校验失败");
    }
    Ok(())
}

fn extract_executable(package: &Path, destination: &Path) -> anyhow::Result<()> {
    let file = File::open(package)?;
    let mut archive = zip::ZipArchive::new(file).context("更新 ZIP 格式无效")?;
    let mut executable_index = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let enclosed = entry.enclosed_name().context("更新 ZIP 包含越界路径")?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            bail!("更新 ZIP 不允许包含符号链接");
        }
        if entry.is_dir() {
            continue;
        }
        let file_name = enclosed
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if file_name.eq_ignore_ascii_case("theme-inject.exe") {
            if executable_index.replace(index).is_some() {
                bail!("更新 ZIP 包含多个 theme-inject.exe");
            }
        } else if enclosed
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
        {
            bail!("更新 ZIP 包含额外可执行文件");
        }
    }
    let index = executable_index.context("更新 ZIP 缺少 theme-inject.exe")?;
    let mut entry = archive.by_index(index)?;
    if entry.size() == 0 || entry.size() > MAX_EXECUTABLE_BYTES {
        bail!("更新程序大小无效");
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_extension("extracting");
    let mut output = File::create(&temporary)?;
    std::io::copy(&mut entry, &mut output)?;
    output.flush()?;
    drop(output);
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(temporary, destination)?;
    Ok(())
}

fn update_directory(paths: &AppPaths, version: &str) -> PathBuf {
    paths.updates.join(format!("v{version}"))
}

fn prepared_executable(paths: &AppPaths, version: &str) -> PathBuf {
    update_directory(paths, version)
        .join("prepared")
        .join("theme-inject.exe")
}

fn install_capability() -> (bool, Option<String>) {
    if cfg!(debug_assertions) {
        return (false, Some("开发构建只检查更新，不自动替换程序".into()));
    }
    let Ok(current) = std::env::current_exe() else {
        return (false, Some("无法定位当前程序，不能自动安装".into()));
    };
    let normalized = current
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    if normalized.contains("\\target\\") {
        return (false, Some("target 目录中的开发程序不能自动安装".into()));
    }
    (true, None)
}

fn schedule_install(
    paths: &AppPaths,
    prepared: &Path,
    relaunch_args: &[String],
) -> anyhow::Result<()> {
    let current = std::env::current_exe().context("无法定位当前 Theme Inject 程序")?;
    let directory = prepared.parent().context("更新程序目录无效")?;
    let helper = directory.join("theme-inject-update-helper.exe");
    fs::copy(&current, &helper).context("无法创建更新辅助程序")?;
    let backup = current.with_file_name("theme-inject-backup.exe");
    let mut command = std::process::Command::new(&helper);
    command
        .arg(INTERNAL_UPDATE_HELPER)
        .arg(std::process::id().to_string())
        .arg(&current)
        .arg(prepared)
        .arg(&backup)
        .arg(relaunch_args.len().to_string())
        .args(relaunch_args)
        .current_dir(paths.root.as_path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.spawn().context("无法启动更新辅助程序")?;
    Ok(())
}

pub fn run_internal_command(args: &[String]) -> anyhow::Result<bool> {
    if args.first().map(String::as_str) != Some(INTERNAL_UPDATE_HELPER) {
        return Ok(false);
    }
    if args.len() < 6 {
        bail!("内部更新参数无效");
    }
    let parent_pid = args[1].parse::<u32>().context("内部更新父进程 PID 无效")?;
    let target = PathBuf::from(&args[2]);
    let prepared = PathBuf::from(&args[3]);
    let backup = PathBuf::from(&args[4]);
    let argument_count = args[5].parse::<usize>().context("内部更新重启参数无效")?;
    if args.len() != 6 + argument_count {
        bail!("内部更新重启参数数量不匹配");
    }
    run_install_helper(parent_pid, &target, &prepared, &backup, &args[6..])?;
    Ok(true)
}

fn run_install_helper(
    parent_pid: u32,
    target: &Path,
    prepared: &Path,
    backup: &Path,
    relaunch_args: &[String],
) -> anyhow::Result<()> {
    wait_for_process(parent_pid, Duration::from_secs(120))?;
    if target
        .file_name()
        .and_then(|name| name.to_str())
        .is_none_or(|name| !name.eq_ignore_ascii_case("theme-inject.exe"))
    {
        bail!("更新目标不是 theme-inject.exe");
    }
    if !prepared.is_file() || target.parent().is_none() {
        bail!("待安装的更新程序无效");
    }
    let staged = target.with_file_name("theme-inject-update-new.exe");
    let _ = fs::remove_file(&staged);
    fs::copy(prepared, &staged).context("无法把更新程序复制到安装目录")?;
    let _ = fs::remove_file(backup);
    fs::rename(target, backup).context("无法备份当前程序")?;
    if let Err(error) = fs::rename(&staged, target) {
        let _ = fs::rename(backup, target);
        return Err(error).context("无法替换 Theme Inject 程序");
    }
    let mut child = match std::process::Command::new(target)
        .args(relaunch_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            restore_backup(target, backup)?;
            return Err(error).context("新版本无法启动，已恢复旧版本");
        }
    };
    std::thread::sleep(Duration::from_secs(5));
    if child.try_wait()?.is_some() {
        restore_backup(target, backup)?;
        let _ = std::process::Command::new(target)
            .args(relaunch_args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        bail!("新版本启动后过早退出，已恢复旧版本");
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

fn restore_backup(target: &Path, backup: &Path) -> anyhow::Result<()> {
    if target.exists() {
        fs::remove_file(target)?;
    }
    fs::rename(backup, target).context("无法恢复旧版本")
}

#[cfg(windows)]
fn wait_for_process(process_id: u32, timeout: Duration) -> anyhow::Result<()> {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_ACCESS_RIGHTS, WaitForSingleObject,
    };
    let handle = unsafe { OpenProcess(PROCESS_ACCESS_RIGHTS(0x0010_0000), false, process_id) }
        .context("无法等待旧 Theme Inject 进程")?;
    let result =
        unsafe { WaitForSingleObject(handle, timeout.as_millis().min(u32::MAX as u128) as u32) };
    let _ = unsafe { CloseHandle(handle) };
    if result == WAIT_OBJECT_0 {
        Ok(())
    } else if result == WAIT_TIMEOUT {
        bail!("等待旧 Theme Inject 退出超时")
    } else {
        bail!("等待旧 Theme Inject 退出失败")
    }
}

#[cfg(not(windows))]
fn wait_for_process(_process_id: u32, _timeout: Duration) -> anyhow::Result<()> {
    Ok(())
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, assets: Vec<GithubAsset>) -> GithubRelease {
        GithubRelease {
            tag_name: tag.into(),
            name: None,
            body: Some("修复内容".into()),
            published_at: Some("2026-07-27T00:00:00Z".into()),
            html_url: format!("https://github.com/{REPOSITORY}/releases/tag/{tag}"),
            draft: false,
            prerelease: false,
            assets,
        }
    }

    fn asset(name: &str, size: u64) -> GithubAsset {
        GithubAsset {
            name: name.into(),
            browser_download_url: format!("{RELEASE_DOWNLOAD_PREFIX}v1.2.3/{name}"),
            size,
        }
    }

    #[test]
    fn parses_expected_stable_release_assets() {
        let zip = "theme-inject-windows-x64-v1.2.3.zip";
        let parsed = parse_release(release(
            "v1.2.3",
            vec![asset(zip, 1024), asset(&format!("{zip}.sha256"), 64)],
        ))
        .unwrap();
        assert_eq!(parsed.version, "1.2.3");
    }

    #[test]
    fn rejects_prerelease_and_foreign_assets() {
        let zip = "theme-inject-windows-x64-v1.2.3-beta.1.zip";
        assert!(
            parse_release(release(
                "v1.2.3-beta.1",
                vec![asset(zip, 1024), asset(&format!("{zip}.sha256"), 64)],
            ))
            .is_err()
        );
        assert!(validate_asset_url("https://example.com/theme-inject.zip").is_err());
    }

    #[test]
    fn validates_sha256_file() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("update.zip");
        let checksum = temp.path().join("update.zip.sha256");
        fs::write(&package, b"theme-inject").unwrap();
        let digest = format!("{:x}", Sha256::digest(b"theme-inject"));
        fs::write(&checksum, format!("{digest}  update.zip\n")).unwrap();
        verify_checksum(&package, &checksum).unwrap();
        fs::write(&checksum, format!("{}  update.zip\n", "0".repeat(64))).unwrap();
        assert!(verify_checksum(&package, &checksum).is_err());
    }

    #[test]
    fn extracts_only_the_expected_executable() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("update.zip");
        {
            let file = File::create(&package).unwrap();
            let mut archive = zip::ZipWriter::new(file);
            archive
                .start_file(
                    "theme-inject/theme-inject.exe",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            archive.write_all(b"new-executable").unwrap();
            archive.finish().unwrap();
        }
        let executable = temp.path().join("prepared/theme-inject.exe");
        extract_executable(&package, &executable).unwrap();
        assert_eq!(fs::read(executable).unwrap(), b"new-executable");
    }

    #[test]
    fn rejects_extra_executables_in_archive() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("update.zip");
        {
            let file = File::create(&package).unwrap();
            let mut archive = zip::ZipWriter::new(file);
            for name in ["theme-inject.exe", "other.exe"] {
                archive
                    .start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                archive.write_all(b"x").unwrap();
            }
            archive.finish().unwrap();
        }
        assert!(extract_executable(&package, &temp.path().join("prepared.exe")).is_err());
    }
}
