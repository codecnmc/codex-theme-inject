use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, bail};
use base64::Engine;
use image::{DynamicImage, ImageFormat, imageops::FilterType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

use crate::storage::{ThemeStore, atomic_write};
use crate::theme::ThemeManifest;

const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 80 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 15 * 1024 * 1024;
const MAX_CSS_BYTES: u64 = 256 * 1024;
const MAX_FILES: usize = 100;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct TrustStore {
    #[serde(default)]
    custom_css: BTreeMap<String, String>,
}

pub struct ThemePackageManager {
    store: ThemeStore,
}

impl ThemePackageManager {
    pub fn new(store: ThemeStore) -> Self {
        Self { store }
    }

    pub async fn import_dialog(&self) -> anyhow::Result<ThemeManifest> {
        let dialog = owned_dialog("安装 Theme Inject 主题包").add_filter("Theme package", &["zip"]);
        let file = dialog.pick_file().await.context("未选择主题包")?;
        let path = file.path().to_path_buf();
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || import_zip(&store, &path)).await?
    }

    pub async fn import_trigger_icon(&self) -> anyhow::Result<()> {
        let dialog = owned_dialog("选择主题入口图标")
            .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp"]);
        let file = dialog.pick_file().await.context("未选择入口图标")?;
        if fs::metadata(file.path())?.len() > MAX_FILE_BYTES {
            bail!("入口图标超过 15 MB");
        }
        let image = image::open(file.path()).context("无法解码入口图标")?;
        let image = image.resize(256, 256, FilterType::Lanczos3).to_rgba8();
        let mut bytes = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, ImageFormat::Png)
            .context("无法编码入口图标")?;
        atomic_write(&self.store.paths.trigger_icon, &bytes.into_inner())
    }

    pub fn reset_trigger_icon(&self) -> anyhow::Result<()> {
        if self.store.paths.trigger_icon.exists() {
            fs::remove_file(&self.store.paths.trigger_icon)?;
        }
        Ok(())
    }

    pub fn update_metadata(
        &self,
        id: &str,
        name: &str,
        description: &str,
    ) -> anyhow::Result<ThemeManifest> {
        let name = name.trim();
        let description = description.trim();
        if name.is_empty() || name.chars().count() > 80 {
            bail!("主题名称不能为空且不能超过 80 个字符");
        }
        if description.chars().count() > 240 {
            bail!("主题介绍不能超过 240 个字符");
        }
        let mut theme = self.store.load(id)?;
        theme.name = name.into();
        theme.description = description.into();
        self.store.save(&theme)?;
        Ok(theme)
    }

    pub async fn export_dialog(&self, id: String) -> anyhow::Result<()> {
        let dialog = owned_dialog("导出 Theme Inject 主题包")
            .set_file_name(format!("{id}.zip"))
            .add_filter("Theme package", &["zip"]);
        let file = dialog.save_file().await.context("未选择导出位置")?;
        let path = file.path().to_path_buf();
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || export_zip(&store, &id, &path)).await?
    }

    pub fn clone_theme(&self, id: &str) -> anyhow::Result<ThemeManifest> {
        let source = self.store.load(id)?;
        let mut clone = source.clone();
        clone.id = format!("local.{}", uuid::Uuid::new_v4().simple());
        clone.name = format!("{} 副本", source.name);
        clone.version = "0.1.0".into();
        clone.author = "Local user".into();
        let source_dir = self.store.paths.themes.join(id);
        let target_dir = self.store.paths.themes.join(&clone.id);
        copy_directory(&source_dir, &target_dir)?;
        self.store.save(&clone)?;
        Ok(clone)
    }

    pub fn create_theme(&self, source_id: &str, name: &str) -> anyhow::Result<ThemeManifest> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            bail!("新主题名称不能为空且不能超过 80 个字符");
        }
        let mut theme = self.clone_theme(source_id)?;
        theme.name = name.to_string();
        theme.description = "本地自定义主题".to_string();
        self.store.save(&theme)?;
        Ok(theme)
    }

    pub fn delete_theme(&self, id: &str, active_id: &str) -> anyhow::Result<()> {
        if id.starts_with("builtin.") {
            bail!("内置主题不能删除");
        }
        if id == active_id {
            bail!("当前活动主题不能删除");
        }
        crate::theme::validate_theme_id(id)?;
        fs::remove_dir_all(self.store.paths.themes.join(id))?;
        Ok(())
    }

    pub async fn import_image_staging(&self, purpose: &str) -> anyhow::Result<(String, String)> {
        self.import_image_staging_many(purpose)
            .await?
            .into_iter()
            .next()
            .context("未选择背景图片")
    }

    pub async fn import_image_staging_many(
        &self,
        purpose: &str,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let dialog = owned_dialog("选择 Theme Inject 图片资源")
            .add_filter("Image", &["png", "jpg", "jpeg", "webp", "gif", "bmp"]);
        let files = dialog.pick_files().await.context("未选择背景图片")?;
        if files.len() > 6 {
            bail!("一次最多选择 6 张参考图");
        }
        files
            .into_iter()
            .map(|file| self.stage_image_file(purpose, file.path()))
            .collect()
    }

    pub fn stage_image_bytes(
        &self,
        purpose: &str,
        extension: &str,
        bytes: &[u8],
    ) -> anyhow::Result<(String, String)> {
        if bytes.is_empty() || bytes.len() as u64 > MAX_FILE_BYTES {
            bail!("图片为空或超过 15 MB");
        }
        let extension = normalize_image_extension(extension)?;
        let purpose = sanitize_asset_purpose(purpose)?;
        let hash = hex_hash(bytes);
        let relative = format!("assets/{purpose}-{hash}.{extension}");
        let session = uuid::Uuid::new_v4().simple().to_string();
        let target = self.store.paths.staging.join(&session).join(&relative);
        fs::create_dir_all(target.parent().unwrap())?;
        atomic_write(&target, bytes)?;
        Ok((session, relative))
    }

    fn stage_image_file(&self, purpose: &str, source: &Path) -> anyhow::Result<(String, String)> {
        if fs::metadata(&source)?.len() > MAX_FILE_BYTES {
            bail!("背景图片超过 15 MB");
        }
        let extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase();
        let extension = normalize_image_extension(&extension)?;
        let purpose = sanitize_asset_purpose(purpose)?;
        let relative = format!(
            "assets/{purpose}-{hash}.{extension}",
            hash = file_hash(&source)?
        );
        let session = uuid::Uuid::new_v4().simple().to_string();
        let target = self.store.paths.staging.join(&session).join(&relative);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(source, target)?;
        Ok((session, relative))
    }

    pub fn background_data_url(
        &self,
        theme_id: &str,
        relative: &str,
        staging_session: Option<&str>,
    ) -> anyhow::Result<String> {
        crate::theme::validate_relative_asset_path(relative)?;
        let path = if let Some(session) = staging_session {
            validate_staging_session(session)?;
            self.store.paths.staging.join(session).join(relative)
        } else {
            crate::theme::validate_theme_id(theme_id)?;
            self.store.paths.themes.join(theme_id).join(relative)
        };
        let root = if staging_session.is_some() {
            self.store.paths.staging.canonicalize()?
        } else {
            self.store.paths.themes.canonicalize()?
        };
        let path = path.canonicalize().context("背景图片不存在")?;
        if !path.starts_with(&root) {
            bail!("背景图片位于主题目录之外");
        }
        let metadata = fs::metadata(&path).context("背景图片不存在")?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            bail!("背景图片无效或超过 15 MB");
        }
        let mime = match path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            Some("bmp") => "image/bmp",
            _ => bail!("不支持的背景图片格式"),
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(fs::read(path)?);
        Ok(format!("data:{mime};base64,{encoded}"))
    }

    pub fn staging_thumbnail_data_url(
        &self,
        session: &str,
        relative: &str,
    ) -> anyhow::Result<String> {
        validate_staging_session(session)?;
        crate::theme::validate_relative_asset_path(relative)?;
        let root = self.store.paths.staging.canonicalize()?;
        let path = self
            .store
            .paths
            .staging
            .join(session)
            .join(relative)
            .canonicalize()
            .context("预览图片不存在")?;
        if !path.starts_with(&root)
            || !path.is_file()
            || fs::metadata(&path)?.len() > MAX_FILE_BYTES
        {
            bail!("预览图片无效或超过 15 MB");
        }
        thumbnail_data_url(&path, 256)
    }

    pub fn theme_thumbnail_data_url(
        &self,
        theme_id: &str,
        relative: &str,
    ) -> anyhow::Result<String> {
        crate::theme::validate_theme_id(theme_id)?;
        crate::theme::validate_relative_asset_path(relative)?;
        let theme = self.store.load(theme_id)?;
        if theme.skin.resources.hero_image.as_deref() != Some(relative) {
            bail!("缩略图资源不是当前主题的 Hero 图片");
        }
        let root = self.store.paths.themes.canonicalize()?;
        let path = self
            .store
            .paths
            .themes
            .join(theme_id)
            .join(relative)
            .canonicalize()
            .context("主题 Hero 图片不存在")?;
        if !path.starts_with(&root)
            || !path.is_file()
            || fs::metadata(&path)?.len() > MAX_FILE_BYTES
        {
            bail!("主题 Hero 图片无效或超过 15 MB");
        }
        thumbnail_data_url(&path, 128)
    }

    pub fn promote_image(
        &self,
        session: &str,
        relative: &str,
        theme_id: &str,
    ) -> anyhow::Result<()> {
        self.promote_image_inner(session, relative, theme_id, true)
    }

    pub fn promote_image_preserving_staging(
        &self,
        session: &str,
        relative: &str,
        theme_id: &str,
    ) -> anyhow::Result<()> {
        self.promote_image_inner(session, relative, theme_id, false)
    }

    fn promote_image_inner(
        &self,
        session: &str,
        relative: &str,
        theme_id: &str,
        remove_source: bool,
    ) -> anyhow::Result<()> {
        validate_staging_session(session)?;
        crate::theme::validate_relative_asset_path(relative)?;
        crate::theme::validate_theme_id(theme_id)?;
        let source = self.store.paths.staging.join(session).join(relative);
        let target = self.store.paths.themes.join(theme_id).join(relative);
        if !source.is_file() {
            bail!("预览背景文件不存在");
        }
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(source, target)?;
        if remove_source {
            self.cancel_staging(session)?;
        }
        Ok(())
    }

    pub fn cancel_staging(&self, session: &str) -> anyhow::Result<()> {
        validate_staging_session(session)?;
        let directory = self.store.paths.staging.join(session);
        if directory.exists() {
            fs::remove_dir_all(directory)?;
        }
        Ok(())
    }

    pub fn custom_css_hash(&self, theme: &ThemeManifest) -> anyhow::Result<Option<String>> {
        let Some(css) = self.store.custom_css(theme)? else {
            return Ok(None);
        };
        Ok(Some(hex_hash(css.as_bytes())))
    }

    pub fn custom_css_trusted(&self, theme: &ThemeManifest) -> anyhow::Result<bool> {
        let Some(hash) = self.custom_css_hash(theme)? else {
            return Ok(false);
        };
        let trust = self.load_trust()?;
        Ok(trust.custom_css.get(&theme.id) == Some(&hash))
    }

    pub fn trust_custom_css(&self, theme: &ThemeManifest) -> anyhow::Result<()> {
        let hash = self
            .custom_css_hash(theme)?
            .context("主题没有 custom.css")?;
        let mut trust = self.load_trust()?;
        trust.custom_css.insert(theme.id.clone(), hash);
        atomic_write(&self.store.paths.trust, &serde_json::to_vec_pretty(&trust)?)
    }

    pub fn revoke_custom_css(&self, theme_id: &str) -> anyhow::Result<()> {
        let mut trust = self.load_trust()?;
        trust.custom_css.remove(theme_id);
        atomic_write(&self.store.paths.trust, &serde_json::to_vec_pretty(&trust)?)
    }

    fn load_trust(&self) -> anyhow::Result<TrustStore> {
        if !self.store.paths.trust.exists() {
            return Ok(TrustStore::default());
        }
        Ok(serde_json::from_slice(&fs::read(&self.store.paths.trust)?)?)
    }
}

