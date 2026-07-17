use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::theme::ThemeManifest;

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub root: PathBuf,
    pub settings: PathBuf,
    pub ai_settings: PathBuf,
    pub ai_generation: PathBuf,
    pub themes: PathBuf,
    pub staging: PathBuf,
    pub logs: PathBuf,
    pub trust: PathBuf,
}

impl AppPaths {
    pub fn discover() -> anyhow::Result<Self> {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| BaseDirs::new().map(|dirs| dirs.data_local_dir().to_path_buf()))
            .context("无法确定 LOCALAPPDATA")?;
        Ok(Self::from_root(local.join("ThemeInject")))
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self {
            settings: root.join("settings.json"),
            ai_settings: root.join("ai-settings.json"),
            ai_generation: root.join("ai-generation.json"),
            themes: root.join("themes"),
            staging: root.join("staging"),
            logs: root.join("logs"),
            trust: root.join("trust.json"),
            root,
        }
    }

    pub fn ensure(&self) -> anyhow::Result<()> {
        for path in [&self.root, &self.themes, &self.staging, &self.logs] {
            fs::create_dir_all(path).with_context(|| format!("无法创建目录 {}", path.display()))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub active_theme_id: String,
    #[serde(default)]
    pub last_codex_version: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            active_theme_id: "builtin.neutral-dark".to_string(),
            last_codex_version: String::new(),
        }
    }
}

#[derive(Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> anyhow::Result<AppSettings> {
        if !self.path.exists() {
            return Ok(AppSettings::default());
        }
        let bytes = fs::read(&self.path)?;
        serde_json::from_slice(&bytes).context("settings.json 格式无效")
    }

    pub fn save(&self, settings: &AppSettings) -> anyhow::Result<()> {
        atomic_write(&self.path, &serde_json::to_vec_pretty(settings)?)
    }
}

#[derive(Clone)]
pub struct ThemeStore {
    pub paths: AppPaths,
}

impl ThemeStore {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn ensure_builtins(&self) -> anyhow::Result<()> {
        self.paths.ensure()?;
        for theme in [
            ThemeManifest::neutral_dark(),
            ThemeManifest::neutral_light(),
            ThemeManifest::midnight_glass(),
        ] {
            let directory = self.paths.themes.join(&theme.id);
            fs::create_dir_all(&directory)?;
            atomic_write(
                &directory.join("theme.json"),
                &serde_json::to_vec_pretty(&theme)?,
            )?;
        }
        Ok(())
    }

    pub fn list(&self) -> anyhow::Result<Vec<ThemeManifest>> {
        let mut themes = Vec::new();
        for entry in fs::read_dir(&self.paths.themes)? {
            let path = entry?.path().join("theme.json");
            if !path.is_file() {
                continue;
            }
            if let Ok(theme) = ThemeManifest::from_slice_migrated(&fs::read(path)?)
                && theme.validate().is_ok()
            {
                themes.push(theme);
            }
        }
        themes.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(themes)
    }

    pub fn load(&self, id: &str) -> anyhow::Result<ThemeManifest> {
        crate::theme::validate_theme_id(id)?;
        let path = self.paths.themes.join(id).join("theme.json");
        let theme = ThemeManifest::from_slice_migrated(
            &fs::read(&path).with_context(|| format!("找不到主题 {id}"))?,
        )?;
        Ok(theme)
    }

    pub fn save(&self, theme: &ThemeManifest) -> anyhow::Result<()> {
        theme.validate()?;
        let directory = self.paths.themes.join(&theme.id);
        fs::create_dir_all(&directory)?;
        atomic_write(
            &directory.join("theme.json"),
            &serde_json::to_vec_pretty(theme)?,
        )
    }

    pub fn custom_css(&self, theme: &ThemeManifest) -> anyhow::Result<Option<String>> {
        let Some(relative) = theme.custom_css.as_deref() else {
            return Ok(None);
        };
        crate::theme::validate_relative_asset_path(relative)?;
        let root = self.paths.themes.join(&theme.id).canonicalize()?;
        let path = root.join(relative).canonicalize()?;
        if !path.starts_with(&root) {
            bail!("custom.css 位于主题目录之外");
        }
        Ok(Some(fs::read_to_string(path)?))
    }

    pub fn save_custom_css(&self, theme: &mut ThemeManifest, css: &str) -> anyhow::Result<()> {
        if css.trim().is_empty() {
            theme.custom_css = None;
            let path = self.paths.themes.join(&theme.id).join("custom.css");
            if path.exists() {
                fs::remove_file(path)?;
            }
            return self.save(theme);
        }
        let directory = self.paths.themes.join(&theme.id);
        fs::create_dir_all(&directory)?;
        atomic_write(&directory.join("custom.css"), css.as_bytes())?;
        theme.custom_css = Some("custom.css".into());
        self.save(theme)
    }

    pub fn cleanup_staging(&self) -> anyhow::Result<()> {
        if !self.paths.staging.exists() {
            return Ok(());
        }
        let preserved = fs::read(&self.paths.ai_generation)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| {
                value
                    .get("assets")
                    .and_then(|assets| assets.as_array())
                    .cloned()
            })
            .unwrap_or_default()
            .into_iter()
            .filter_map(|asset| {
                asset
                    .get("session")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            })
            .collect::<std::collections::BTreeSet<_>>();
        for entry in fs::read_dir(&self.paths.staging)? {
            let path = entry?.path();
            if path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|session| preserved.contains(session))
            {
                continue;
            }
            if path.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("file")
    ));
    fs::write(&temp, bytes)?;
    replace_file(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    use windows::core::PCWSTR;

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> anyhow::Result<()> {
    if target.exists() {
        fs::remove_file(target)?;
    }
    fs::rename(source, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_builtins_and_settings() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        let themes = ThemeStore::new(paths.clone());
        themes.ensure_builtins().unwrap();
        assert_eq!(themes.list().unwrap().len(), 3);

        let settings = SettingsStore::new(paths.settings);
        settings.save(&AppSettings::default()).unwrap();
        assert_eq!(
            settings.load().unwrap().active_theme_id,
            "builtin.neutral-dark"
        );
    }

    #[test]
    fn loads_and_saves_legacy_theme_as_schema_v2() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let store = ThemeStore::new(paths.clone());
        let mut legacy = serde_json::to_value(ThemeManifest::neutral_light()).unwrap();
        legacy["schemaVersion"] = serde_json::json!(1);
        legacy["id"] = serde_json::json!("local.legacy");
        legacy["background"] = serde_json::json!({
            "kind": "image",
            "path": "assets/background.png",
            "fit": "cover",
            "opacity": 0.75,
            "region": "content"
        });
        let directory = paths.themes.join("local.legacy");
        fs::create_dir_all(directory.join("assets")).unwrap();
        fs::write(
            directory.join("theme.json"),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();
        let theme = store.load("local.legacy").unwrap();
        assert_eq!(theme.schema_version, 2);
        assert_eq!(
            theme.background.content.image_path(),
            Some("assets/background.png")
        );
        store.save(&theme).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("theme.json")).unwrap()).unwrap();
        assert_eq!(saved["schemaVersion"], 2);
        assert_eq!(saved["background"]["mode"], "per-region");
    }
}
