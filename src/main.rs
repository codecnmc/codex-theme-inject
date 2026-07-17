#![cfg_attr(windows, windows_subsystem = "windows")]

mod ai;
mod asset_server;
mod bridge;
mod cdp;
mod diagnostic;
mod launcher;
mod storage;
mod theme;
mod theme_package;
mod watchdog;
mod windows_app;
mod windows_dialog;

use std::path::PathBuf;

use anyhow::Context;
use launcher::{LaunchOptions, ThemeLauncher};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if windows_app::run_internal_command(&args)? {
        return Ok(());
    }
    let options = parse_args(args.into_iter())?;
    if let Err(error) = ThemeLauncher::run(options).await {
        diagnostic::log(
            "launcher.fatal",
            serde_json::json!({ "message": format!("{error:#}") }),
        );
        let message = format!("Theme Inject 启动失败：\n\n{error:#}");
        windows_dialog::show_error("Theme Inject", &message);
        return Err(error);
    }
    Ok(())
}

fn parse_args(args: impl Iterator<Item = String>) -> anyhow::Result<LaunchOptions> {
    let mut options = LaunchOptions::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--app-path" => {
                options.app_path = Some(PathBuf::from(
                    args.next().context("--app-path 需要一个路径")?,
                ));
            }
            "--debug-port" => {
                options.debug_port = Some(
                    args.next()
                        .context("--debug-port 需要端口")?
                        .parse()
                        .context("--debug-port 必须是有效端口")?,
                );
            }
            "--open-panel" => options.open_panel = true,
            "--help" | "-h" => {
                windows_dialog::show_info(
                    "Theme Inject",
                    "用法：theme-inject [--app-path PATH] [--debug-port PORT] [--open-panel]",
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("未知参数：{other}"),
        }
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_launch_options() {
        let options = parse_args(
            [
                "--app-path",
                "C:\\Codex",
                "--debug-port",
                "9333",
                "--open-panel",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert_eq!(options.app_path, Some(PathBuf::from("C:\\Codex")));
        assert_eq!(options.debug_port, Some(9333));
        assert!(options.open_panel);
    }
}
