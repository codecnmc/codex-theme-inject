use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::storage::AppPaths;

pub struct AssetServer {
    pub port: u16,
    pub token: String,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl AssetServer {
    pub async fn start(paths: AppPaths) -> anyhow::Result<Self> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let token = uuid::Uuid::new_v4().to_string();
        let expected_token = token.clone();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        if let Ok((stream, address)) = accepted {
                            if !address.ip().is_loopback() { continue; }
                            let paths = paths.clone();
                            let token = expected_token.clone();
                            tokio::spawn(async move { let _ = handle(stream, &paths, &token).await; });
                        }
                    }
                }
            }
        });
        Ok(Self {
            port,
            token,
            shutdown: Some(shutdown_tx),
            task,
        })
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = self.task.await;
    }
}

async fn handle(
    mut stream: tokio::net::TcpStream,
    paths: &AppPaths,
    token: &str,
) -> anyhow::Result<()> {
    let mut buffer = vec![0; 8192];
    let size = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..size]);
    let request_line = request.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let raw_uri = parts.next().unwrap_or_default();
    if method == "OPTIONS" {
        return response(&mut stream, "204 No Content", "text/plain", b"", "no-store").await;
    }
    if method != "GET" {
        return response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain",
            b"method not allowed",
            "no-store",
        )
        .await;
    }
    let (path, query) = raw_uri.split_once('?').unwrap_or((raw_uri, ""));
    let supplied_token = query
        .split('&')
        .find_map(|pair| pair.split_once('='))
        .filter(|(key, _)| *key == "token")
        .map(|(_, value)| value);
    if supplied_token != Some(token) {
        return response(
            &mut stream,
            "403 Forbidden",
            "text/plain",
            b"forbidden",
            "no-store",
        )
        .await;
    }
    let relative = path.trim_start_matches('/');
    let (root, relative) = if let Some(relative) = relative.strip_prefix("assets/") {
        (&paths.themes, relative)
    } else if let Some(relative) = relative.strip_prefix("pets/") {
        (&paths.pets, relative)
    } else if let Some(relative) = relative.strip_prefix("staging/") {
        (&paths.staging, relative)
    } else {
        return response(
            &mut stream,
            "404 Not Found",
            "text/plain",
            b"not found",
            "no-store",
        )
        .await;
    };
    let file = safe_file(root, relative)?;
    let Some(content_type) = image_content_type(&file) else {
        return response(
            &mut stream,
            "415 Unsupported Media Type",
            "text/plain",
            b"unsupported",
            "no-store",
        )
        .await;
    };
    let bytes = std::fs::read(&file).with_context(|| format!("无法读取 {}", file.display()))?;
    response(
        &mut stream,
        "200 OK",
        content_type,
        &bytes,
        if path.starts_with("/staging/") {
            "no-store"
        } else {
            "public, max-age=31536000, immutable"
        },
    )
    .await
}

fn safe_file(root: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    crate::theme::validate_relative_asset_path(relative)?;
    let root = root.canonicalize()?;
    let candidate = root.join(relative).canonicalize()?;
    if !candidate.starts_with(&root) || !candidate.is_file() {
        bail!("资源路径越界");
    }
    Ok(candidate)
}

fn image_content_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

async fn response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    cache: &str,
) -> anyhow::Result<()> {
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nCache-Control: {cache}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nAccess-Control-Allow-Private-Network: true\r\nCross-Origin-Resource-Policy: cross-origin\r\nX-Content-Type-Options: nosniff\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_parent_paths() {
        let temp = tempfile::tempdir().unwrap();
        assert!(safe_file(temp.path(), "../secret.png").is_err());
    }

    #[test]
    fn recognizes_webp_pet_assets() {
        assert_eq!(
            image_content_type(Path::new("spritesheet.webp")),
            Some("image/webp")
        );
    }
}
