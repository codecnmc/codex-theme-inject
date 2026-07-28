use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, bail};
use base64::Engine;
use rfd::AsyncFileDialog;
use zip::write::SimpleFileOptions;

use crate::pet::{PetManifest, PetSummary, validate_pet_directory, validate_pet_id};
use crate::storage::AppPaths;

const MAX_ARCHIVE_BYTES: u64 = 40 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 40 * 1024 * 1024;
const MAX_FILES: usize = 32;

#[derive(Clone)]
pub struct PetPackageManager {
    paths: AppPaths,
    _asset_base: String,
    _asset_token: String,
}

impl PetPackageManager {
    pub fn new(paths: AppPaths, asset_base: String, asset_token: String) -> Self {
        Self {
            paths,
            _asset_base: asset_base,
            _asset_token: asset_token,
        }
    }

    pub fn list(&self) -> anyhow::Result<Vec<PetSummary>> {
        self.paths.ensure()?;
        let codex_pets = codex_pets_root()?;
        let mut pets = Vec::new();
        for entry in fs::read_dir(&self.paths.pets)? {
            let root = entry?.path();
            if !root.is_dir() {
                continue;
            }
            let Ok(manifest) = validate_pet_directory(&root) else {
                continue;
            };
            let installed = codex_pets.join(&manifest.id).is_dir();
            pets.push(PetSummary {
                asset_url: String::new(),
                manifest,
                installed,
            });
        }
        pets.sort_by(|left, right| left.manifest.display_name.cmp(&right.manifest.display_name));
        Ok(pets)
    }

