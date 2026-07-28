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

#[cfg(windows)]
pub fn apply_foreground_window_chrome(background: &str, foreground: &str) -> anyhow::Result<()> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    let caption = colorref(background)?;
    let text = colorref(&accessible_caption_foreground(background, foreground)?)?;
    let dark = i32::from(relative_luminance(background)? < 0.5);
    let process_ids = codex_process_ids();
    if process_ids.is_empty() {
        bail!("未找到 Codex 进程");
    }
    struct WindowChrome<'a> {
        process_ids: &'a [u32],
        caption: u32,
        text: u32,
        dark: i32,
        applied: usize,
        unsupported_color: usize,
    }
    unsafe extern "system" fn apply(window: HWND, parameter: LPARAM) -> BOOL {
        let state = unsafe { &mut *(parameter.0 as *mut WindowChrome<'_>) };
        let mut process_id = 0;
        unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) };
        if state.process_ids.contains(&process_id) && unsafe { IsWindowVisible(window).as_bool() } {
            let dark_result = unsafe {
                DwmSetWindowAttribute(
                    window,
                    DWMWINDOWATTRIBUTE(20),
                    &state.dark as *const i32 as _,
                    4,
                )
            };
            let caption_result = unsafe {
                DwmSetWindowAttribute(
                    window,
                    DWMWINDOWATTRIBUTE(35),
                    &state.caption as *const u32 as _,
                    4,
                )
            };
            let text_result = unsafe {
                DwmSetWindowAttribute(
                    window,
                    DWMWINDOWATTRIBUTE(36),
                    &state.text as *const u32 as _,
                    4,
                )
            };
            if dark_result.is_ok() || caption_result.is_ok() || text_result.is_ok() {
                state.applied += 1;
            }
            if caption_result.is_err() || text_result.is_err() {
                state.unsupported_color += 1;
            }
        }
        BOOL(1)
    }
    let mut state = WindowChrome {
        process_ids: &process_ids,
        caption,
        text,
        dark,
        applied: 0,
        unsupported_color: 0,
    };
    unsafe {
        EnumWindows(
            Some(apply),
            LPARAM(&mut state as *mut WindowChrome<'_> as isize),
        )?;
    }
    if state.applied == 0 {
        bail!("Windows 未接受 Codex 标题栏深浅模式设置");
    }
    Ok(())
}

#[cfg(windows)]
fn codex_process_ids() -> Vec<u32> {
    use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return Vec::new();
        };
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ids = Vec::new();
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
                    ids.push(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        ids
    }
}

#[cfg(not(windows))]
pub fn apply_foreground_window_chrome(_background: &str, _foreground: &str) -> anyhow::Result<()> {
    Ok(())
}

fn colorref(value: &str) -> anyhow::Result<u32> {
    let value = value.strip_prefix('#').context("窗口颜色格式无效")?;
    if value.len() != 6 {
        bail!("窗口颜色格式无效");
    }
    let red = u32::from_str_radix(&value[0..2], 16)?;
    let green = u32::from_str_radix(&value[2..4], 16)?;
    let blue = u32::from_str_radix(&value[4..6], 16)?;
    Ok(red | (green << 8) | (blue << 16))
}

fn relative_luminance(value: &str) -> anyhow::Result<f32> {
    let color = colorref(value)?;
    let channel = |value: u32| {
        let value = value as f32 / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    Ok(channel(color & 0xff) * 0.2126
        + channel((color >> 8) & 0xff) * 0.7152
        + channel((color >> 16) & 0xff) * 0.0722)
}

fn accessible_caption_foreground(background: &str, requested: &str) -> anyhow::Result<String> {
    let background_luminance = relative_luminance(background)?;
    let requested_luminance = relative_luminance(requested)?;
    if contrast_ratio(background_luminance, requested_luminance) >= 4.5 {
        return Ok(requested.to_ascii_uppercase());
    }
    let black = contrast_ratio(background_luminance, 0.0);
    let white = contrast_ratio(background_luminance, 1.0);
    Ok(if white >= black { "#FFFFFF" } else { "#000000" }.into())
}

fn contrast_ratio(left: f32, right: f32) -> f32 {
    let lighter = left.max(right);
    let darker = left.min(right);
    (lighter + 0.05) / (darker + 0.05)
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
        let script = r#"$processes = Get-CimInstance Win32_Process | Where-Object { ($_.Name -eq 'ChatGPT.exe' -or $_.Name -eq 'Codex.exe') -and $_.CommandLine -match '--remote-debugging-port=(\d+)' }; $processes | ForEach-Object { if ($_.CommandLine -match '--remote-debugging-port=(\d+)') { $matches[1] } } | Sort-Object -Unique"#;
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
        let candidates = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.trim().parse::<u16>().ok())
            .collect::<Vec<_>>();
        for port in candidates {
            if crate::cdp::list_targets(port).await.is_ok() {
                return Some(port);
            }
        }
        None
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

    #[test]
    fn caption_foreground_keeps_window_buttons_readable() {
        assert_eq!(
            accessible_caption_foreground("#000000", "#26282D").unwrap(),
            "#FFFFFF"
        );
        assert_eq!(
            accessible_caption_foreground("#FFFFFF", "#F7FAFF").unwrap(),
            "#000000"
        );
        assert_eq!(
            accessible_caption_foreground("#000000", "#F7FAFF").unwrap(),
            "#F7FAFF"
        );
    }
}