fn thumbnail_data_url(path: &Path, max_dimension: u32) -> anyhow::Result<String> {
    let image = image::open(path).context("无法解码预览图片")?;
    let thumbnail = if image.width() > max_dimension || image.height() > max_dimension {
        image.resize(max_dimension, max_dimension, FilterType::Triangle)
    } else {
        image
    };
    let mut bytes = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(thumbnail.to_rgba8())
        .write_to(&mut bytes, ImageFormat::Png)
        .context("无法编码预览缩略图")?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    ))
}

fn owned_dialog(title: &str) -> rfd::AsyncFileDialog {
    let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
    if let Some(parent) = crate::windows_app::foreground_dialog_parent().as_ref() {
        dialog = dialog.set_parent(parent);
    }
    dialog
}

fn sanitize_asset_purpose(value: &str) -> anyhow::Result<&str> {
    if value.is_empty()
        || value.len() > 32
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        bail!("图片资源用途无效");
    }
    Ok(value)
}

fn normalize_image_extension(value: &str) -> anyhow::Result<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "png" => Ok("png"),
        "jpg" | "jpeg" => Ok("jpg"),
        "webp" => Ok("webp"),
        "gif" => Ok("gif"),
        "bmp" => Ok("bmp"),
        _ => bail!("不支持的图片资源格式"),
    }
}