    pub fn data_url(&self, id: &str) -> anyhow::Result<String> {
        validate_pet_id(id)?;
        let root = self.paths.pets.join(id);
        validate_pet_directory(&root)?;
        let bytes = fs::read(root.join("spritesheet.webp"))?;
        Ok(format!(
            "data:image/webp;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    }

    pub async fn import_dialog(&self) -> anyhow::Result<PetManifest> {
        let file = owned_dialog("选择 Codex V2 宠物包")
            .add_filter("Codex Pet", &["tipet", "zip"])
            .pick_file()
            .await
            .context("未选择宠物包")?;
        import_zip(&self.paths, file.path())
    }

    pub async fn export_dialog(&self, id: &str) -> anyhow::Result<()> {
        validate_pet_id(id)?;
        let file = owned_dialog("导出 Codex 宠物包")
            .add_filter("Codex Pet ZIP", &["zip", "tipet"])
            .set_file_name(format!("{id}.zip"))
            .save_file()
            .await
            .context("未选择导出位置")?;
        export_zip(&self.paths, id, file.path())
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        validate_pet_id(id)?;
        let target = self.paths.pets.join(id);
        if !target.is_dir() {
            bail!("找不到宠物 {id}");
        }
        fs::remove_dir_all(target)?;
        Ok(())
    }

    pub fn update_metadata(
        &self,
        id: &str,
        display_name: &str,
        description: &str,
    ) -> anyhow::Result<PetManifest> {
        validate_pet_id(id)?;
        let root = self.paths.pets.join(id);
        let mut manifest = validate_pet_directory(&root)?;
        manifest.display_name = display_name.trim().to_string();
        manifest.description = description.trim().to_string();
        manifest.validate_metadata()?;

        let manifest_path = root.join("pet.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path)?).context("pet.json 格式无效")?;
        let object = value.as_object_mut().context("pet.json 必须是对象")?;
        object.insert(
            "displayName".into(),
            serde_json::Value::String(manifest.display_name.clone()),
        );
        object.insert(
            "description".into(),
            serde_json::Value::String(manifest.description.clone()),
        );
        crate::storage::atomic_write(&manifest_path, &serde_json::to_vec_pretty(&value)?)?;
        validate_pet_directory(&root)
    }

    pub fn install(&self, id: &str) -> anyhow::Result<PathBuf> {
        self.install_to_root(id, &codex_pets_root()?)
    }

    fn install_to_root(&self, id: &str, root: &Path) -> anyhow::Result<PathBuf> {
        validate_pet_id(id)?;
        let source = self.paths.pets.join(id);
        validate_pet_directory(&source)?;
        fs::create_dir_all(root)?;
        let target = root.join(id);
        let staging = root.join(format!(".{id}.install-{}", uuid::Uuid::new_v4().simple()));
        copy_pet_files(&source, &staging)?;
        validate_pet_directory(&staging)?;
        let rollback = if target.exists() {
            let rollback = root.join(format!(".{id}.rollback-{}", uuid::Uuid::new_v4().simple()));
            fs::rename(&target, &rollback)?;
            let backup_result = (|| -> anyhow::Result<()> {
                fs::create_dir_all(&self.paths.pet_backups)?;
                let backup = self
                    .paths
                    .pet_backups
                    .join(format!("{id}-{}", unix_timestamp_ms()));
                copy_pet_files(&rollback, &backup)?;
                validate_pet_directory(&backup)?;
                Ok(())
            })();
            if let Err(error) = backup_result {
                let _ = fs::remove_dir_all(&staging);
                fs::rename(&rollback, &target)?;
                return Err(error).context("备份现有宠物失败，未执行替换");
            }
            Some(rollback)
        } else {
            None
        };
        if let Err(error) = fs::rename(&staging, &target) {
            let _ = fs::remove_dir_all(&staging);
            if let Some(rollback) = rollback {
                let _ = fs::rename(rollback, &target);
            }
            return Err(error).context("安装宠物失败，已尝试恢复原宠物");
        }
        if let Some(rollback) = rollback {
            fs::remove_dir_all(rollback)?;
        }
        Ok(target)
    }
}

fn import_zip(paths: &AppPaths, archive: &Path) -> anyhow::Result<PetManifest> {
    if fs::metadata(archive)?.len() > MAX_ARCHIVE_BYTES {
        bail!("宠物包超过 40 MB");
    }
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).context("宠物包不是有效 ZIP")?;
    if zip.is_empty() || zip.len() > MAX_FILES {
        bail!("宠物包文件数量无效");
    }
    let staging = paths
        .staging
        .join(format!("pet-import-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&staging)?;
    let result = (|| -> anyhow::Result<PetManifest> {
        let mut total = 0u64;
        let mut package_root: Option<PathBuf> = None;
        for index in 0..zip.len() {
            let entry = zip.by_index(index)?;
            if entry.is_dir() {
                continue;
            }
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                bail!("宠物包不能包含符号链接");
            }
            let relative = entry
                .enclosed_name()
                .context("宠物包包含越界路径")?
                .to_path_buf();
            if relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            {
                bail!("宠物包包含无效路径");
            }
            total = total.saturating_add(entry.size());
            if total > MAX_UNPACKED_BYTES {
                bail!("宠物包解压大小超过 40 MB");
            }
            if entry.size() > 32 * 1024 * 1024 {
                bail!("宠物包中的单个文件超过 32 MB");
            }
            let Some(file_name) = relative.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if file_name != "pet.json" {
                continue;
            }
            let parent = relative.parent().unwrap_or_else(|| Path::new(""));
            if parent.components().count() > 1 {
                bail!("宠物运行文件只能位于 ZIP 根目录或单层宠物目录中");
            }
            if package_root.replace(parent.to_path_buf()).is_some() {
                bail!("宠物包包含多个 pet.json");
            }
        }
        let package_root = package_root.context("宠物包缺少 pet.json")?;
        for (archive_path, target_name) in [
            (package_root.join("pet.json"), "pet.json"),
            (package_root.join("spritesheet.webp"), "spritesheet.webp"),
        ] {
            let mut entry = zip
                .by_name(&archive_path.to_string_lossy().replace('\\', "/"))
                .with_context(|| format!("宠物包缺少与 pet.json 同目录的 {target_name}"))?;
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes)?;
            fs::write(staging.join(target_name), bytes)?;
        }
        let manifest = validate_pet_directory(&staging)?;
        let target = paths.pets.join(&manifest.id);
        if target.exists() {
            bail!("宠物 {} 已存在", manifest.id);
        }
        fs::rename(&staging, target)?;
        Ok(manifest)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn export_zip(paths: &AppPaths, id: &str, output: &Path) -> anyhow::Result<()> {
    let root = paths.pets.join(id);
    validate_pet_directory(&root)?;
    let temp = output.with_extension("zip.tmp");
    let mut zip = zip::ZipWriter::new(File::create(&temp)?);
    for relative in ["pet.json", "spritesheet.webp"] {
        zip.start_file(
            relative,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )?;
        zip.write_all(&fs::read(root.join(relative))?)?;
    }
    zip.finish()?;
    if output.exists() {
        fs::remove_file(output)?;
    }
    fs::rename(temp, output)?;
    Ok(())
}

fn codex_pets_root() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|value| PathBuf::from(value).join(".codex"))
        })
        .context("无法确定 CODEX_HOME")?;
    Ok(home.join("pets"))
}

fn copy_pet_files(source: &Path, target: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(target)?;
    for relative in ["pet.json", "spritesheet.webp"] {
        fs::copy(source.join(relative), target.join(relative))?;
    }
    Ok(())
}

fn owned_dialog(title: &str) -> AsyncFileDialog {
    let mut dialog = AsyncFileDialog::new().set_title(title);
    if let Some(parent) = crate::windows_app::foreground_dialog_parent().as_ref() {
        dialog = dialog.set_parent(parent);
    }
    dialog
}

fn unix_timestamp_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba, RgbaImage};

