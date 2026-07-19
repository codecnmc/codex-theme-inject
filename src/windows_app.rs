use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, bail};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
pub struct DialogParent(windows::Win32::Foundation::HWND);

#[cfg(windows)]
impl raw_window_handle::HasWindowHandle for DialogParent {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::Win32WindowHandle::new(
            std::num::NonZeroIsize::new(self.0.0 as isize)
                .ok_or(raw_window_handle::HandleError::Unavailable)?,
        );
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::Win32(
                handle,
            ))
        })
    }
}

#[cfg(windows)]
impl raw_window_handle::HasDisplayHandle for DialogParent {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe {
            raw_window_handle::DisplayHandle::borrow_raw(
                raw_window_handle::RawDisplayHandle::Windows(
                    raw_window_handle::WindowsDisplayHandle::new(),
                ),
            )
        })
    }
}

#[cfg(windows)]
pub fn foreground_dialog_parent() -> Option<DialogParent> {
    let window = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    (!window.is_invalid()).then_some(DialogParent(window))
}

#[cfg(not(windows))]
pub struct DialogParent;

#[cfg(not(windows))]
pub fn foreground_dialog_parent() -> Option<DialogParent> {
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodexApp {
    Packaged {
        app_dir: PathBuf,
        app_user_model_id: String,
        version: String,
    },
    Standalone {
        executable: PathBuf,
        version: String,
    },
}

impl CodexApp {
    pub fn version(&self) -> &str {
        match self {
            Self::Packaged { version, .. } | Self::Standalone { version, .. } => version,
        }
    }

    pub async fn launch(&self, debug_port: u16) -> anyhow::Result<u32> {
        let arguments = vec![
            format!("--remote-debugging-port={debug_port}"),
            format!("--remote-allow-origins=http://127.0.0.1:{debug_port}"),
        ];
        match self {
            Self::Packaged {
                app_user_model_id, ..
            } => launch_packaged_activation_helper(app_user_model_id, &quote_arguments(&arguments)),
            Self::Standalone { executable, .. } => {
                let child = tokio::process::Command::new(executable)
                    .args(&arguments)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .with_context(|| format!("无法启动 {}", executable.display()))?;
                child.id().context("Codex 进程没有 PID")
            }
        }
    }
}

const INTERNAL_ACTIVATE_PACKAGED: &str = "--internal-activate-packaged";

pub fn run_internal_command(args: &[String]) -> anyhow::Result<bool> {
    if args.first().map(String::as_str) != Some(INTERNAL_ACTIVATE_PACKAGED) {
        return Ok(false);
    }
    if args.len() != 3 {
        bail!("内部 MSIX 激活参数无效");
    }
    activate_packaged_blocking(&args[1], &args[2])?;
    Ok(true)
}

#[cfg(windows)]
fn launch_packaged_activation_helper(
    app_user_model_id: &str,
    arguments: &str,
) -> anyhow::Result<u32> {
    use std::os::windows::process::CommandExt;

    let executable = std::env::current_exe().context("无法定位 Theme Inject 可执行文件")?;
    let child = std::process::Command::new(executable)
        .arg(INTERNAL_ACTIVATE_PACKAGED)
        .arg(app_user_model_id)
        .arg(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("无法启动隔离的 MSIX 激活 helper")?;
    Ok(child.id())
}

#[cfg(not(windows))]
fn launch_packaged_activation_helper(
    _app_user_model_id: &str,
    _arguments: &str,
) -> anyhow::Result<u32> {
    bail!("MSIX 启动仅支持 Windows")
}

pub fn discover(explicit: Option<&Path>) -> anyhow::Result<CodexApp> {
    if let Some(path) = explicit {
        return app_from_explicit_path(path);
    }
    let mut packaged = discover_windows_apps();
    packaged.sort_by_key(|left| version_key(left.version()));
    if let Some(app) = packaged.pop() {
        return Ok(app);
    }
    if let Some(app) = discover_with_powershell() {
        return Ok(app);
    }
    for path in standalone_candidates() {
        if path.is_file() {
            return Ok(CodexApp::Standalone {
                executable: path,
                version: "unknown".into(),
            });
        }
    }
    bail!("未找到 Codex，请使用 --app-path 指定 ChatGPT.exe、Codex.exe 或应用目录")
}

pub fn codex_is_running() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        };
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return false;
            };
            if snapshot == INVALID_HANDLE_VALUE {
                return false;
            }
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut found = false;
            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let length = entry
                        .szExeFile
                        .iter()
                        .position(|value| *value == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..length]);
                    if name.eq_ignore_ascii_case("ChatGPT.exe")
                        || name.eq_ignore_ascii_case("Codex.exe")
                    {
                        found = true;
                        break;
                    }
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
            found
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub async fn running_debug_port() -> Option<u16> {
    #[cfg(windows)]
    {
        let script = r#"$process = Get-CimInstance Win32_Process | Where-Object { ($_.Name -eq 'ChatGPT.exe' -or $_.Name -eq 'Codex.exe') -and $_.CommandLine -match '--remote-debugging-port=(\d+)' } | Select-Object -First 1; if ($process -and $process.CommandLine -match '--remote-debugging-port=(\d+)') { $matches[1] }"#;
        let mut command = tokio::process::Command::new("powershell");
        command
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .creation_flags(CREATE_NO_WINDOW);
        let output = tokio::time::timeout(Duration::from_secs(5), command.output())
            .await
            .ok()?
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn discover_windows_apps() -> Vec<CodexApp> {
    let root = PathBuf::from(r"C:\Program Files\WindowsApps");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| packaged_from_directory(&entry.path()))
        .collect()
}

fn discover_with_powershell() -> Option<CodexApp> {
    let mut command = std::process::Command::new("powershell");
    command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "$names=@('OpenAI.Codex','OpenAI.CodexBeta'); Get-AppxPackage | Where-Object { $names -contains $_.Name } | Sort-Object Version -Descending | Select-Object -First 1 -ExpandProperty InstallLocation",
        ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    packaged_from_directory(Path::new(path))
}

fn packaged_from_directory(path: &Path) -> Option<CodexApp> {
    let package_name = path.file_name()?.to_str()?;
    let (identity, version, publisher) = parse_package_name(package_name)?;
    let app_dir = if path.join("app").is_dir() {
        path.join("app")
    } else {
        path.to_path_buf()
    };
    if !["ChatGPT.exe", "Codex.exe", "codex.exe"]
        .iter()
        .any(|name| app_dir.join(name).is_file())
    {
        return None;
    }
    Some(CodexApp::Packaged {
        app_dir,
        app_user_model_id: format!("{identity}_{publisher}!App"),
        version,
    })
}

fn app_from_explicit_path(path: &Path) -> anyhow::Result<CodexApp> {
    if path.is_file() {
        return Ok(CodexApp::Standalone {
            executable: path.to_path_buf(),
            version: "explicit".into(),
        });
    }
    let package_dir = if path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("app"))
    {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    if let Some(app) = packaged_from_directory(package_dir) {
        return Ok(app);
    }
    for name in ["ChatGPT.exe", "Codex.exe", "codex.exe"] {
        let executable = path.join(name);
        if executable.is_file() {
            return Ok(CodexApp::Standalone {
                executable,
                version: "explicit".into(),
            });
        }
    }
    bail!("指定路径中没有可用的 Codex")
}

fn parse_package_name(name: &str) -> Option<(String, String, String)> {
    let identity = if name.starts_with("OpenAI.CodexBeta_") {
        "OpenAI.CodexBeta"
    } else if name.starts_with("OpenAI.Codex_") {
        "OpenAI.Codex"
    } else {
        return None;
    };
    let rest = name.strip_prefix(&format!("{identity}_"))?;
    let (left, publisher) = rest.rsplit_once("__")?;
    let version = left.split('_').next()?.to_string();
    if publisher.is_empty() || version.split('.').count() < 2 {
        return None;
    }
    Some((identity.into(), version, publisher.into()))
}

fn version_key(version: &str) -> Vec<u32> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn standalone_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        for directory in [
            local.join("Programs/Codex"),
            local.join("OpenAI/Codex"),
            local.join("OpenAI/ChatGPT"),
        ] {
            candidates.push(directory.join("ChatGPT.exe"));
            candidates.push(directory.join("Codex.exe"));
        }
    }
    candidates
}