fn import_zip(store: &ThemeStore, path: &Path) -> anyhow::Result<ThemeManifest> {
    if fs::metadata(path)?.len() > MAX_ARCHIVE_BYTES {
        bail!("主题包超过 64 MB");
    }
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() > MAX_FILES {
        bail!("主题包文件数量超过 100");
    }
    let staging = store
        .paths
        .staging
        .join(uuid::Uuid::new_v4().simple().to_string());
    fs::create_dir_all(&staging)?;
    let result = (|| {
        let mut total = 0u64;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let Some(relative) = entry.enclosed_name() else {
                bail!("主题包包含非法路径");
            };
            if entry.is_symlink() {
                bail!("主题包不能包含符号链接");
            }
            total = total.saturating_add(entry.size());
            if entry.size() > MAX_FILE_BYTES || total > MAX_UNPACKED_BYTES {
                bail!("主题包解压大小超过 80 MB 或单文件超过 15 MB");
            }
            if relative == Path::new("custom.css") && entry.size() > MAX_CSS_BYTES {
                bail!("custom.css 不能超过 256 KB");
            }
            let output = staging.join(&relative);
            if entry.is_dir() {
                fs::create_dir_all(output)?;
                continue;
            }
            if !allowed_file(&relative) {
                bail!("主题包包含不支持的文件：{}", relative.display());
            }
            fs::create_dir_all(output.parent().unwrap())?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            fs::write(output, bytes)?;
        }
        let theme = ThemeManifest::from_slice_migrated(
            &fs::read(staging.join("theme.json")).context("主题包缺少 theme.json")?,
        )?;
        validate_packaged_files(&staging, &theme)?;
        if theme.id.starts_with("builtin.") {
            bail!("导入主题不能使用 builtin. 命名空间");
        }
        let target = store.paths.themes.join(&theme.id);
        if target.exists() {
            bail!("主题 {} 已经安装", theme.id);
        }
        fs::rename(&staging, &target)?;
        store.save(&theme)?;
        Ok(theme)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn export_zip(store: &ThemeStore, id: &str, path: &Path) -> anyhow::Result<()> {
    let theme = store.load(id)?;
    store.save(&theme)?;
    let root = store.paths.themes.join(&theme.id);
    validate_packaged_files(&root, &theme)?;
    let temp = path.with_extension("zip.tmp");
    let result = (|| {
        let file = File::create(&temp)?;
        let mut zip = zip::ZipWriter::new(file);
        let mut files = std::collections::BTreeSet::from(["theme.json".to_string()]);
        files.extend(theme.asset_paths().into_iter().map(str::to_string));
        if let Some(custom_css) = &theme.custom_css {
            files.insert(custom_css.clone());
        }
        if root.join("preview.png").is_file() {
            files.insert("preview.png".into());
        }
        let mut total = 0u64;
        for relative in files {
            let file = root.join(&relative);
            let metadata = fs::metadata(&file)?;
            total = total.saturating_add(metadata.len());
            if metadata.len() > MAX_FILE_BYTES || total > MAX_UNPACKED_BYTES {
                bail!("主题资源总大小超过导出限制");
            }
            add_file_to_zip(&mut zip, &file, &relative)?;
        }
        zip.finish()?;
        if fs::metadata(&temp)?.len() > MAX_ARCHIVE_BYTES {
            bail!("压缩后的主题包超过 64 MB");
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temp, path)?;
    Ok(())
}

fn add_file_to_zip(
    zip: &mut zip::ZipWriter<File>,
    file: &Path,
    relative: &str,
) -> anyhow::Result<()> {
    if !allowed_file(Path::new(relative)) {
        bail!("主题目录包含不能导出的文件：{relative}");
    }
    zip.start_file(
        relative.replace('\\', "/"),
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
    )?;
    zip.write_all(&fs::read(file)?)?;
    Ok(())
}

fn validate_packaged_files(root: &Path, theme: &ThemeManifest) -> anyhow::Result<()> {
    let root = root.canonicalize()?;
    for relative in theme
        .asset_paths()
        .into_iter()
        .chain(theme.custom_css.as_deref())
    {
        crate::theme::validate_relative_asset_path(relative)?;
        let path = root
            .join(relative)
            .canonicalize()
            .with_context(|| format!("主题包缺少引用资源：{relative}"))?;
        if !path.starts_with(&root) || !path.is_file() || !allowed_file(Path::new(relative)) {
            bail!("主题包引用资源无效：{relative}");
        }
    }
    Ok(())
}

fn allowed_file(path: &Path) -> bool {
    let relative = path.to_string_lossy().replace('\\', "/");
    relative == "theme.json"
        || relative == "custom.css"
        || relative == "preview.png"
        || (relative.starts_with("assets/")
            && matches!(
                path.extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp")
            ))
}

fn copy_directory(source: &Path, target: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

fn file_hash(path: &Path) -> anyhow::Result<String> {
    Ok(hex_hash(&fs::read(path)?)[..16].to_string())
}
fn hex_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_staging_session(session: &str) -> anyhow::Result<()> {
    if session.len() != 32
        || !session
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        bail!("预览会话标识无效");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme_store() -> (tempfile::TempDir, ThemeStore) {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::storage::AppPaths::from_root(temp.path().join("data"));
        paths.ensure().unwrap();
        (temp, ThemeStore::new(paths))
    }

    fn write_theme_zip(path: &Path, theme: &ThemeManifest, extra_name: Option<&str>) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("theme.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(theme).unwrap()).unwrap();
        if let Some(name) = extra_name {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"payload").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn filters_package_files() {
        assert!(allowed_file(Path::new("theme.json")));
        assert!(allowed_file(Path::new("assets/bg.png")));
        assert!(!allowed_file(Path::new("assets/run.exe")));
    }

    #[test]
    fn staging_thumbnail_is_small_and_preserves_transparency() {
        let (_temp, store) = theme_store();
        let manager = ThemePackageManager::new(store.clone());
        let mut source = image::RgbaImage::new(1200, 600);
        for pixel in source.pixels_mut().take(300 * 300) {
            *pixel = image::Rgba([240, 80, 120, 255]);
        }
        let mut encoded = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(source)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        let (session, path) = manager
            .stage_image_bytes("thumbnail-test", "png", encoded.get_ref())
            .unwrap();
        let data_url = manager.staging_thumbnail_data_url(&session, &path).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_url.split_once(',').unwrap().1)
            .unwrap();
        let thumbnail = image::load_from_memory(&bytes).unwrap();
        assert!(thumbnail.width() <= 256 && thumbnail.height() <= 256);
        assert!(thumbnail.color().has_alpha());
    }

    #[test]
    fn theme_thumbnail_tracks_current_hero_path() {
        let (_temp, store) = theme_store();
        let manager = ThemePackageManager::new(store.clone());
        let mut theme = ThemeManifest::neutral_dark();
        theme.id = "local.thumbnail".into();
        theme.skin.enabled = true;
        theme.skin.resources.hero_image = Some("assets/hero.png".into());
        let root = store.paths.themes.join(&theme.id);
        fs::create_dir_all(root.join("assets")).unwrap();
        DynamicImage::ImageRgba8(image::RgbaImage::new(640, 360))
            .save(root.join("assets/hero.png"))
            .unwrap();
        store.save(&theme).unwrap();

        let data_url = manager
            .theme_thumbnail_data_url(&theme.id, "assets/hero.png")
            .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_url.split_once(',').unwrap().1)
            .unwrap();
        let thumbnail = image::load_from_memory(&bytes).unwrap();
        assert!(thumbnail.width() <= 128 && thumbnail.height() <= 128);
        assert!(
            manager
                .theme_thumbnail_data_url(&theme.id, "assets/old-hero.png")
                .is_err()
        );
    }

    #[test]
    fn imports_safe_theme_package() {
        let (temp, store) = theme_store();
        let mut theme = ThemeManifest::neutral_dark();
        theme.id = "local.imported".into();
        let archive = temp.path().join("theme.zip");
        write_theme_zip(&archive, &theme, Some("assets/background.png"));
        let imported = import_zip(&store, &archive).unwrap();
        assert_eq!(imported.id, "local.imported");
        assert!(
            store
                .paths
                .themes
                .join("local.imported/theme.json")
                .is_file()
        );
    }

    #[test]
    fn skin_assets_round_trip_with_theme_package() {
        let (temp, store) = theme_store();
        let mut theme = ThemeManifest::neutral_dark();
        theme.id = "local.skin-roundtrip".into();
        theme.skin.enabled = true;
        theme.skin.resources.logo = Some("assets/logo.png".into());
        theme.skin.resources.hero_image = Some("assets/hero.png".into());
        theme
            .skin
            .icons
            .mappings
            .insert("search".into(), "assets/search.png".into());
        theme.skin.decorations.push(crate::theme::Decoration {
            asset: "assets/star.png".into(),
            region: crate::theme::DecorationRegion::Content,
            anchor: crate::theme::DecorationAnchor::TopRight,
            offset_x: 12,
            offset_y: 16,
            width: 96,
            opacity: 0.8,
            layer: 1,
            hidden_below: 0,
        });
        theme.skin.home.cards.push(crate::theme::HomeCard {
            title: "解释代码".into(),
            description: "理解当前实现".into(),
            prompt: "解释选中的代码".into(),
            icon: Some("assets/card.png".into()),
        });
        store.save(&theme).unwrap();
        let root = store.paths.themes.join(&theme.id);
        fs::create_dir_all(root.join("assets")).unwrap();
        for name in ["logo.png", "hero.png", "search.png", "star.png", "card.png"] {
            fs::write(root.join("assets").join(name), [0x89, b'P', b'N', b'G']).unwrap();
        }
        let archive = temp.path().join("skin.zip");
        export_zip(&store, &theme.id, &archive).unwrap();
        fs::remove_dir_all(&root).unwrap();
        let imported = import_zip(&store, &archive).unwrap();
        assert!(imported.skin.enabled);
        assert_eq!(
            imported.skin.resources.hero_image.as_deref(),
            Some("assets/hero.png")
        );
        for relative in imported.asset_paths() {
            assert!(root.join(relative).is_file(), "missing {relative}");
        }
    }

    #[test]
    fn rejects_theme_package_with_missing_skin_asset() {
        let (temp, store) = theme_store();
        let mut theme = ThemeManifest::neutral_light();
        theme.id = "local.missing-skin".into();
        theme.skin.enabled = true;
        theme.skin.resources.hero_image = Some("assets/missing.png".into());
        let archive = temp.path().join("missing.zip");
        write_theme_zip(&archive, &theme, None);
        assert!(import_zip(&store, &archive).is_err());
    }

    #[test]
    fn export_omits_unreferenced_theme_assets() {
        let (temp, store) = theme_store();
        let mut theme = ThemeManifest::neutral_light();
        theme.id = "local.export-clean".into();
        theme.skin.enabled = true;
        theme.skin.resources.logo = Some("assets/logo.png".into());
        store.save(&theme).unwrap();
        let assets = store.paths.themes.join(&theme.id).join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();
        fs::write(assets.join("obsolete.png"), [0x89, b'P', b'N', b'G']).unwrap();
        let archive = temp.path().join("clean.zip");
        export_zip(&store, &theme.id, &archive).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(archive).unwrap()).unwrap();
        let names = (0..zip.len())
            .map(|index| zip.by_index(index).unwrap().name().to_string())
            .collect::<Vec<_>>();
        assert!(names.contains(&"assets/logo.png".to_string()));
        assert!(!names.contains(&"assets/obsolete.png".to_string()));
    }

    #[test]
    fn imported_package_cannot_replace_builtin() {
        let (temp, store) = theme_store();
        let archive = temp.path().join("theme.zip");
        write_theme_zip(&archive, &ThemeManifest::neutral_dark(), None);
        assert!(import_zip(&store, &archive).is_err());
    }

    #[test]
    fn css_trust_is_bound_to_contents() {
        let (_temp, store) = theme_store();
        let manager = ThemePackageManager::new(store.clone());
        let mut theme = ThemeManifest::neutral_dark();
        theme.id = "local.css".into();
        store
            .save_custom_css(&mut theme, "body { color: red; }")
            .unwrap();
        manager.trust_custom_css(&theme).unwrap();
        assert!(manager.custom_css_trusted(&theme).unwrap());
        store
            .save_custom_css(&mut theme, "body { color: blue; }")
            .unwrap();
        assert!(!manager.custom_css_trusted(&theme).unwrap());
    }

    #[test]
    fn staging_session_validation_rejects_paths() {
        assert!(validate_staging_session("../escape").is_err());
        assert!(validate_staging_session("0123456789abcdef0123456789abcdef").is_ok());
    }

    #[test]
    fn background_data_url_is_scoped_to_theme_assets() {
        let (_temp, store) = theme_store();
        let manager = ThemePackageManager::new(store.clone());
        let mut theme = ThemeManifest::neutral_dark();
        theme.id = "local.background".into();
        store.save(&theme).unwrap();
        let asset = store
            .paths
            .themes
            .join(&theme.id)
            .join("assets/background.png");
        fs::create_dir_all(asset.parent().unwrap()).unwrap();
        fs::write(&asset, [0x89, b'P', b'N', b'G']).unwrap();

        let data = manager
            .background_data_url(&theme.id, "assets/background.png", None)
            .unwrap();
        assert!(data.starts_with("data:image/png;base64,"));
        assert!(
            manager
                .background_data_url(&theme.id, "../outside.png", None)
                .is_err()
        );
    }
}
