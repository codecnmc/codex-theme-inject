use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use anyhow::{Context, bail};
use base64::Engine;
use futures_util::{StreamExt, stream};
use image::{DynamicImage, ImageFormat, RgbaImage, imageops::FilterType};
use reqwest::header::{CONTENT_TYPE, RETRY_AFTER};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::storage::{AppPaths, atomic_write};
use crate::theme::{
    BackgroundMode, BackgroundSource, BaseMode, Decoration, DecorationAnchor, DecorationRegion,
    HomeCard, IconMode, RegionBackground, ThemeManifest,
};

const MAX_PROMPT_CHARS: usize = 6_000;
const MAX_CHAT_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_IMAGE_RESPONSE_BYTES: usize = 22 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 15 * 1024 * 1024;
const MAX_GENERATED_ASSETS: usize = 6;
const MAX_GENERATED_TOTAL_BYTES: usize = 72 * 1024 * 1024;
const IMAGE_GENERATION_CONCURRENCY: usize = 2;
const MAX_AI_REQUEST_CONCURRENCY: usize = 5;
const MAX_IMAGE_RATE_LIMIT_RETRIES: usize = 8;
const ICON_ACTIONS: [(&str, &str); 9] = [
    ("sidebar-toggle", "toggle the application sidebar"),
    ("new-task", "create a new coding task"),
    ("search", "search"),
    ("scheduled", "scheduled tasks and calendar"),
    ("plugins", "plugins and extensions"),
    ("pull-requests", "git pull requests"),
    ("settings", "settings and preferences"),
    ("send", "send a message"),
    ("terminal", "terminal and command line"),
];
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_TEXT_MODEL: &str = "gpt-4.1-mini";
const DEFAULT_IMAGE_MODEL: &str = "gpt-image-1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiPublicSettings {
    pub base_url: String,
    #[serde(default)]
    pub image_base_url: String,
    pub text_model: String,
    pub vision_model: String,
    pub image_model: String,
    pub image_size: String,
    #[serde(default)]
    pub has_api_key: bool,
    #[serde(default)]
    pub has_image_api_key: bool,
}

impl Default for AiPublicSettings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.into(),
            image_base_url: String::new(),
            text_model: DEFAULT_TEXT_MODEL.into(),
            vision_model: DEFAULT_TEXT_MODEL.into(),
            image_model: DEFAULT_IMAGE_MODEL.into(),
            image_size: "3:2".into(),
            has_api_key: false,
            has_image_api_key: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAiSettings {
    #[serde(default = "default_base_url")]
    base_url: String,
    #[serde(default)]
    image_base_url: String,
    #[serde(default = "default_text_model")]
    text_model: String,
    #[serde(default = "default_text_model")]
    vision_model: String,
    #[serde(default = "default_image_model")]
    image_model: String,
    #[serde(default = "default_image_size")]
    image_size: String,
    #[serde(default)]
    encrypted_api_key: Option<String>,
    #[serde(default)]
    encrypted_image_api_key: Option<String>,
}

impl Default for StoredAiSettings {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            image_base_url: String::new(),
            text_model: default_text_model(),
            vision_model: default_text_model(),
            image_model: default_image_model(),
            image_size: default_image_size(),
            encrypted_api_key: None,
            encrypted_image_api_key: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiSettingsStore {
    path: PathBuf,
}

impl AiSettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn public(&self) -> anyhow::Result<AiPublicSettings> {
        let stored = self.load()?;
        Ok(AiPublicSettings {
            base_url: stored.base_url,
            image_base_url: stored.image_base_url,
            text_model: stored.text_model,
            vision_model: stored.vision_model,
            image_model: stored.image_model,
            image_size: normalize_image_ratio(&stored.image_size)?,
            has_api_key: stored.encrypted_api_key.is_some(),
            has_image_api_key: stored.encrypted_image_api_key.is_some(),
        })
    }

    pub fn save(
        &self,
        public: AiPublicSettings,
        api_key: Option<&str>,
        image_api_key: Option<&str>,
        clear_api_key: bool,
        clear_image_api_key: bool,
    ) -> anyhow::Result<AiPublicSettings> {
        validate_public_settings(&public)?;
        let previous = self.load()?;
        let encrypted_api_key = if clear_api_key {
            None
        } else if let Some(api_key) = api_key.map(str::trim).filter(|value| !value.is_empty()) {
            validate_api_key(api_key, "基础 API Key")?;
            Some(
                base64::engine::general_purpose::STANDARD
                    .encode(protect_secret(api_key.as_bytes())?),
            )
        } else {
            previous.encrypted_api_key
        };
        let encrypted_image_api_key = if clear_image_api_key {
            None
        } else if let Some(image_api_key) = image_api_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            validate_api_key(image_api_key, "生图 API Key")?;
            Some(
                base64::engine::general_purpose::STANDARD
                    .encode(protect_secret(image_api_key.as_bytes())?),
            )
        } else {
            previous.encrypted_image_api_key
        };
        let stored = StoredAiSettings {
            base_url: public.base_url.trim().trim_end_matches('/').to_string(),
            image_base_url: public
                .image_base_url
                .trim()
                .trim_end_matches('/')
                .to_string(),
            text_model: public.text_model.trim().to_string(),
            vision_model: public.vision_model.trim().to_string(),
            image_model: public.image_model.trim().to_string(),
            image_size: normalize_image_ratio(&public.image_size)?,
            encrypted_api_key,
            encrypted_image_api_key,
        };
        atomic_write(&self.path, &serde_json::to_vec_pretty(&stored)?)?;
        self.public()
    }

    fn load(&self) -> anyhow::Result<StoredAiSettings> {
        if !self.path.exists() {
            return Ok(StoredAiSettings::default());
        }
        let settings: StoredAiSettings =
            serde_json::from_slice(&fs::read(&self.path)?).context("ai-settings.json 格式无效")?;
        validate_stored_settings(&settings)?;
        Ok(settings)
    }

    fn credentials(&self) -> anyhow::Result<AiCredentials> {
        let stored = self.load()?;
        let encrypted = stored
            .encrypted_api_key
            .as_deref()
            .context("尚未保存 API Key")?;
        let encrypted = base64::engine::general_purpose::STANDARD
            .decode(encrypted)
            .context("已保存的 API Key 数据无效")?;
        let api_key = String::from_utf8(unprotect_secret(&encrypted)?)
            .context("已保存的 API Key 无法解码")?;
        let image_api_key = stored
            .encrypted_image_api_key
            .as_deref()
            .map(|encrypted| -> anyhow::Result<String> {
                let encrypted = base64::engine::general_purpose::STANDARD
                    .decode(encrypted)
                    .context("已保存的生图 API Key 数据无效")?;
                String::from_utf8(unprotect_secret(&encrypted)?)
                    .context("已保存的生图 API Key 无法解码")
            })
            .transpose()?;
        Ok(AiCredentials {
            base_url: stored.base_url,
            image_base_url: stored.image_base_url,
            text_model: stored.text_model,
            vision_model: stored.vision_model,
            image_model: stored.image_model,
            image_size: stored.image_size,
            api_key,
            image_api_key,
        })
    }
}

#[derive(Clone, Debug)]
struct AiCredentials {
    base_url: String,
    image_base_url: String,
    text_model: String,
    vision_model: String,
    image_model: String,
    image_size: String,
    api_key: String,
    image_api_key: Option<String>,
}