pub fn quote_arguments(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_windows_argument(argument: &str) -> String {
    if !argument
        .chars()
        .any(|character| character.is_whitespace() || character == '"')
    {
        return argument.to_string();
    }
    let mut output = String::from("\"");
    let mut backslashes = 0;
    for character in argument.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                output.push_str(&"\\".repeat(backslashes * 2 + 1));
                output.push('"');
                backslashes = 0;
            }
            _ => {
                output.push_str(&"\\".repeat(backslashes));
                output.push(character);
                backslashes = 0;
            }
        }
    }
    output.push_str(&"\\".repeat(backslashes * 2));
    output.push('"');
    output
}

#[cfg(windows)]
fn activate_packaged_blocking(app_user_model_id: &str, arguments: &str) -> anyhow::Result<u32> {
    use windows::Win32::System::Com::{
        CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        ACTIVATEOPTIONS, ApplicationActivationManager, IApplicationActivationManager,
    };
    use windows::core::HSTRING;
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let manager: IApplicationActivationManager =
            CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)?;
        let result = manager.ActivateApplication(
            &HSTRING::from(app_user_model_id),
            &HSTRING::from(arguments),
            ACTIVATEOPTIONS(0),
        );
        drop(manager);
        if initialized {
            CoUninitialize();
        }
        Ok(result?)
    }
}

#[cfg(not(windows))]
fn activate_packaged_blocking(_app_user_model_id: &str, _arguments: &str) -> anyhow::Result<u32> {
    bail!("MSIX 启动仅支持 Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_msix_package() {
        assert_eq!(
            parse_package_name("OpenAI.Codex_26.707.9981.0_x64__2p2nqsd0c76g0"),
            Some((
                "OpenAI.Codex".into(),
                "26.707.9981.0".into(),
                "2p2nqsd0c76g0".into()
            ))
        );
    }

    #[test]
    fn quotes_windows_arguments() {
        assert_eq!(quote_windows_argument("plain"), "plain");
        assert_eq!(quote_windows_argument("hello world"), "\"hello world\"");
    }

    #[test]
    fn version_sort_key_is_numeric() {
        assert!(version_key("26.707.10.0") > version_key("26.99.9999.0"));
    }
}