    fn write_valid_pet(root: &Path, description: &str) {
        fs::create_dir_all(root).unwrap();
        let manifest = serde_json::json!({
            "id": "test-pet",
            "displayName": "Test Pet",
            "description": description,
            "spriteVersionNumber": 2,
            "spritesheetPath": "spritesheet.webp"
        });
        fs::write(
            root.join("pet.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let mut image = RgbaImage::new(crate::pet::PET_WIDTH, crate::pet::PET_V2_HEIGHT);
        for (row, used) in [6u32, 8, 8, 4, 5, 8, 6, 6, 6, 8, 8].into_iter().enumerate() {
            for column in 0..used {
                image.put_pixel(
                    column * crate::pet::PET_CELL_WIDTH + 20,
                    row as u32 * crate::pet::PET_CELL_HEIGHT + 20,
                    Rgba([20, 40, 60, 255]),
                );
            }
        }
        image
            .save_with_format(root.join("spritesheet.webp"), ImageFormat::WebP)
            .unwrap();
    }

    #[test]
    fn codex_home_uses_environment_contract() {
        let path = codex_pets_root().unwrap();
        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("pets")
        );
    }

    #[test]
    fn installs_and_backs_up_existing_pet() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        write_valid_pet(&paths.pets.join("test-pet"), "new");
        let codex_pets = temp.path().join("codex-pets");
        write_valid_pet(&codex_pets.join("test-pet"), "old");
        let manager =
            PetPackageManager::new(paths.clone(), "http://localhost".into(), "token".into());

        let installed = manager.install_to_root("test-pet", &codex_pets).unwrap();
        assert_eq!(
            PetManifest::from_path(&installed.join("pet.json"))
                .unwrap()
                .description,
            "new"
        );
        let backup = fs::read_dir(paths.pet_backups)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            PetManifest::from_path(&backup.join("pet.json"))
                .unwrap()
                .description,
            "old"
        );
    }

    #[test]
    fn package_round_trip_preserves_codex_files() {
        let temp = tempfile::tempdir().unwrap();
        let source_paths = AppPaths::from_root(temp.path().join("source"));
        source_paths.ensure().unwrap();
        write_valid_pet(&source_paths.pets.join("test-pet"), "round trip");
        let archive = temp.path().join("test-pet.tipet");
        export_zip(&source_paths, "test-pet", &archive).unwrap();

        let target_paths = AppPaths::from_root(temp.path().join("target"));
        target_paths.ensure().unwrap();
        let imported = import_zip(&target_paths, &archive).unwrap();
        assert_eq!(imported.id, "test-pet");
        assert_eq!(
            validate_pet_directory(&target_paths.pets.join("test-pet")).unwrap(),
            imported
        );
    }

    #[test]
    fn returns_csp_safe_pet_data_url() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("source"));
        paths.ensure().unwrap();
        write_valid_pet(&paths.pets.join("test-pet"), "preview");
        let manager = PetPackageManager::new(paths, "http://localhost".into(), "token".into());

        let data_url = manager.data_url("test-pet").unwrap();
        assert!(data_url.starts_with("data:image/webp;base64,"));
        assert!(!data_url.contains("localhost"));
    }

    #[test]
    fn updates_metadata_without_dropping_market_fields() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("source"));
        paths.ensure().unwrap();
        let root = paths.pets.join("test-pet");
        write_valid_pet(&root, "old description");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("pet.json")).unwrap()).unwrap();
        value["kind"] = serde_json::json!("object");
        fs::write(
            root.join("pet.json"),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        let manager = PetPackageManager::new(paths, "http://localhost".into(), "token".into());

        let updated = manager
            .update_metadata("test-pet", "  New Name  ", "  New description  ")
            .unwrap();

        assert_eq!(updated.display_name, "New Name");
        assert_eq!(updated.description, "New description");
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("pet.json")).unwrap()).unwrap();
        assert_eq!(saved["kind"], "object");
        assert_eq!(saved["displayName"], "New Name");
        assert_eq!(saved["description"], "New description");
    }

    #[test]
    fn imports_nested_community_v1_package_with_market_extras() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = temp.path().join("fixture");
        fs::create_dir_all(&fixture).unwrap();
        fs::write(
            fixture.join("pet.json"),
            br#"{"id":"community-pet","displayName":"Community Pet","description":"market","spritesheetPath":"spritesheet.webp","kind":"object"}"#,
        )
        .unwrap();
        let mut image = RgbaImage::new(crate::pet::PET_WIDTH, crate::pet::PET_V1_HEIGHT);
        for (row, used) in [6u32, 8, 8, 4, 5, 8, 6, 6, 6].into_iter().enumerate() {
            for column in 0..used {
                image.put_pixel(
                    column * crate::pet::PET_CELL_WIDTH + 20,
                    row as u32 * crate::pet::PET_CELL_HEIGHT + 20,
                    Rgba([20, 40, 60, 255]),
                );
            }
        }
        image
            .save_with_format(fixture.join("spritesheet.webp"), ImageFormat::WebP)
            .unwrap();
        let archive = temp.path().join("community.zip");
        let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
        for (source, target) in [
            (fixture.join("pet.json"), "community/pet.json"),
            (
                fixture.join("spritesheet.webp"),
                "community/spritesheet.webp",
            ),
        ] {
            zip.start_file(target, SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&fs::read(source).unwrap()).unwrap();
        }
        zip.start_file("community/README.md", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"market notes").unwrap();
        zip.finish().unwrap();

        let paths = AppPaths::from_root(temp.path().join("target"));
        paths.ensure().unwrap();
        let imported = import_zip(&paths, &archive).unwrap();
        assert_eq!(imported.id, "community-pet");
        assert_eq!(imported.sprite_version_number, 1);
    }

    #[test]
    fn imports_external_market_fixture_when_configured() {
        let Ok(archive) = std::env::var("THEME_INJECT_TEST_PET_ARCHIVE") else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("target"));
        paths.ensure().unwrap();

        let imported = import_zip(&paths, Path::new(&archive)).unwrap();
        assert!(matches!(imported.sprite_version_number, 1 | 2));
        assert_eq!(
            validate_pet_directory(&paths.pets.join(&imported.id)).unwrap(),
            imported
        );

        let exported = temp.path().join("market-round-trip.zip");
        export_zip(&paths, &imported.id, &exported).unwrap();
        let file = File::open(exported).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        assert_eq!(zip.len(), 2);
        assert!(zip.by_name("spritesheet.webp").is_ok());
        let manifest: serde_json::Value =
            serde_json::from_reader(zip.by_name("pet.json").unwrap()).unwrap();
        assert_eq!(manifest["id"], imported.id);
        if imported.sprite_version_number == 1 {
            assert!(manifest.get("spriteVersionNumber").is_none());
            assert_eq!(manifest["kind"], "object");
        }
    }
}
