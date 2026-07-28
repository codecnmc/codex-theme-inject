use std::fs;
use std::path::Path;

use anyhow::{Context, bail};
use image::GenericImageView;
use serde::{Deserialize, Serialize};

pub const PET_COLUMNS: u32 = 8;
pub const PET_V1_ROWS: u32 = 9;
pub const PET_V2_ROWS: u32 = 11;
pub const PET_CELL_WIDTH: u32 = 192;
pub const PET_CELL_HEIGHT: u32 = 208;
pub const PET_WIDTH: u32 = PET_COLUMNS * PET_CELL_WIDTH;
pub const PET_V1_HEIGHT: u32 = PET_V1_ROWS * PET_CELL_HEIGHT;
pub const PET_V2_HEIGHT: u32 = PET_V2_ROWS * PET_CELL_HEIGHT;
const USED_FRAMES: [u32; 11] = [6, 8, 8, 4, 5, 8, 6, 6, 6, 8, 8];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PetManifest {
    pub id: String,
    pub display_name: String,
    pub description: String,
    #[serde(default)]
    pub sprite_version_number: u32,
    pub spritesheet_path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetSummary {
    #[serde(flatten)]
    pub manifest: PetManifest,
    pub installed: bool,
    pub asset_url: String,
}

impl PetManifest {
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        let manifest: Self = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("找不到宠物清单 {}", path.display()))?,
        )
        .context("pet.json 格式无效")?;
        manifest.validate_metadata()?;
        Ok(manifest)
    }

    pub(crate) fn validate_metadata(&self) -> anyhow::Result<()> {
        validate_pet_id(&self.id)?;
        if self.display_name.trim().is_empty() || self.display_name.chars().count() > 80 {
            bail!("宠物名称不能为空且不能超过 80 个字符");
        }
        if self.description.chars().count() > 240 || self.description.contains(['\0', '\r']) {
            bail!("宠物介绍格式无效或超过 240 个字符");
        }
        if self.sprite_version_number > 2 {
            bail!(
                "不支持 spriteVersionNumber {} 宠物",
                self.sprite_version_number
            );
        }
        if self.spritesheet_path != "spritesheet.webp" {
            bail!("宠物图集必须命名为 spritesheet.webp");
        }
        Ok(())
    }
}

pub fn validate_pet_id(id: &str) -> anyhow::Result<()> {
    if id.is_empty()
        || id.len() > 80
        || id.starts_with('.')
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        bail!("宠物 ID 只能包含小写字母、数字、点、下划线和连字符");
    }
    Ok(())
}

pub fn validate_pet_directory(root: &Path) -> anyhow::Result<PetManifest> {
    let mut manifest = PetManifest::from_path(&root.join("pet.json"))?;
    let sheet_path = root.join(&manifest.spritesheet_path);
    let metadata = fs::metadata(&sheet_path).context("宠物包缺少 spritesheet.webp")?;
    if metadata.len() == 0 || metadata.len() > 32 * 1024 * 1024 {
        bail!("宠物图集为空或超过 32 MB");
    }
    let image = image::open(&sheet_path).context("无法解码宠物图集")?;
    let rows = match image.dimensions() {
        (PET_WIDTH, PET_V1_HEIGHT) => 1,
        (PET_WIDTH, PET_V2_HEIGHT) => 2,
        _ => {
            bail!("宠物图集必须为 V1 {PET_WIDTH}x{PET_V1_HEIGHT} 或 V2 {PET_WIDTH}x{PET_V2_HEIGHT}")
        }
    };
    if manifest.sprite_version_number == 0 {
        manifest.sprite_version_number = rows;
    } else if manifest.sprite_version_number != rows {
        bail!(
            "spriteVersionNumber {} 与图集实际版本 V{rows} 不一致",
            manifest.sprite_version_number
        );
    }
    if !image.color().has_alpha() {
        bail!("宠物图集必须包含透明通道");
    }
    let rgba = image.to_rgba8();
    for (row, used) in USED_FRAMES
        .into_iter()
        .take(if rows == 1 { PET_V1_ROWS } else { PET_V2_ROWS } as usize)
        .enumerate()
    {
        for column in 0..PET_COLUMNS {
            let mut visible = false;
            for y in row as u32 * PET_CELL_HEIGHT..(row as u32 + 1) * PET_CELL_HEIGHT {
                for x in column * PET_CELL_WIDTH..(column + 1) * PET_CELL_WIDTH {
                    if rgba.get_pixel(x, y)[3] != 0 {
                        visible = true;
                        break;
                    }
                }
                if visible {
                    break;
                }
            }
            if column < used && !visible {
                bail!("宠物动作第 {} 行第 {} 帧为空", row + 1, column + 1);
            }
            if column >= used && visible {
                bail!("宠物动作第 {} 行未使用帧必须全透明", row + 1);
            }
        }
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba, RgbaImage};

    fn valid_pet(root: &Path) {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("pet.json"),
            br#"{"id":"test-pet","displayName":"Test Pet","description":"Test","spriteVersionNumber":2,"spritesheetPath":"spritesheet.webp"}"#,
        )
        .unwrap();
        let mut image = RgbaImage::new(PET_WIDTH, PET_V2_HEIGHT);
        for (row, used) in USED_FRAMES.into_iter().enumerate() {
            for column in 0..used {
                image.put_pixel(
                    column * PET_CELL_WIDTH + 20,
                    row as u32 * PET_CELL_HEIGHT + 20,
                    Rgba([1, 2, 3, 255]),
                );
            }
        }
        image
            .save_with_format(root.join("spritesheet.webp"), ImageFormat::WebP)
            .unwrap();
    }

    #[test]
    fn validates_v2_pet() {
        let temp = tempfile::tempdir().unwrap();
        valid_pet(temp.path());
        assert_eq!(validate_pet_directory(temp.path()).unwrap().id, "test-pet");
    }

    #[test]
    fn rejects_visible_unused_frame() {
        let temp = tempfile::tempdir().unwrap();
        valid_pet(temp.path());
        let mut image = image::open(temp.path().join("spritesheet.webp"))
            .unwrap()
            .to_rgba8();
        image.put_pixel(7 * PET_CELL_WIDTH + 10, 10, Rgba([1, 2, 3, 255]));
        image
            .save_with_format(temp.path().join("spritesheet.webp"), ImageFormat::WebP)
            .unwrap();
        assert!(validate_pet_directory(temp.path()).is_err());
    }

    #[test]
    fn infers_community_v1_when_version_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path()).unwrap();
        fs::write(
            temp.path().join("pet.json"),
            br#"{"id":"community-pet","displayName":"Community Pet","description":"Legacy market package","spritesheetPath":"spritesheet.webp","kind":"object"}"#,
        )
        .unwrap();
        let mut image = RgbaImage::new(PET_WIDTH, PET_V1_HEIGHT);
        for (row, used) in USED_FRAMES
            .into_iter()
            .take(PET_V1_ROWS as usize)
            .enumerate()
        {
            for column in 0..used {
                image.put_pixel(
                    column * PET_CELL_WIDTH + 20,
                    row as u32 * PET_CELL_HEIGHT + 20,
                    Rgba([1, 2, 3, 255]),
                );
            }
        }
        image
            .save_with_format(temp.path().join("spritesheet.webp"), ImageFormat::WebP)
            .unwrap();

        assert_eq!(
            validate_pet_directory(temp.path())
                .unwrap()
                .sprite_version_number,
            1
        );
    }
}