impl AiCredentials {
    fn effective_image_base_url(&self) -> &str {
        if self.image_base_url.trim().is_empty() {
            &self.base_url
        } else {
            &self.image_base_url
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    pub prompt: String,
    #[serde(default = "default_theme_language")]
    pub language: String,
    #[serde(default)]
    pub generate_images: bool,
    #[serde(default)]
    pub generate_sidebar_watermark: bool,
    #[serde(default)]
    pub generate_system_icons: bool,
    #[serde(default = "default_image_concurrency")]
    pub image_concurrency: usize,
    #[serde(default)]
    pub reference_session: Option<String>,
    #[serde(default)]
    pub reference_path: Option<String>,
    #[serde(default)]
    pub references: Vec<ReferenceImage>,
    #[serde(default)]
    pub resource_plans: Vec<ResourcePlan>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceImage {
    pub session: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePlan {
    pub slot: String,
    pub prompt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedAsset {
    pub slot: String,
    pub session: String,
    pub path: String,
    pub preview_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerationCheckpoint {
    fingerprint: String,
    theme: ThemeManifest,
    plans: Vec<CheckpointPlan>,
    assets: Vec<GeneratedAsset>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointPlan {
    slot: String,
    prompt: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResult {
    pub theme: ThemeManifest,
    pub assets: Vec<GeneratedAsset>,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub theme: ThemeManifest,
    pub assets: Vec<GeneratedAsset>,
    pub plans: Vec<ResourcePlan>,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestLog {
    pub timestamp_ms: u64,
    pub stage: String,
    pub endpoint: String,
    pub model: String,
    pub outcome: String,
    pub http_status: Option<u16>,
    pub duration_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThemeBlueprint {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    base_mode: BlueprintMode,
    palette: BlueprintPalette,
    #[serde(default)]
    brand: BlueprintBrand,
    #[serde(default)]
    home: BlueprintHome,
    #[serde(default)]
    style: BlueprintStyle,
    #[serde(default)]
    background: BlueprintBackground,
    #[serde(default)]
    icon_style: String,
    #[serde(default)]
    assets: Vec<BlueprintAsset>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum BlueprintMode {
    Light,
    #[default]
    Dark,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlueprintPalette {
    app: String,
    sidebar: String,
    content: String,
    elevated: String,
    input: String,
    foreground: String,
    muted: String,
    border: String,
    accent: String,
    accent_foreground: String,
    code_background: String,
    code_foreground: String,
}

#[derive(Clone, Debug)]
struct NormalizedPalette {
    app: String,
    sidebar: String,
    content: String,
    elevated: String,
    input: String,
    foreground: String,
    muted: String,
    border: String,
    accent: String,
    accent_foreground: String,
    code_background: String,
    code_foreground: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlueprintBrand {
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlueprintHome {
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: String,
    #[serde(default)]
    cards: Vec<BlueprintCard>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlueprintCard {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    prompt: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlueprintStyle {
    #[serde(default = "default_radius")]
    radius: u16,
    #[serde(default = "default_density")]
    density: f32,
    #[serde(default = "default_sidebar_width")]
    sidebar_width: u16,
    #[serde(default = "default_content_width")]
    content_max_width: u16,
    #[serde(default = "default_hero_height")]
    hero_height: u16,
    #[serde(default = "default_surface_opacity")]
    surface_opacity: f32,
    #[serde(default = "default_blur")]
    blur: u16,
}

impl Default for BlueprintStyle {
    fn default() -> Self {
        Self {
            radius: default_radius(),
            density: default_density(),
            sidebar_width: default_sidebar_width(),
            content_max_width: default_content_width(),
            hero_height: default_hero_height(),
            surface_opacity: default_surface_opacity(),
            blur: default_blur(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum BlueprintBackground {
    #[default]
    None,
    Solid,
    Gradient,
    Image,
    PerRegion,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlueprintAsset {
    slot: String,
    prompt: String,
}

pub struct AiThemeService {
    paths: AppPaths,
    settings: AiSettingsStore,
    client: Client,
    asset_base: String,
    asset_token: String,
    request_log: StdMutex<Vec<AiRequestLog>>,
    generation_progress: StdMutex<GenerationProgress>,
    request_semaphore: Semaphore,
}

impl AiThemeService {
    pub fn new(paths: AppPaths, asset_base: String, asset_token: String) -> anyhow::Result<Self> {
        let settings = AiSettingsStore::new(paths.ai_settings.clone());
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(5 * 60))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("ThemeInject/0.1")
            .build()?;
        Ok(Self {
            paths,
            settings,
            client,
            asset_base,
            asset_token,
            request_log: StdMutex::new(Vec::new()),
            generation_progress: StdMutex::new(GenerationProgress::default()),
            request_semaphore: Semaphore::new(MAX_AI_REQUEST_CONCURRENCY),
        })
    }

    pub fn request_log(&self) -> Vec<AiRequestLog> {
        self.request_log
            .lock()
            .map(|entries| entries.clone())
            .unwrap_or_default()
    }

    pub fn generation_progress(&self) -> GenerationProgress {
        self.generation_progress
            .lock()
            .map(|progress| progress.clone())
            .unwrap_or_default()
    }

    fn set_generation_state(&self, state: &str, message: &str) {
        if let Ok(mut progress) = self.generation_progress.lock() {
            progress.state = state.into();
            progress.message = message.into();
        }
    }

    fn set_generation_plans(
        &self,
        plans: &[(GeneratedSlot, String)],
        completed: &[GeneratedAsset],
    ) {
        let completed = completed
            .iter()
            .map(|asset| (asset.slot.as_str(), asset))
            .collect::<BTreeMap<_, _>>();
        if let Ok(mut progress) = self.generation_progress.lock() {
            progress.state = "generating".into();
            progress.message = "主题蓝图已完成，正在生成图片资源".into();
            progress.items = plans
                .iter()
                .map(|(slot, prompt)| {
                    let slot_id = slot.asset_slot();
                    let asset = completed.get(slot_id.as_str()).copied();
                    GenerationProgressItem {
                        slot: slot_id,
                        label: slot.stage_label(),
                        prompt: prompt.clone(),
                        status: if asset.is_none() {
                            "queued"
                        } else {
                            "completed"
                        }
                        .into(),
                        preview_url: asset
                            .map(|asset| asset.preview_url.clone())
                            .unwrap_or_default(),
                        session: asset.map(|asset| asset.session.clone()).unwrap_or_default(),
                        path: asset.map(|asset| asset.path.clone()).unwrap_or_default(),
                        message: String::new(),
                    }
                })
                .collect();
        }
    }

    fn update_generation_item(
        &self,
        slot: GeneratedSlot,
        status: &str,
        preview_url: &str,
        session: &str,
        path: &str,
        message: &str,
    ) {
        if let Ok(mut progress) = self.generation_progress.lock()
            && let Some(item) = progress
                .items
                .iter_mut()
                .find(|item| item.slot == slot.asset_slot())
        {
            item.status = status.into();
            item.preview_url = preview_url.into();
            item.session = session.into();
            item.path = path.into();
            item.message = message.chars().take(500).collect();
        }
    }

    pub fn clear_checkpoint_for_theme(&self, theme_id: &str) -> anyhow::Result<()> {
        let Some(checkpoint) = self.read_checkpoint()? else {
            return Ok(());
        };
        if checkpoint.theme.id == theme_id {
            self.discard_checkpoint(&checkpoint);
        }
        Ok(())
    }

    pub fn clear_checkpoint(&self) -> anyhow::Result<()> {
        let Some(checkpoint) = self.read_checkpoint()? else {
            return Ok(());
        };
        self.discard_checkpoint(&checkpoint);
        Ok(())
    }

    pub fn restore_checkpoint(&self) -> anyhow::Result<RestoreResult> {
        let mut checkpoint = self
            .read_checkpoint()?
            .context("没有可恢复的 AI 生成结果")?;
        checkpoint
            .theme
            .validate()
            .context("上次生成的主题数据无效")?;
        for asset in &mut checkpoint.assets {
            validate_session(&asset.session)?;
            crate::theme::validate_image_asset_path(&asset.path)?;
            if !self
                .paths
                .staging
                .join(&asset.session)
                .join(&asset.path)
                .is_file()
            {
                bail!("上次生成的资源文件不存在：{}", asset.slot);
            }
            asset.preview_url = self.reference_preview_url(&asset.session, &asset.path)?;
        }
        let plans = checkpoint
            .plans
            .into_iter()
            .map(|plan| ResourcePlan {
                slot: plan.slot,
                prompt: plan.prompt,
            })
            .collect::<Vec<_>>();
        Ok(RestoreResult {
            summary: format!(
                "已恢复“{}”：{} 个图片资源",
                checkpoint.theme.name,
                checkpoint.assets.len()
            ),
            theme: checkpoint.theme,
            assets: checkpoint.assets,
            plans,
        })
    }

    pub fn checkpoint_has_session(&self, session: &str) -> bool {
        self.read_checkpoint()
            .ok()
            .flatten()
            .is_some_and(|checkpoint| {
                checkpoint
                    .assets
                    .iter()
                    .any(|asset| asset.session == session)
            })
    }

    fn clear_request_log(&self) {
        if let Ok(mut entries) = self.request_log.lock() {
            entries.clear();
        }
    }

    fn read_checkpoint(&self) -> anyhow::Result<Option<GenerationCheckpoint>> {
        if !self.paths.ai_generation.exists() {
            return Ok(None);
        }
        let checkpoint = serde_json::from_slice(&fs::read(&self.paths.ai_generation)?)
            .context("AI 生成检查点格式无效")?;
        Ok(Some(checkpoint))
    }

    fn load_checkpoint(&self, fingerprint: &str) -> anyhow::Result<Option<GenerationCheckpoint>> {
        let Some(mut checkpoint) = self.read_checkpoint()? else {
            return Ok(None);
        };
        let valid = checkpoint.fingerprint == fingerprint
            && checkpoint.theme.validate().is_ok()
            && checkpoint.assets.iter().all(|asset| {
                self.paths
                    .staging
                    .join(&asset.session)
                    .join(&asset.path)
                    .is_file()
            });
        if !valid {
            return Ok(None);
        }
        for asset in &mut checkpoint.assets {
            asset.preview_url = self.reference_preview_url(&asset.session, &asset.path)?;
        }
        Ok(Some(checkpoint))
    }

    fn save_checkpoint(
        &self,
        fingerprint: &str,
        theme: &ThemeManifest,
        plans: &[(GeneratedSlot, String)],
        assets: &[GeneratedAsset],
    ) -> anyhow::Result<()> {
        let checkpoint = GenerationCheckpoint {
            fingerprint: fingerprint.into(),
            theme: theme.clone(),
            plans: plans
                .iter()
                .map(|(slot, prompt)| CheckpointPlan {
                    slot: slot.asset_slot(),
                    prompt: prompt.clone(),
                })
                .collect(),
            assets: assets.to_vec(),
        };
        atomic_write(
            &self.paths.ai_generation,
            &serde_json::to_vec_pretty(&checkpoint)?,
        )
    }

    fn discard_checkpoint(&self, checkpoint: &GenerationCheckpoint) {
        self.cleanup_generated_assets(&checkpoint.assets);
        let _ = fs::remove_file(&self.paths.ai_generation);
    }

    fn generated_asset_bytes(&self, assets: &[GeneratedAsset]) -> anyhow::Result<usize> {
        assets.iter().try_fold(0usize, |total, asset| {
            let length =
                fs::metadata(self.paths.staging.join(&asset.session).join(&asset.path))?.len();
            Ok(total.saturating_add(usize::try_from(length).unwrap_or(usize::MAX)))
        })
    }

    pub fn public_settings(&self) -> anyhow::Result<AiPublicSettings> {
        self.settings.public()
    }

    pub fn save_settings(
        &self,
        settings: AiPublicSettings,
        api_key: Option<&str>,
        image_api_key: Option<&str>,
        clear_api_key: bool,
        clear_image_api_key: bool,
    ) -> anyhow::Result<AiPublicSettings> {
        self.settings.save(
            settings,
            api_key,
            image_api_key,
            clear_api_key,
            clear_image_api_key,
        )
    }

    pub fn reference_preview_url(&self, session: &str, path: &str) -> anyhow::Result<String> {
        validate_session(session)?;
        crate::theme::validate_image_asset_path(path)?;
        Ok(format!(
            "{}/staging/{}/{}?token={}",
            self.asset_base,
            session,
            path.split('/')
                .map(url_component)
                .collect::<Vec<_>>()
                .join("/"),
            url_component(&self.asset_token)
        ))
    }

    pub async fn generate(&self, mut request: GenerateRequest) -> anyhow::Result<GenerateResult> {
        self.clear_request_log();
        self.set_generation_state("planning", "正在调用基础/视觉模型分析主题需求并生成蓝图");
        let references = self.reference_data_urls(&request)?;
        if request.prompt.trim().is_empty() && !references.is_empty() {
            request.prompt =
                "请以参考图组为视觉方向，生成一套完整、可读、适合长时间编码的 Codex 皮肤主题"
                    .into();
        }
        validate_prompt(&request.prompt)?;
        validate_theme_language(&request.language)?;
        request.image_concurrency = request.image_concurrency.clamp(1, 4);
        validate_resource_plans(&request.resource_plans)?;
        let credentials = self.settings.credentials()?;
        if request.generate_images && credentials.image_api_key.is_none() {
            bail!("尚未保存生图 API Key");
        }
        let fingerprint = generation_fingerprint(&request, &references, &credentials);
        let resumed = request
            .generate_images
            .then(|| self.load_checkpoint(&fingerprint))
            .transpose()?
            .flatten();
        let (mut theme, plans, mut generated_assets) = if let Some(checkpoint) = resumed {
            crate::diagnostic::log(
                "ai.generation.resumed",
                json!({ "themeId": checkpoint.theme.id, "completed": checkpoint.assets.len(), "total": checkpoint.plans.len() }),
            );
            (
                checkpoint.theme,
                checkpoint
                    .plans
                    .into_iter()
                    .filter_map(|plan| {
                        GeneratedSlot::from_asset_slot(&plan.slot).map(|slot| (slot, plan.prompt))
                    })
                    .collect::<Vec<_>>(),
                checkpoint.assets,
            )
        } else {
            let blueprint = self
                .generate_blueprint(
                    &credentials,
                    request.prompt.trim(),
                    request.language.trim(),
                    &references,
                )
                .await
                .context("主题蓝图生成失败")?;
            let theme = theme_from_blueprint(&blueprint)?;
            let mut plans = blueprint
                .assets
                .iter()
                .take(MAX_GENERATED_ASSETS)
                .filter_map(|asset| {
                    GeneratedSlot::parse(&asset.slot).map(|slot| (slot, asset.prompt.clone()))
                })
                .collect::<Vec<_>>();
            plans.extend(icon_generation_plans(&blueprint));
            merge_resource_plans(&mut plans, &request.resource_plans);
            if request.generate_sidebar_watermark
                && !plans
                    .iter()
                    .any(|(slot, _)| *slot == GeneratedSlot::SidebarWatermark)
            {
                plans.push((
                    GeneratedSlot::SidebarWatermark,
                    sidebar_watermark_prompt(&blueprint),
                ));
            } else if !request.generate_sidebar_watermark {
                plans.retain(|(slot, _)| *slot != GeneratedSlot::SidebarWatermark);
            }
            if request.generate_system_icons {
                plans.push((
                    GeneratedSlot::ActionIconAtlas,
                    action_icon_atlas_prompt(&blueprint),
                ));
            }
            if request.generate_images {
                self.save_checkpoint(&fingerprint, &theme, &plans, &[])?;
            }
            (theme, plans, Vec::new())
        };
        if request.generate_images {
            self.set_generation_plans(&plans, &generated_assets);
            let completed = generated_assets
                .iter()
                .map(|asset| asset.slot.as_str())
                .collect::<BTreeSet<_>>();
            let pending = plans
                .iter()
                .filter(|(slot, _)| !completed.contains(slot.asset_slot().as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let credentials = &credentials;
            let mut results = stream::iter(pending.into_iter().map(|(slot, prompt)| async move {
                let stage = slot.stage_label();
                self.update_generation_item(slot, "generating", "", "", "", "");
                let result = self
                    .generate_image(credentials, slot, &prompt, &stage)
                    .await
                    .with_context(|| format!("{stage}生成失败"));
                if let Err(error) = &result {
                    self.update_generation_item(slot, "failed", "", "", "", &format!("{error:#}"));
                }
                (slot, result)
            }))
            .buffer_unordered(request.image_concurrency);
            let mut generated_bytes = self.generated_asset_bytes(&generated_assets)?;
            let mut first_error = None;
            while let Some((slot, result)) = results.next().await {
                let bytes = match result {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                        continue;
                    }
                };
                generated_bytes = generated_bytes.saturating_add(bytes.len());
                if generated_bytes > MAX_GENERATED_TOTAL_BYTES {
                    bail!("AI 生成资源总大小超过 72 MB");
                }
                let staged = if slot == GeneratedSlot::ActionIconAtlas {
                    let (atlas, icons) = self.stage_action_icon_atlas(&bytes)?;
                    for (action_slot, generated) in &icons {
                        apply_generated_slot(&mut theme, *action_slot, &generated.path)?;
                    }
                    self.update_generation_item(
                        slot,
                        "completed",
                        &atlas.preview_url,
                        &atlas.session,
                        &atlas.path,
                        "已切割为 9 个系统图标",
                    );
                    generated_assets.push(atlas);
                    generated_assets.extend(icons.into_iter().map(|(_, asset)| asset));
                    None
                } else {
                    let bytes = post_process_generated_asset(slot, &bytes)?;
                    let staged = self.stage_generated_asset(slot, &bytes)?;
                    apply_generated_slot(&mut theme, slot, &staged.path)?;
                    self.update_generation_item(
                        slot,
                        "completed",
                        &staged.preview_url,
                        &staged.session,
                        &staged.path,
                        "",
                    );
                    Some(staged)
                };
                if let Some(staged) = staged {
                    generated_assets.push(staged);
                }
                self.save_checkpoint(&fingerprint, &theme, &plans, &generated_assets)?;
            }
            if let Some(error) = first_error {
                return Err(error.context(format!(
                    "已持久化保存 {}/{} 个图片资源；再次使用相同描述生成时将从未完成处继续",
                    generated_assets.len(),
                    plans.len()
                )));
            }
        }
        theme.validate()?;
        self.set_generation_state("completed", "主题蓝图和图片资源生成完成");
        Ok(GenerateResult {
            summary: format!(
                "已生成“{}”：{} 个图片资源，可继续编辑后应用",
                theme.name,
                generated_assets.len()
            ),
            theme,
            assets: generated_assets,
        })
    }

    async fn generate_blueprint(
        &self,
        credentials: &AiCredentials,
        prompt: &str,
        language: &str,
        references: &[String],
    ) -> anyhow::Result<ThemeBlueprint> {
        let model = if references.is_empty() {
            credentials.text_model.as_str()
        } else {
            credentials.vision_model.as_str()
        };
        let input = if references.is_empty() {
            json!(format!(
                "用户要求：{prompt}\n界面语言：{}。所有面向用户的主题名称、品牌文案、首页标题、说明和卡片文字必须使用该语言。",
                theme_language_name(language)
            ))
        } else {
            let mut content = vec![
                json!({ "type": "input_text", "text": format!("用户要求：{prompt}\n界面语言：{}。请综合分析所有参考图的色彩、层级、氛围、布局和共同风格，生成可读且实用的 Codex 皮肤蓝图。所有面向用户的主题名称、品牌文案、首页标题、说明和卡片文字必须使用该语言。", theme_language_name(language)) }),
            ];
            content.extend(references.iter().map(|reference| {
                json!({ "type": "input_image", "image_url": reference, "detail": "high" })
            }));
            json!([{ "role": "user", "content": content }])
        };
        let body = json!({
            "model": model,
            "instructions": blueprint_system_prompt(),
            "input": input
        });
        let endpoint = endpoint(&credentials.base_url, EndpointKind::Responses)?;
        let response = self
            .send_json(
                &endpoint,
                &credentials.api_key,
                &body,
                MAX_CHAT_RESPONSE_BYTES,
                "主题蓝图",
                model,
            )
            .await;
        let response = match response {
            Ok(value) => value,
            Err(error) => return Err(error.error),
        };
        let content = responses_content(&response)?;
        let json = extract_json_object(&content)?;
        serde_json::from_str(&json).context("基础模型返回的主题蓝图结构无效")
    }

    async fn generate_image(
        &self,
        credentials: &AiCredentials,
        slot: GeneratedSlot,
        prompt: &str,
        stage: &str,
    ) -> anyhow::Result<Vec<u8>> {
        if prompt.trim().is_empty() || prompt.chars().count() > 2_000 {
            bail!("生图提示词为空或过长");
        }
        let endpoint = endpoint(credentials.effective_image_base_url(), EndpointKind::Images)?;
        let image_api_key = credentials
            .image_api_key
            .as_deref()
            .context("尚未保存生图 API Key")?;
        let mut body = image_request_body(credentials, slot, prompt)?;
        let mut rate_limit_retries = 0usize;
        let mut compatibility_retry = false;
        let response = loop {
            let request_stage = if compatibility_retry {
                format!("{stage}（兼容重试）")
            } else if rate_limit_retries > 0 {
                format!("{stage}（限流续跑 {}）", rate_limit_retries)
            } else {
                stage.into()
            };
            match self
                .send_json(
                    &endpoint,
                    image_api_key,
                    &body,
                    MAX_IMAGE_RESPONSE_BYTES,
                    &request_stage,
                    &credentials.image_model,
                )
                .await
            {
                Ok(value) => break value,
                Err(error) if error.retry_without_json_mode && !compatibility_retry => {
                    body.as_object_mut().unwrap().remove("response_format");
                    compatibility_retry = true;
                }
                Err(error)
                    if matches!(
                        error.http_status,
                        Some(StatusCode::TOO_MANY_REQUESTS | StatusCode::BAD_GATEWAY)
                    ) && rate_limit_retries < MAX_IMAGE_RATE_LIMIT_RETRIES =>
                {
                    rate_limit_retries += 1;
                    let delay = image_retry_delay(error.retry_after, rate_limit_retries, stage);
                    let message = format!(
                        "接口返回 {}，等待 {} 秒后仅继续此未完成资源（第 {}/{} 次重试）",
                        error
                            .http_status
                            .map(|status| status.as_u16())
                            .unwrap_or_default(),
                        delay.as_secs_f32(),
                        rate_limit_retries,
                        MAX_IMAGE_RATE_LIMIT_RETRIES
                    );
                    self.record_request(
                        &format!("{stage}（等待续跑）"),
                        &endpoint,
                        &credentials.image_model,
                        "retrying",
                        error.http_status.map(|status| status.as_u16()),
                        delay,
                        &message,
                    );
                    tokio::time::sleep(delay).await;
                }
                Err(error) => return Err(error.error),
            }
        };
        let item = response
            .pointer("/data/0")
            .and_then(Value::as_object)
            .context("生图接口未返回 data[0]")?;
        let bytes = if let Some(encoded) = item.get("b64_json").and_then(Value::as_str) {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .context("生图接口返回的 base64 无效")?
        } else if let Some(url) = item.get("url").and_then(Value::as_str) {
            let url = Url::parse(url).context("生图接口返回的 URL 无效")?;
            if url.scheme() != "https" || !public_image_host(&url) {
                bail!("生图接口返回了不安全的 URL");
            }
            let response = self.client.get(url).send().await?.error_for_status()?;
            read_response_limited(response, MAX_IMAGE_BYTES).await?
        } else {
            bail!("生图接口既未返回 b64_json 也未返回 URL");
        };
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
            bail!("生成图片为空或超过 15 MB");
        }
        Ok(bytes)
    }

    async fn send_json(
        &self,
        endpoint: &Url,
        api_key: &str,
        body: &Value,
        max_response_bytes: usize,
        stage: &str,
        model: &str,
    ) -> Result<Value, ApiRequestError> {
        let _permit = self
            .request_semaphore
            .acquire()
            .await
            .map_err(|_| ApiRequestError::new(anyhow::anyhow!("AI 请求并发控制器已关闭")))?;
        let started = std::time::Instant::now();
        crate::diagnostic::log(
            "ai.request.started",
            json!({ "stage": stage, "endpoint": endpoint.as_str(), "model": model }),
        );
        let response = self
            .client
            .post(endpoint.clone())
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                let message = error.to_string();
                self.record_request(
                    stage,
                    endpoint,
                    model,
                    "error",
                    None,
                    started.elapsed(),
                    &message,
                );
                return Err(ApiRequestError::new(anyhow::anyhow!(
                    "{stage}请求失败：{endpoint}，模型 {model}：{message}"
                )));
            }
        };
        let status = response.status();
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = match read_response_limited(response, max_response_bytes).await {
            Ok(bytes) => bytes,
            Err(error) => {
                let message = format!("{error:#}");
                self.record_request(
                    stage,
                    endpoint,
                    model,
                    "error",
                    Some(status.as_u16()),
                    started.elapsed(),
                    &message,
                );
                return Err(ApiRequestError::new(
                    error.context(format!("{stage}请求失败：{endpoint}，模型 {model}")),
                ));
            }
        };
        let value = match parse_api_response(&bytes, &content_type) {
            Ok(value) => value,
            Err(error) => {
                let error = error.context(format!("AI 接口 {endpoint} 响应无效"));
                let message = format!("{error:#}");
                self.record_request(
                    stage,
                    endpoint,
                    model,
                    "error",
                    Some(status.as_u16()),
                    started.elapsed(),
                    &message,
                );
                return Err(ApiRequestError::new(
                    error.context(format!("{stage}请求失败，模型 {model}")),
                ));
            }
        };
        if !status.is_success() {
            let message = api_error_message(&value, status);
            let normalized = message.to_ascii_lowercase();
            let retry_without_json_mode = (body.get("response_format").is_some()
                || body.get("text").is_some())
                && matches!(
                    status,
                    StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
                )
                && (normalized.contains("response_format")
                    || normalized.contains("text.format")
                    || normalized.contains("json_object")
                    || normalized.contains("unsupported parameter")
                    || normalized.contains("unknown field")
                    || normalized.contains("extra input"));
            self.record_request(
                stage,
                endpoint,
                model,
                "error",
                Some(status.as_u16()),
                started.elapsed(),
                &message,
            );
            return Err(ApiRequestError {
                retry_without_json_mode,
                http_status: Some(status),
                retry_after,
                error: anyhow::anyhow!("{stage}请求失败：{endpoint}，模型 {model}：{message}"),
            });
        }
        self.record_request(
            stage,
            endpoint,
            model,
            "success",
            Some(status.as_u16()),
            started.elapsed(),
            "请求成功",
        );
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    fn record_request(
        &self,
        stage: &str,
        endpoint: &Url,
        model: &str,
        outcome: &str,
        http_status: Option<u16>,
        duration: Duration,
        message: &str,
    ) {
        let entry = AiRequestLog {
            timestamp_ms: unix_timestamp_ms(),
            stage: stage.into(),
            endpoint: endpoint.as_str().into(),
            model: model.into(),
            outcome: outcome.into(),
            http_status,
            duration_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
            message: message.chars().take(500).collect(),
        };
        crate::diagnostic::log("ai.request.finished", json!(entry));
        if let Ok(mut entries) = self.request_log.lock() {
            entries.push(entry);
            if entries.len() > 64 {
                let remove = entries.len() - 64;
                entries.drain(..remove);
            }
        }
    }

    fn reference_data_urls(&self, request: &GenerateRequest) -> anyhow::Result<Vec<String>> {
        let mut references = request.references.clone();
        match (&request.reference_session, &request.reference_path) {
            (Some(session), Some(path)) => references.push(ReferenceImage {
                session: session.clone(),
                path: path.clone(),
            }),
            (None, None) => {}
            _ => bail!("参考图会话参数不完整"),
        }
        if references.len() > 6 {
            bail!("参考图最多选择 6 张");
        }
        references
            .iter()
            .map(|reference| {
                validate_session(&reference.session)?;
                crate::theme::validate_image_asset_path(&reference.path)?;
                let root = self.paths.staging.canonicalize()?;
                let path = self
                    .paths
                    .staging
                    .join(&reference.session)
                    .join(&reference.path)
                    .canonicalize()
                    .context("参考图不存在")?;
                if !path.starts_with(&root) || !path.is_file() {
                    bail!("参考图路径无效");
                }
                let bytes = fs::read(&path)?;
                if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
                    bail!("参考图为空或超过 15 MB");
                }
                let mime = image_mime(&path)?;
                Ok(format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ))
            })
            .collect()
    }

    fn stage_generated_asset(
        &self,
        slot: GeneratedSlot,
        bytes: &[u8],
    ) -> anyhow::Result<GeneratedAsset> {
        let session = uuid::Uuid::new_v4().simple().to_string();
        let extension = generated_image_extension(bytes)?;
        let hash = format!("{:x}", Sha256::digest(bytes));
        let path = format!(
            "assets/ai-{}-{}.{}",
            slot.file_stem(),
            &hash[..16],
            extension
        );
        let target = self.paths.staging.join(&session).join(&path);
        fs::create_dir_all(target.parent().unwrap())?;
        atomic_write(&target, bytes)?;
        Ok(GeneratedAsset {
            slot: slot.asset_slot(),
            preview_url: self.reference_preview_url(&session, &path)?,
            session,
            path,
        })
    }

    fn stage_action_icon_atlas(
        &self,
        bytes: &[u8],
    ) -> anyhow::Result<(GeneratedAsset, Vec<(GeneratedSlot, GeneratedAsset)>)> {
        let atlas = self.stage_generated_asset(GeneratedSlot::ActionIconAtlas, bytes)?;
        let mut icons = Vec::with_capacity(ICON_ACTIONS.len());
        let result = (|| -> anyhow::Result<()> {
            let image = image::load_from_memory(bytes)
                .context("无法解码系统图标图集")?
                .to_rgba8();
            let cell_width = image.width() / 3;
            let cell_height = image.height() / 3;
            if cell_width < 128 || cell_height < 128 {
                bail!("系统图标图集尺寸过小");
            }
            for (index, &(action, _)) in ICON_ACTIONS.iter().enumerate() {
                let column = u32::try_from(index % 3).unwrap_or_default();
                let row = u32::try_from(index / 3).unwrap_or_default();
                let cell = image::imageops::crop_imm(
                    &image,
                    column * cell_width,
                    row * cell_height,
                    cell_width,
                    cell_height,
                )
                .to_image();
                let bytes = encode_png(&normalize_transparent_asset(cell, 512, 512)?)?;
                let slot = GeneratedSlot::ActionIcon(action);
                icons.push((slot, self.stage_generated_asset(slot, &bytes)?));
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.cleanup_generated_assets(std::slice::from_ref(&atlas));
            for (_, icon) in &icons {
                self.cleanup_generated_assets(std::slice::from_ref(icon));
            }
            return Err(error);
        }
        Ok((atlas, icons))
    }

    fn cleanup_generated_assets(&self, assets: &[GeneratedAsset]) {
        for asset in assets {
            let _ = fs::remove_dir_all(self.paths.staging.join(&asset.session));
        }
    }
}

struct ApiRequestError {
    retry_without_json_mode: bool,
    http_status: Option<StatusCode>,
    retry_after: Option<Duration>,
    error: anyhow::Error,
}

impl ApiRequestError {
    fn new(error: anyhow::Error) -> Self {
        Self {
            retry_without_json_mode: false,
            http_status: None,
            retry_after: None,
            error,
        }
    }
}

impl From<reqwest::Error> for ApiRequestError {
    fn from(error: reqwest::Error) -> Self {
        Self::new(error.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EndpointKind {
    Responses,
    Images,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum GeneratedSlot {
    BackgroundFullscreen,
    BackgroundContent,
    BackgroundSidebar,
    Logo,
    SidebarWatermark,
    HeroImage,
    HeroBadge,
    Avatar,
    Sticker,
    ComposerDecoration,
    Decoration(usize),
    ActionIconAtlas,
    ActionIcon(&'static str),
    HomeCardIcon(usize),
}

impl GeneratedSlot {
    fn requires_transparency(self) -> bool {
        !matches!(
            self,
            Self::BackgroundFullscreen
                | Self::BackgroundContent
                | Self::BackgroundSidebar
                | Self::HeroImage
        )
    }

    fn image_spec(
        self,
        background_ratio: &str,
        image_model: &str,
    ) -> anyhow::Result<(String, String)> {
        let ratio = match self {
            Self::BackgroundFullscreen | Self::BackgroundContent => {
                normalize_image_ratio(background_ratio)?
            }
            Self::BackgroundSidebar | Self::SidebarWatermark => "1:2".into(),
            Self::Logo | Self::HeroImage => "2:1".into(),
            Self::ComposerDecoration => "3:1".into(),
            Self::HeroBadge
            | Self::Avatar
            | Self::Sticker
            | Self::Decoration(_)
            | Self::ActionIconAtlas
            | Self::ActionIcon(_)
            | Self::HomeCardIcon(_) => "1:1".into(),
        };
        let requested_dimensions = match self {
            Self::BackgroundFullscreen | Self::BackgroundContent => {
                target_dimensions_for_ratio(&ratio, 1536)
            }
            Self::BackgroundSidebar | Self::SidebarWatermark => "768x1536".into(),
            Self::Logo => "1536x768".into(),
            Self::HeroImage => "1536x768".into(),
            Self::ComposerDecoration => "1536x512".into(),
            Self::ActionIconAtlas => "1536x1536".into(),
            Self::HeroBadge | Self::Avatar | Self::ActionIcon(_) | Self::HomeCardIcon(_) => {
                "1024x1024".into()
            }
            Self::Sticker | Self::Decoration(_) => "1024x1024".into(),
        };
        let dimensions = if uses_legacy_gpt_image_sizes(image_model) {
            standard_gpt_image_dimensions(&ratio).into()
        } else {
            requested_dimensions
        };
        Ok((dimensions, ratio))
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "background.fullscreen" => Self::BackgroundFullscreen,
            "background.content" => Self::BackgroundContent,
            "background.sidebar" => Self::BackgroundSidebar,
            "skin.logo" => Self::Logo,
            "skin.sidebarWatermark" => Self::SidebarWatermark,
            "skin.heroImage" => Self::HeroImage,
            "skin.heroBadge" => Self::HeroBadge,
            "skin.avatar" => Self::Avatar,
            "skin.sticker" => Self::Sticker,
            "skin.composerDecoration" => Self::ComposerDecoration,
            _ => return None,
        })
    }

    fn asset_slot(self) -> String {
        match self {
            Self::BackgroundFullscreen => "background.fullscreen".into(),
            Self::BackgroundContent => "background.content".into(),
            Self::BackgroundSidebar => "background.sidebar".into(),
            Self::Logo => "skin.logo".into(),
            Self::SidebarWatermark => "skin.sidebarWatermark".into(),
            Self::HeroImage => "skin.heroImage".into(),
            Self::HeroBadge => "skin.heroBadge".into(),
            Self::Avatar => "skin.avatar".into(),
            Self::Sticker => "skin.sticker".into(),
            Self::ComposerDecoration => "skin.composerDecoration".into(),
            Self::Decoration(index) => format!("skin.decorations.{index}.asset"),
            Self::ActionIconAtlas => "skin.icons.atlas".into(),
            Self::ActionIcon(action) => format!("skin.icons.{action}"),
            Self::HomeCardIcon(index) => format!("skin.home.cards.{index}.icon"),
        }
    }

    fn from_asset_slot(value: &str) -> Option<Self> {
        if value == "skin.icons.atlas" {
            return Some(Self::ActionIconAtlas);
        }
        if let Some(action) = value.strip_prefix("skin.icons.") {
            return ICON_ACTIONS.iter().find_map(|&(candidate, _)| {
                (candidate == action).then_some(Self::ActionIcon(candidate))
            });
        }
        if let Some(index) = value
            .strip_prefix("skin.decorations.")
            .and_then(|value| value.strip_suffix(".asset"))
            .and_then(|value| value.parse::<usize>().ok())
        {
            return (index == 0).then_some(Self::Decoration(index));
        }
        if let Some(index) = value
            .strip_prefix("skin.home.cards.")
            .and_then(|value| value.strip_suffix(".icon"))
            .and_then(|value| value.parse::<usize>().ok())
        {
            return (index < 4).then_some(Self::HomeCardIcon(index));
        }
        Self::parse(value)
    }

    fn file_stem(self) -> String {
        match self {
            Self::BackgroundFullscreen => "background-fullscreen".into(),
            Self::BackgroundContent => "background-content".into(),
            Self::BackgroundSidebar => "background-sidebar".into(),
            Self::Logo => "logo".into(),
            Self::SidebarWatermark => "sidebar-watermark".into(),
            Self::HeroImage => "hero".into(),
            Self::HeroBadge => "hero-badge".into(),
            Self::Avatar => "avatar".into(),
            Self::Sticker => "sticker".into(),
            Self::ComposerDecoration => "composer-decoration".into(),
            Self::Decoration(index) => format!("decoration-{}", index + 1),
            Self::ActionIconAtlas => "icon-atlas".into(),
            Self::ActionIcon(action) => format!("icon-{action}"),
            Self::HomeCardIcon(index) => format!("home-card-{}", index + 1),
        }
    }

    fn stage_label(self) -> String {
        match self {
            Self::BackgroundFullscreen => "全屏背景".into(),
            Self::BackgroundContent => "内容区背景".into(),
            Self::BackgroundSidebar => "侧栏背景".into(),
            Self::Logo => "Logo".into(),
            Self::SidebarWatermark => "侧栏水印".into(),
            Self::HeroImage => "Hero 图片".into(),
            Self::HeroBadge => "Hero 徽章".into(),
            Self::Avatar => "头像".into(),
            Self::Sticker => "角落贴纸".into(),
            Self::ComposerDecoration => "输入区装饰".into(),
            Self::Decoration(index) => format!("自定义装饰 {}", index + 1),
            Self::ActionIconAtlas => "同主题系统图标图集".into(),
            Self::ActionIcon(action) => format!("动作图标 {action}"),
            Self::HomeCardIcon(index) => format!("快捷卡片图标 {}", index + 1),
        }
    }
}

fn is_gpt_image_model(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("gpt-image-")
}

fn uses_legacy_gpt_image_sizes(model: &str) -> bool {
    is_gpt_image_model(model) && !model.trim().eq_ignore_ascii_case("gpt-image-2")
}

fn standard_gpt_image_dimensions(ratio: &str) -> &'static str {
    let (width, height) = ratio.split_once(':').unwrap_or(("1", "1"));
    let width = width.parse::<u32>().unwrap_or(1);
    let height = height.parse::<u32>().unwrap_or(1);
    if width > height {
        "1536x1024"
    } else if width < height {
        "1024x1536"
    } else {
        "1024x1024"
    }
}

fn post_process_generated_asset(slot: GeneratedSlot, bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    if !slot.requires_transparency() {
        return Ok(bytes.to_vec());
    }
    let image = image::load_from_memory(bytes)
        .context("无法解码待抠图的生成资源")?
        .to_rgba8();
    let (width, height) = match slot {
        GeneratedSlot::Logo => (768, 384),
        GeneratedSlot::SidebarWatermark => (384, 768),
        GeneratedSlot::ComposerDecoration => (768, 256),
        _ => (512, 512),
    };
    encode_png(&normalize_transparent_asset(image, width, height)?)
}

fn normalize_transparent_asset(
    mut image: RgbaImage,
    target_width: u32,
    target_height: u32,
) -> anyhow::Result<RgbaImage> {
    let pixel_count = image.width() as usize * image.height() as usize;
    let mut strengths = vec![0_u8; pixel_count];
    let mut hard_keyed = vec![false; pixel_count];
    for (index, pixel) in image.pixels().enumerate() {
        let [red, green, blue, _] = pixel.0;
        hard_keyed[index] = green >= 240 && red <= 16 && blue <= 16;
        let dominance = green.saturating_sub(red.max(blue));
        if hard_keyed[index] {
            strengths[index] = 255;
        } else if green > 80 && dominance > 28 {
            strengths[index] = ((u16::from(dominance - 28) * 255 / 150).min(255)) as u8;
        }
    }
    let mut keyed = vec![false; pixel_count];
    let mut queue = VecDeque::new();
    for x in 0..image.width() {
        queue.push_back((x, 0));
        queue.push_back((x, image.height() - 1));
    }
    for y in 0..image.height() {
        queue.push_back((0, y));
        queue.push_back((image.width() - 1, y));
    }
    while let Some((x, y)) = queue.pop_front() {
        let index = y as usize * image.width() as usize + x as usize;
        if keyed[index] || strengths[index] < 12 {
            continue;
        }
        keyed[index] = true;
        if x > 0 {
            queue.push_back((x - 1, y));
        }
        if x + 1 < image.width() {
            queue.push_back((x + 1, y));
        }
        if y > 0 {
            queue.push_back((x, y - 1));
        }
        if y + 1 < image.height() {
            queue.push_back((x, y + 1));
        }
    }
    for (keyed, hard_keyed) in keyed.iter_mut().zip(hard_keyed) {
        *keyed |= hard_keyed;
    }
    let keyed_count = keyed.iter().filter(|&&value| value).count();
    if keyed_count * 20 < pixel_count {
        bail!("生成资源没有可识别的纯绿背景，请重试此资源");
    }
    let mut opaque = 0usize;
    let mut transparent = 0usize;
    for (index, pixel) in image.pixels_mut().enumerate() {
        let [red, green, blue, source_alpha] = pixel.0;
        let chroma_alpha = if keyed[index] {
            255 - strengths[index]
        } else {
            255
        };
        let alpha = ((u16::from(source_alpha) * u16::from(chroma_alpha)) / 255) as u8;
        if alpha > 18 {
            opaque += 1;
        } else {
            transparent += 1;
        }
        if alpha < 250 && green > red.max(blue) {
            let neutral_green = red.max(blue);
            pixel.0 = [red, neutral_green, blue, alpha];
        } else {
            pixel.0[3] = alpha;
        }
    }
    let total = image.width() as usize * image.height() as usize;
    if transparent * 20 < total {
        bail!("绿幕抠图后没有生成有效透明背景，请重试此资源");
    }
    if opaque == 0 || opaque * 100 < total {
        bail!("绿幕抠图后没有检测到有效主体");
    }
    let bounds = alpha_bounds(&image).context("绿幕抠图后没有检测到主体")?;
    let cropped = image::imageops::crop_imm(
        &image,
        bounds.0,
        bounds.1,
        bounds.2 - bounds.0 + 1,
        bounds.3 - bounds.1 + 1,
    )
    .to_image();
    let padding = ((target_width.min(target_height) as f32) * 0.09).round() as u32;
    let available_width = target_width.saturating_sub(padding * 2).max(1);
    let available_height = target_height.saturating_sub(padding * 2).max(1);
    let scale = (available_width as f32 / cropped.width() as f32)
        .min(available_height as f32 / cropped.height() as f32);
    let width = (cropped.width() as f32 * scale).round().max(1.0) as u32;
    let height = (cropped.height() as f32 * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(&cropped, width, height, FilterType::Lanczos3);
    let mut canvas = RgbaImage::new(target_width, target_height);
    image::imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((target_width - width) / 2),
        i64::from((target_height - height) / 2),
    );
    Ok(canvas)
}

fn alpha_bounds(image: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut bounds = (image.width(), image.height(), 0, 0);
    let mut found = false;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel.0[3] <= 18 {
            continue;
        }
        found = true;
        bounds.0 = bounds.0.min(x);
        bounds.1 = bounds.1.min(y);
        bounds.2 = bounds.2.max(x);
        bounds.3 = bounds.3.max(y);
    }
    found.then_some(bounds)
}

fn encode_png(image: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut output = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut output, ImageFormat::Png)
        .context("无法编码透明 PNG")?;
    Ok(output.into_inner())
}

fn icon_generation_plans(blueprint: &ThemeBlueprint) -> Vec<(GeneratedSlot, String)> {
    let style = if blueprint.icon_style.trim().is_empty() {
        "clean geometric product iconography with consistent stroke weight"
    } else {
        blueprint.icon_style.trim()
    };
    let palette = &blueprint.palette;
    let common = format!(
        "Create one centered standalone icon from a cohesive desktop IDE icon set. \
         Theme: {}. Style direction: {}. Use {} as the main icon color with {} accents. \
         Use a flat pure green chroma key background (#00FF00) with no shadows touching the edge. \
         Never use transparent-looking checkerboard, white, gray, or a framed square background. Simple recognizable silhouette, consistent stroke weight, generous padding, no border frame.",
        blueprint.name.trim(),
        style,
        palette.foreground,
        palette.accent
    );
    let mut plans = Vec::new();
    plans.extend(blueprint.home.cards.iter().enumerate().map(|(index, card)| {
        (
            GeneratedSlot::HomeCardIcon(index),
            format!(
                "{common} This is a shortcut card icon for the action titled '{}'. Meaning: {}.",
                card.title.trim(),
                card.description.trim()
            ),
        )
    }));
    plans
}

fn action_icon_atlas_prompt(blueprint: &ThemeBlueprint) -> String {
    let meanings = ICON_ACTIONS
        .iter()
        .enumerate()
        .map(|(index, (_, meaning))| format!("cell {}: {meaning}", index + 1))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "Create a precise 3 by 3 sprite atlas for a cohesive desktop IDE action icon set. \
         Use exactly nine equal cells in row-major order: {meanings}. Theme: {}. Style: {}. \
         Every cell contains one centered icon with identical scale, stroke weight and generous padding. \
         The entire canvas and every gap must be solid pure chroma green #00FF00. \
         No text, labels, dividers, frames, shadows, gradients, checkerboard or extra objects.",
        blueprint.name.trim(),
        if blueprint.icon_style.trim().is_empty() {
            "clean geometric product iconography"
        } else {
            blueprint.icon_style.trim()
        }
    )
}

fn sidebar_watermark_prompt(blueprint: &ThemeBlueprint) -> String {
    format!(
        "Create one subtle decorative sidebar watermark for the '{}' desktop IDE theme. \
         Style: {}. Keep the subject entirely inside the lower 35 percent of the canvas with \
         generous empty space above so navigation labels remain unobstructed. Use a simple \
         low-detail silhouette with no text, frame, full-height stripe, border, shadow or glow.",
        blueprint.name.trim(),
        if blueprint.icon_style.trim().is_empty() {
            "cohesive understated theme ornament"
        } else {
            blueprint.icon_style.trim()
        }
    )
}

fn generation_fingerprint(
    request: &GenerateRequest,
    references: &[String],
    credentials: &AiCredentials,
) -> String {
    let mut hash = Sha256::new();
    for value in [
        request.prompt.trim(),
        request.language.trim(),
        &credentials.base_url,
        credentials.effective_image_base_url(),
        &credentials.text_model,
        &credentials.vision_model,
        &credentials.image_model,
        &credentials.image_size,
        if request.generate_images {
            "images"
        } else {
            "theme-only"
        },
        if request.generate_sidebar_watermark {
            "sidebar-watermark"
        } else {
            "no-sidebar-watermark"
        },
        if request.generate_system_icons {
            "system-icons"
        } else {
            "native-icons"
        },
    ] {
        hash.update(value.as_bytes());
        hash.update([0]);
    }
    for reference in references {
        hash.update(reference.as_bytes());
        hash.update([0]);
    }
    for plan in &request.resource_plans {
        hash.update(plan.slot.as_bytes());
        hash.update([0]);
        hash.update(plan.prompt.as_bytes());
        hash.update([0]);
    }
    format!("{:x}", hash.finalize())
}

fn validate_resource_plans(plans: &[ResourcePlan]) -> anyhow::Result<()> {
    if plans.len() > 16 {
        bail!("自定义生成资源不能超过 16 个");
    }
    let mut slots = BTreeSet::new();
    for plan in plans {
        if GeneratedSlot::from_asset_slot(&plan.slot).is_none() {
            bail!("不支持的资源槽：{}", plan.slot);
        }
        if plan.prompt.trim().is_empty() || plan.prompt.chars().count() > 2_000 {
            bail!("资源提示词为空或过长");
        }
        if !slots.insert(plan.slot.as_str()) {
            bail!("自定义生成资源槽重复");
        }
    }
    Ok(())
}

fn merge_resource_plans(plans: &mut Vec<(GeneratedSlot, String)>, custom: &[ResourcePlan]) {
    for plan in custom {
        let Some(slot) = GeneratedSlot::from_asset_slot(&plan.slot) else {
            continue;
        };
        if let Some(existing) = plans.iter_mut().find(|(candidate, _)| *candidate == slot) {
            existing.1 = plan.prompt.trim().into();
        } else {
            plans.push((slot, plan.prompt.trim().into()));
        }
    }
}

fn theme_from_blueprint(blueprint: &ThemeBlueprint) -> anyhow::Result<ThemeManifest> {
    validate_blueprint(blueprint)?;
    let mut theme = match blueprint.base_mode {
        BlueprintMode::Light => ThemeManifest::neutral_light(),
        BlueprintMode::Dark => ThemeManifest::neutral_dark(),
    };
    theme.id = format!("local.ai-{}", uuid::Uuid::new_v4().simple());
    theme.name = blueprint.name.trim().to_string();
    theme.version = "0.1.0".into();
    theme.author = "AI + Local user".into();
    theme.description = if blueprint.description.trim().is_empty() {
        "AI 生成的 Theme Inject 主题".into()
    } else {
        blueprint.description.trim().to_string()
    };
    theme.base_mode = match blueprint.base_mode {
        BlueprintMode::Light => BaseMode::Light,
        BlueprintMode::Dark => BaseMode::Dark,
    };
    let palette = normalize_blueprint_palette(blueprint)?;
    theme.tokens.app_background = palette.app.clone();
    theme.tokens.sidebar_background = palette.sidebar.clone();
    theme.tokens.content_background = palette.content.clone();
    theme.tokens.elevated_background = palette.elevated.clone();
    theme.tokens.input_background = palette.input.clone();
    theme.tokens.foreground = palette.foreground.clone();
    theme.tokens.muted_foreground = palette.muted.clone();
    let surfaces = palette.surface_colors();
    theme.tokens.subtle_foreground = color_toward_until(
        &mix_hex(&palette.muted, &palette.content, 0.18)?,
        &palette.muted,
        |candidate| contrasts_with_all(candidate, &surfaces, 3.0),
    )?;
    theme.tokens.border = palette.border.clone();
    theme.tokens.accent = palette.accent.clone();
    theme.tokens.accent_foreground = palette.accent_foreground.clone();
    theme.tokens.selection = mix_hex(&palette.accent, &palette.content, 0.32)?;
    theme.tokens.link = palette.accent.clone();
    theme.tokens.code_background = palette.code_background.clone();
    theme.tokens.code_foreground = palette.code_foreground.clone();
    theme.tokens.hover_background = safe_hover_color(&palette.sidebar, &palette.foreground)?;
    theme.tokens.active_background = mix_hex(&palette.sidebar, &palette.accent, 0.18)?;
    theme.terminal.background = theme.tokens.code_background.clone();
    theme.terminal.foreground = theme.tokens.code_foreground.clone();
    theme.terminal.cursor = color_toward_until(
        &theme.tokens.accent,
        &theme.tokens.code_foreground,
        |candidate| {
            color_contrast(candidate, &theme.tokens.code_background).map(|ratio| ratio >= 3.0)
        },
    )?;
    theme.terminal.selection = mix_hex(&palette.accent, &palette.code_background, 0.28)?;
    theme.chrome.titlebar_background = Some(theme.tokens.app_background.clone());
    theme.chrome.titlebar_foreground = Some(theme.tokens.foreground.clone());
    theme.chrome.titlebar_muted_foreground = Some(theme.tokens.muted_foreground.clone());
    theme.chrome.titlebar_border = Some(border_for_surface(
        &theme.tokens.border,
        &theme.tokens.app_background,
        &theme.tokens.foreground,
    )?);
    theme.chrome.titlebar_hover = Some(safe_hover_color(
        &theme.tokens.app_background,
        &theme.tokens.foreground,
    )?);
    theme.chrome.sidebar_foreground = Some(theme.tokens.foreground.clone());
    theme.chrome.sidebar_muted_foreground = Some(theme.tokens.muted_foreground.clone());
    theme.chrome.sidebar_icon = Some(theme.tokens.foreground.clone());
    theme.chrome.sidebar_border = Some(border_for_surface(
        &theme.tokens.border,
        &theme.tokens.sidebar_background,
        &theme.tokens.foreground,
    )?);
    theme.chrome.sidebar_hover = Some(safe_hover_color(
        &theme.tokens.sidebar_background,
        &theme.tokens.foreground,
    )?);
    theme.chrome.sidebar_active = Some(theme.tokens.active_background.clone());
    theme.chrome.sidebar_active_foreground = Some(accessible_foreground_for_surface(
        &theme.tokens.foreground,
        &theme.tokens.active_background,
        4.5,
    )?);
    theme.layout.density = blueprint.style.density.clamp(0.75, 1.35);
    theme.layout.sidebar_width = blueprint.style.sidebar_width.clamp(180, 520);
    theme.layout.content_max_width = blueprint.style.content_max_width.clamp(480, 2400);
    theme.shape.radius = blueprint.style.radius.min(40);
    theme.effects.panel_opacity = blueprint.style.surface_opacity.clamp(0.25, 1.0);
    theme.effects.background_blur = blueprint.style.blur.min(6);
    theme.skin.enabled = true;
    theme.skin.brand.title = blueprint.brand.title.trim().to_string();
    theme.skin.brand.subtitle = blueprint.brand.subtitle.trim().to_string();
    theme.skin.icons.mode = IconMode::Recolor;
    theme.skin.icons.color = Some(theme.tokens.foreground.clone());
    theme.skin.icons.active_color = Some(theme.tokens.accent.clone());
    for surface in [
        &mut theme.skin.surfaces.titlebar,
        &mut theme.skin.surfaces.sidebar,
        &mut theme.skin.surfaces.content,
        &mut theme.skin.surfaces.hero,
        &mut theme.skin.surfaces.card,
        &mut theme.skin.surfaces.composer,
        &mut theme.skin.surfaces.popover,
    ] {
        surface.radius = blueprint.style.radius.min(48);
        surface.blur = blueprint.style.blur.min(12);
        surface.opacity = blueprint.style.surface_opacity.clamp(0.25, 1.0);
        surface.border_color = Some(theme.tokens.border.clone());
        surface.border_width = 1;
        surface.shadow_color = Some("#000000".into());
        surface.shadow_opacity = 0.18;
    }
    theme.skin.surfaces.titlebar.fill = Some(theme.tokens.app_background.clone());
    theme.skin.surfaces.sidebar.fill = Some(theme.tokens.sidebar_background.clone());
    theme.skin.surfaces.content.fill = Some(theme.tokens.content_background.clone());
    theme.skin.surfaces.hero.fill = Some(theme.tokens.elevated_background.clone());
    theme.skin.surfaces.card.fill = Some(theme.tokens.elevated_background.clone());
    theme.skin.surfaces.composer.fill = Some(theme.tokens.input_background.clone());
    theme.skin.surfaces.popover.fill = Some(theme.tokens.elevated_background.clone());
    theme.skin.home.enabled = true;
    theme.skin.home.title = if blueprint.home.title.trim().is_empty() {
        "今天想构建什么？".into()
    } else {
        blueprint.home.title.trim().to_string()
    };
    theme.skin.home.subtitle = blueprint.home.subtitle.trim().to_string();
    theme.skin.home.hero_height = blueprint.style.hero_height.clamp(160, 720);
    theme.skin.home.card_columns = blueprint.home.cards.len().clamp(1, 4) as u8;
    theme.skin.home.cards = blueprint
        .home
        .cards
        .iter()
        .take(4)
        .map(|card| HomeCard {
            title: card.title.trim().to_string(),
            description: card.description.trim().to_string(),
            prompt: card.prompt.trim().to_string(),
            icon: None,
        })
        .collect();
    match blueprint.background {
        BlueprintBackground::None => {}
        BlueprintBackground::Solid => {
            theme.background.fullscreen = solid_background(&theme.tokens.app_background, 0.92);
        }
        BlueprintBackground::Gradient => {
            theme.background.fullscreen =
                gradient_background(&theme.tokens.app_background, &theme.tokens.accent, 0.82);
        }
        BlueprintBackground::Image => {
            theme.background.fullscreen =
                gradient_background(&theme.tokens.app_background, &theme.tokens.accent, 0.72);
        }
        BlueprintBackground::PerRegion => {
            theme.background.mode = BackgroundMode::PerRegion;
            theme.background.content =
                gradient_background(&theme.tokens.content_background, &theme.tokens.accent, 0.72);
            theme.background.sidebar = solid_background(&theme.tokens.sidebar_background, 0.86);
        }
    }
    theme.validate()?;
    Ok(theme)
}

fn apply_generated_slot(
    theme: &mut ThemeManifest,
    slot: GeneratedSlot,
    path: &str,
) -> anyhow::Result<()> {
    match slot {
        GeneratedSlot::BackgroundFullscreen => {
            theme.background.mode = BackgroundMode::Fullscreen;
            theme.background.fullscreen = image_background(path, 0.34);
        }
        GeneratedSlot::BackgroundContent => {
            theme.background.mode = BackgroundMode::PerRegion;
            theme.background.content = image_background(path, 0.38);
        }
        GeneratedSlot::BackgroundSidebar => {
            theme.background.mode = BackgroundMode::PerRegion;
            theme.background.sidebar = image_background(path, 0.48);
        }
        GeneratedSlot::Logo => theme.skin.resources.logo = Some(path.into()),
        GeneratedSlot::SidebarWatermark => {
            theme.skin.resources.sidebar_watermark = Some(path.into())
        }
        GeneratedSlot::HeroImage => theme.skin.resources.hero_image = Some(path.into()),
        GeneratedSlot::HeroBadge => theme.skin.resources.hero_badge = Some(path.into()),
        GeneratedSlot::Avatar => theme.skin.resources.avatar = Some(path.into()),
        GeneratedSlot::Sticker => theme.skin.resources.sticker = Some(path.into()),
        GeneratedSlot::ComposerDecoration => {
            theme.skin.resources.composer_decoration = Some(path.into())
        }
        GeneratedSlot::Decoration(index) => {
            if index >= 16 {
                bail!("AI 生成的装饰层索引无效");
            }
            while theme.skin.decorations.len() <= index {
                theme.skin.decorations.push(Decoration {
                    asset: String::new(),
                    region: DecorationRegion::Content,
                    anchor: DecorationAnchor::BottomRight,
                    offset_x: 24,
                    offset_y: 24,
                    width: 180,
                    opacity: 1.0,
                    layer: 0,
                    hidden_below: 0,
                });
            }
            theme.skin.decorations[index].asset = path.into();
        }
        GeneratedSlot::ActionIconAtlas => bail!("系统图标图集必须先切割"),
        GeneratedSlot::ActionIcon(action) => {
            theme.skin.icons.mode = IconMode::Custom;
            theme.skin.icons.mappings.insert(action.into(), path.into());
        }
        GeneratedSlot::HomeCardIcon(index) => {
            let card = theme
                .skin
                .home
                .cards
                .get_mut(index)
                .context("AI 生成的快捷卡片图标索引无效")?;
            card.icon = Some(path.into());
        }
    }
    theme.validate()
}

fn validate_blueprint(blueprint: &ThemeBlueprint) -> anyhow::Result<()> {
    if blueprint.name.trim().is_empty() || blueprint.name.chars().count() > 80 {
        bail!("AI 生成的主题名称为空或过长");
    }
    if blueprint.description.chars().count() > 240
        || blueprint.brand.title.chars().count() > 80
        || blueprint.brand.subtitle.chars().count() > 160
        || blueprint.home.title.chars().count() > 120
        || blueprint.home.subtitle.chars().count() > 240
        || blueprint.icon_style.chars().count() > 500
        || blueprint.home.cards.len() > 4
        || blueprint.assets.len() > MAX_GENERATED_ASSETS
    {
        bail!("AI 生成的主题文本或项目数量超过限制");
    }
    for color in [
        &blueprint.palette.app,
        &blueprint.palette.sidebar,
        &blueprint.palette.content,
        &blueprint.palette.elevated,
        &blueprint.palette.input,
        &blueprint.palette.foreground,
        &blueprint.palette.muted,
        &blueprint.palette.border,
        &blueprint.palette.accent,
        &blueprint.palette.accent_foreground,
        &blueprint.palette.code_background,
        &blueprint.palette.code_foreground,
    ] {
        validate_blueprint_color(color)?;
    }
    for card in &blueprint.home.cards {
        if card.title.chars().count() > 80
            || card.description.chars().count() > 160
            || card.prompt.chars().count() > 4_000
        {
            bail!("AI 生成的快捷卡片内容超过限制");
        }
    }
    let mut slots = BTreeSet::new();
    for asset in &blueprint.assets {
        if GeneratedSlot::parse(&asset.slot).is_none()
            || asset.prompt.trim().is_empty()
            || asset.prompt.chars().count() > 2_000
        {
            bail!("AI 生成的图片资源计划无效");
        }
        if !slots.insert(asset.slot.as_str()) {
            bail!("AI 生成的图片资源槽重复");
        }
    }
    if slots.contains("background.fullscreen")
        && (slots.contains("background.content") || slots.contains("background.sidebar"))
    {
        bail!("AI 生成的全屏背景与分区背景不能同时存在");
    }
    Ok(())
}

fn validate_prompt(prompt: &str) -> anyhow::Result<()> {
    let prompt = prompt.trim();
    if prompt.is_empty() || prompt.chars().count() > MAX_PROMPT_CHARS {
        bail!("主题描述不能为空且不能超过 {MAX_PROMPT_CHARS} 个字符");
    }
    Ok(())
}

fn default_theme_language() -> String {
    "zh-CN".into()
}

fn validate_theme_language(language: &str) -> anyhow::Result<()> {
    if !matches!(
        language.trim(),
        "zh-CN" | "zh-TW" | "en-US" | "ja-JP" | "ko-KR"
    ) {
        bail!("不支持的主题界面语言");
    }
    Ok(())
}

fn theme_language_name(language: &str) -> &'static str {
    match language.trim() {
        "zh-TW" => "Traditional Chinese",
        "en-US" => "English",
        "ja-JP" => "Japanese",
        "ko-KR" => "Korean",
        _ => "Simplified Chinese",
    }
}

fn validate_public_settings(settings: &AiPublicSettings) -> anyhow::Result<()> {
    validate_endpoint_base(&settings.base_url)?;
    if !settings.image_base_url.trim().is_empty() {
        validate_endpoint_base(&settings.image_base_url)?;
    }
    for (value, label) in [
        (&settings.text_model, "基础模型"),
        (&settings.vision_model, "视觉模型"),
        (&settings.image_model, "生图模型"),
    ] {
        let value = value.trim();
        if value.is_empty() || value.len() > 200 || value.contains(['\0', '\r', '\n']) {
            bail!("{label}名称无效");
        }
    }
    normalize_image_ratio(&settings.image_size)?;
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationProgress {
    pub state: String,
    pub message: String,
    pub items: Vec<GenerationProgressItem>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationProgressItem {
    pub slot: String,
    pub label: String,
    pub prompt: String,
    pub status: String,
    pub preview_url: String,
    pub session: String,
    pub path: String,
    pub message: String,
}

fn normalize_image_ratio(value: &str) -> anyhow::Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if value == "auto" {
        return Ok("3:2".into());
    }
    let (width, height) = value
        .split_once(':')
        .or_else(|| value.split_once('x'))
        .context("图片比例格式无效，请使用 3:2 或 16:9")?;
    let width = width
        .parse::<u32>()
        .context("图片比例格式无效，请使用 3:2 或 16:9")?;
    let height = height
        .parse::<u32>()
        .context("图片比例格式无效，请使用 3:2 或 16:9")?;
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        bail!("图片比例格式无效，请使用 3:2 或 16:9");
    }
    if width > height.saturating_mul(3) || height > width.saturating_mul(3) {
        bail!("图片比例不能超过 3:1 或 1:3");
    }
    let divisor = greatest_common_divisor(width, height);
    Ok(format!("{}:{}", width / divisor, height / divisor))
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn target_dimensions_for_ratio(ratio: &str, longest_edge: u32) -> String {
    let (width, height) = ratio.split_once(':').unwrap_or(("1", "1"));
    let width = width.parse::<u32>().unwrap_or(1);
    let height = height.parse::<u32>().unwrap_or(1);
    let minimum_short_edge = (longest_edge / 3).max(16);
    let (target_width, target_height) = if width >= height {
        (
            longest_edge,
            round_to_multiple((longest_edge * height / width).max(minimum_short_edge), 16),
        )
    } else {
        (
            round_to_multiple((longest_edge * width / height).max(minimum_short_edge), 16),
            longest_edge,
        )
    };
    format!("{target_width}x{target_height}")
}

fn round_to_multiple(value: u32, multiple: u32) -> u32 {
    ((value + multiple / 2) / multiple).max(1) * multiple
}

fn image_request_body(
    credentials: &AiCredentials,
    slot: GeneratedSlot,
    prompt: &str,
) -> anyhow::Result<Value> {
    let (target_dimensions, aspect_ratio) =
        slot.image_spec(&credentials.image_size, &credentials.image_model)?;
    let transparent_asset = slot.requires_transparency();
    let background_direction = if transparent_asset {
        "Render the subject over a perfectly flat, uniform pure chroma green background (#00FF00). The green must reach every canvas edge with no texture, gradient, vignette, shadow, glow, antialias halo, checkerboard, fake transparency, white box or gray box. Keep the subject fully separated from the background for deterministic chroma-key removal."
    } else {
        "Fill the complete canvas naturally. Do not add a frame, checkerboard, fake transparency, watermark or readable text."
    };
    let mut body = json!({
        "model": credentials.image_model,
        "prompt": format!("Create a polished visual asset for a desktop coding application theme. Target canvas dimensions: {target_dimensions} pixels. Keep the composition at a {aspect_ratio} aspect ratio. Background and UI ambience assets must be crisp and inspectable, not blurred or foggy; use clear shapes, readable contrast, and low haze. Do not design or cover the app title bar, native task header, send button, settings controls, or window controls. No readable text, no UI screenshot, no watermark. User art direction: {prompt}. Mandatory output constraint: {background_direction}"),
        "n": 1,
        "size": target_dimensions
    });
    if !is_gpt_image_model(&credentials.image_model) {
        body["response_format"] = json!("b64_json");
    } else {
        body["background"] = json!("opaque");
        body["output_format"] = json!("png");
    }
    Ok(body)
}

fn validate_api_key(value: &str, label: &str) -> anyhow::Result<()> {
    if value.len() > 8_192 || value.contains(['\0', '\r', '\n']) {
        bail!("{label} 格式无效");
    }
    Ok(())
}

fn validate_stored_settings(settings: &StoredAiSettings) -> anyhow::Result<()> {
    validate_public_settings(&AiPublicSettings {
        base_url: settings.base_url.clone(),
        image_base_url: settings.image_base_url.clone(),
        text_model: settings.text_model.clone(),
        vision_model: settings.vision_model.clone(),
        image_model: settings.image_model.clone(),
        image_size: settings.image_size.clone(),
        has_api_key: settings.encrypted_api_key.is_some(),
        has_image_api_key: settings.encrypted_image_api_key.is_some(),
    })
}

fn validate_endpoint_base(value: &str) -> anyhow::Result<Url> {
    let url = Url::parse(value.trim()).context("API 地址不是有效 URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || value.len() > 2_048
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("API 地址必须是不含账号、密码、查询参数或片段的 HTTP(S) URL");
    }
    Ok(url)
}

fn endpoint(base: &str, kind: EndpointKind) -> anyhow::Result<Url> {
    let mut url = validate_endpoint_base(base)?;
    let current = url.path().trim_end_matches('/');
    // Most OpenAI-compatible providers expose their API below /v1 while their
    // origin root serves a marketing website. Treat a bare origin as /v1.
    let current = if current.is_empty() { "/v1" } else { current };
    let suffix = match kind {
        EndpointKind::Responses => "responses",
        EndpointKind::Images => "images/generations",
    };
    let path = if current.ends_with("/responses")
        || current.ends_with("/chat/completions")
        || current.ends_with("/images/generations")
    {
        let prefix = current
            .trim_end_matches("/responses")
            .trim_end_matches("/chat/completions")
            .trim_end_matches("/images/generations");
        format!("{prefix}/{suffix}")
    } else {
        format!("{current}/{suffix}")
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn parse_api_response(bytes: &[u8], content_type: &str) -> anyhow::Result<Value> {
    if bytes.is_empty() || bytes.iter().all(u8::is_ascii_whitespace) {
        bail!("AI 接口返回了空响应");
    }
    if let Ok(value) = serde_json::from_slice(bytes) {
        return Ok(value);
    }

    let text = String::from_utf8_lossy(bytes);
    if content_type
        .to_ascii_lowercase()
        .contains("text/event-stream")
        || text
            .lines()
            .any(|line| line.trim_start().starts_with("data:"))
    {
        return parse_sse_response(&text);
    }

    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if content_type.to_ascii_lowercase().contains("text/html")
        || trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
    {
        bail!("AI 接口返回了 HTML 页面；请检查 API 地址，根域名通常需要添加 /v1");
    }
    let preview = response_preview(&text);
    bail!(
        "AI 接口返回的不是 JSON（Content-Type: {}）：{}",
        if content_type.is_empty() {
            "未知"
        } else {
            content_type
        },
        preview
    )
}

fn parse_sse_response(text: &str) -> anyhow::Result<Value> {
    let mut last = None;
    let mut content = String::new();
    for data in text
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("data:"))
        .map(str::trim)
        .filter(|data| !data.is_empty() && *data != "[DONE]")
    {
        let value: Value = serde_json::from_str(data)
            .with_context(|| format!("AI 接口返回了无效的 SSE JSON：{}", response_preview(data)))?;
        if let Some(delta) = value
            .pointer("/choices/0/delta/content")
            .and_then(Value::as_str)
        {
            content.push_str(delta);
        }
        if value.get("type").and_then(Value::as_str) == Some("response.output_text.delta")
            && let Some(delta) = value.get("delta").and_then(Value::as_str)
        {
            content.push_str(delta);
        }
        last = Some(value);
    }
    if !content.is_empty() {
        return Ok(json!({
            "output": [{ "type": "message", "content": [{ "type": "output_text", "text": content }] }]
        }));
    }
    last.context("AI 接口返回的 SSE 响应没有 data JSON")
}

fn response_preview(text: &str) -> String {
    let preview = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut preview = preview.chars().take(240).collect::<String>();
    if text.chars().count() > 240 {
        preview.push('…');
    }
    if preview.is_empty() {
        "<空>".into()
    } else {
        preview
    }
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    let seconds = value.trim().parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds.clamp(1, 5 * 60)))
}

fn image_retry_delay(retry_after: Option<Duration>, attempt: usize, stage: &str) -> Duration {
    let base = retry_after.unwrap_or_else(|| {
        Duration::from_secs((15_u64.saturating_mul(attempt as u64)).clamp(15, 60))
    });
    let jitter = stage.bytes().fold(0_u64, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u64::from(byte))
    }) % 5;
    base.saturating_add(Duration::from_secs(jitter))
}

fn unix_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn responses_content(response: &Value) -> anyhow::Result<String> {
    if let Some(text) = response.get("output_text").and_then(Value::as_str)
        && !text.is_empty()
    {
        return Ok(text.into());
    }
    let text = response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|part| {
            matches!(
                part.get("type").and_then(Value::as_str),
                Some("output_text" | "text")
            )
        })
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("");
    if text.is_empty() {
        bail!("Responses API 响应缺少 output[].content[].output_text")
    }
    Ok(text)
}

fn extract_json_object(content: &str) -> anyhow::Result<String> {
    let trimmed = content.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Ok(trimmed.to_string());
    }
    if let Some(start) = trimmed.find('{')
        && let Some(end) = trimmed.rfind('}')
        && end > start
    {
        return Ok(trimmed[start..=end].to_string());
    }
    bail!("基础模型没有返回 JSON 对象")
}

fn api_error_message(value: &Value, status: StatusCode) -> String {
    let message = value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("未知错误");
    format!("AI 接口返回 HTTP {}：{}", status.as_u16(), message)
}

async fn read_response_limited(
    response: reqwest::Response,
    max_bytes: usize,
) -> anyhow::Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        bail!("AI 接口响应超过允许大小");
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if body.len().saturating_add(chunk.len()) > max_bytes {
            bail!("AI 接口响应超过允许大小");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn blueprint_system_prompt() -> &'static str {
    r##"You are a desktop IDE theme art director. Return one JSON object only. Never include markdown or CSS.
Schema:
{
  "name":"max 80 chars", "description":"max 240 chars", "baseMode":"light|dark",
  "palette":{"app":"#RRGGBB","sidebar":"#RRGGBB","content":"#RRGGBB","elevated":"#RRGGBB","input":"#RRGGBB","foreground":"#RRGGBB","muted":"#RRGGBB","border":"#RRGGBB","accent":"#RRGGBB","accentForeground":"#RRGGBB","codeBackground":"#RRGGBB","codeForeground":"#RRGGBB"},
  "brand":{"title":"max 80 chars","subtitle":"max 160 chars"},
  "home":{"title":"max 120 chars","subtitle":"max 240 chars","cards":[{"title":"max 80 chars","description":"max 160 chars","prompt":"safe coding prompt"}]},
  "style":{"radius":0-40,"density":0.75-1.35,"sidebarWidth":180-520,"contentMaxWidth":480-2400,"heroHeight":160-720,"surfaceOpacity":0.25-0.68,"blur":0-12},
  "background":"none|solid|gradient|image|per-region",
  "iconStyle":"max 500 chars, concise English direction for a cohesive transparent-background IDE icon set",
  "assets":[{"slot":"allowed slot","prompt":"English visual prompt, no text or watermark"}]
}
Allowed image slots: background.fullscreen, background.content, background.sidebar, skin.logo, skin.heroImage, skin.heroBadge, skin.avatar, skin.sticker, skin.composerDecoration. Do not include skin.sidebarWatermark unless the user explicitly asks for a watermark; the application owns that opt-in.
Use strict valid JSON syntax with double-quoted property names and strings. Every palette value must be exactly seven ASCII characters in #RRGGBB form; never emit shorthand, named colors, rgb(), alpha, #RRGGBBAA, transparent, or gradients in palette fields. Use the language explicitly requested by the user for every user-facing name, description, brand string, home title, subtitle, and card string. Use at most 4 cards and at most 6 assets. Prefer skin.heroImage plus only assets that materially improve the requested design. Never design, describe, or restyle the operating-system title bar, Codex task header, window controls, settings controls, send button, permission controls, or other native interaction chrome; those remain native and must stay clearly recognizable. System action icons and sidebar watermarks are controlled by explicit application options; never add them to assets automatically. Background images must be clear, crisp, and recognizable, not heavily blurred ambience; prefer blur 0-4 and surfaceOpacity 0.25-0.44 so the image remains visible while text surfaces stay readable.
Treat readability as a hard constraint, not an aesthetic suggestion. Calculate WCAG 2 relative-luminance contrast before returning JSON: palette.foreground must be at least 4.5:1 against app, sidebar, content, elevated, and input; muted must be at least 3:1 against all five surfaces; accentForeground must be at least 4.5:1 against accent; codeForeground must be at least 7:1 against codeBackground. Title-bar labels, the Codex task-header title and controls, sidebar labels, terminal text, editor text, and code-review text must never use the same or a near-identical color as their background. Code-review additions, deletions, unchanged rows, inline highlights, and selected rows must retain at least 4.5:1 text contrast and avoid competing saturated red/green fills. Borders, icons, focus indicators, and essential controls should reach 3:1 against adjacent surfaces. Verify both default and hover/active states, and keep native light settings cards readable even when the requested theme is dark. Do not imitate a living artist, include copyrighted logos, or request readable text inside generated images."##
}

fn solid_background(color: &str, surface_opacity: f32) -> RegionBackground {
    RegionBackground {
        source: BackgroundSource::Solid {
            color: color.into(),
        },
        surface_opacity,
        ..RegionBackground::default()
    }
}

fn gradient_background(from: &str, to: &str, surface_opacity: f32) -> RegionBackground {
    RegionBackground {
        source: BackgroundSource::LinearGradient {
            angle: 135,
            from: from.into(),
            to: to.into(),
        },
        surface_opacity,
        ..RegionBackground::default()
    }
}

fn image_background(path: &str, surface_opacity: f32) -> RegionBackground {
    RegionBackground {
        source: BackgroundSource::Image {
            path: path.into(),
            fit: "cover".into(),
        },
        surface_opacity,
        ..RegionBackground::default()
    }
}

impl NormalizedPalette {
    fn surface_colors(&self) -> [&str; 5] {
        [
            &self.app,
            &self.sidebar,
            &self.content,
            &self.elevated,
            &self.input,
        ]
    }
}

fn normalize_blueprint_palette(blueprint: &ThemeBlueprint) -> anyhow::Result<NormalizedPalette> {
    let source = &blueprint.palette;
    let mut app = source.app.to_ascii_uppercase();
    let mut sidebar = source.sidebar.to_ascii_uppercase();
    let mut content = source.content.to_ascii_uppercase();
    let mut elevated = source.elevated.to_ascii_uppercase();
    let mut input = source.input.to_ascii_uppercase();
    let requested_foreground = source.foreground.to_ascii_uppercase();
    let source_surfaces = [
        app.as_str(),
        sidebar.as_str(),
        content.as_str(),
        elevated.as_str(),
        input.as_str(),
    ];
    let foreground = if contrasts_with_all(&requested_foreground, &source_surfaces, 4.5)? {
        requested_foreground
    } else {
        match blueprint.base_mode {
            BlueprintMode::Light => "#000000".into(),
            BlueprintMode::Dark => "#FFFFFF".into(),
        }
    };
    app = background_for_foreground(&app, &foreground, 4.5)?;
    sidebar = background_for_foreground(&sidebar, &foreground, 4.5)?;
    content = background_for_foreground(&content, &foreground, 4.5)?;
    elevated = background_for_foreground(&elevated, &foreground, 4.5)?;
    input = background_for_foreground(&input, &foreground, 4.5)?;
    let surfaces = [
        app.as_str(),
        sidebar.as_str(),
        content.as_str(),
        elevated.as_str(),
        input.as_str(),
    ];
    let muted = color_toward_until(&source.muted, &foreground, |candidate| {
        contrasts_with_all(candidate, &surfaces, 3.0)
    })?;
    let accent = source.accent.to_ascii_uppercase();
    let accent_foreground =
        accessible_foreground_for_surface(&source.accent_foreground, &accent, 4.5)?;
    let mut code_background = source.code_background.to_ascii_uppercase();
    let requested_code_foreground = source.code_foreground.to_ascii_uppercase();
    let code_foreground = if color_contrast(&requested_code_foreground, &code_background)? >= 7.0 {
        requested_code_foreground
    } else {
        best_black_or_white(&code_background, blueprint.base_mode)?
    };
    code_background = background_for_foreground(&code_background, &code_foreground, 7.0)?;
    Ok(NormalizedPalette {
        app,
        sidebar,
        content,
        elevated,
        input,
        foreground,
        muted,
        border: source.border.to_ascii_uppercase(),
        accent,
        accent_foreground,
        code_background,
        code_foreground,
    })
}

fn validate_blueprint_color(color: &str) -> anyhow::Result<()> {
    let bytes = color.as_bytes();
    if bytes.len() != 7
        || bytes.first() != Some(&b'#')
        || !bytes[1..].iter().all(u8::is_ascii_hexdigit)
    {
        bail!("AI 调色板颜色必须严格使用 #RRGGBB：{color}");
    }
    Ok(())
}

fn color_channels(color: &str) -> anyhow::Result<[u8; 3]> {
    validate_blueprint_color(color)?;
    Ok([
        u8::from_str_radix(&color[1..3], 16)?,
        u8::from_str_radix(&color[3..5], 16)?,
        u8::from_str_radix(&color[5..7], 16)?,
    ])
}

fn relative_luminance(color: &str) -> anyhow::Result<f64> {
    let channels = color_channels(color)?.map(|channel| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    Ok(channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722)
}

fn color_contrast(left: &str, right: &str) -> anyhow::Result<f64> {
    let left = relative_luminance(left)?;
    let right = relative_luminance(right)?;
    let (lighter, darker) = if left >= right {
        (left, right)
    } else {
        (right, left)
    };
    Ok((lighter + 0.05) / (darker + 0.05))
}

fn contrasts_with_all(
    foreground: &str,
    backgrounds: &[&str],
    minimum: f64,
) -> anyhow::Result<bool> {
    backgrounds.iter().try_fold(true, |passes, background| {
        Ok(passes && color_contrast(foreground, background)? >= minimum)
    })
}

fn color_toward_until(
    start: &str,
    target: &str,
    predicate: impl Fn(&str) -> anyhow::Result<bool>,
) -> anyhow::Result<String> {
    validate_blueprint_color(start)?;
    validate_blueprint_color(target)?;
    for step in 0..=255 {
        let candidate = mix_hex(start, target, step as f32 / 255.0)?;
        if predicate(&candidate)? {
            return Ok(candidate);
        }
    }
    bail!("无法生成满足文字对比度要求的颜色")
}

fn background_for_foreground(
    background: &str,
    foreground: &str,
    minimum: f64,
) -> anyhow::Result<String> {
    if color_contrast(foreground, background)? >= minimum {
        return Ok(background.to_ascii_uppercase());
    }
    let target = if relative_luminance(foreground)? > 0.5 {
        "#000000"
    } else {
        "#FFFFFF"
    };
    color_toward_until(background, target, |candidate| {
        Ok(color_contrast(foreground, candidate)? >= minimum)
    })
}

fn best_black_or_white(background: &str, tie_breaker: BlueprintMode) -> anyhow::Result<String> {
    let black = color_contrast("#000000", background)?;
    let white = color_contrast("#FFFFFF", background)?;
    if (black - white).abs() < f64::EPSILON {
        return Ok(match tie_breaker {
            BlueprintMode::Light => "#000000".into(),
            BlueprintMode::Dark => "#FFFFFF".into(),
        });
    }
    Ok(if black > white {
        "#000000".into()
    } else {
        "#FFFFFF".into()
    })
}

fn accessible_foreground_for_surface(
    requested: &str,
    background: &str,
    minimum: f64,
) -> anyhow::Result<String> {
    if color_contrast(requested, background)? >= minimum {
        return Ok(requested.to_ascii_uppercase());
    }
    let replacement = best_black_or_white(background, BlueprintMode::Dark)?;
    if color_contrast(&replacement, background)? < minimum {
        bail!("背景颜色无法满足 {minimum}:1 的文字对比度")
    }
    Ok(replacement)
}

fn border_for_surface(requested: &str, surface: &str, foreground: &str) -> anyhow::Result<String> {
    color_toward_until(requested, foreground, |candidate| {
        Ok(color_contrast(candidate, surface)? >= 3.0)
    })
}

fn safe_hover_color(surface: &str, foreground: &str) -> anyhow::Result<String> {
    for amount in [0.12, 0.08, 0.04] {
        let candidate = mix_hex(surface, foreground, amount)?;
        if color_contrast(foreground, &candidate)? >= 4.5 {
            return Ok(candidate);
        }
    }
    Ok(surface.to_ascii_uppercase())
}

fn mix_hex(left: &str, right: &str, right_amount: f32) -> anyhow::Result<String> {
    crate::theme::validate_color(left)?;
    crate::theme::validate_color(right)?;
    let parse = |value: &str, offset: usize| u8::from_str_radix(&value[offset..offset + 2], 16);
    let mut result = String::from("#");
    for offset in [1, 3, 5] {
        let left = f32::from(parse(left, offset)?);
        let right = f32::from(parse(right, offset)?);
        let value = (left * (1.0 - right_amount) + right * right_amount).round() as u8;
        result.push_str(&format!("{value:02X}"));
    }
    Ok(result)
}

fn image_mime(path: &Path) -> anyhow::Result<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Ok("image/png"),
        Some("jpg" | "jpeg") => Ok("image/jpeg"),
        Some("webp") => Ok("image/webp"),
        Some("gif") => Ok("image/gif"),
        Some("bmp") => Ok("image/bmp"),
        _ => bail!("不支持的参考图格式"),
    }
}

fn generated_image_extension(bytes: &[u8]) -> anyhow::Result<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Ok("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Ok("jpg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Ok("webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Ok("gif")
    } else if bytes.starts_with(b"BM") {
        Ok("bmp")
    } else {
        bail!("生图接口返回的内容不是受支持的图片")
    }
}

fn validate_session(session: &str) -> anyhow::Result<()> {
    if session.len() != 32
        || !session
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        bail!("AI 临时会话标识无效");
    }
    Ok(())
}

fn public_image_host(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return false;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(address)) => {
            !(address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_broadcast()
                || address.is_unspecified())
        }
        Ok(std::net::IpAddr::V6(address)) => {
            !(address.is_loopback() || address.is_unspecified() || address.is_unique_local())
        }
        Err(_) => true,
    }
}

fn url_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.into()
}
fn default_text_model() -> String {
    DEFAULT_TEXT_MODEL.into()
}
fn default_image_model() -> String {
    DEFAULT_IMAGE_MODEL.into()
}
fn default_image_size() -> String {
    "3:2".into()
}
fn default_image_concurrency() -> usize {
    IMAGE_GENERATION_CONCURRENCY
}
fn default_radius() -> u16 {
    16
}
fn default_density() -> f32 {
    1.0
}
fn default_sidebar_width() -> u16 {
    280
}
fn default_content_width() -> u16 {
    1080
}
fn default_hero_height() -> u16 {
    360
}
fn default_surface_opacity() -> f32 {
    0.88
}
fn default_blur() -> u16 {
    0
}

#[cfg(windows)]
fn protect_secret(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };
    use windows::core::w;
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().context("API Key 过长")?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            w!("Theme Inject AI API Key"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )?;
        let protected = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(HLOCAL(output.pbData.cast()));
        Ok(protected)
    }
}

#[cfg(windows)]
fn unprotect_secret(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().context("加密 API Key 过长")?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )?;
        let plaintext = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(HLOCAL(output.pbData.cast()));
        Ok(plaintext)
    }
}

#[cfg(not(windows))]
fn protect_secret(_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    bail!("AI API Key 加密存储首版仅支持 Windows")
}

#[cfg(not(windows))]
fn unprotect_secret(_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    bail!("AI API Key 解密首版仅支持 Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ThemeStore;
    use image::GenericImageView;

    fn blueprint() -> ThemeBlueprint {
        serde_json::from_value(json!({
            "name": "Aurora Studio",
            "description": "冷色玻璃感开发主题",
            "baseMode": "dark",
            "palette": {
                "app": "#10131A", "sidebar": "#151B26", "content": "#111722",
                "elevated": "#1A2433", "input": "#172130", "foreground": "#EEF7FF",
                "muted": "#A9B8C8", "border": "#33506B", "accent": "#52D6D8",
                "accentForeground": "#071315", "codeBackground": "#0C1119", "codeForeground": "#DCEAFF"
            },
            "brand": { "title": "Aurora Codex", "subtitle": "Build in calm light" },
            "home": { "title": "今天构建什么？", "subtitle": "从一个清晰目标开始", "cards": [
                { "title": "解释代码", "description": "理解当前实现", "prompt": "解释选中的代码" }
            ] },
            "style": { "radius": 18, "density": 1.0, "sidebarWidth": 290, "contentMaxWidth": 1100, "heroHeight": 360, "surfaceOpacity": 0.86, "blur": 20 },
            "background": "gradient",
            "assets": [{ "slot": "skin.heroImage", "prompt": "abstract cyan aurora over a dark horizon" }]
        })).unwrap()
    }

    #[test]
    fn builds_valid_theme_from_blueprint() {
        let theme = theme_from_blueprint(&blueprint()).unwrap();
        assert!(theme.id.starts_with("local.ai-"));
        assert!(theme.skin.enabled);
        assert!(theme.skin.home.enabled);
        assert_eq!(theme.skin.home.cards.len(), 1);
        theme.validate().unwrap();
    }

    fn assert_contrast(foreground: &str, background: &str, minimum: f64) {
        let ratio = color_contrast(foreground, background).unwrap();
        assert!(
            ratio >= minimum,
            "{foreground} on {background} has {ratio:.3}:1, expected at least {minimum}:1"
        );
    }

    fn assert_theme_text_contrast(theme: &ThemeManifest) {
        for background in [
            &theme.tokens.app_background,
            &theme.tokens.sidebar_background,
            &theme.tokens.content_background,
            &theme.tokens.elevated_background,
            &theme.tokens.input_background,
        ] {
            assert_contrast(&theme.tokens.foreground, background, 4.5);
            assert_contrast(&theme.tokens.muted_foreground, background, 3.0);
        }
        assert_contrast(&theme.tokens.accent_foreground, &theme.tokens.accent, 4.5);
        assert_contrast(
            &theme.tokens.code_foreground,
            &theme.tokens.code_background,
            7.0,
        );
        assert_contrast(&theme.terminal.foreground, &theme.terminal.background, 7.0);
        assert_eq!(
            theme.chrome.titlebar_foreground.as_deref(),
            Some(theme.tokens.foreground.as_str())
        );
        assert_eq!(
            theme.chrome.sidebar_foreground.as_deref(),
            Some(theme.tokens.foreground.as_str())
        );
        assert_eq!(
            theme.chrome.titlebar_muted_foreground.as_deref(),
            Some(theme.tokens.muted_foreground.as_str())
        );
        assert_eq!(
            theme.chrome.sidebar_muted_foreground.as_deref(),
            Some(theme.tokens.muted_foreground.as_str())
        );
        assert_contrast(
            theme.chrome.titlebar_foreground.as_deref().unwrap(),
            theme.chrome.titlebar_background.as_deref().unwrap(),
            4.5,
        );
        assert_contrast(
            theme.chrome.sidebar_foreground.as_deref().unwrap(),
            &theme.tokens.sidebar_background,
            4.5,
        );
        assert_contrast(
            theme.chrome.titlebar_border.as_deref().unwrap(),
            theme.chrome.titlebar_background.as_deref().unwrap(),
            3.0,
        );
        assert_contrast(
            theme.chrome.sidebar_border.as_deref().unwrap(),
            &theme.tokens.sidebar_background,
            3.0,
        );
        assert_contrast(
            theme.chrome.titlebar_foreground.as_deref().unwrap(),
            theme.chrome.titlebar_hover.as_deref().unwrap(),
            4.5,
        );
        assert_contrast(
            theme.chrome.sidebar_foreground.as_deref().unwrap(),
            theme.chrome.sidebar_hover.as_deref().unwrap(),
            4.5,
        );
    }

    #[test]
    fn normalizes_all_white_palette_for_readability() {
        let mut blueprint = blueprint();
        for color in [
            &mut blueprint.palette.app,
            &mut blueprint.palette.sidebar,
            &mut blueprint.palette.content,
            &mut blueprint.palette.elevated,
            &mut blueprint.palette.input,
            &mut blueprint.palette.foreground,
            &mut blueprint.palette.muted,
            &mut blueprint.palette.border,
            &mut blueprint.palette.accent,
            &mut blueprint.palette.accent_foreground,
            &mut blueprint.palette.code_background,
            &mut blueprint.palette.code_foreground,
        ] {
            *color = "#FFFFFF".into();
        }
        let theme = theme_from_blueprint(&blueprint).unwrap();
        assert_theme_text_contrast(&theme);
        theme.validate().unwrap();
    }

    #[test]
    fn normalizes_all_black_palette_for_readability() {
        let mut blueprint = blueprint();
        for color in [
            &mut blueprint.palette.app,
            &mut blueprint.palette.sidebar,
            &mut blueprint.palette.content,
            &mut blueprint.palette.elevated,
            &mut blueprint.palette.input,
            &mut blueprint.palette.foreground,
            &mut blueprint.palette.muted,
            &mut blueprint.palette.border,
            &mut blueprint.palette.accent,
            &mut blueprint.palette.accent_foreground,
            &mut blueprint.palette.code_background,
            &mut blueprint.palette.code_foreground,
        ] {
            *color = "#000000".into();
        }
        let theme = theme_from_blueprint(&blueprint).unwrap();
        assert_theme_text_contrast(&theme);
        theme.validate().unwrap();
    }

    #[test]
    fn normalizes_near_identical_text_and_surface_colors() {
        let mut blueprint = blueprint();
        blueprint.palette.app = "#222222".into();
        blueprint.palette.sidebar = "#232323".into();
        blueprint.palette.content = "#242424".into();
        blueprint.palette.elevated = "#252525".into();
        blueprint.palette.input = "#262626".into();
        blueprint.palette.foreground = "#242424".into();
        blueprint.palette.muted = "#252525".into();
        blueprint.palette.accent = "#777777".into();
        blueprint.palette.accent_foreground = "#787878".into();
        blueprint.palette.code_background = "#111111".into();
        blueprint.palette.code_foreground = "#121212".into();
        let theme = theme_from_blueprint(&blueprint).unwrap();
        assert_theme_text_contrast(&theme);
    }

    #[test]
    fn blueprint_rejects_non_rrggbb_palette_colors() {
        for invalid in [
            "#FFF",
            "#FFFFFFFF",
            "#11223380",
            "transparent",
            "rgb(1,2,3)",
            "112233",
            "#GGGGGG",
        ] {
            let mut blueprint = blueprint();
            blueprint.palette.foreground = invalid.into();
            let error = theme_from_blueprint(&blueprint).unwrap_err();
            assert!(
                format!("{error:#}").contains("#RRGGBB"),
                "unexpected validation error for {invalid}: {error:#}"
            );
        }
    }

    #[test]
    fn blueprint_prompt_states_strict_color_and_wcag_requirements() {
        let prompt = blueprint_system_prompt();
        assert!(prompt.contains("exactly seven ASCII characters in #RRGGBB form"));
        assert!(
            prompt.contains("at least 4.5:1 against app, sidebar, content, elevated, and input")
        );
        assert!(prompt.contains("muted must be at least 3:1"));
        assert!(prompt.contains("codeForeground must be at least 7:1"));
        assert!(prompt.contains("Codex task-header title"));
        assert!(prompt.contains("code-review text"));
    }

    #[test]
    fn plans_and_applies_automatic_icons() {
        let blueprint = blueprint();
        let plans = icon_generation_plans(&blueprint);
        assert_eq!(plans.len(), blueprint.home.cards.len());
        assert!(matches!(
            plans.last().unwrap().0,
            GeneratedSlot::HomeCardIcon(0)
        ));

        let mut theme = theme_from_blueprint(&blueprint).unwrap();
        apply_generated_slot(
            &mut theme,
            GeneratedSlot::ActionIcon("search"),
            "assets/search.png",
        )
        .unwrap();
        apply_generated_slot(
            &mut theme,
            GeneratedSlot::HomeCardIcon(0),
            "assets/card-1.png",
        )
        .unwrap();
        assert_eq!(theme.skin.icons.mode, IconMode::Custom);
        assert_eq!(
            theme.skin.icons.mappings.get("search").map(String::as_str),
            Some("assets/search.png")
        );
        assert_eq!(
            theme.skin.home.cards[0].icon.as_deref(),
            Some("assets/card-1.png")
        );
    }

    #[test]
    fn watermark_is_not_an_automatic_icon_plan() {
        let blueprint = blueprint();
        assert!(
            icon_generation_plans(&blueprint)
                .iter()
                .all(|(slot, _)| *slot != GeneratedSlot::SidebarWatermark)
        );
        assert_eq!(
            GeneratedSlot::ActionIconAtlas.asset_slot(),
            "skin.icons.atlas"
        );
        let prompt = sidebar_watermark_prompt(&blueprint);
        assert!(prompt.contains("lower 35 percent"));
        assert!(prompt.contains("no text"));
    }

    #[test]
    fn endpoint_accepts_base_or_complete_url() {
        assert_eq!(
            endpoint("https://api.example.com", EndpointKind::Responses)
                .unwrap()
                .as_str(),
            "https://api.example.com/v1/responses"
        );
        assert_eq!(
            endpoint("https://api.example.com/v1", EndpointKind::Responses)
                .unwrap()
                .as_str(),
            "https://api.example.com/v1/responses"
        );
        assert_eq!(
            endpoint(
                "https://api.example.com/v1/chat/completions",
                EndpointKind::Images
            )
            .unwrap()
            .as_str(),
            "https://api.example.com/v1/images/generations"
        );
    }

    #[test]
    fn parses_json_inside_code_fence() {
        assert_eq!(
            extract_json_object("```json\n{\"name\":\"test\"}\n```").unwrap(),
            "{\"name\":\"test\"}"
        );
    }

    #[test]
    fn parses_json_and_streaming_api_responses() {
        let json = parse_api_response(
            br#"{"output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]}"#,
            "application/json",
        )
        .unwrap();
        assert_eq!(responses_content(&json).unwrap(), "ok");

        let sse = parse_api_response(
            b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hel\"}\n\nevent: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\"}\n\ndata: [DONE]\n",
            "text/event-stream",
        )
        .unwrap();
        assert_eq!(responses_content(&sse).unwrap(), "hello");
    }

    #[test]
    fn explains_html_and_empty_api_responses() {
        let html = parse_api_response(b"<!doctype html><html></html>", "text/html").unwrap_err();
        assert!(format!("{html:#}").contains("添加 /v1"));
        let empty = parse_api_response(b"  \n", "application/json").unwrap_err();
        assert!(format!("{empty:#}").contains("空响应"));
    }

    #[test]
    fn calculates_bounded_rate_limit_delays() {
        assert_eq!(parse_retry_after("12"), Some(Duration::from_secs(12)));
        assert_eq!(parse_retry_after("9999"), Some(Duration::from_secs(300)));
        assert!(parse_retry_after("invalid").is_none());
        assert_eq!(
            image_retry_delay(Some(Duration::from_secs(1)), 1, "d"),
            Duration::from_secs(1)
        );
        assert!(image_retry_delay(None, 2, "d") >= Duration::from_secs(30));
    }

    #[tokio::test]
    async fn resumes_rate_limited_and_bad_gateway_image_request() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            for attempt in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 4096];
                loop {
                    let size = stream.read(&mut buffer).await.unwrap();
                    request.extend_from_slice(&buffer[..size]);
                    if request.windows(4).any(|part| part == b"\r\n\r\n") {
                        break;
                    }
                }
                assert!(
                    String::from_utf8_lossy(&request).starts_with("POST /v1/images/generations ")
                );
                let response = if attempt < 2 {
                    json!({ "error": { "message": if attempt == 0 { "group requests-per-minute limit exceeded" } else { "temporary upstream failure" } } })
                } else {
                    let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
                    json!({ "data": [{ "b64_json": base64::engine::general_purpose::STANDARD.encode(png) }] })
                };
                let body = serde_json::to_vec(&response).unwrap();
                let status = match attempt {
                    0 => "429 Too Many Requests",
                    1 => "502 Bad Gateway",
                    _ => "200 OK",
                };
                let retry_after = if attempt < 2 {
                    "Retry-After: 1\r\n"
                } else {
                    ""
                };
                let headers = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{retry_after}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap();
            }
        });

        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths, "http://127.0.0.1:1".into(), "token".into()).unwrap();
        let credentials = AiCredentials {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            image_base_url: String::new(),
            text_model: "mock-text".into(),
            vision_model: "mock-vision".into(),
            image_model: "mock-image".into(),
            image_size: "1024x1024".into(),
            api_key: "sk-base".into(),
            image_api_key: Some("sk-image".into()),
        };
        let bytes = service
            .generate_image(
                &credentials,
                GeneratedSlot::ActionIcon("search"),
                "test icon",
                "d",
            )
            .await
            .unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        assert_eq!(
            service
                .request_log()
                .iter()
                .filter(|entry| matches!(entry.http_status, Some(429 | 502)))
                .count(),
            4
        );
        server.await.unwrap();
    }

    #[test]
    fn normalizes_image_ratios_and_assigns_slot_dimensions() {
        assert_eq!(normalize_image_ratio("1536x1024").unwrap(), "3:2");
        assert_eq!(normalize_image_ratio("16:9").unwrap(), "16:9");
        assert_eq!(normalize_image_ratio("auto").unwrap(), "3:2");
        assert!(normalize_image_ratio("1536").is_err());

        assert_eq!(
            GeneratedSlot::BackgroundFullscreen
                .image_spec("16:9", "gpt-image-2")
                .unwrap(),
            ("1536x864".into(), "16:9".into())
        );
        assert_eq!(
            GeneratedSlot::ActionIcon("search")
                .image_spec("16:9", "gpt-image-2")
                .unwrap(),
            ("1024x1024".into(), "1:1".into())
        );

        let credentials = AiCredentials {
            base_url: "https://api.example.com/v1".into(),
            image_base_url: String::new(),
            text_model: "text".into(),
            vision_model: "vision".into(),
            image_model: "gpt-image-2".into(),
            image_size: "1536x1024".into(),
            api_key: "base-key".into(),
            image_api_key: Some("image-key".into()),
        };
        let body = image_request_body(
            &credentials,
            GeneratedSlot::ActionIcon("search"),
            "search icon",
        )
        .unwrap();
        assert_eq!(body["size"], "1024x1024");
        assert!(body.get("response_format").is_none());
        assert_eq!(body["background"], "opaque");
        assert_eq!(body["output_format"], "png");
        assert!(
            body["prompt"]
                .as_str()
                .unwrap()
                .contains("1024x1024 pixels")
        );
        assert!(body["prompt"].as_str().unwrap().contains("#00FF00"));
        assert!(!body["prompt"].as_str().unwrap().contains("1536x1024"));
    }

    #[test]
    fn legacy_gpt_image_models_use_supported_orientation_sizes() {
        for model in ["gpt-image-1", "gpt-image-1.5", "gpt-image-mini"] {
            assert_eq!(
                GeneratedSlot::HeroImage.image_spec("16:9", model).unwrap(),
                ("1536x1024".into(), "2:1".into())
            );
            assert_eq!(
                GeneratedSlot::SidebarWatermark
                    .image_spec("16:9", model)
                    .unwrap(),
                ("1024x1536".into(), "1:2".into())
            );
            assert_eq!(
                GeneratedSlot::ActionIconAtlas
                    .image_spec("16:9", model)
                    .unwrap(),
                ("1024x1024".into(), "1:1".into())
            );
        }

        let credentials = AiCredentials {
            base_url: "https://api.example.com/v1".into(),
            image_base_url: String::new(),
            text_model: "text".into(),
            vision_model: "vision".into(),
            image_model: "gpt-image-1".into(),
            image_size: "16:9".into(),
            api_key: "base-key".into(),
            image_api_key: Some("image-key".into()),
        };
        let body =
            image_request_body(&credentials, GeneratedSlot::BackgroundFullscreen, "hero").unwrap();
        assert_eq!(body["size"], "1536x1024");
        assert!(body["prompt"].as_str().unwrap().contains("16:9"));
    }

    #[test]
    fn chroma_key_assets_become_real_transparent_png() {
        let mut source = RgbaImage::from_pixel(96, 96, image::Rgba([0, 255, 0, 255]));
        for (x, y, pixel) in source.enumerate_pixels_mut() {
            if (24..72).contains(&x) && (20..76).contains(&y) {
                pixel.0 = [225, 45, 75, 255];
            }
        }
        let output = normalize_transparent_asset(source, 512, 512).unwrap();
        assert_eq!(output.dimensions(), (512, 512));
        assert_eq!(output.get_pixel(0, 0).0[3], 0);
        assert!(output.pixels().any(|pixel| pixel.0[3] == 255));
        let encoded = encode_png(&output).unwrap();
        assert!(
            image::load_from_memory(&encoded)
                .unwrap()
                .color()
                .has_alpha()
        );
    }

    #[test]
    fn opaque_fake_transparency_is_rejected() {
        let source = RgbaImage::from_pixel(96, 96, image::Rgba([245, 245, 245, 255]));
        let error = normalize_transparent_asset(source, 512, 512).unwrap_err();
        assert!(format!("{error:#}").contains("纯绿背景"));
    }

    #[test]
    fn weak_green_that_remains_opaque_is_rejected() {
        let mut source = RgbaImage::from_pixel(96, 96, image::Rgba([0, 85, 0, 255]));
        for (x, y, pixel) in source.enumerate_pixels_mut() {
            if (24..72).contains(&x) && (20..76).contains(&y) {
                pixel.0 = [225, 45, 75, 255];
            }
        }
        let error = normalize_transparent_asset(source, 512, 512).unwrap_err();
        assert!(format!("{error:#}").contains("有效透明背景"));
    }

    #[test]
    fn pure_green_hole_inside_closed_icon_becomes_transparent() {
        let mut source = RgbaImage::from_pixel(96, 96, image::Rgba([0, 255, 0, 255]));
        for (x, y, pixel) in source.enumerate_pixels_mut() {
            let on_ring = ((20..76).contains(&x) && (20..26).contains(&y))
                || ((20..76).contains(&x) && (70..76).contains(&y))
                || ((20..26).contains(&x) && (20..76).contains(&y))
                || ((70..76).contains(&x) && (20..76).contains(&y));
            if on_ring {
                pixel.0 = [225, 45, 75, 255];
            }
        }
        let output = normalize_transparent_asset(source, 512, 512).unwrap();
        assert_eq!(output.get_pixel(256, 256).0[3], 0);
        assert!(output.pixels().any(|pixel| pixel.0[3] == 255));
    }

    #[test]
    fn system_icon_atlas_uses_fixed_grid_order() {
        let blueprint = blueprint();
        let prompt = action_icon_atlas_prompt(&blueprint);
        assert!(prompt.contains("3 by 3"));
        for (_, meaning) in ICON_ACTIONS {
            assert!(prompt.contains(meaning));
        }
        assert!(prompt.contains("#00FF00"));
    }

    #[test]
    fn system_icon_atlas_is_split_into_nine_transparent_assets() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        let mut atlas = RgbaImage::from_pixel(768, 768, image::Rgba([0, 255, 0, 255]));
        for index in 0..9_u32 {
            let left = index % 3 * 256 + 72;
            let top = index / 3 * 256 + 72;
            let color = image::Rgba([80 + index as u8 * 12, 24, 180, 255]);
            for y in top..top + 112 {
                for x in left..left + 112 {
                    atlas.put_pixel(x, y, color);
                }
            }
        }
        let bytes = encode_png(&atlas).unwrap();
        let (atlas, assets) = service.stage_action_icon_atlas(&bytes).unwrap();
        assert_eq!(atlas.slot, GeneratedSlot::ActionIconAtlas.asset_slot());
        assert!(
            paths
                .staging
                .join(&atlas.session)
                .join(&atlas.path)
                .is_file()
        );
        assert_eq!(assets.len(), ICON_ACTIONS.len());
        for ((slot, asset), (action, _)) in assets.iter().zip(ICON_ACTIONS) {
            assert_eq!(*slot, GeneratedSlot::ActionIcon(action));
            assert_eq!(asset.slot, format!("skin.icons.{action}"));
            let image = image::open(paths.staging.join(&asset.session).join(&asset.path)).unwrap();
            assert_eq!(image.dimensions(), (512, 512));
            assert!(image.color().has_alpha());
        }
    }

    #[test]
    fn system_icon_atlas_failure_rolls_back_source_and_prior_slices() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        let mut atlas = RgbaImage::from_pixel(768, 768, image::Rgba([245, 245, 245, 255]));
        for y in 0..256 {
            for x in 0..256 {
                atlas.put_pixel(x, y, image::Rgba([0, 255, 0, 255]));
            }
        }
        for y in 72..184 {
            for x in 72..184 {
                atlas.put_pixel(x, y, image::Rgba([120, 24, 180, 255]));
            }
        }

        let error = service
            .stage_action_icon_atlas(&encode_png(&atlas).unwrap())
            .unwrap_err();
        assert!(format!("{error:#}").contains("纯绿背景"));
        assert!(fs::read_dir(&paths.staging).unwrap().next().is_none());
    }

    #[test]
    fn validates_and_merges_custom_resource_plans() {
        let custom = vec![ResourcePlan {
            slot: "skin.decorations.0.asset".into(),
            prompt: "transparent corner ornament".into(),
        }];
        validate_resource_plans(&custom).unwrap();
        let mut plans = vec![(GeneratedSlot::HeroImage, "hero".into())];
        merge_resource_plans(&mut plans, &custom);
        assert!(matches!(plans[1].0, GeneratedSlot::Decoration(0)));
        assert_eq!(plans[1].1, "transparent corner ornament");

        let invalid = vec![ResourcePlan {
            slot: "skin.decorations.1.asset".into(),
            prompt: "invalid reserved slot".into(),
        }];
        assert!(validate_resource_plans(&invalid).is_err());
    }

    #[test]
    fn validates_theme_language_and_names_it_for_prompts() {
        for language in ["zh-CN", "zh-TW", "en-US", "ja-JP", "ko-KR"] {
            validate_theme_language(language).unwrap();
            assert!(!theme_language_name(language).is_empty());
        }
        assert!(validate_theme_language("fr-FR").is_err());
    }

    #[test]
    fn persists_checkpoint_and_preserves_its_staging_assets() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        let slot = GeneratedSlot::HeroImage;
        let staged = service
            .stage_generated_asset(slot, &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
            .unwrap();
        let mut theme = theme_from_blueprint(&blueprint()).unwrap();
        apply_generated_slot(&mut theme, slot, &staged.path).unwrap();
        service
            .save_checkpoint(
                "fingerprint",
                &theme,
                &[(slot, "hero prompt".into())],
                std::slice::from_ref(&staged),
            )
            .unwrap();
        let unrelated = paths.staging.join("unrelated");
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(unrelated.join("file.png"), b"x").unwrap();

        ThemeStore::new(paths.clone()).cleanup_staging().unwrap();
        assert!(
            paths
                .staging
                .join(&staged.session)
                .join(&staged.path)
                .is_file()
        );
        assert!(!unrelated.exists());
        let loaded = service.load_checkpoint("fingerprint").unwrap().unwrap();
        assert_eq!(loaded.assets.len(), 1);
        assert_eq!(loaded.plans.len(), 1);
    }

    #[test]
    fn restores_checkpoint_without_original_fingerprint() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths, "http://127.0.0.1:1".into(), "token".into()).unwrap();
        let slot = GeneratedSlot::HeroImage;
        let staged = service
            .stage_generated_asset(slot, &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
            .unwrap();
        let mut theme = theme_from_blueprint(&blueprint()).unwrap();
        apply_generated_slot(&mut theme, slot, &staged.path).unwrap();
        service
            .save_checkpoint(
                "unavailable-original-fingerprint",
                &theme,
                &[(slot, "hero prompt".into())],
                std::slice::from_ref(&staged),
            )
            .unwrap();

        let restored = service.restore_checkpoint().unwrap();
        assert_eq!(restored.theme.id, theme.id);
        assert_eq!(restored.assets.len(), 1);
        assert_eq!(restored.plans[0].slot, "skin.heroImage");
        assert!(restored.assets[0].preview_url.contains(&staged.session));
    }

    #[test]
    fn restore_rejects_missing_staged_asset_without_deleting_checkpoint() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        let slot = GeneratedSlot::HeroImage;
        let staged = service
            .stage_generated_asset(slot, &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
            .unwrap();
        let mut theme = theme_from_blueprint(&blueprint()).unwrap();
        apply_generated_slot(&mut theme, slot, &staged.path).unwrap();
        service
            .save_checkpoint("fingerprint", &theme, &[], std::slice::from_ref(&staged))
            .unwrap();
        fs::remove_file(paths.staging.join(&staged.session).join(&staged.path)).unwrap();

        assert!(service.restore_checkpoint().is_err());
        assert!(paths.ai_generation.is_file());
    }

    #[test]
    fn image_endpoint_uses_separate_base_or_falls_back() {
        let mut credentials = AiCredentials {
            base_url: "https://text.example.com/v1".into(),
            image_base_url: String::new(),
            text_model: "text".into(),
            vision_model: "vision".into(),
            image_model: "image".into(),
            image_size: "3:2".into(),
            api_key: "base-key".into(),
            image_api_key: Some("image-key".into()),
        };
        assert_eq!(credentials.effective_image_base_url(), credentials.base_url);
        credentials.image_base_url = "https://images.example.com/v1".into();
        assert_eq!(
            endpoint(credentials.effective_image_base_url(), EndpointKind::Images)
                .unwrap()
                .as_str(),
            "https://images.example.com/v1/images/generations"
        );
    }

    #[test]
    fn accepts_http_ai_endpoints_and_rejects_private_image_urls() {
        assert!(validate_endpoint_base("http://example.com/v1").is_ok());
        assert!(validate_endpoint_base("http://127.0.0.1:1234/v1").is_ok());
        assert!(
            validate_public_settings(&AiPublicSettings {
                base_url: "http://api.example.com/v1".into(),
                image_base_url: "http://192.168.1.16:8080".into(),
                text_model: "text".into(),
                vision_model: "vision".into(),
                image_model: "image".into(),
                image_size: "3:2".into(),
                has_api_key: false,
                has_image_api_key: false,
            })
            .is_ok()
        );
        assert_eq!(
            endpoint("http://api.example.com/v1", EndpointKind::Responses)
                .unwrap()
                .as_str(),
            "http://api.example.com/v1/responses"
        );
        assert_eq!(
            endpoint("http://192.168.1.16:8080", EndpointKind::Images)
                .unwrap()
                .as_str(),
            "http://192.168.1.16:8080/v1/images/generations"
        );
        assert!(!public_image_host(
            &Url::parse("https://127.0.0.1/image.png").unwrap()
        ));
        assert!(public_image_host(
            &Url::parse("https://cdn.example.com/image.png").unwrap()
        ));
    }

    #[cfg(windows)]
    #[test]
    fn secret_round_trip() {
        let protected = protect_secret(b"sk-theme-inject-test").unwrap();
        assert_ne!(protected, b"sk-theme-inject-test");
        assert_eq!(
            unprotect_secret(&protected).unwrap(),
            b"sk-theme-inject-test"
        );
    }

    #[tokio::test]
    async fn generates_theme_and_image_through_compatible_http_api() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let blueprint = serde_json::to_string(&json!({
            "name": "Mock Ocean",
            "description": "本地模拟生成主题",
            "baseMode": "dark",
            "palette": {
                "app": "#10131A", "sidebar": "#151B26", "content": "#111722",
                "elevated": "#1A2433", "input": "#172130", "foreground": "#EEF7FF",
                "muted": "#A9B8C8", "border": "#33506B", "accent": "#52D6D8",
                "accentForeground": "#071315", "codeBackground": "#0C1119", "codeForeground": "#DCEAFF"
            },
            "brand": { "title": "Mock Ocean", "subtitle": "Generated locally" },
            "home": { "title": "开始构建", "subtitle": "选择一个方向", "cards": [] },
            "style": {},
            "background": "image",
            "assets": [{ "slot": "skin.heroImage", "prompt": "abstract cyan ocean light" }]
        }))
        .unwrap();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for index in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let header_end;
                loop {
                    let size = stream.read(&mut buffer).await.unwrap();
                    assert!(size > 0);
                    bytes.extend_from_slice(&buffer[..size]);
                    if let Some(position) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        header_end = position + 4;
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                while bytes.len() < header_end + content_length {
                    let size = stream.read(&mut buffer).await.unwrap();
                    assert!(size > 0);
                    bytes.extend_from_slice(&buffer[..size]);
                }
                let request = String::from_utf8(bytes).unwrap();
                requests.push(request.clone());
                let response = if index == 0 {
                    assert!(request.starts_with("POST /v1/responses "));
                    assert!(
                        request
                            .to_ascii_lowercase()
                            .contains("authorization: bearer sk-base")
                    );
                    let request_body = request.split("\r\n\r\n").nth(1).unwrap();
                    let request_json: Value = serde_json::from_str(request_body).unwrap();
                    assert_eq!(request_json["model"], "mock-text");
                    assert!(request_json.get("instructions").is_some());
                    assert!(request_json.get("input").is_some());
                    assert!(request_json.get("text").is_none());
                    assert!(request_json.get("messages").is_none());
                    json!({ "output": [{ "type": "message", "content": [{ "type": "output_text", "text": blueprint }] }] })
                } else {
                    assert!(request.starts_with("POST /v1/images/generations "));
                    assert!(
                        request
                            .to_ascii_lowercase()
                            .contains("authorization: bearer sk-image")
                    );
                    let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
                    json!({ "data": [{ "b64_json": base64::engine::general_purpose::STANDARD.encode(png) }] })
                };
                let body = serde_json::to_vec(&response).unwrap();
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(headers.as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap();
            }
            requests
        });

        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        service
            .save_settings(
                AiPublicSettings {
                    base_url: format!("http://127.0.0.1:{port}/v1"),
                    image_base_url: String::new(),
                    text_model: "mock-text".into(),
                    vision_model: "mock-vision".into(),
                    image_model: "mock-image".into(),
                    image_size: "1024x1024".into(),
                    has_api_key: false,
                    has_image_api_key: false,
                },
                Some("sk-base"),
                Some("sk-image"),
                false,
                false,
            )
            .unwrap();
        let generated = service
            .generate(GenerateRequest {
                prompt: "生成海洋主题".into(),
                language: "zh-CN".into(),
                generate_images: true,
                generate_sidebar_watermark: false,
                generate_system_icons: false,
                image_concurrency: 2,
                reference_session: None,
                reference_path: None,
                references: Vec::new(),
                resource_plans: Vec::new(),
            })
            .await
            .unwrap();
        assert_eq!(generated.theme.name, "Mock Ocean");
        assert_eq!(generated.assets.len(), 1);
        assert_eq!(generated.theme.skin.icons.mode, IconMode::Recolor);
        assert!(generated.theme.skin.icons.mappings.is_empty());
        assert!(
            paths
                .staging
                .join(&generated.assets[0].session)
                .join(&generated.assets[0].path)
                .is_file()
        );
        assert_eq!(server.await.unwrap().len(), 2);
    }
}
