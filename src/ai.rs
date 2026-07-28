use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use anyhow::{Context, bail};
use base64::Engine;
use futures_util::{StreamExt, stream};
use image::{
    DynamicImage, ImageFormat, RgbaImage, codecs::jpeg::JpegEncoder, imageops::FilterType,
};
use reqwest::header::{CONTENT_TYPE, RETRY_AFTER};
use reqwest::{
    Client, StatusCode, Url,
    multipart::{Form, Part},
};
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
const MAX_GENERATED_SIDEBAR_WIDTH: u16 = 360;
const IMAGE_GENERATION_CONCURRENCY: usize = 4;
const MAX_IMAGE_GENERATION_CONCURRENCY: usize = 4;
const MAX_AI_REQUEST_CONCURRENCY: usize = 5;
const IMAGE_POSTPROCESS_CONCURRENCY: usize = 1;
const MAX_IMAGE_ATTEMPTS: usize = 3;
const PET_STRIP_GENERATION_ATTEMPTS: usize = 2;
const ICON_ATLAS_COLUMNS: u32 = 6;
const ICON_ATLAS_ROWS: u32 = 5;
const ICON_ACTIONS: [(&str, &str); 30] = [
    ("sidebar-toggle", "toggle the application sidebar"),
    ("new-task", "create a new coding task"),
    ("search", "search"),
    ("scheduled", "scheduled tasks and calendar"),
    ("plugins", "plugins and extensions"),
    ("pull-requests", "git pull requests"),
    ("settings", "settings and preferences"),
    ("send", "send a message"),
    ("terminal", "terminal and command line"),
    ("files", "files and folders"),
    ("browser", "web browser"),
    ("environments", "development environments"),
    ("git", "git source control branches"),
    ("connections", "remote connections"),
    ("worktrees", "git worktrees"),
    ("hooks", "automation hooks"),
    ("account", "user account"),
    ("general", "general settings"),
    ("appearance", "appearance settings"),
    ("voice", "voice settings"),
    ("configuration", "configuration"),
    ("personalization", "personalization"),
    ("pets", "pets"),
    ("keyboard-shortcuts", "keyboard shortcuts"),
    ("computer-control", "computer control"),
    ("project-folder", "collapsed project folder"),
    ("project-open", "expanded project folder"),
    ("section-toggle", "expand or collapse a navigation section"),
    ("more-actions", "more actions and options menu"),
    ("project-app", "current project or application entry"),
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    pub prompt: String,
    #[serde(default)]
    pub theme_name: String,
    #[serde(default)]
    pub theme_description: String,
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
    #[serde(default)]
    pub resource_plans_only: bool,
    #[serde(default)]
    pub theme: Option<ThemeManifest>,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResourceRequest {
    pub theme: ThemeManifest,
    pub slot: String,
    pub prompt: String,
    #[serde(default)]
    pub use_current_resource_reference: bool,
    #[serde(default)]
    pub references: Vec<ReferenceImage>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResourceResult {
    pub theme: ThemeManifest,
    pub asset: GeneratedAsset,
    pub assets: Vec<GeneratedAsset>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratePetRequest {
    pub theme: ThemeManifest,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub prompt: String,
    #[serde(default)]
    pub action_prompt: String,
    #[serde(default)]
    pub action_prompts: BTreeMap<String, String>,
    #[serde(default)]
    pub references: Vec<ReferenceImage>,
    #[serde(default = "default_image_concurrency")]
    pub image_concurrency: usize,
    #[serde(default)]
    pub resume: bool,
    #[serde(default = "default_pet_generation_phase")]
    pub phase: String,
    #[serde(default)]
    pub lock_first_reference_identity: bool,
    #[serde(default)]
    pub use_first_reference_as_base: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratePetResult {
    pub theme: ThemeManifest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<crate::pet::PetSummary>,
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<GeneratedAsset>,
    #[serde(default)]
    pub actions: Vec<GeneratedAsset>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePetGenerationResult {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub action_prompt: String,
    pub action_prompts: BTreeMap<String, String>,
    pub image_concurrency: usize,
    pub stage: String,
    pub base: GeneratedAsset,
    #[serde(default)]
    pub actions: Vec<GeneratedAsset>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestPetActionPromptsRequest {
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub action_prompts: BTreeMap<String, String>,
    #[serde(default)]
    pub slot: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestPetActionPromptsResult {
    pub action_prompts: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratePetActionRequest {
    #[serde(default)]
    pub pet_id: String,
    pub slot: String,
    pub prompt: String,
    #[serde(default)]
    pub references: Vec<ReferenceImage>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPetActionRequest {
    #[serde(default)]
    pub pet_id: String,
    pub slot: String,
    pub session: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetGenerationCheckpoint {
    fingerprint: String,
    request: GeneratePetRequest,
    pet_id: String,
    session: String,
    #[serde(default)]
    completed: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedAsset {
    pub slot: String,
    pub session: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preview_path: String,
    pub preview_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerationCheckpoint {
    fingerprint: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    theme_name: String,
    #[serde(default)]
    theme_description: String,
    theme: ThemeManifest,
    plans: Vec<CheckpointPlan>,
    assets: Vec<GeneratedAsset>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointPlan {
    slot: String,
    prompt: String,
    #[serde(default)]
    reference_indexes: Vec<usize>,
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
    pub prompt: String,
    pub theme_name: String,
    pub theme_description: String,
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
    #[serde(default)]
    reference_indexes: Vec<usize>,
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
    image_postprocess_semaphore: Semaphore,
    generation_cancelled: AtomicBool,
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
            image_postprocess_semaphore: Semaphore::new(IMAGE_POSTPROCESS_CONCURRENCY),
            generation_cancelled: AtomicBool::new(false),
        })
    }

    async fn run_image_postprocess<T, F>(&self, stage: String, work: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    {
        let queued = std::time::Instant::now();
        crate::diagnostic::log("ai.image_postprocess.queued", json!({ "stage": stage }));
        let _permit = self
            .image_postprocess_semaphore
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("图片后处理队列已关闭"))?;
        let wait = queued.elapsed();
        crate::diagnostic::log(
            "ai.image_postprocess.started",
            json!({ "stage": stage, "waitMs": wait.as_millis() }),
        );
        let started = std::time::Instant::now();
        let result = tokio::task::spawn_blocking(work)
            .await
            .with_context(|| format!("{stage}后处理任务异常退出"))?;
        crate::diagnostic::log(
            "ai.image_postprocess.finished",
            json!({
                "stage": stage,
                "durationMs": started.elapsed().as_millis(),
                "outcome": if result.is_ok() { "success" } else { "error" }
            }),
        );
        result
    }

    async fn validate_pet_strip_queued(
        &self,
        bytes: Vec<u8>,
        frames: u32,
        stage: String,
        error_message: String,
    ) -> anyhow::Result<Vec<u8>> {
        self.run_image_postprocess(stage, move || {
            let extracted = extract_pet_strip(&bytes, frames).with_context(|| error_message)?;
            validate_pet_frame_scale(&extracted)?;
            Ok(bytes)
        })
        .await
    }

    async fn reuse_pet_validation_source(
        &self,
        checkpoint: &PetGenerationCheckpoint,
        slot: &str,
        frames: u32,
        stage: &str,
        validation_error: &str,
    ) -> Option<Vec<u8>> {
        let bytes = fs::read(self.pet_validation_source_artifact(checkpoint, slot)).ok()?;
        self.update_pet_generation_item(slot, "generating", "正在重新检查上次生成结果");
        self.validate_pet_strip_queued(
            bytes,
            frames,
            format!("重新检查{stage}"),
            validation_error.to_string(),
        )
        .await
        .ok()
    }

    async fn generate_valid_pet_strip<F>(
        &self,
        credentials: &AiCredentials,
        base_prompt: &str,
        dimensions: &str,
        references: &[String],
        stage: &str,
        frames: u32,
        item_slot: Option<&str>,
        validation_error: &str,
        mut on_candidate: F,
    ) -> anyhow::Result<Vec<u8>>
    where
        F: FnMut(&[u8]),
    {
        let mut prompt = base_prompt.to_string();
        for attempt in 0..PET_STRIP_GENERATION_ATTEMPTS {
            self.ensure_generation_active()?;
            let attempt_stage = if attempt == 0 {
                stage.to_string()
            } else {
                format!(
                    "{stage}（自动修复切图 {attempt}/{})",
                    PET_STRIP_GENERATION_ATTEMPTS - 1
                )
            };
            let bytes = self
                .generate_pet_image(credentials, &prompt, dimensions, references, &attempt_stage)
                .await?;
            on_candidate(&bytes);
            match self
                .validate_pet_strip_queued(
                    bytes,
                    frames,
                    format!("处理{attempt_stage}"),
                    validation_error.to_string(),
                )
                .await
            {
                Ok(bytes) => return Ok(bytes),
                Err(error) if attempt + 1 < PET_STRIP_GENERATION_ATTEMPTS => {
                    if let Some(slot) = item_slot {
                        self.update_pet_generation_item(
                            slot,
                            "generating",
                            &format!("切图检查未通过，正在缩小主体并自动重试：{error:#}"),
                        );
                    }
                    prompt = pet_strip_repair_prompt(base_prompt, &format!("{error:#}"), frames);
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("pet strip generation attempts must be positive")
    }

    pub fn cancel_generation(&self) {
        self.generation_cancelled.store(true, Ordering::SeqCst);
        self.set_generation_state("cancelled", "生成已中断，已完成素材仍可继续使用或手动补充");
    }

    fn ensure_generation_active(&self) -> anyhow::Result<()> {
        if self.generation_cancelled.load(Ordering::SeqCst) {
            bail!("生成已由用户中断");
        }
        Ok(())
    }

    async fn cancellable<T>(&self, future: impl Future<Output = T>) -> anyhow::Result<T> {
        tokio::pin!(future);
        loop {
            tokio::select! {
                value = &mut future => return Ok(value),
                _ = tokio::time::sleep(Duration::from_millis(100)) => self.ensure_generation_active()?,
            }
        }
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
            if state == "planning" {
                progress.items.clear();
            }
        }
    }

    fn set_generation_plans(
        &self,
        plans: &[(GeneratedSlot, String)],
        reference_assignments: &BTreeMap<String, Vec<usize>>,
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
                        slot: slot_id.clone(),
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
                        reference_indexes: reference_assignments
                            .get(&slot_id)
                            .cloned()
                            .unwrap_or_default(),
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

    fn start_resource_generation(&self, slot: GeneratedSlot, prompt: &str, reference_count: usize) {
        if let Ok(mut progress) = self.generation_progress.lock() {
            progress.state = "generating".into();
            progress.message = format!(
                "正在生成{}；已加载 {reference_count} 张参考图",
                slot.stage_label()
            );
            let item = GenerationProgressItem {
                slot: slot.asset_slot(),
                label: slot.stage_label(),
                prompt: prompt.into(),
                status: "generating".into(),
                preview_url: String::new(),
                session: String::new(),
                path: String::new(),
                message: if reference_count == 0 {
                    "正在请求图片模型".into()
                } else {
                    format!("正在使用 {reference_count} 张参考图请求图片模型")
                },
                reference_indexes: Vec::new(),
            };
            if let Some(existing) = progress
                .items
                .iter_mut()
                .find(|existing| existing.slot == item.slot)
            {
                *existing = item;
            } else {
                progress.items.push(item);
            }
        }
    }

    fn record_local_event(&self, stage: &str, outcome: &str, message: &str) {
        let entry = AiRequestLog {
            timestamp_ms: unix_timestamp_ms(),
            stage: stage.into(),
            endpoint: "local://theme-inject/resource".into(),
            model: "本地处理".into(),
            outcome: outcome.into(),
            http_status: None,
            duration_ms: 0,
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
        if checkpoint.prompt.trim().is_empty() {
            bail!("旧版生成记录缺少原始提示词，无法确认图片来源，请重新生成主题");
        }
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
            prompt: checkpoint.prompt,
            theme_name: checkpoint.theme_name,
            theme_description: checkpoint.theme_description,
        })
    }

    pub fn restore_checkpoint_matching(
        &self,
        mut request: GenerateRequest,
    ) -> anyhow::Result<RestoreResult> {
        let references = self.reference_data_urls(&request)?;
        if request.prompt.trim().is_empty() && !references.is_empty() {
            request.prompt =
                "请以参考图组为视觉方向，生成一套完整、可读、适合长时间编码的 Codex 皮肤主题"
                    .into();
        }
        let credentials = self.settings.credentials()?;
        let expected = generation_fingerprint(&request, &references, &credentials);
        let checkpoint = self
            .read_checkpoint()?
            .context("没有可恢复的 AI 生成结果")?;
        if checkpoint.fingerprint != expected {
            bail!("上次生成与当前提示词、参考图或生成选项不一致，请重新生成");
        }
        self.restore_checkpoint()
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
        request: &GenerateRequest,
        theme: &ThemeManifest,
        plans: &[(GeneratedSlot, String)],
        reference_assignments: &BTreeMap<String, Vec<usize>>,
        assets: &[GeneratedAsset],
    ) -> anyhow::Result<()> {
        let checkpoint = GenerationCheckpoint {
            fingerprint: fingerprint.into(),
            prompt: request.prompt.trim().into(),
            theme_name: request.theme_name.trim().into(),
            theme_description: request.theme_description.trim().into(),
            theme: theme.clone(),
            plans: plans
                .iter()
                .map(|(slot, prompt)| CheckpointPlan {
                    slot: slot.asset_slot(),
                    prompt: prompt.clone(),
                    reference_indexes: reference_assignments
                        .get(&slot.asset_slot())
                        .cloned()
                        .unwrap_or_default(),
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
        self.generation_cancelled.store(false, Ordering::SeqCst);
        self.clear_request_log();
        self.set_generation_state(
            "planning",
            if request.resource_plans_only {
                "正在读取当前主题并准备批量资源计划"
            } else {
                "正在调用基础/视觉模型分析主题需求并生成蓝图"
            },
        );
        let references = self.reference_data_urls(&request)?;
        if request.prompt.trim().is_empty() && !references.is_empty() {
            request.prompt =
                "请以参考图组为视觉方向，生成一套完整、可读、适合长时间编码的 Codex 皮肤主题"
                    .into();
        }
        validate_prompt(&request.prompt)?;
        validate_requested_metadata(&request)?;
        validate_theme_language(&request.language)?;
        request.image_concurrency = request
            .image_concurrency
            .clamp(1, MAX_IMAGE_GENERATION_CONCURRENCY);
        crate::diagnostic::log(
            "ai.generation.started",
            json!({
                "generateImages": request.generate_images,
                "referenceCount": references.len(),
                "imageConcurrency": request.image_concurrency,
                "resourcePlanCount": request.resource_plans.len(),
            }),
        );
        validate_resource_plans(&request.resource_plans)?;
        if request.resource_plans_only && request.resource_plans.is_empty() {
            bail!("仅生成资源计划时至少需要一个资源");
        }
        let credentials = self.settings.credentials()?;
        if request.generate_images && credentials.image_api_key.is_none() {
            bail!("尚未保存生图 API Key");
        }
        let fingerprint = generation_fingerprint(&request, &references, &credentials);
        let effective_prompt = prompt_with_requested_metadata(&request);
        let resumed = request
            .generate_images
            .then(|| self.load_checkpoint(&fingerprint))
            .transpose()?
            .flatten();
        let (mut theme, plans, reference_assignments, mut generated_assets) = if let Some(
            checkpoint,
        ) = resumed
        {
            crate::diagnostic::log(
                "ai.generation.resumed",
                json!({ "themeId": checkpoint.theme.id, "completed": checkpoint.assets.len(), "total": checkpoint.plans.len() }),
            );
            let reference_assignments = checkpoint
                .plans
                .iter()
                .map(|plan| (plan.slot.clone(), plan.reference_indexes.clone()))
                .collect::<BTreeMap<_, _>>();
            (
                checkpoint.theme,
                checkpoint
                    .plans
                    .into_iter()
                    .filter_map(|plan| {
                        GeneratedSlot::from_asset_slot(&plan.slot).map(|slot| (slot, plan.prompt))
                    })
                    .collect::<Vec<_>>(),
                reference_assignments,
                checkpoint.assets,
            )
        } else if request.resource_plans_only {
            let (theme, plans, reference_assignments) =
                resource_plan_generation_context(&request, references.len())?;
            (theme, plans, reference_assignments, Vec::new())
        } else {
            let blueprint = self
                .generate_blueprint(
                    &credentials,
                    &effective_prompt,
                    request.language.trim(),
                    &references,
                )
                .await
                .context("主题蓝图生成失败")?;
            let mut theme = theme_from_blueprint(&blueprint)?;
            apply_requested_metadata(&mut theme, &request)?;
            let mut reference_assignments = BTreeMap::new();
            let mut plans = blueprint
                .assets
                .iter()
                .take(MAX_GENERATED_ASSETS)
                .filter_map(|asset| {
                    GeneratedSlot::parse(&asset.slot).map(|slot| {
                        let slot_id = slot.asset_slot();
                        reference_assignments.insert(
                            slot_id,
                            normalize_reference_indexes(&asset.reference_indexes, references.len()),
                        );
                        (slot, asset.prompt.clone())
                    })
                })
                .collect::<Vec<_>>();
            if request.resource_plans_only {
                plans.clear();
                reference_assignments.clear();
            }
            let icon_plans = icon_generation_plans(&blueprint);
            if !request.resource_plans_only {
                for (slot, _) in &icon_plans {
                    reference_assignments
                        .entry(slot.asset_slot())
                        .or_insert_with(|| default_reference_indexes(references.len()));
                }
                plans.extend(icon_plans);
            }
            merge_resource_plans(&mut plans, &request.resource_plans);
            for plan in &request.resource_plans {
                if let Some(slot) = GeneratedSlot::from_asset_slot(&plan.slot) {
                    reference_assignments
                        .entry(slot.asset_slot())
                        .or_insert_with(|| default_reference_indexes(references.len()));
                }
            }
            if !request.resource_plans_only
                && request.generate_sidebar_watermark
                && !plans
                    .iter()
                    .any(|(slot, _)| *slot == GeneratedSlot::SidebarWatermark)
            {
                plans.push((
                    GeneratedSlot::SidebarWatermark,
                    sidebar_watermark_prompt(&blueprint),
                ));
                reference_assignments
                    .entry(GeneratedSlot::SidebarWatermark.asset_slot())
                    .or_insert_with(|| default_reference_indexes(references.len()));
            } else if !request.generate_sidebar_watermark {
                plans.retain(|(slot, _)| *slot != GeneratedSlot::SidebarWatermark);
                reference_assignments.remove(&GeneratedSlot::SidebarWatermark.asset_slot());
            }
            if !request.resource_plans_only && request.generate_system_icons {
                plans.push((
                    GeneratedSlot::ActionIconAtlas,
                    action_icon_atlas_prompt(&blueprint),
                ));
                reference_assignments
                    .entry(GeneratedSlot::ActionIconAtlas.asset_slot())
                    .or_insert_with(|| default_reference_indexes(references.len()));
            }
            if plans.is_empty() {
                plans.extend(fallback_resource_plans(&blueprint));
                for (slot, _) in &plans {
                    reference_assignments
                        .entry(slot.asset_slot())
                        .or_insert_with(|| default_reference_indexes(references.len()));
                }
            }
            for (slot, _) in &plans {
                reference_assignments
                    .entry(slot.asset_slot())
                    .or_insert_with(|| default_reference_indexes(references.len()));
            }
            for (_, asset_prompt) in &mut plans {
                *asset_prompt = bind_user_art_direction(asset_prompt, &effective_prompt);
            }
            if request.generate_images {
                self.save_checkpoint(
                    &fingerprint,
                    &request,
                    &theme,
                    &plans,
                    &reference_assignments,
                    &[],
                )?;
            }
            (theme, plans, reference_assignments, Vec::new())
        };
        if request.generate_images {
            self.set_generation_plans(&plans, &reference_assignments, &generated_assets);
            let completed = generated_assets
                .iter()
                .map(|asset| asset.slot.as_str())
                .collect::<BTreeSet<_>>();
            let pending = plans
                .iter()
                .filter(|(slot, _)| !completed.contains(slot.asset_slot().as_str()))
                .map(|(slot, prompt)| {
                    (
                        *slot,
                        prompt.clone(),
                        selected_reference_urls(
                            &references,
                            reference_assignments
                                .get(&slot.asset_slot())
                                .map(Vec::as_slice)
                                .unwrap_or(&[]),
                        ),
                    )
                })
                .collect::<Vec<_>>();
            let credentials = &credentials;
            let mut results = stream::iter(pending.into_iter().map(
                |(slot, prompt, reference_urls)| async move {
                    if let Err(error) = self.ensure_generation_active() {
                        return (slot, Err(error));
                    }
                    let stage = slot.stage_label();
                    self.update_generation_item(slot, "generating", "", "", "", "");
                    let result = self
                        .generate_image(credentials, slot, &prompt, &reference_urls, &stage)
                        .await
                        .with_context(|| format!("{stage}生成失败"));
                    if let Err(error) = &result {
                        self.update_generation_item(
                            slot,
                            "failed",
                            "",
                            "",
                            "",
                            &format!("{error:#}"),
                        );
                    }
                    (slot, result)
                },
            ))
            .buffer_unordered(request.image_concurrency);
            let mut generated_bytes = self.generated_asset_bytes(&generated_assets)?;
            let mut first_error = None;
            while let Some((slot, result)) = results.next().await {
                if self.generation_cancelled.load(Ordering::SeqCst) {
                    break;
                }
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
                let processed = (|| -> anyhow::Result<Option<GeneratedAsset>> {
                    if slot == GeneratedSlot::ActionIconAtlas {
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
                            "已切割为 30 个系统图标",
                        );
                        generated_assets.push(atlas);
                        generated_assets.extend(icons.into_iter().map(|(_, asset)| asset));
                        Ok(None)
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
                        Ok(Some(staged))
                    }
                })();
                let staged = match processed {
                    Ok(staged) => staged,
                    Err(error) => {
                        self.update_generation_item(
                            slot,
                            "failed",
                            "",
                            "",
                            "",
                            &format!("{error:#}"),
                        );
                        if first_error.is_none() {
                            first_error =
                                Some(error.context(format!("{}处理失败", slot.stage_label())));
                        }
                        self.save_checkpoint(
                            &fingerprint,
                            &request,
                            &theme,
                            &plans,
                            &reference_assignments,
                            &generated_assets,
                        )?;
                        continue;
                    }
                };
                if let Some(staged) = staged {
                    generated_assets.push(staged);
                }
                self.save_checkpoint(
                    &fingerprint,
                    &request,
                    &theme,
                    &plans,
                    &reference_assignments,
                    &generated_assets,
                )?;
            }
            if first_error.is_some() || self.generation_cancelled.load(Ordering::SeqCst) {
                let completed_plans = plans
                    .iter()
                    .filter(|(slot, _)| {
                        generated_assets
                            .iter()
                            .any(|asset| asset.slot == slot.asset_slot())
                    })
                    .count();
                self.set_generation_state(
                    if self.generation_cancelled.load(Ordering::SeqCst) {
                        "cancelled"
                    } else {
                        "partial"
                    },
                    &format!(
                        "已完成 {completed_plans}/{} 个图片资源，可在资源页手动补充缺失素材",
                        plans.len()
                    ),
                );
                theme.validate()?;
                return Ok(GenerateResult {
                    summary: format!(
                        "已保留 {completed_plans}/{} 个图片资源，请手动补充缺失素材",
                        plans.len()
                    ),
                    theme,
                    assets: generated_assets,
                });
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

    pub async fn generate_resource(
        &self,
        mut request: GenerateResourceRequest,
    ) -> anyhow::Result<GenerateResourceResult> {
        self.generation_cancelled.store(false, Ordering::SeqCst);
        self.clear_request_log();
        request.theme.validate()?;
        let slot = GeneratedSlot::from_asset_slot(request.slot.trim()).context("不支持的资源槽")?;
        if matches!(slot, GeneratedSlot::HomeCardIcon(_)) {
            bail!("该资源槽不能单独生成");
        }
        let user_prompt = request.prompt.trim();
        if user_prompt.is_empty() || user_prompt.chars().count() > MAX_PROMPT_CHARS + 2_500 {
            bail!("资源提示词为空或过长");
        }
        let prompt = resource_generation_prompt(slot, user_prompt, &request.theme);
        let credentials = self.settings.credentials()?;
        if credentials.image_api_key.is_none() {
            bail!("尚未保存生图 API Key");
        }
        let mut reference_urls = if request.use_current_resource_reference {
            self.current_resource_reference_data_urls(&request.theme, slot)?
        } else {
            Vec::new()
        };
        for reference in self.reference_data_urls_from(request.references)? {
            if reference_urls.len() >= 6 {
                break;
            }
            if !reference_urls.contains(&reference) {
                reference_urls.push(reference);
            }
        }
        self.start_resource_generation(slot, user_prompt, reference_urls.len());
        self.record_local_event(
            &format!("准备{}参考图", slot.stage_label()),
            "success",
            &format!("已准备 {} 张参考图，开始调用图片模型", reference_urls.len()),
        );
        let stage = format!("重新生成{}", slot.stage_label());
        let mut bytes = match self
            .generate_image(&credentials, slot, &prompt, &reference_urls, &stage)
            .await
        {
            Ok(bytes) => bytes,
            Err(error) => {
                self.fail_resource_generation(slot, &error);
                return Err(error);
            }
        };
        self.ensure_generation_active()?;
        self.update_generation_item(
            slot,
            "generating",
            "",
            "",
            "",
            "图片已返回，正在进行绿幕抠图和尺寸优化",
        );
        self.record_local_event(
            &format!("处理{}", slot.stage_label()),
            "processing",
            "图片已返回，正在执行绿幕抠图、主体检测和尺寸优化",
        );
        let (asset, assets) = if slot == GeneratedSlot::ActionIconAtlas {
            let (atlas, icons) = match self.stage_action_icon_atlas(&bytes) {
                Ok(processed) => processed,
                Err(first_error) if retryable_resource_processing_error(&first_error) => {
                    bytes = self
                        .retry_resource_after_processing_failure(
                            &credentials,
                            slot,
                            &prompt,
                            &reference_urls,
                            &stage,
                            &first_error,
                        )
                        .await?;
                    self.stage_action_icon_atlas(&bytes).map_err(|error| {
                        self.fail_resource_generation_with_preview(slot, &bytes, &error);
                        error
                    })?
                }
                Err(error) => {
                    self.fail_resource_generation_with_preview(slot, &bytes, &error);
                    return Err(error);
                }
            };
            self.ensure_generation_active()?;
            let mut assets = Vec::with_capacity(icons.len() + 1);
            assets.push(atlas.clone());
            for (action_slot, icon) in icons {
                if let Err(error) =
                    apply_generated_slot(&mut request.theme, action_slot, &icon.path)
                {
                    assets.push(icon);
                    self.cleanup_generated_assets(&assets);
                    self.fail_resource_generation(slot, &error);
                    return Err(error);
                }
                assets.push(icon);
            }
            (atlas, assets)
        } else {
            let processed = match post_process_generated_asset(slot, &bytes) {
                Ok(processed) => processed,
                Err(first_error) if slot.requires_transparency() => {
                    bytes = self
                        .retry_resource_after_processing_failure(
                            &credentials,
                            slot,
                            &prompt,
                            &reference_urls,
                            &stage,
                            &first_error,
                        )
                        .await?;
                    post_process_generated_asset(slot, &bytes).map_err(|error| {
                        self.fail_resource_generation(slot, &error);
                        error
                    })?
                }
                Err(error) => {
                    self.fail_resource_generation(slot, &error);
                    return Err(error);
                }
            };
            self.ensure_generation_active()?;
            let asset = self
                .stage_generated_asset(slot, &processed)
                .map_err(|error| {
                    self.fail_resource_generation(slot, &error);
                    error
                })?;
            if let Err(error) = apply_generated_slot(&mut request.theme, slot, &asset.path) {
                self.cleanup_generated_assets(std::slice::from_ref(&asset));
                self.fail_resource_generation(slot, &error);
                return Err(error);
            }
            (asset.clone(), vec![asset])
        };
        request.theme.validate().map_err(|error| {
            self.fail_resource_generation(slot, &error);
            error
        })?;
        self.update_generation_item(
            slot,
            "completed",
            &asset.preview_url,
            &asset.session,
            &asset.path,
            if slot == GeneratedSlot::ActionIconAtlas {
                "已切割为 30 个动作图标，等待应用"
            } else {
                "候选资源已生成，等待应用"
            },
        );
        self.set_generation_state(
            "completed",
            &format!("{}候选资源已生成，等待确认", slot.stage_label()),
        );
        self.record_local_event(
            &format!("保存{}", slot.stage_label()),
            "success",
            "绿幕抠图和主体检测通过，候选资源已写入预览区",
        );
        Ok(GenerateResourceResult {
            theme: request.theme,
            asset,
            assets,
        })
    }

    async fn retry_resource_after_processing_failure(
        &self,
        credentials: &AiCredentials,
        slot: GeneratedSlot,
        prompt: &str,
        reference_urls: &[String],
        stage: &str,
        first_error: &anyhow::Error,
    ) -> anyhow::Result<Vec<u8>> {
        let first_message = format!("{first_error:#}");
        let retry_description = if slot == GeneratedSlot::ActionIconAtlas {
            "正在强化 6×5 网格、逐格主体和纯绿背景约束后重试一次"
        } else {
            "正在自动强化主体和绿幕约束后重试一次"
        };
        self.record_local_event(
            &format!("处理{}", slot.stage_label()),
            "retrying",
            &format!("{first_message}；{retry_description}"),
        );
        self.update_generation_item(
            slot,
            "generating",
            "",
            "",
            "",
            if slot == GeneratedSlot::ActionIconAtlas {
                "首次图集不符合 6×5 切割要求，正在强化逐格图标和绿幕约束后重试"
            } else {
                "首次抠图未得到有效主体，正在自动强化主体构图后重试"
            },
        );
        let original_prompt = prompt.chars().take(1_500).collect::<String>();
        let retry_prompt = resource_processing_retry_prompt(slot, &original_prompt);
        match self
            .generate_image(
                credentials,
                slot,
                &retry_prompt,
                reference_urls,
                &format!("{stage}（主体检测重试）"),
            )
            .await
        {
            Ok(bytes) => Ok(bytes),
            Err(error) => {
                self.fail_resource_generation(slot, &error);
                Err(error)
            }
        }
    }

    fn fail_resource_generation(&self, slot: GeneratedSlot, error: &anyhow::Error) {
        let message = format!("{error:#}");
        self.update_generation_item(slot, "failed", "", "", "", &message);
        self.set_generation_state("failed", &message);
        self.record_local_event(&format!("处理{}", slot.stage_label()), "error", &message);
    }

    fn fail_resource_generation_with_preview(
        &self,
        slot: GeneratedSlot,
        bytes: &[u8],
        error: &anyhow::Error,
    ) {
        self.fail_resource_generation(slot, error);
        let Ok(preview_bytes) = resize_and_encode_png(bytes, 1024, 1024) else {
            return;
        };
        let Ok(asset) = self.stage_generated_asset(slot, &preview_bytes) else {
            return;
        };
        self.update_generation_item(
            slot,
            "failed",
            &asset.preview_url,
            &asset.session,
            &asset.path,
            &format!("{error:#}；已保留模型原始返回图供检查"),
        );
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
                "用户原始要求（最高优先级，不得弱化、替换风格或擅自改题）：{prompt}\n界面语言：{}。先逐项提取用户指定的主题、主体、颜色、材质、氛围、构图、装饰和禁用项，再让 palette、skin、home、background 及每个 assets.prompt 明确落实这些约束。所有面向用户的主题名称、品牌文案、首页标题、说明和卡片文字必须使用该语言。",
                theme_language_name(language)
            ))
        } else {
            let mut content = vec![
                json!({ "type": "input_text", "text": format!("用户原始要求（最高优先级，不得弱化、替换风格或擅自改题）：{prompt}\n界面语言：{}。逐张分析参考图，并明确提取共同的主色、辅助色、材质、主体、构图重心、留白、装饰密度和光影；palette、background、skin 和每个 assets.prompt 必须可追溯地采用这些特征，而不是只生成泛化的同类风格。如果参考图与文字冲突，以用户文字为准；如果不冲突，两者都必须落实。所有面向用户的主题名称、品牌文案、首页标题、说明和卡片文字必须使用该语言。", theme_language_name(language)) }),
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
        reference_urls: &[String],
        stage: &str,
    ) -> anyhow::Result<Vec<u8>> {
        if prompt.trim().is_empty() || prompt.chars().count() > 2_000 {
            bail!("生图提示词为空或过长");
        }
        let body = image_request_body(credentials, slot, prompt)?;
        self.generate_image_request(credentials, body, reference_urls, stage)
            .await
    }

    pub async fn generate_pet(
        &self,
        request: GeneratePetRequest,
    ) -> anyhow::Result<GeneratePetResult> {
        let result = self.generate_pet_inner(request).await;
        if let Err(error) = &result {
            self.set_generation_state("failed", &format!("宠物生成已暂停：{error:#}"));
        }
        result
    }

    pub async fn generate_pet_base(
        &self,
        mut request: GeneratePetRequest,
    ) -> anyhow::Result<GeneratePetResult> {
        request.phase = "base".into();
        request.resume = false;
        self.generate_pet(request).await
    }

    pub async fn generate_pet_actions(
        &self,
        mut request: GeneratePetRequest,
    ) -> anyhow::Result<GeneratePetResult> {
        request.phase = "actions".into();
        request.resume = !request.use_first_reference_as_base;
        self.generate_pet(request).await
    }

    pub async fn publish_pet(
        &self,
        mut request: GeneratePetRequest,
    ) -> anyhow::Result<GeneratePetResult> {
        request.phase = "publish".into();
        request.resume = true;
        request.use_first_reference_as_base = false;
        self.generate_pet(request).await
    }

    pub fn restore_pet_generation(
        &self,
        theme_id: &str,
    ) -> anyhow::Result<Option<RestorePetGenerationResult>> {
        let Some(checkpoint) = self.load_active_pet_checkpoint(theme_id)? else {
            return Ok(None);
        };
        if !checkpoint.completed.contains("pet.base") {
            return Ok(None);
        }
        let actions = self.pet_checkpoint_action_assets(&checkpoint)?;
        let expected_actions = pet_generation_jobs().len();
        let stage = if actions.len() == expected_actions {
            "actions_ready"
        } else if actions.is_empty() {
            "base_ready"
        } else {
            "actions_partial"
        };
        Ok(Some(RestorePetGenerationResult {
            name: checkpoint.request.name.clone(),
            description: checkpoint.request.description.clone(),
            prompt: checkpoint.request.prompt.clone(),
            action_prompt: checkpoint.request.action_prompt.clone(),
            action_prompts: checkpoint.request.action_prompts.clone(),
            image_concurrency: checkpoint.request.image_concurrency,
            stage: stage.into(),
            base: self.pet_checkpoint_generated_asset(&checkpoint, "pet.base")?,
            actions,
        }))
    }

    pub async fn suggest_pet_action_prompts(
        &self,
        request: SuggestPetActionPromptsRequest,
    ) -> anyhow::Result<SuggestPetActionPromptsResult> {
        if request.prompt.chars().count() > 6_000 {
            bail!("宠物描述过长");
        }
        if !request.slot.is_empty()
            && !pet_generation_jobs()
                .iter()
                .any(|job| job.slot == request.slot)
        {
            bail!("宠物动作类型无效");
        }
        let credentials = self.settings.credentials()?;
        let jobs = pet_generation_jobs();
        let selected = jobs
            .iter()
            .filter(|job| request.slot.is_empty() || job.slot == request.slot)
            .map(|job| {
                format!(
                    "{} | {} | {} frames | trigger: {} | required cycle: {}",
                    job.slot, job.label, job.frames, job.trigger, job.action
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let body = json!({
            "model": credentials.text_model,
            "instructions": "You write concise animation directions for Codex pet sprite generation. Return one JSON object only, shaped as {\"actionPrompts\":{\"pet.idle\":\"...\"}}. Use only the requested slot keys. Each value must describe exactly the fixed number of frames in numbered order, preserve the same character identity and size, show one complete action within those frames, and make the last frame reconnect naturally to the first. Never add extra frames, a second unfinished cycle, text, UI, floating symbols, or new props. Running-left and running-right must use a compact short-step gait with an upright torso, never a stretched leap. The state named pet.running means task processing, not locomotion.",
            "input": format!(
                "Pet identity and user direction: {}\nExisting per-action directions: {}\nGenerate directions for:\n{}",
                request.prompt.trim(),
                serde_json::to_string(&request.action_prompts)?,
                selected
            )
        });
        let endpoint = endpoint(&credentials.base_url, EndpointKind::Responses)?;
        let response = self
            .send_json(
                &endpoint,
                &credentials.api_key,
                &body,
                MAX_CHAT_RESPONSE_BYTES,
                "智能生成宠物动作提示词",
                &credentials.text_model,
            )
            .await
            .map_err(|error| error.error)?;
        let content = responses_content(&response)?;
        let json = extract_json_object(&content)?;
        let value: Value = serde_json::from_str(&json).context("动作提示词返回格式无效")?;
        let suggested = value
            .get("actionPrompts")
            .or_else(|| value.get("action_prompts"))
            .cloned()
            .context("动作提示词响应缺少 actionPrompts")?;
        let mut action_prompts: BTreeMap<String, String> =
            serde_json::from_value(suggested).context("动作提示词结构无效")?;
        action_prompts.retain(|slot, prompt| {
            jobs.iter().any(|job| job.slot == slot)
                && (request.slot.is_empty() || request.slot == *slot)
                && !prompt.trim().is_empty()
                && prompt.chars().count() <= 6_000
        });
        if action_prompts.is_empty() {
            bail!("基础模型没有返回有效的动作提示词");
        }
        Ok(SuggestPetActionPromptsResult { action_prompts })
    }

    pub fn discard_pet_generation(&self, theme_id: &str) -> anyhow::Result<bool> {
        crate::theme::validate_theme_id(theme_id)?;
        let Some(checkpoint) = self.read_pet_checkpoint()? else {
            return Ok(false);
        };
        if checkpoint.request.theme.id != theme_id {
            return Ok(false);
        }
        self.discard_pet_checkpoint(&checkpoint);
        Ok(true)
    }

    pub async fn generate_pet_action_candidate(
        &self,
        request: GeneratePetActionRequest,
    ) -> anyhow::Result<GeneratedAsset> {
        validate_prompt(&request.prompt)?;
        self.generation_cancelled.store(false, Ordering::SeqCst);
        self.clear_request_log();
        let job = pet_generation_jobs()
            .into_iter()
            .find(|job| job.slot == request.slot)
            .context("宠物动作类型无效")?;
        let credentials = self.settings.credentials()?;
        let mut references = Vec::new();
        if request.pet_id.trim().is_empty() {
            let checkpoint = self
                .read_pet_checkpoint()?
                .context("没有正在编辑的宠物检查点")?;
            let base = fs::read(self.pet_checkpoint_artifact(&checkpoint, "pet.base"))?;
            references.push(image_data_url(&base)?);
        } else {
            crate::pet::validate_pet_id(&request.pet_id)?;
            let root = self.paths.pets.join(&request.pet_id);
            crate::pet::validate_pet_directory(&root)?;
            let atlas = fs::read(root.join("spritesheet.webp"))?;
            references.push(image_data_url(&atlas)?);
            references.push(image_data_url(&pet_identity_cell(&atlas)?)?);
        }
        references.extend(
            self.reference_data_urls_from(request.references)?
                .into_iter()
                .take(4),
        );
        let bytes = self
            .generate_valid_pet_strip(
                &credentials,
                &pet_row_prompt(&request.prompt, &job),
                pet_strip_dimensions(job.frames),
                &references,
                &format!("调优宠物动作：{}", job.label),
                job.frames,
                None,
                &format!("{}动作行无法切分", job.label),
                |_| {},
            )
            .await?;
        self.stage_pet_action_candidate(job.slot, &bytes)
    }

    pub fn apply_pet_action_candidate(
        &self,
        request: ApplyPetActionRequest,
    ) -> anyhow::Result<Option<crate::pet::PetSummary>> {
        let job = pet_generation_jobs()
            .into_iter()
            .find(|job| job.slot == request.slot)
            .context("宠物动作类型无效")?;
        validate_session(&request.session)?;
        crate::theme::validate_image_asset_path(&request.path)?;
        let candidate = self
            .paths
            .staging
            .join(&request.session)
            .join(&request.path)
            .canonicalize()
            .context("动作候选图不存在")?;
        if !candidate.starts_with(self.paths.staging.canonicalize()?) {
            bail!("动作候选图路径无效");
        }
        let bytes = fs::read(candidate)?;
        extract_pet_strip(&bytes, job.frames)?;
        if request.pet_id.trim().is_empty() {
            let mut checkpoint = self
                .read_pet_checkpoint()?
                .context("没有正在编辑的宠物检查点")?;
            self.commit_pet_checkpoint_artifact(&mut checkpoint, job.slot, &bytes)?;
            self.update_pet_generation_item(job.slot, "completed", "已采用调优后的动作行");
            return Ok(None);
        }
        crate::pet::validate_pet_id(&request.pet_id)?;
        let root = self.paths.pets.join(&request.pet_id);
        let manifest = crate::pet::validate_pet_directory(&root)?;
        let atlas_path = root.join("spritesheet.webp");
        let mut atlas = image::open(&atlas_path)?.to_rgba8();
        replace_pet_atlas_row(&mut atlas, &bytes, &job)?;
        let staging = self.paths.staging.join(format!(
            "pet-action-apply-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&staging)?;
        fs::copy(root.join("pet.json"), staging.join("pet.json"))?;
        save_pet_atlas(&staging.join("spritesheet.webp"), &atlas)?;
        crate::pet::validate_pet_directory(&staging)?;
        let backup =
            self.paths
                .pet_backups
                .join(format!("{}-{}", request.pet_id, unix_timestamp_ms()));
        fs::create_dir_all(&self.paths.pet_backups)?;
        fs::rename(&root, &backup)?;
        if let Err(error) = fs::rename(&staging, &root) {
            let _ = fs::rename(&backup, &root);
            return Err(error).context("替换宠物动作失败，已恢复原宠物");
        }
        Ok(Some(crate::pet::PetSummary {
            manifest,
            installed: false,
            asset_url: String::new(),
        }))
    }

    async fn generate_pet_inner(
        &self,
        mut request: GeneratePetRequest,
    ) -> anyhow::Result<GeneratePetResult> {
        request.theme.validate()?;
        validate_prompt(&request.prompt)?;
        if !request.action_prompt.trim().is_empty() {
            validate_prompt(&request.action_prompt)?;
        }
        for prompt in request.action_prompts.values() {
            if !prompt.trim().is_empty() {
                validate_prompt(prompt)?;
            }
        }
        if request.name.chars().count() > 80 || request.description.chars().count() > 240 {
            bail!("宠物名称或介绍超过长度限制");
        }
        let credentials = self.settings.credentials()?;
        if credentials.image_api_key.is_none() {
            bail!("尚未保存生图 API Key");
        }
        self.generation_cancelled.store(false, Ordering::SeqCst);
        self.clear_request_log();
        request.image_concurrency = request
            .image_concurrency
            .clamp(1, MAX_IMAGE_GENERATION_CONCURRENCY);
        if !matches!(
            request.phase.as_str(),
            "base" | "actions" | "publish" | "complete"
        ) {
            bail!("宠物生成阶段无效");
        }
        let requested_phase = request.phase.clone();
        let requested_action_prompt = request.action_prompt.trim().to_string();
        let requested_action_prompts = request.action_prompts.clone();
        let requested_references = request.references.clone();
        let requested_resume = request.resume;
        let user_references = self.reference_data_urls_from(request.references.clone())?;
        let name = if request.name.trim().is_empty() {
            format!("{} 伙伴", request.theme.name)
        } else {
            request.name.trim().to_string()
        };
        let description = if request.description.trim().is_empty() {
            format!("为“{}”生成的配套 Codex 宠物", request.theme.name)
        } else {
            request.description.trim().to_string()
        };
        let jobs = pet_generation_jobs();
        let fingerprint = pet_generation_fingerprint(&request, &user_references, &credentials)?;
        let mut checkpoint = if request.resume {
            self.load_active_pet_checkpoint(&request.theme.id)?
                .context("没有可继续的宠物生成检查点，请重新生成基础形象")?
        } else {
            if let Some(previous) = self.read_pet_checkpoint()? {
                self.discard_pet_checkpoint(&previous);
            }
            let mut stored_request = request.clone();
            stored_request.resume = false;
            let checkpoint = PetGenerationCheckpoint {
                fingerprint,
                request: stored_request,
                pet_id: unique_pet_id(&self.paths, &name),
                session: uuid::Uuid::new_v4().simple().to_string(),
                completed: BTreeSet::new(),
            };
            fs::create_dir_all(self.paths.staging.join(&checkpoint.session))?;
            self.save_pet_checkpoint(&checkpoint)?;
            checkpoint
        };
        if requested_resume {
            let mut checkpoint_changed = false;
            if requested_phase == "actions"
                && checkpoint.request.action_prompt != requested_action_prompt
            {
                checkpoint.request.action_prompt = requested_action_prompt;
                checkpoint_changed = true;
            }
            if requested_phase == "actions"
                && checkpoint.request.action_prompts != requested_action_prompts
            {
                checkpoint.request.action_prompts = requested_action_prompts;
                checkpoint_changed = true;
            }
            for reference in requested_references {
                if checkpoint.request.references.iter().any(|existing| {
                    existing.session == reference.session && existing.path == reference.path
                }) {
                    continue;
                }
                checkpoint.request.references.push(reference);
                checkpoint_changed = true;
            }
            if checkpoint_changed {
                self.save_pet_checkpoint(&checkpoint)?;
            }
        }
        let resume_concurrency = request.image_concurrency;
        request = checkpoint.request.clone();
        request.image_concurrency = resume_concurrency;
        request.phase = requested_phase;
        let user_references = self.reference_data_urls_from(request.references.clone())?;
        let supplemental_references = pet_supplemental_references(
            &request.phase,
            &user_references,
            request.use_first_reference_as_base,
        );
        self.set_pet_generation_plans(&jobs, &checkpoint.completed);
        if !checkpoint.completed.is_empty() {
            self.set_generation_state(
                "generating",
                &format!(
                    "已恢复 {} 项宠物素材，继续补齐未完成动作",
                    checkpoint.completed.len()
                ),
            );
        }

        let base_prompt = pet_base_prompt(
            &request.theme,
            &request.prompt,
            request.lock_first_reference_identity,
        );
        let base = if checkpoint.completed.contains("pet.base") {
            fs::read(self.pet_checkpoint_artifact(&checkpoint, "pet.base"))?
        } else {
            self.update_pet_generation_item("pet.base", "generating", "正在构思宠物基础形象");
            let bytes = if request.use_first_reference_as_base {
                let reference = request
                    .references
                    .first()
                    .context("请先上传一张宠物基础形象")?;
                let uploaded = fs::read(self.reference_path(reference)?)?;
                normalize_pet_identity_reference(&uploaded)?
            } else {
                let result = self
                    .generate_pet_image(
                        &credentials,
                        &base_prompt,
                        "1024x1024",
                        &user_references,
                        "生成宠物基础形象",
                    )
                    .await;
                match result {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        self.update_pet_generation_item(
                            "pet.base",
                            "failed",
                            &format!("{error:#}"),
                        );
                        return Err(error);
                    }
                }
            };
            image_data_url(&bytes)?;
            self.commit_pet_checkpoint_artifact(&mut checkpoint, "pet.base", &bytes)?;
            bytes
        };
        let normalized_base = normalize_pet_identity_reference(&base)?;
        if normalized_base != base {
            self.commit_pet_checkpoint_artifact(&mut checkpoint, "pet.base", &normalized_base)?;
        }
        let base = normalized_base;
        let base_reference = image_data_url(&base)?;
        self.update_pet_generation_item("pet.base", "completed", "基础形象已完成");
        if request.phase == "base" {
            self.set_generation_state(
                "awaiting_confirmation",
                "基础形象已生成，请确认后继续生成动作",
            );
            return Ok(GeneratePetResult {
                theme: request.theme,
                pet: None,
                stage: "base_ready".into(),
                base: Some(self.pet_checkpoint_generated_asset(&checkpoint, "pet.base")?),
                actions: self.pet_checkpoint_action_assets(&checkpoint)?,
            });
        }

        let pending_standard = jobs
            .iter()
            .copied()
            .filter(|job| job.row < 9 && !checkpoint.completed.contains(job.slot))
            .collect::<Vec<_>>();
        if !pending_standard.is_empty() {
            self.set_generation_state(
                "generating",
                &format!(
                    "正在以 {} 路并发生成 {} 组标准动作",
                    request.image_concurrency,
                    pending_standard.len()
                ),
            );
        }
        let validation_checkpoint = checkpoint.clone();
        let mut standard_results = stream::iter(pending_standard.into_iter().map(|job| {
            let mut references = vec![base_reference.clone()];
            references.extend(supplemental_references.iter().take(4).cloned());
            let action_direction = request
                .action_prompts
                .get(job.slot)
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    (!request.action_prompt.trim().is_empty())
                        .then_some(request.action_prompt.as_str())
                })
                .unwrap_or(request.prompt.as_str());
            let prompt = pet_row_prompt(action_direction, &job);
            let credentials = credentials.clone();
            let validation_checkpoint = validation_checkpoint.clone();
            async move {
                self.update_pet_generation_item(job.slot, "generating", "正在生成完整动作行");
                let validation_error = format!("{}动作行无法切分，请继续生成此动作", job.label);
                let cached = self
                    .reuse_pet_validation_source(
                        &validation_checkpoint,
                        job.slot,
                        job.frames,
                        &format!("宠物动作：{}", job.label),
                        &validation_error,
                    )
                    .await;
                let result = if let Some(bytes) = cached {
                    Ok(bytes)
                } else {
                    self.generate_valid_pet_strip(
                        &credentials,
                        &prompt,
                        pet_strip_dimensions(job.frames),
                        &references,
                        &format!("生成宠物动作：{}", job.label),
                        job.frames,
                        Some(job.slot),
                        &validation_error,
                        |bytes| {
                            if let Ok(asset) = self.stage_pet_validation_source(
                                &validation_checkpoint,
                                job.slot,
                                bytes,
                            ) {
                                self.update_pet_generation_item_asset(job.slot, &asset);
                            }
                        },
                    )
                    .await
                };
                if result.is_ok() {
                    self.clear_pet_validation_source(&validation_checkpoint, job.slot);
                }
                (job, result)
            }
        }))
        .buffer_unordered(request.image_concurrency);
        let mut first_error = None;
        while let Some((job, result)) = standard_results.next().await {
            match result {
                Ok(bytes) => {
                    self.commit_pet_checkpoint_artifact(&mut checkpoint, job.slot, &bytes)?;
                    if let Ok(asset) = self.pet_checkpoint_generated_asset(&checkpoint, job.slot) {
                        self.update_pet_generation_item_asset(job.slot, &asset);
                    }
                    self.update_pet_generation_item(
                        job.slot,
                        "completed",
                        "动作行已保存到恢复检查点",
                    );
                }
                Err(error) => {
                    self.update_pet_generation_item(job.slot, "failed", &format!("{error:#}"));
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error.context("标准动作未全部完成，可点击继续上次生成"));
        }

        if request.phase == "actions" {
            self.set_generation_state("awaiting_confirmation", "动作已生成，请确认后再加入宠物库");
            return Ok(GeneratePetResult {
                theme: request.theme,
                pet: None,
                stage: "actions_ready".into(),
                base: Some(self.pet_checkpoint_generated_asset(&checkpoint, "pet.base")?),
                actions: self.pet_checkpoint_action_assets(&checkpoint)?,
            });
        }

        let mut atlas = RgbaImage::new(crate::pet::PET_WIDTH, crate::pet::PET_V1_HEIGHT);
        for job in &jobs {
            let bytes = fs::read(self.pet_checkpoint_artifact(&checkpoint, job.slot))?;
            for (column, frame) in extract_pet_strip_for_job(&bytes, job)?
                .into_iter()
                .enumerate()
            {
                image::imageops::overlay(
                    &mut atlas,
                    &frame,
                    i64::from(column as u32 * crate::pet::PET_CELL_WIDTH),
                    i64::from(job.row * crate::pet::PET_CELL_HEIGHT),
                );
            }
        }
        despill_pet_atlas(&mut atlas);

        let pet_root = self.paths.pets.join(&checkpoint.pet_id);
        let final_staging = self
            .paths
            .staging
            .join(format!("pet-final-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&final_staging)?;
        let result = (|| -> anyhow::Result<crate::pet::PetManifest> {
            let manifest = crate::pet::PetManifest {
                id: checkpoint.pet_id.clone(),
                display_name: name,
                description,
                sprite_version_number: 1,
                spritesheet_path: "spritesheet.webp".into(),
            };
            fs::write(
                final_staging.join("pet.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            save_pet_atlas(&final_staging.join("spritesheet.webp"), &atlas)?;
            crate::pet::validate_pet_directory(&final_staging)?;
            fs::rename(&final_staging, &pet_root)?;
            Ok(manifest)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&final_staging);
        }
        let manifest = result?;
        self.discard_pet_checkpoint(&checkpoint);
        request.theme.set_companion_pet_id(Some(&manifest.id))?;
        self.set_generation_state("completed", "配套宠物已生成并加入宠物库");
        Ok(GeneratePetResult {
            theme: request.theme,
            pet: Some(crate::pet::PetSummary {
                manifest,
                installed: false,
                asset_url: String::new(),
            }),
            stage: "completed".into(),
            base: None,
            actions: Vec::new(),
        })
    }

    fn read_pet_checkpoint(&self) -> anyhow::Result<Option<PetGenerationCheckpoint>> {
        if !self.paths.ai_pet_generation.exists() {
            return Ok(None);
        }
        let checkpoint = serde_json::from_slice(&fs::read(&self.paths.ai_pet_generation)?)
            .context("宠物生成检查点格式无效")?;
        Ok(Some(checkpoint))
    }

    #[cfg(test)]
    fn load_pet_checkpoint(
        &self,
        fingerprint: &str,
    ) -> anyhow::Result<Option<PetGenerationCheckpoint>> {
        let Some(checkpoint) = self.load_valid_pet_checkpoint()? else {
            return Ok(None);
        };
        if checkpoint.fingerprint != fingerprint {
            return Ok(None);
        }
        Ok(Some(checkpoint))
    }

    fn load_active_pet_checkpoint(
        &self,
        theme_id: &str,
    ) -> anyhow::Result<Option<PetGenerationCheckpoint>> {
        crate::theme::validate_theme_id(theme_id)?;
        let Some(checkpoint) = self.load_valid_pet_checkpoint()? else {
            return Ok(None);
        };
        if checkpoint.request.theme.id != theme_id {
            return Ok(None);
        }
        Ok(Some(checkpoint))
    }

    fn load_valid_pet_checkpoint(&self) -> anyhow::Result<Option<PetGenerationCheckpoint>> {
        let Some(checkpoint) = self.read_pet_checkpoint()? else {
            return Ok(None);
        };
        let checkpoint = checkpoint;
        for slot in checkpoint.completed.clone() {
            let current = self.pet_checkpoint_artifact(&checkpoint, &slot);
            if current.is_file() {
                continue;
            }
            let legacy = self.pet_checkpoint_legacy_artifact(&checkpoint, &slot);
            if legacy.is_file() {
                fs::create_dir_all(current.parent().context("宠物检查点路径无父目录")?)?;
                fs::rename(legacy, current)?;
            }
        }
        if checkpoint.request.theme.validate().is_err()
            || validate_session(&checkpoint.session).is_err()
            || checkpoint
                .completed
                .iter()
                .any(|slot| !self.pet_checkpoint_artifact(&checkpoint, slot).is_file())
        {
            return Ok(None);
        }
        Ok(Some(checkpoint))
    }

    fn save_pet_checkpoint(&self, checkpoint: &PetGenerationCheckpoint) -> anyhow::Result<()> {
        atomic_write(
            &self.paths.ai_pet_generation,
            &serde_json::to_vec_pretty(checkpoint)?,
        )
    }

    fn pet_checkpoint_artifact(&self, checkpoint: &PetGenerationCheckpoint, slot: &str) -> PathBuf {
        let relative = match slot {
            "pet.base" => "assets/pet-base.png".to_string(),
            "pet.look-cardinals" => "assets/pet-look-cardinals.png".to_string(),
            _ => format!(
                "assets/pet-row-{}.png",
                slot.strip_prefix("pet.").unwrap_or("invalid")
            ),
        };
        self.paths.staging.join(&checkpoint.session).join(relative)
    }

    fn pet_checkpoint_legacy_artifact(
        &self,
        checkpoint: &PetGenerationCheckpoint,
        slot: &str,
    ) -> PathBuf {
        let relative = match slot {
            "pet.base" => "base.png".to_string(),
            "pet.look-cardinals" => "look-cardinals.png".to_string(),
            _ => format!(
                "rows/{}.png",
                slot.strip_prefix("pet.").unwrap_or("invalid")
            ),
        };
        self.paths.staging.join(&checkpoint.session).join(relative)
    }

    fn pet_validation_source_artifact(
        &self,
        checkpoint: &PetGenerationCheckpoint,
        slot: &str,
    ) -> PathBuf {
        let file_stem = slot.strip_prefix("pet.").unwrap_or("invalid");
        self.paths
            .staging
            .join(&checkpoint.session)
            .join(format!("assets/pet-validation-{file_stem}.png"))
    }

    fn pet_checkpoint_preview_artifact(
        &self,
        checkpoint: &PetGenerationCheckpoint,
        slot: &str,
    ) -> PathBuf {
        let file_stem = slot.strip_prefix("pet.").unwrap_or("invalid");
        self.paths
            .staging
            .join(&checkpoint.session)
            .join(format!("assets/previews/{file_stem}.png"))
    }

    fn stage_pet_validation_source(
        &self,
        checkpoint: &PetGenerationCheckpoint,
        slot: &str,
        bytes: &[u8],
    ) -> anyhow::Result<GeneratedAsset> {
        let path = self.pet_validation_source_artifact(checkpoint, slot);
        fs::create_dir_all(path.parent().context("宠物原始动作图路径无父目录")?)?;
        atomic_write(&path, bytes)?;
        let relative = path
            .strip_prefix(self.paths.staging.join(&checkpoint.session))?
            .to_string_lossy()
            .replace('\\', "/");
        Ok(GeneratedAsset {
            slot: slot.into(),
            session: checkpoint.session.clone(),
            path: relative.clone(),
            preview_path: String::new(),
            preview_url: self.reference_preview_url(&checkpoint.session, &relative)?,
        })
    }

    fn clear_pet_validation_source(&self, checkpoint: &PetGenerationCheckpoint, slot: &str) {
        let _ = fs::remove_file(self.pet_validation_source_artifact(checkpoint, slot));
    }

    fn pet_checkpoint_generated_asset(
        &self,
        checkpoint: &PetGenerationCheckpoint,
        slot: &str,
    ) -> anyhow::Result<GeneratedAsset> {
        let path = self.pet_checkpoint_artifact(checkpoint, slot);
        let preview_path = self.pet_checkpoint_preview_artifact(checkpoint, slot);
        if !preview_path.is_file() {
            let preview_bytes = resize_and_encode_png(&fs::read(&path)?, 384, 256)?;
            fs::create_dir_all(
                preview_path
                    .parent()
                    .context("宠物动作缩略图路径无父目录")?,
            )?;
            atomic_write(&preview_path, &preview_bytes)?;
        }
        let relative = path
            .strip_prefix(self.paths.staging.join(&checkpoint.session))?
            .to_string_lossy()
            .replace('\\', "/");
        let preview_relative = preview_path
            .strip_prefix(self.paths.staging.join(&checkpoint.session))?
            .to_string_lossy()
            .replace('\\', "/");
        Ok(GeneratedAsset {
            slot: slot.into(),
            session: checkpoint.session.clone(),
            path: relative,
            preview_path: preview_relative.clone(),
            preview_url: self.reference_preview_url(&checkpoint.session, &preview_relative)?,
        })
    }

    fn pet_checkpoint_action_assets(
        &self,
        checkpoint: &PetGenerationCheckpoint,
    ) -> anyhow::Result<Vec<GeneratedAsset>> {
        pet_generation_jobs()
            .iter()
            .filter(|job| checkpoint.completed.contains(job.slot))
            .map(|job| self.pet_checkpoint_generated_asset(checkpoint, job.slot))
            .collect()
    }

    fn commit_pet_checkpoint_artifact(
        &self,
        checkpoint: &mut PetGenerationCheckpoint,
        slot: &str,
        bytes: &[u8],
    ) -> anyhow::Result<()> {
        let path = self.pet_checkpoint_artifact(checkpoint, slot);
        let _ = fs::remove_file(self.pet_checkpoint_preview_artifact(checkpoint, slot));
        fs::create_dir_all(path.parent().context("宠物检查点路径无父目录")?)?;
        atomic_write(&path, bytes)?;
        checkpoint.completed.insert(slot.to_string());
        self.save_pet_checkpoint(checkpoint)
    }

    fn discard_pet_checkpoint(&self, checkpoint: &PetGenerationCheckpoint) {
        let _ = fs::remove_dir_all(self.paths.staging.join(&checkpoint.session));
        let _ = fs::remove_file(&self.paths.ai_pet_generation);
    }

    fn set_pet_generation_plans(&self, jobs: &[PetGenerationJob], completed: &BTreeSet<String>) {
        if let Ok(mut progress) = self.generation_progress.lock() {
            progress.state = "generating".into();
            progress.message = "正在生成配套宠物".into();
            progress.items = std::iter::once(GenerationProgressItem {
                slot: "pet.base".into(),
                label: "宠物基础形象".into(),
                prompt: String::new(),
                status: if completed.contains("pet.base") {
                    "completed"
                } else {
                    "queued"
                }
                .into(),
                preview_url: String::new(),
                session: String::new(),
                path: String::new(),
                message: String::new(),
                reference_indexes: Vec::new(),
            })
            .chain(jobs.iter().map(|job| {
                GenerationProgressItem {
                    slot: job.slot.into(),
                    label: job.label.into(),
                    prompt: String::new(),
                    status: if completed.contains(job.slot) {
                        "completed"
                    } else {
                        "queued"
                    }
                    .into(),
                    preview_url: String::new(),
                    session: String::new(),
                    path: String::new(),
                    message: String::new(),
                    reference_indexes: Vec::new(),
                }
            }))
            .collect();
        }
    }

    fn update_pet_generation_item(&self, slot: &str, status: &str, message: &str) {
        if let Ok(mut progress) = self.generation_progress.lock()
            && let Some(item) = progress.items.iter_mut().find(|item| item.slot == slot)
        {
            item.status = status.into();
            item.message = message.into();
        }
    }

    fn update_pet_generation_item_asset(&self, slot: &str, asset: &GeneratedAsset) {
        if let Ok(mut progress) = self.generation_progress.lock()
            && let Some(item) = progress.items.iter_mut().find(|item| item.slot == slot)
        {
            item.preview_url = asset.preview_url.clone();
            item.session = asset.session.clone();
            item.path = asset.path.clone();
        }
    }

    async fn generate_pet_image(
        &self,
        credentials: &AiCredentials,
        prompt: &str,
        dimensions: &str,
        references: &[String],
        stage: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let mut body = json!({
            "model": credentials.image_model,
            "prompt": prompt,
            "n": 1,
            "size": dimensions,
        });
        if !is_gpt_image_model(&credentials.image_model) {
            body["response_format"] = json!("b64_json");
        } else {
            body["background"] = json!("opaque");
            body["output_format"] = json!("png");
        }
        self.generate_image_request(credentials, body, references, stage)
            .await
    }

    async fn generate_image_request(
        &self,
        credentials: &AiCredentials,
        mut body: Value,
        reference_urls: &[String],
        stage: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let endpoint = endpoint(
            credentials.effective_image_base_url(),
            if reference_urls.is_empty() {
                EndpointKind::Images
            } else {
                EndpointKind::ImageEdits
            },
        )?;
        let image_api_key = credentials
            .image_api_key
            .as_deref()
            .context("尚未保存生图 API Key")?;
        let mut attempt = 1usize;
        let mut compatibility_retry = false;
        let response = loop {
            self.ensure_generation_active()?;
            let request_stage = if compatibility_retry {
                format!("{stage}（兼容重试）")
            } else if attempt > 1 {
                format!("{stage}（第 {attempt}/{MAX_IMAGE_ATTEMPTS} 次尝试）")
            } else {
                stage.into()
            };
            let response = if reference_urls.is_empty() {
                self.cancellable(self.send_json(
                    &endpoint,
                    image_api_key,
                    &body,
                    MAX_IMAGE_RESPONSE_BYTES,
                    &request_stage,
                    &credentials.image_model,
                ))
                .await?
            } else {
                self.cancellable(self.send_image_edit(
                    &endpoint,
                    image_api_key,
                    &body,
                    reference_urls,
                    MAX_IMAGE_RESPONSE_BYTES,
                    &request_stage,
                    &credentials.image_model,
                ))
                .await?
            };
            match response {
                Ok(value) => break value,
                Err(error)
                    if error.retry_without_json_mode
                        && !compatibility_retry
                        && attempt < MAX_IMAGE_ATTEMPTS =>
                {
                    body.as_object_mut().unwrap().remove("response_format");
                    compatibility_retry = true;
                    attempt += 1;
                }
                Err(error)
                    if retryable_image_request_error(&error) && attempt < MAX_IMAGE_ATTEMPTS =>
                {
                    let delay = image_retry_delay(error.retry_after, attempt, stage);
                    let status = error
                        .http_status
                        .map(|status| format!("HTTP {}", status.as_u16()))
                        .unwrap_or_else(|| "网络错误".into());
                    let message = format!(
                        "{status}，等待 {} 秒后重试此资源（下一次为第 {}/{} 次尝试）",
                        delay.as_secs_f32(),
                        attempt + 1,
                        MAX_IMAGE_ATTEMPTS
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
                    self.cancellable(tokio::time::sleep(delay)).await?;
                    attempt += 1;
                }
                Err(error) => return Err(error.error),
            }
        };
        let mut response = response;
        let item = response
            .pointer_mut("/data/0")
            .and_then(Value::as_object_mut)
            .context("生图接口未返回 data[0]")?;
        let encoded = match item.remove("b64_json") {
            Some(Value::String(encoded)) => Some(encoded),
            _ => None,
        };
        let image_url = match item.remove("url") {
            Some(Value::String(url)) => Some(url),
            _ => None,
        };
        drop(response);
        let bytes = if let Some(encoded) = encoded {
            self.run_image_postprocess(format!("{stage}：解码 base64"), move || {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .context("生图接口返回的 base64 无效")
            })
            .await?
        } else if let Some(url) = image_url {
            let url = Url::parse(&url).context("生图接口返回的 URL 无效")?;
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
                return Err(ApiRequestError::transient(anyhow::anyhow!(
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
                return Err(ApiRequestError::transient(
                    error.context(format!("{stage}请求失败：{endpoint}，模型 {model}")),
                ));
            }
        };
        let parse_stage = format!("{stage}：解析接口响应");
        let value = match if max_response_bytes == MAX_IMAGE_RESPONSE_BYTES {
            self.run_image_postprocess(parse_stage, move || {
                parse_api_response(&bytes, &content_type)
            })
            .await
        } else {
            parse_api_response(&bytes, &content_type)
        } {
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
                transient: false,
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

    async fn send_image_edit(
        &self,
        endpoint: &Url,
        api_key: &str,
        body: &Value,
        reference_urls: &[String],
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
            json!({ "stage": stage, "endpoint": endpoint.as_str(), "model": model, "referenceCount": reference_urls.len() }),
        );
        let form = multipart_image_form(body, reference_urls).map_err(ApiRequestError::new)?;
        let response = match self
            .client
            .post(endpoint.clone())
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
        {
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
                return Err(ApiRequestError::transient(anyhow::anyhow!(
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
                return Err(ApiRequestError::transient(
                    error.context(format!("{stage}请求失败：{endpoint}，模型 {model}")),
                ));
            }
        };
        let value = match self
            .run_image_postprocess(format!("{stage}：解析接口响应"), move || {
                parse_api_response(&bytes, &content_type)
            })
            .await
        {
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
                retry_without_json_mode: false,
                http_status: Some(status),
                retry_after,
                transient: false,
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
        self.reference_data_urls_from(references)
    }

    fn reference_data_urls_from(
        &self,
        references: Vec<ReferenceImage>,
    ) -> anyhow::Result<Vec<String>> {
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

    fn reference_path(&self, reference: &ReferenceImage) -> anyhow::Result<PathBuf> {
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
        Ok(path)
    }

    fn current_resource_reference_data_urls(
        &self,
        theme: &ThemeManifest,
        slot: GeneratedSlot,
    ) -> anyhow::Result<Vec<String>> {
        let mut references = Vec::new();
        for relative in current_resource_paths(theme, slot) {
            if references.len() >= 6 {
                break;
            }
            let Some(reference) = self.theme_asset_data_url(theme, &relative)? else {
                continue;
            };
            if !references.contains(&reference) {
                references.push(reference);
            }
        }
        Ok(references)
    }

    fn theme_asset_data_url(
        &self,
        theme: &ThemeManifest,
        relative: &str,
    ) -> anyhow::Result<Option<String>> {
        crate::theme::validate_theme_id(&theme.id)?;
        crate::theme::validate_image_asset_path(relative)?;
        let theme_root = self.paths.themes.join(&theme.id);
        let candidate = theme_root.join(relative);
        if !theme_root.is_dir() || !candidate.is_file() {
            return Ok(None);
        }
        let root = theme_root.canonicalize()?;
        let path = candidate.canonicalize()?;
        if !path.starts_with(&root) || !path.is_file() {
            bail!("当前资源路径无效");
        }
        let bytes = fs::read(&path)?;
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
            bail!("当前资源为空或超过 15 MB");
        }
        let mime = image_mime(&path)?;
        Ok(Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )))
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
            preview_path: String::new(),
            session,
            path,
        })
    }

    fn stage_pet_action_candidate(
        &self,
        slot: &str,
        bytes: &[u8],
    ) -> anyhow::Result<GeneratedAsset> {
        let session = uuid::Uuid::new_v4().simple().to_string();
        let file_stem = slot.strip_prefix("pet.").unwrap_or("action");
        let hash = format!("{:x}", Sha256::digest(bytes));
        let path = format!("assets/pet-{file_stem}-{}.png", &hash[..16]);
        let target = self.paths.staging.join(&session).join(&path);
        fs::create_dir_all(target.parent().context("动作候选路径无父目录")?)?;
        atomic_write(&target, bytes)?;
        Ok(GeneratedAsset {
            slot: slot.into(),
            preview_url: self.reference_preview_url(&session, &path)?,
            preview_path: String::new(),
            session,
            path,
        })
    }

    fn stage_action_icon_atlas(
        &self,
        bytes: &[u8],
    ) -> anyhow::Result<(GeneratedAsset, Vec<(GeneratedSlot, GeneratedAsset)>)> {
        let atlas_bytes = resize_and_encode_png(bytes, 1024, 1024)?;
        let atlas = self.stage_generated_asset(GeneratedSlot::ActionIconAtlas, &atlas_bytes)?;
        let mut icons = Vec::with_capacity(ICON_ACTIONS.len());
        let result = (|| -> anyhow::Result<()> {
            let image = image::load_from_memory(bytes)
                .context("无法解码系统图标图集")?
                .to_rgba8();
            let cell_width = image.width() / ICON_ATLAS_COLUMNS;
            let cell_height = image.height() / ICON_ATLAS_ROWS;
            if cell_width < 128 || cell_height < 128 {
                bail!("系统图标图集尺寸过小");
            }
            for (index, &(action, _)) in ICON_ACTIONS.iter().enumerate() {
                let index = u32::try_from(index).unwrap_or_default();
                let column = index % ICON_ATLAS_COLUMNS;
                let row = index / ICON_ATLAS_COLUMNS;
                let cell = image::imageops::crop_imm(
                    &image,
                    column * cell_width,
                    row * cell_height,
                    cell_width,
                    cell_height,
                )
                .to_image();
                let normalized = normalize_transparent_asset_with_min_subject(cell, 512, 512, 8)
                    .with_context(|| format!("图集第 {} 格（{action}）处理失败", index + 1))?;
                let bytes = encode_png(&normalized)?;
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
    transient: bool,
    error: anyhow::Error,
}

impl ApiRequestError {
    fn new(error: anyhow::Error) -> Self {
        Self {
            retry_without_json_mode: false,
            http_status: None,
            retry_after: None,
            transient: false,
            error,
        }
    }

    fn transient(error: anyhow::Error) -> Self {
        Self {
            transient: true,
            ..Self::new(error)
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
    ImageEdits,
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

#[derive(Clone, Copy)]
struct PetGenerationJob {
    slot: &'static str,
    label: &'static str,
    row: u32,
    frames: u32,
    trigger: &'static str,
    action: &'static str,
}

fn pet_supplemental_references(
    phase: &str,
    references: &[String],
    use_first_reference_as_base: bool,
) -> Vec<String> {
    if phase == "actions" {
        return Vec::new();
    }
    references
        .iter()
        .skip(usize::from(use_first_reference_as_base))
        .cloned()
        .collect()
}

fn pet_generation_jobs() -> [PetGenerationJob; 9] {
    [
        PetGenerationJob {
            slot: "pet.idle",
            label: "待机",
            row: 0,
            frames: 6,
            trigger: "the pet has no active notification and remains visible",
            action: "exactly six chronological frames: 1 neutral; 2 tiny inhale; 3 eyes close for one blink; 4 tiny exhale; 5 eyes reopen; 6 return almost exactly to frame 1",
        },
        PetGenerationJob {
            slot: "pet.running-right",
            label: "向右移动",
            row: 1,
            frames: 8,
            trigger: "the user drags the pet toward screen-right",
            action: "exactly eight chronological frames forming one compact short-step gait toward screen-right: four alternating contact and passing poses, then repeat those four phases once to reconnect to frame 1; keep the torso upright and never stretch the whole body horizontally",
        },
        PetGenerationJob {
            slot: "pet.running-left",
            label: "向左移动",
            row: 2,
            frames: 8,
            trigger: "the user drags the pet toward screen-left",
            action: "exactly eight chronological frames forming one compact short-step gait toward screen-left: four alternating contact and passing poses, then repeat those four phases once to reconnect to frame 1; keep the torso upright and never stretch the whole body horizontally",
        },
        PetGenerationJob {
            slot: "pet.waving",
            label: "挥手",
            row: 3,
            frames: 4,
            trigger: "the pet first appears or wakes and greets the user",
            action: "exactly four chronological frames: 1 neutral greeting pose; 2 raise one limb; 3 wave that same limb outward; 4 return close to frame 1",
        },
        PetGenerationJob {
            slot: "pet.jumping",
            label: "跳跃",
            row: 4,
            frames: 5,
            trigger: "the pointer hovers over the pet",
            action: "exactly five chronological frames completing one short hover response: 1 anticipation; 2 rise; 3 apex; 4 descend; 5 land and return to frame 1",
        },
        PetGenerationJob {
            slot: "pet.failed",
            label: "失败",
            row: 5,
            frames: 8,
            trigger: "the current task fails or becomes blocked",
            action: "exactly eight chronological frames completing one disappointed reaction: 1 attentive; 2 notice failure; 3 shoulders or ears lower; 4 deepen the reaction; 5 hold; 6 begin recovery; 7 settle; 8 reconnect to the subdued starting pose, without floating symbols",
        },
        PetGenerationJob {
            slot: "pet.waiting",
            label: "等待输入",
            row: 6,
            frames: 6,
            trigger: "Codex needs user input or approval before it can continue",
            action: "exactly six chronological frames: 1 attentive neutral; 2 look toward the user; 3 lean forward slightly; 4 make one small open asking gesture; 5 hold briefly; 6 return close to frame 1",
        },
        PetGenerationJob {
            slot: "pet.running",
            label: "任务处理中",
            row: 7,
            frames: 6,
            trigger: "Codex is actively thinking or processing the task",
            action: "exactly six chronological frames completing one focused working cycle: 1 stable working posture; 2 begin one small deliberate motion; 3 reach its midpoint; 4 complete that motion; 5 retract or settle; 6 return to frame 1; never run on foot and never begin a second unfinished action",
        },
        PetGenerationJob {
            slot: "pet.review",
            label: "检查结果",
            row: 8,
            frames: 6,
            trigger: "the task finishes and an unread result is ready for review",
            action: "exactly six chronological frames: 1 stable attentive posture; 2 focus on the result; 3 lean or tilt slightly; 4 hold the inspection; 5 make one small confirming nod; 6 return close to frame 1 without adding props",
        },
    ]
}

fn pet_base_prompt(
    theme: &ThemeManifest,
    user_prompt: &str,
    lock_first_reference_identity: bool,
) -> String {
    let reference_direction = if lock_first_reference_identity {
        "REFERENCE IDENTITY LOCK: the supplied reference images are mandatory subject identity evidence, not loose style inspiration. Reference image 1 is the primary authority. Create a full-body 2D completion of that exact same subject. Preserve its species, breed or character type, face geometry, head shape, eye shape and color, nose and mouth placement, ear shape, fur or skin colors, every visible marking, material, rendering style and distinctive asymmetry. If the primary image is cropped or close-up, infer only the unseen body while leaving every visible feature unchanged. Additional references may clarify the same identity, but if references conflict, image 1 wins. Never substitute a generic member of the same species, recolor it with the theme palette, beautify it, make it more chibi, change realism level, simplify markings or redesign the face."
    } else {
        "The supplied images, when present, are broad visual and style references rather than a mandatory pet identity. Use the theme palette as broad style grounding."
    };
    format!(
        "Create one centered full-body Codex pet identity reference. {reference_direction} User pet direction is mandatory and highest priority: {user_prompt}. Theme name: {}. Theme description: {}. Theme colors are {} foreground, {} accent and {} background; use them only when they do not alter a locked reference identity. Compact whole-body silhouette, consistent face, proportions, material, markings and props, readable inside a 192x208 sprite cell. Perfectly flat uniform pure #00FF00 chroma background touching all edges. No text, UI, scenery, floor, shadow, glow, detached effects or readable logo. Do not use #00FF00 in the pet.",
        theme.name,
        theme.description,
        theme.tokens.foreground,
        theme.tokens.accent,
        theme.tokens.content_background
    )
}

fn pet_row_prompt(user_prompt: &str, job: &PetGenerationJob) -> String {
    let layout = pet_strip_layout(job.frames);
    let source_slot_width = 1536 / layout.columns;
    let source_slot_height = 1024 / layout.rows;
    let mut production_height = source_slot_height * 76 / 100;
    let mut production_width = (production_height * crate::pet::PET_CELL_WIDTH
        + crate::pet::PET_CELL_HEIGHT / 2)
        / crate::pet::PET_CELL_HEIGHT;
    let maximum_width = source_slot_width * 76 / 100;
    if production_width > maximum_width {
        production_width = maximum_width;
        production_height = (production_width * crate::pet::PET_CELL_HEIGHT
            + crate::pet::PET_CELL_WIDTH / 2)
            / crate::pet::PET_CELL_WIDTH;
    }
    let unused_cells = layout.columns * layout.rows - job.frames;
    let action_guard = match job.slot {
        "pet.idle" => {
            " Idle-specific rule: keep the feet or lower-body anchor fixed. Only the chest, eyelids and at most a tiny head bob may move. Total body displacement must stay under 4% of the frame. Do not walk, wave, jump, reach, travel across the cell, swing a long limb or introduce any new handheld or floating prop."
        }
        "pet.waving" => {
            " Waving-specific rule: keep the feet and torso anchor fixed, and keep the head in the same registered position in all four frames. Only one chosen paw, hand, wing or limb performs the wave; the other limbs remain quiet. No alternating full-body poses."
        }
        "pet.running-right" | "pet.running-left" => {
            " Drag-movement-specific rule: use compact short steps under an upright torso. Keep the torso height and apparent body scale equal to idle. Legs, paws and tail may alternate, but the head and torso must never become a long horizontal leap, flying pose or stretched sprint silhouette. The character stays centered inside each sprite cell; motion across the desktop is performed by Codex itself."
        }
        "pet.waiting" => {
            " Waiting-specific rule: communicate a polite request for user input with one restrained open-palm, paw, ear or head gesture; keep the feet and torso anchor fixed, distinguish it from idle, and do not type, scan, review, run or use random gestures."
        }
        "pet.running" => {
            " Task-processing-specific rule: this means Codex is actively working, not locomotion; keep the feet and torso anchor fixed and use only a compact eye, head or existing-paw working motion. Do not walk, jog, bounce, alternate strides, wave or perform an asking gesture."
        }
        "pet.review" => {
            " Review-specific rule: communicate deliberate inspection and a small confirming nod; keep the feet and torso anchor fixed and do not scan repeatedly from side to side, type, run, wave, ask for input or add a magnifier, paper, screen, text or symbol."
        }
        _ => "",
    };
    let long_prop_guard = " Long-prop containment rule: preserve every sword, beam saber, wing, tail, antenna and other identity-defining long part, but angle it inward, upright or toward the body so its complete visible silhouette ends well inside the production box. Use natural foreshortening when needed; do not remove or redesign the prop and never point its tip toward a cell boundary.";
    format!(
        "Create one coherent animation pose sheet on a 1536x1024 API canvas with exactly {} poses and no visible grid. Arrange poses in a {} column by {} row grid, filled in strict row-major order from left to right and then top to bottom. The production target is fixed at 192x208 pixels per pet frame; the attached reference image's canvas dimensions, aspect ratio, margins, background and subject scale are identity evidence only and must never control the output geometry. Each grid cell is about {}x{} pixels. Inside every used cell, center one {}x{} production box with the same 192:208 aspect ratio as the final pet cell and keep the complete pet, including ears, tail, limbs and every attached prop, entirely inside that box. This leaves a minimum 12% uninterrupted pure-green safety zone on every side of every cell and a continuous pure-green corridor between neighboring poses. Keep every unused grid cell completely pure green. The attached pet identity reference is authoritative for the face, silhouette, proportions, palette, material, markings, existing props and apparent body scale in every pose. Animation continuity is mandatory: use one fixed camera, the same apparent torso and head size as the identity reference and idle action, one fixed ground baseline and one stable torso anchor across the complete row. Adjacent frames must be small chronological changes of the same action, never unrelated poses. The named action must begin, develop and finish entirely within exactly {} frames; do not begin a secondary cycle that cannot finish. Frame {} must reconnect naturally to frame 1 so playback never stops halfway or snaps. Real trigger meaning: {}.{} Action timeline: {}.{} User direction for this action: {}. Draw one complete separated full-body pose per used cell in temporal order, with consistent scale and baseline. No foreground pixel may touch or cross a cell boundary or enter another cell's safety zone. Fill the complete canvas and every gap with perfectly flat uniform pure #00FF00 chroma green. No text, labels, borders, guide marks, shadows, floors, glow, blur, speed lines, dust, detached effects, cropped body parts, overlap between cells or extra poses. Screen-left and screen-right are viewer coordinates. There are {} unused cells and they must contain only pure green.",
        job.frames,
        layout.columns,
        layout.rows,
        source_slot_width,
        source_slot_height,
        production_width,
        production_height,
        job.frames,
        job.frames,
        job.trigger,
        long_prop_guard,
        job.action,
        action_guard,
        user_prompt,
        unused_cells
    )
}

fn pet_strip_repair_prompt(base_prompt: &str, validation_error: &str, frames: u32) -> String {
    format!(
        "{base_prompt}\n\nDeterministic repair pass for all {frames} poses. The previous candidate failed: {validation_error}. Redraw the complete sheet from the identity references; do not crop, shift or reuse the failed sheet. Correct the pose rather than changing character identity or apparent body scale. If the subject was too small, restore the same head and torso size as the base identity and idle action, then make wide limbs or props compact by angling them inward with natural foreshortening. Never aim a long prop toward a neighboring cell. If an edge was touched, keep at least 12% perfectly pure #00FF00 padding on all four sides and a continuous pure-green corridor between cells. No ear, tail, limb, prop, effect or stray pixel may touch a cell edge, cross a boundary or enter an unused cell. Preserve the same pet identity, complete action timing, frame order and exact frame count. Keep one stable torso and lower-body anchor, use incremental neighboring poses, prevent size popping or random body shifts, complete the action before the last frame, and make the last frame reconnect smoothly to the first."
    )
}

fn pet_strip_dimensions(frames: u32) -> &'static str {
    match frames {
        4..=8 => "1536x1024",
        _ => "1536x1024",
    }
}

fn normalize_pet_identity_reference(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let image = image::load_from_memory(bytes)
        .context("宠物基础形象无法解码")?
        .to_rgba8();
    let target_width = crate::pet::PET_CELL_WIDTH * 4;
    let target_height = crate::pet::PET_CELL_HEIGHT * 4;
    if has_chroma_key_border(&image) {
        return encode_png(&normalize_transparent_asset_with_min_subject(
            image,
            target_width,
            target_height,
            12,
        )?);
    }

    let source = if image.pixels().any(|pixel| pixel[3] <= 18) {
        let bounds = alpha_bounds(&image).context("上传的宠物基础形象没有可见内容")?;
        image::imageops::crop_imm(
            &image,
            bounds.0,
            bounds.1,
            bounds.2 - bounds.0 + 1,
            bounds.3 - bounds.1 + 1,
        )
        .to_image()
    } else {
        image
    };
    encode_png(&fit_pet_identity_on_canvas(
        &source,
        target_width,
        target_height,
    ))
}

fn has_chroma_key_border(image: &RgbaImage) -> bool {
    let mut border = 0usize;
    let mut keyed = 0usize;
    for (x, y, pixel) in image.enumerate_pixels() {
        if x != 0 && y != 0 && x + 1 != image.width() && y + 1 != image.height() {
            continue;
        }
        border += 1;
        let [red, green, blue, _] = pixel.0;
        if green >= 240 && red <= 16 && blue <= 16 {
            keyed += 1;
        }
    }
    border > 0 && keyed * 4 >= border
}

fn fit_pet_identity_on_canvas(
    source: &RgbaImage,
    target_width: u32,
    target_height: u32,
) -> RgbaImage {
    let padding = target_width.min(target_height) * 7 / 100;
    let available_width = target_width.saturating_sub(padding * 2).max(1);
    let available_height = target_height.saturating_sub(padding * 2).max(1);
    let scale = (available_width as f32 / source.width() as f32)
        .min(available_height as f32 / source.height() as f32);
    let width = (source.width() as f32 * scale).round().max(1.0) as u32;
    let height = (source.height() as f32 * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(source, width, height, FilterType::Lanczos3);
    let mut canvas = RgbaImage::new(target_width, target_height);
    image::imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((target_width - width) / 2),
        i64::from((target_height - height) / 2),
    );
    canvas
}

fn pet_identity_cell(atlas: &[u8]) -> anyhow::Result<Vec<u8>> {
    let image = image::load_from_memory(atlas)
        .context("无法解码宠物图集")?
        .to_rgba8();
    if image.width() != crate::pet::PET_WIDTH
        || !matches!(
            image.height(),
            crate::pet::PET_V1_HEIGHT | crate::pet::PET_V2_HEIGHT
        )
    {
        bail!("宠物图集尺寸无效");
    }
    let cell = image::imageops::crop_imm(
        &image,
        0,
        0,
        crate::pet::PET_CELL_WIDTH,
        crate::pet::PET_CELL_HEIGHT,
    )
    .to_image();
    encode_png(&cell)
}

fn replace_pet_atlas_row(
    atlas: &mut RgbaImage,
    strip: &[u8],
    job: &PetGenerationJob,
) -> anyhow::Result<()> {
    if atlas.width() != crate::pet::PET_WIDTH
        || !matches!(
            atlas.height(),
            crate::pet::PET_V1_HEIGHT | crate::pet::PET_V2_HEIGHT
        )
        || job.row * crate::pet::PET_CELL_HEIGHT >= atlas.height()
    {
        bail!("宠物图集与动作版本不匹配");
    }
    let top = job.row * crate::pet::PET_CELL_HEIGHT;
    for y in top..top + crate::pet::PET_CELL_HEIGHT {
        for x in 0..crate::pet::PET_WIDTH {
            atlas.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
        }
    }
    for (column, frame) in extract_pet_strip_for_job(strip, job)?
        .into_iter()
        .enumerate()
    {
        image::imageops::overlay(
            atlas,
            &frame,
            i64::from(column as u32 * crate::pet::PET_CELL_WIDTH),
            i64::from(top),
        );
    }
    despill_pet_atlas(atlas);
    Ok(())
}

fn extract_pet_strip(bytes: &[u8], frames: u32) -> anyhow::Result<Vec<RgbaImage>> {
    extract_pet_strip_with_options(bytes, frames, false)
}

fn validate_pet_frame_scale(frames: &[RgbaImage]) -> anyhow::Result<()> {
    let mut heights = frames
        .iter()
        .filter_map(alpha_bounds)
        .map(|bounds| bounds.3 - bounds.1 + 1)
        .collect::<Vec<_>>();
    if heights.len() != frames.len() || heights.is_empty() {
        bail!("宠物动作存在空白帧");
    }
    heights.sort_unstable();
    let median = heights[heights.len() / 2];
    if median < 148 {
        bail!(
            "宠物动作主体高度只有 {median}px，明显小于统一目标尺寸，请保持躯干直立并使用紧凑动作"
        );
    }
    Ok(())
}

fn extract_pet_strip_for_job(
    bytes: &[u8],
    job: &PetGenerationJob,
) -> anyhow::Result<Vec<RgbaImage>> {
    extract_pet_strip_with_options(bytes, job.frames, job.slot == "pet.jumping")
}

fn extract_pet_strip_with_options(
    bytes: &[u8],
    frames: u32,
    preserve_vertical_motion: bool,
) -> anyhow::Result<Vec<RgbaImage>> {
    if !(4..=8).contains(&frames) {
        bail!("宠物动作帧数无效");
    }
    let image = image::load_from_memory(bytes)
        .context("无法解码宠物动作条带")?
        .to_rgba8();
    match extract_pet_grid_with_options(&image, frames, preserve_vertical_motion) {
        Ok(output) => Ok(output),
        Err(grid_error) => match extract_pet_adaptive_grid_with_options(
            &image,
            frames,
            preserve_vertical_motion,
        ) {
            Ok(output) => Ok(output),
            Err(adaptive_error) => {
                extract_pet_horizontal_strip_with_options(
                    &image,
                    frames,
                    preserve_vertical_motion,
                )
                .with_context(|| {
                    format!(
                        "固定网格切割失败：{grid_error:#}；逐行纯绿走廊切割失败：{adaptive_error:#}；兼容横向条带切割也失败"
                    )
                })
            }
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PetStripLayout {
    columns: u32,
    rows: u32,
}

fn pet_strip_layout(frames: u32) -> PetStripLayout {
    match frames {
        4 => PetStripLayout {
            columns: 2,
            rows: 2,
        },
        5 | 6 => PetStripLayout {
            columns: 3,
            rows: 2,
        },
        7 | 8 => PetStripLayout {
            columns: 4,
            rows: 2,
        },
        _ => PetStripLayout {
            columns: frames.max(1),
            rows: 1,
        },
    }
}

#[cfg(test)]
fn extract_pet_grid(image: &RgbaImage, frames: u32) -> anyhow::Result<Vec<RgbaImage>> {
    extract_pet_grid_with_options(image, frames, false)
}

fn extract_pet_grid_with_options(
    image: &RgbaImage,
    frames: u32,
    preserve_vertical_motion: bool,
) -> anyhow::Result<Vec<RgbaImage>> {
    let layout = pet_strip_layout(frames);
    if image.width() < layout.columns * 96 || image.height() < layout.rows * 192 {
        bail!("宠物动作网格尺寸过小");
    }
    let source_slot_width = image.width().div_ceil(layout.columns);
    let source_slot_height = image.height().div_ceil(layout.rows);
    let mut cells = Vec::with_capacity(frames as usize);
    for index in 0..layout.columns * layout.rows {
        let column = index % layout.columns;
        let row = index / layout.columns;
        let left = image.width() * column / layout.columns;
        let right = image.width() * (column + 1) / layout.columns;
        let top = image.height() * row / layout.rows;
        let bottom = image.height() * (row + 1) / layout.rows;
        let cell =
            image::imageops::crop_imm(image, left, top, right - left, bottom - top).to_image();
        if index >= frames {
            ensure_unused_pet_grid_cell(&cell, index + 1)?;
            continue;
        }
        ensure_pet_grid_cell_border(&cell, index + 1)?;
        cells.push(cell);
    }
    normalize_pet_animation_cells(
        cells,
        source_slot_width,
        source_slot_height,
        preserve_vertical_motion,
    )
}

#[cfg(test)]
fn extract_pet_adaptive_grid(image: &RgbaImage, frames: u32) -> anyhow::Result<Vec<RgbaImage>> {
    extract_pet_adaptive_grid_with_options(image, frames, false)
}

fn extract_pet_adaptive_grid_with_options(
    image: &RgbaImage,
    frames: u32,
    preserve_vertical_motion: bool,
) -> anyhow::Result<Vec<RgbaImage>> {
    let layout = pet_strip_layout(frames);
    if image.width() < layout.columns * 96 || image.height() < layout.rows * 192 {
        bail!("宠物动作网格尺寸过小");
    }
    let mut cells = Vec::with_capacity(frames as usize);
    let mut source_slot_width = image.width().div_ceil(layout.columns);
    let source_slot_height = image.height().div_ceil(layout.rows);
    for row in 0..layout.rows {
        let top = image.height() * row / layout.rows;
        let bottom = image.height() * (row + 1) / layout.rows;
        let row_image =
            image::imageops::crop_imm(image, 0, top, image.width(), bottom - top).to_image();
        let cuts = pet_strip_frame_cuts(&row_image, layout.columns)
            .with_context(|| format!("宠物动作网格第 {} 行没有稳定的纯绿列间隔", row + 1))?;
        for column in 0..layout.columns {
            let index = row * layout.columns + column;
            let left = cuts[column as usize];
            let right = cuts[column as usize + 1];
            let cell =
                image::imageops::crop_imm(&row_image, left, 0, right - left, row_image.height())
                    .to_image();
            if index >= frames {
                ensure_unused_pet_grid_cell(&cell, index + 1)?;
                continue;
            }
            ensure_pet_grid_cell_border(&cell, index + 1)?;
            source_slot_width = source_slot_width.max(cell.width());
            cells.push(cell);
        }
    }
    normalize_pet_animation_cells(
        cells,
        source_slot_width,
        source_slot_height,
        preserve_vertical_motion,
    )
}

fn extract_pet_horizontal_strip_with_options(
    image: &RgbaImage,
    frames: u32,
    preserve_vertical_motion: bool,
) -> anyhow::Result<Vec<RgbaImage>> {
    if image.width() < frames * 96 || image.height() < 192 {
        bail!("宠物动作横向条带尺寸过小");
    }
    let cuts = pet_strip_frame_cuts(image, frames)?;
    let mut cells = Vec::with_capacity(frames as usize);
    let mut source_slot_width = image.width().div_ceil(frames);
    for column in 0..frames {
        let left = cuts[column as usize];
        let right = cuts[column as usize + 1];
        let cell =
            image::imageops::crop_imm(image, left, 0, right - left, image.height()).to_image();
        source_slot_width = source_slot_width.max(cell.width());
        cells.push(cell);
    }
    normalize_pet_animation_cells(
        cells,
        source_slot_width,
        image.height(),
        preserve_vertical_motion,
    )
}

fn ensure_pet_grid_cell_border(cell: &RgbaImage, cell_number: u32) -> anyhow::Result<()> {
    let columns = pet_strip_foreground_columns(cell);
    let rows = pet_strip_foreground_rows(cell);
    let column_threshold = (cell.height() / 256).max(1);
    let row_threshold = (cell.width() / 256).max(1);
    let horizontal_border = (cell.width() / 128).clamp(2, 6) as usize;
    let vertical_border = (cell.height() / 128).clamp(2, 6) as usize;
    let touches_horizontal = columns[..horizontal_border]
        .iter()
        .chain(columns[columns.len() - horizontal_border..].iter())
        .any(|count| *count > column_threshold);
    let touches_vertical = rows[..vertical_border]
        .iter()
        .chain(rows[rows.len() - vertical_border..].iter())
        .any(|count| *count > row_threshold);
    if touches_horizontal || touches_vertical {
        bail!("宠物动作网格第 {cell_number} 格的主体或道具贴住边缘");
    }
    Ok(())
}

fn ensure_unused_pet_grid_cell(cell: &RgbaImage, cell_number: u32) -> anyhow::Result<()> {
    let foreground = pet_strip_foreground_columns(cell)
        .into_iter()
        .map(u64::from)
        .sum::<u64>();
    let area = u64::from(cell.width()) * u64::from(cell.height());
    if foreground * 200 > area {
        bail!("宠物动作网格第 {cell_number} 格应为空白但检测到额外内容");
    }
    Ok(())
}

fn pet_strip_frame_cuts(image: &RgbaImage, frames: u32) -> anyhow::Result<Vec<u32>> {
    let width = image.width();
    let slot_width = width / frames;
    let foreground = pet_strip_foreground_columns(image);
    let quiet_threshold = (image.height() / 180).max(2);
    let minimum_gap = (slot_width * 2 / 100).max(4);
    let outer_margin = (minimum_gap / 2).max(2) as usize;
    if foreground[..outer_margin]
        .iter()
        .any(|count| *count > quiet_threshold)
        || foreground[foreground.len() - outer_margin..]
            .iter()
            .any(|count| *count > quiet_threshold)
    {
        bail!("宠物动作条带的主体贴住画布边缘，请重新生成此动作");
    }

    let mut cuts = Vec::with_capacity(frames as usize + 1);
    cuts.push(0);
    let search_radius = (slot_width / 3).max(minimum_gap);
    for boundary in 1..frames {
        let expected = width * boundary / frames;
        let start = expected.saturating_sub(search_radius);
        let end = (expected + search_radius).min(width - 1);
        let mut best: Option<(u32, u32)> = None;
        let mut cursor = start;
        while cursor <= end {
            if foreground[cursor as usize] > quiet_threshold {
                cursor += 1;
                continue;
            }
            let gap_start = cursor;
            while cursor <= end && foreground[cursor as usize] <= quiet_threshold {
                cursor += 1;
            }
            let gap_end = cursor;
            if gap_end - gap_start < minimum_gap {
                continue;
            }
            let cut = gap_start + (gap_end - gap_start) / 2;
            let distance = cut.abs_diff(expected);
            if best
                .map(|(_, best_distance)| distance < best_distance)
                .unwrap_or(true)
            {
                best = Some((cut, distance));
            }
        }
        let (cut, _) = best.with_context(|| {
            format!(
                "宠物动作条带第 {boundary} 帧与下一帧之间没有足够的纯绿间隔，主体可能重叠或被裁切，请重新生成此动作"
            )
        })?;
        cuts.push(cut);
    }
    cuts.push(width);
    Ok(cuts)
}

fn pet_strip_foreground_columns(image: &RgbaImage) -> Vec<u32> {
    let mut columns = vec![0; image.width() as usize];
    for (x, _y, pixel) in image.enumerate_pixels() {
        let [red, green, blue, alpha] = pixel.0;
        let green_dominance = green.saturating_sub(red.max(blue));
        if alpha > 18 && !(green > 80 && green_dominance > 28) {
            columns[x as usize] += 1;
        }
    }
    columns
}

fn pet_strip_foreground_rows(image: &RgbaImage) -> Vec<u32> {
    let mut rows = vec![0; image.height() as usize];
    for (_x, y, pixel) in image.enumerate_pixels() {
        let [red, green, blue, alpha] = pixel.0;
        let green_dominance = green.saturating_sub(red.max(blue));
        if alpha > 18 && !(green > 80 && green_dominance > 28) {
            rows[y as usize] += 1;
        }
    }
    rows
}

fn image_data_url(bytes: &[u8]) -> anyhow::Result<String> {
    let image = image::load_from_memory(bytes).context("无法解码宠物参考图")?;
    let mut encoded = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, ImageFormat::Png)
        .context("无法编码宠物参考图")?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(encoded.into_inner())
    ))
}

fn save_pet_atlas(path: &Path, atlas: &RgbaImage) -> anyhow::Result<()> {
    // 宠物像素和透明边缘不能承受有损压缩；显式使用无损 WebP，避免
    // 不同 image 版本或默认编码器参数改变输出质量。
    let file = fs::File::create(path).context("无法创建宠物图集")?;
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(file);
    image::ImageEncoder::write_image(
        encoder,
        atlas.as_raw(),
        atlas.width(),
        atlas.height(),
        image::ExtendedColorType::Rgba8,
    )
    .context("无法编码无损宠物图集")
}

fn unique_pet_id(paths: &AppPaths, name: &str) -> String {
    let slug = name
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                byte.to_ascii_lowercase() as char
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = if slug.is_empty() || slug.len() > 56 {
        "ai-pet".to_string()
    } else {
        slug
    };
    if !paths.pets.join(&base).exists() {
        return base;
    }
    format!("{base}-{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
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
            Self::ActionIconAtlas => format!("{ICON_ATLAS_COLUMNS}:{ICON_ATLAS_ROWS}"),
            Self::HeroBadge
            | Self::Avatar
            | Self::Sticker
            | Self::Decoration(_)
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
            Self::ActionIconAtlas => "1536x1280".into(),
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
            Self::BackgroundSidebar => "左侧导航背景".into(),
            Self::Logo => "Logo".into(),
            Self::SidebarWatermark => "左侧导航水印".into(),
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
    let image = image::load_from_memory(bytes).context("无法解码待优化的生成资源")?;
    if !slot.requires_transparency() {
        let (max_width, max_height) = match slot {
            GeneratedSlot::BackgroundFullscreen | GeneratedSlot::BackgroundContent => (1600, 1600),
            GeneratedSlot::BackgroundSidebar => (640, 1280),
            GeneratedSlot::HeroImage => (1200, 600),
            _ => (1200, 1200),
        };
        return encode_jpeg(&resize_within(image, max_width, max_height), 84);
    }
    let image = image.to_rgba8();
    let (width, height) = match slot {
        GeneratedSlot::Logo => (768, 384),
        GeneratedSlot::SidebarWatermark => (384, 768),
        GeneratedSlot::ComposerDecoration => (768, 256),
        _ => (512, 512),
    };
    encode_png(&normalize_transparent_asset(image, width, height)?)
}

fn retryable_resource_processing_error(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}");
    [
        "绿幕",
        "纯绿背景",
        "有效主体",
        "检测到主体",
        "图标图集尺寸过小",
        "无法解码系统图标图集",
        "无法解码待优化的生成资源",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn resize_within(image: DynamicImage, max_width: u32, max_height: u32) -> DynamicImage {
    if image.width() <= max_width && image.height() <= max_height {
        image
    } else {
        image.resize(max_width, max_height, FilterType::Lanczos3)
    }
}

fn resize_and_encode_png(bytes: &[u8], max_width: u32, max_height: u32) -> anyhow::Result<Vec<u8>> {
    let image = image::load_from_memory(bytes).context("无法解码待优化的 PNG 资源")?;
    let image = resize_within(image, max_width, max_height).to_rgba8();
    encode_png(&image)
}

fn encode_jpeg(image: &DynamicImage, quality: u8) -> anyhow::Result<Vec<u8>> {
    let rgb = image.to_rgb8();
    let mut output = Vec::new();
    JpegEncoder::new_with_quality(&mut output, quality)
        .encode(
            &rgb,
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .context("无法编码优化后的 JPEG")?;
    Ok(output)
}

fn normalize_transparent_asset(
    image: RgbaImage,
    target_width: u32,
    target_height: u32,
) -> anyhow::Result<RgbaImage> {
    normalize_transparent_asset_with_min_subject(image, target_width, target_height, 100)
}

fn chroma_key_to_alpha(
    image: &mut RgbaImage,
    minimum_subject_basis_points: usize,
) -> anyhow::Result<(u32, u32, u32, u32)> {
    let pixel_count = image.width() as usize * image.height() as usize;
    let mut strengths = vec![0_u8; pixel_count];
    let mut hard_keyed = vec![false; pixel_count];
    for (index, pixel) in image.pixels().enumerate() {
        let [red, green, blue, _] = pixel.0;
        let dominance = green.saturating_sub(red.max(blue));
        hard_keyed[index] = is_pet_chroma_key_color(red, green, blue);
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
        if alpha <= 18 {
            pixel.0 = [0, 0, 0, 0];
        } else if alpha < 250 && green > red.max(blue) {
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
    if opaque == 0 || opaque * 10_000 < total * minimum_subject_basis_points {
        bail!("绿幕抠图后没有检测到有效主体");
    }
    alpha_bounds(image).context("绿幕抠图后没有检测到主体")
}

fn normalize_transparent_asset_with_min_subject(
    mut image: RgbaImage,
    target_width: u32,
    target_height: u32,
    minimum_subject_basis_points: usize,
) -> anyhow::Result<RgbaImage> {
    let bounds = chroma_key_to_alpha(&mut image, minimum_subject_basis_points)?;
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

fn normalize_pet_animation_cells(
    mut images: Vec<RgbaImage>,
    source_slot_width: u32,
    source_slot_height: u32,
    preserve_vertical_motion: bool,
) -> anyhow::Result<Vec<RgbaImage>> {
    if images.is_empty() || source_slot_width == 0 || source_slot_height == 0 {
        bail!("宠物动作没有可用帧");
    }
    let mut canvases = Vec::with_capacity(images.len());
    let mut bounds = Vec::with_capacity(images.len());
    for mut image in images.drain(..) {
        chroma_key_to_alpha(&mut image, 12)?;
        let mut canvas = RgbaImage::new(source_slot_width, source_slot_height);
        let left = i64::from(source_slot_width.saturating_sub(image.width()) / 2);
        let top = i64::from(source_slot_height.saturating_sub(image.height()) / 2);
        image::imageops::overlay(&mut canvas, &image, left, top);
        bounds.push(alpha_bounds(&canvas).context("绿幕抠图后没有检测到主体")?);
        canvases.push(canvas);
    }

    let target_width = crate::pet::PET_CELL_WIDTH;
    let target_height = crate::pet::PET_CELL_HEIGHT;
    let target_anchor_x = target_width as f32 / 2.0;
    let target_baseline = target_height as f32 - 16.0;

    let mut areas = canvases.iter().map(pet_opaque_area).collect::<Vec<_>>();
    areas.sort_unstable();
    let median_area = areas[areas.len() / 2].max(1) as f32;
    let corrections = canvases
        .iter()
        .map(|canvas| {
            (median_area / pet_opaque_area(canvas).max(1) as f32)
                .sqrt()
                .clamp(0.80, 1.25)
        })
        .collect::<Vec<_>>();

    let mut heights = bounds
        .iter()
        .map(|value| value.3 - value.1 + 1)
        .collect::<Vec<_>>();
    heights.sort_unstable();
    let median_height = heights[heights.len() / 2].max(1) as f32;
    let mut shared_scale = 168.0 / median_height;
    for ((canvas, bounds), correction) in canvases.iter().zip(&bounds).zip(&corrections) {
        let anchor_x = pet_body_anchor_x(canvas, *bounds);
        let left_extent = (anchor_x - bounds.0 as f32 + 1.0).max(1.0);
        let right_extent = (bounds.2 as f32 - anchor_x + 1.0).max(1.0);
        let height = (bounds.3 - bounds.1 + 1) as f32;
        shared_scale = shared_scale
            .min((target_anchor_x - 8.0) / (left_extent * correction))
            .min((target_width as f32 - target_anchor_x - 8.0) / (right_extent * correction))
            .min((target_baseline - 8.0) / (height * correction));
    }
    if !shared_scale.is_finite() || shared_scale <= 0.0 {
        bail!("宠物动作主体尺寸无效");
    }

    let frame_bottoms = bounds
        .iter()
        .map(|value| value.3 as f32)
        .collect::<Vec<_>>();
    let mut sorted_bottoms = bounds.iter().map(|value| value.3).collect::<Vec<_>>();
    sorted_bottoms.sort_unstable();
    let source_median_bottom = sorted_bottoms[sorted_bottoms.len() / 2] as f32;
    let mut output = Vec::with_capacity(canvases.len());
    for (((canvas, bounds), correction), source_bottom) in canvases
        .into_iter()
        .zip(bounds)
        .zip(corrections)
        .zip(frame_bottoms)
    {
        let anchor_x = pet_body_anchor_x(&canvas, bounds);
        let crop = image::imageops::crop_imm(
            &canvas,
            bounds.0,
            bounds.1,
            bounds.2 - bounds.0 + 1,
            bounds.3 - bounds.1 + 1,
        )
        .to_image();
        let scale = shared_scale * correction;
        let width = (crop.width() as f32 * scale).round().max(1.0) as u32;
        let height = (crop.height() as f32 * scale).round().max(1.0) as u32;
        let resized = image::imageops::resize(&crop, width, height, FilterType::Lanczos3);
        let resized_anchor_x = (anchor_x - bounds.0 as f32) * scale;
        let left = (target_anchor_x - resized_anchor_x).round() as i64;
        let baseline_offset = if preserve_vertical_motion {
            ((source_bottom - source_median_bottom) * shared_scale).clamp(-32.0, 32.0)
        } else {
            0.0
        };
        let top = (target_baseline + baseline_offset - height as f32 + 1.0).round() as i64;
        let mut frame = RgbaImage::new(target_width, target_height);
        image::imageops::overlay(&mut frame, &resized, left, top);
        output.push(frame);
    }
    Ok(output)
}

fn pet_opaque_area(image: &RgbaImage) -> u64 {
    image.pixels().filter(|pixel| pixel.0[3] > 18).count() as u64
}

fn pet_body_anchor_x(image: &RgbaImage, bounds: (u32, u32, u32, u32)) -> f32 {
    let height = bounds.3 - bounds.1 + 1;
    let band_top = bounds.1 + height * 35 / 100;
    let band_bottom = bounds.1 + height * 78 / 100;
    let mut columns = Vec::new();
    for y in band_top..=band_bottom.min(bounds.3) {
        for x in bounds.0..=bounds.2 {
            if image.get_pixel(x, y).0[3] > 18 {
                columns.push(x);
            }
        }
    }
    if columns.is_empty() {
        return (bounds.0 + bounds.2) as f32 / 2.0;
    }
    columns.sort_unstable();
    columns[columns.len() / 2] as f32
}

fn despill_pet_edges(image: &mut RgbaImage) {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let source = image.clone();
    let mut distance = vec![u8::MAX; width * height];
    let mut queue = VecDeque::new();
    for (index, pixel) in source.pixels().enumerate() {
        let [red, green, blue, alpha] = pixel.0;
        if alpha <= 18 || is_pet_chroma_key_color(red, green, blue) {
            distance[index] = 0;
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        let current = distance[index];
        if current >= 8 {
            continue;
        }
        let x = index % width;
        let y = index / width;
        for neighbor in [
            (x > 0).then(|| index - 1),
            (x + 1 < width).then(|| index + 1),
            (y > 0).then(|| index - width),
            (y + 1 < height).then(|| index + width),
        ]
        .into_iter()
        .flatten()
        {
            if distance[neighbor] == u8::MAX {
                distance[neighbor] = current + 1;
                queue.push_back(neighbor);
            }
        }
    }

    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let pixel = image.get_pixel_mut(x as u32, y as u32);
            let [red, green, blue, source_alpha] = pixel.0;
            if source_alpha <= 18 {
                pixel.0 = [0, 0, 0, 0];
                continue;
            }
            let neutral_green = red.max(blue);
            let dominance = green.saturating_sub(neutral_green);
            let boundary_spill = distance[index] <= 8 && green >= 96 && dominance > 18;
            let isolated_key = is_pet_chroma_key_color(red, green, blue);
            if !boundary_spill && !isolated_key {
                continue;
            }

            let alpha = if isolated_key || (boundary_spill && green >= 140 && dominance >= 72) {
                0
            } else {
                let strength =
                    ((u16::from(dominance.saturating_sub(18)) * 255 / 96).min(255)) as u8;
                ((u16::from(source_alpha) * u16::from(255 - strength)) / 255) as u8
            };
            if alpha <= 18 {
                pixel.0 = [0, 0, 0, 0];
                continue;
            }
            let replacement =
                nearest_clean_pet_color(&source, x, y, 8).unwrap_or([red, neutral_green, blue]);
            pixel.0 = [replacement[0], replacement[1], replacement[2], alpha];
        }
    }
}

fn is_pet_chroma_key_color(red: u8, green: u8, blue: u8) -> bool {
    let dominance = green.saturating_sub(red.max(blue));
    (green >= 240 && red <= 16 && blue <= 16)
        || (green >= 150 && dominance >= 96 && red <= 96 && blue <= 96)
}

fn nearest_clean_pet_color(
    image: &RgbaImage,
    x: usize,
    y: usize,
    max_radius: usize,
) -> Option<[u8; 3]> {
    let width = image.width() as usize;
    let height = image.height() as usize;
    for radius in 1..=max_radius {
        let left = x.saturating_sub(radius);
        let right = (x + radius).min(width - 1);
        let top = y.saturating_sub(radius);
        let bottom = (y + radius).min(height - 1);
        for candidate_y in top..=bottom {
            for candidate_x in left..=right {
                if candidate_x != left
                    && candidate_x != right
                    && candidate_y != top
                    && candidate_y != bottom
                {
                    continue;
                }
                let [red, green, blue, alpha] =
                    image.get_pixel(candidate_x as u32, candidate_y as u32).0;
                if alpha > 128 && green.saturating_sub(red.max(blue)) <= 18 {
                    return Some([red, green, blue]);
                }
            }
        }
    }
    None
}

fn despill_pet_atlas(atlas: &mut RgbaImage) {
    let columns = atlas.width() / crate::pet::PET_CELL_WIDTH;
    let rows = atlas.height() / crate::pet::PET_CELL_HEIGHT;
    for row in 0..rows {
        for column in 0..columns {
            let mut frame = image::imageops::crop_imm(
                atlas,
                column * crate::pet::PET_CELL_WIDTH,
                row * crate::pet::PET_CELL_HEIGHT,
                crate::pet::PET_CELL_WIDTH,
                crate::pet::PET_CELL_HEIGHT,
            )
            .to_image();
            despill_pet_edges(&mut frame);
            image::imageops::replace(
                atlas,
                &frame,
                i64::from(column * crate::pet::PET_CELL_WIDTH),
                i64::from(row * crate::pet::PET_CELL_HEIGHT),
            );
        }
    }
}

#[cfg(test)]
fn normalize_pet_animation_cell(image: RgbaImage) -> anyhow::Result<RgbaImage> {
    let source_slot_width = image.width();
    let source_slot_height = image.height();
    normalize_pet_animation_cells(vec![image], source_slot_width, source_slot_height, false)?
        .pop()
        .context("宠物动作没有可用帧")
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
         Never use transparent-looking checkerboard, white, gray, or a framed square background. The icon subject itself must not be black or near-black and must maintain at least 4.5:1 contrast against the theme's input and accent surfaces. No solid circular or square button plate. Simple recognizable silhouette, consistent stroke weight, generous padding, no border frame.",
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
        "Create a precise 6 column by 5 row sprite atlas for a cohesive desktop IDE action icon set. \
         Use exactly thirty equal cells in row-major order: {meanings}. Theme: {}. Style: {}. \
         Every cell contains one centered icon with identical scale, stroke weight and generous padding. \
         The entire canvas and every gap must be solid pure chroma green #00FF00. \
         Render the icon subjects in a bright high-contrast color derived from the theme foreground/accent; never use black or near-black subjects and never draw circular or square button plates. No text, labels, dividers, frames, shadows, gradients, checkerboard or extra objects.",
        blueprint.name.trim(),
        if blueprint.icon_style.trim().is_empty() {
            "clean geometric product iconography"
        } else {
            blueprint.icon_style.trim()
        }
    )
}

fn resource_generation_prompt(
    slot: GeneratedSlot,
    user_prompt: &str,
    theme: &ThemeManifest,
) -> String {
    if slot != GeneratedSlot::ActionIconAtlas {
        return user_prompt.to_string();
    }
    let meanings = ICON_ACTIONS
        .iter()
        .enumerate()
        .map(|(index, (action, meaning))| format!("cell {}: {action} ({meaning})", index + 1))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "User style direction: {user_prompt}. Create one exact 6 column by 5 row sprite atlas with thirty equal cells in this fixed row-major order: {meanings}. Theme colors: foreground {}, accent {}, sidebar background {}. Every cell must contain exactly one centered, clearly visible action icon occupying 35% to 60% of that cell, with consistent scale, stroke weight and padding. Fill the complete canvas, including every cell background and gap, with uniform pure chroma green #00FF00. Do not draw text, labels, grid lines, dividers, frames, button plates, shadows, gradients, checkerboards or empty cells.",
        theme.tokens.foreground, theme.tokens.accent, theme.tokens.sidebar_background
    )
}

fn resource_processing_retry_prompt(slot: GeneratedSlot, original_prompt: &str) -> String {
    if slot == GeneratedSlot::ActionIconAtlas {
        format!(
            "{original_prompt}. RETRY REQUIREMENT: preserve the exact 6 column by 5 row atlas and fixed row-major order. Draw all thirty icons, one centered icon in every cell, each occupying 40% to 65% of its cell. Every cell and every gap must touch the same flat pure #00FF00 background. No missing or tiny icons, no merged cells, no single large subject, no text, grid lines, dividers, frames or button plates."
        )
    } else {
        format!(
            "{original_prompt}. RETRY REQUIREMENT: draw exactly one large, clearly visible subject centered in the canvas, occupying 45% to 70% of the image area. Keep a wide uninterrupted pure #00FF00 border around it. No tiny subject, no green subject, no scattered objects, no empty canvas."
        )
    }
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
        request.theme_name.trim(),
        request.theme_description.trim(),
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
    hash.update([u8::from(request.resource_plans_only)]);
    if request.resource_plans_only
        && let Some(theme) = &request.theme
    {
        hash.update(serde_json::to_vec(theme).unwrap_or_default());
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

fn pet_generation_fingerprint(
    request: &GeneratePetRequest,
    references: &[String],
    credentials: &AiCredentials,
) -> anyhow::Result<String> {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&request.theme)?);
    hash.update([0]);
    for value in [
        request.name.trim(),
        request.description.trim(),
        request.prompt.trim(),
        credentials.effective_image_base_url(),
        &credentials.image_model,
    ] {
        hash.update(value.as_bytes());
        hash.update([0]);
    }
    hash.update([u8::from(request.lock_first_reference_identity)]);
    hash.update([0]);
    hash.update(request.action_prompt.trim().as_bytes());
    hash.update([0]);
    for (slot, prompt) in &request.action_prompts {
        hash.update(slot.as_bytes());
        hash.update([0]);
        hash.update(prompt.trim().as_bytes());
        hash.update([0]);
    }
    for reference in references {
        hash.update(reference.as_bytes());
        hash.update([0]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn resource_plan_generation_context(
    request: &GenerateRequest,
    reference_count: usize,
) -> anyhow::Result<(
    ThemeManifest,
    Vec<(GeneratedSlot, String)>,
    BTreeMap<String, Vec<usize>>,
)> {
    let theme = request
        .theme
        .clone()
        .context("仅生成资源计划时缺少当前主题")?;
    theme.validate()?;
    let plans = request
        .resource_plans
        .iter()
        .map(|plan| {
            GeneratedSlot::from_asset_slot(&plan.slot)
                .map(|slot| (slot, plan.prompt.clone()))
                .context("批量资源计划包含不支持的资源槽")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let reference_assignments = plans
        .iter()
        .map(|(slot, _)| {
            (
                slot.asset_slot(),
                default_reference_indexes(reference_count),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok((theme, plans, reference_assignments))
}

fn validate_requested_metadata(request: &GenerateRequest) -> anyhow::Result<()> {
    let name = request.theme_name.trim();
    let description = request.theme_description.trim();
    if name.chars().count() > 80 || description.chars().count() > 240 {
        bail!("主题名称或介绍超过长度限制");
    }
    Ok(())
}

fn prompt_with_requested_metadata(request: &GenerateRequest) -> String {
    format!(
        "{}\n\n用户指定的最终主题名称：{}\n用户指定的最终主题介绍：{}\n视觉蓝图、品牌文案和所有图片资源必须围绕以上名称、介绍与原始需求保持一致。",
        request.prompt.trim(),
        request.theme_name.trim(),
        request.theme_description.trim()
    )
}

fn apply_requested_metadata(
    theme: &mut ThemeManifest,
    request: &GenerateRequest,
) -> anyhow::Result<()> {
    validate_requested_metadata(request)?;
    let name = request.theme_name.trim();
    let description = request.theme_description.trim();
    if !name.is_empty() {
        theme.name = name.into();
        theme.skin.brand.title = name.into();
    }
    if !description.is_empty() {
        theme.description = description.into();
        theme.skin.brand.subtitle = description.into();
    }
    Ok(())
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

fn fallback_resource_plans(blueprint: &ThemeBlueprint) -> Vec<(GeneratedSlot, String)> {
    vec![(
        GeneratedSlot::HeroImage,
        format!(
            "Create a polished hero image for the {} desktop coding theme. Theme description: {}. Use the requested palette, mood, materials and visual direction. Keep the composition calm enough for readable interface content and do not include text or UI screenshots.",
            blueprint.name.trim(),
            blueprint.description.trim()
        ),
    )]
}

fn bind_user_art_direction(resource_prompt: &str, user_prompt: &str) -> String {
    format!(
        "{resource_prompt}\n\nBinding original user specification (must be followed in full; do not omit, generalize, or reinterpret any explicit requirement): {user_prompt}"
    )
}

fn default_reference_indexes(reference_count: usize) -> Vec<usize> {
    (1..=reference_count).collect()
}

fn normalize_reference_indexes(indexes: &[usize], reference_count: usize) -> Vec<usize> {
    let mut normalized = indexes
        .iter()
        .copied()
        .filter(|index| (1..=reference_count).contains(index))
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized.dedup();
    if normalized.is_empty() {
        default_reference_indexes(reference_count)
    } else {
        normalized
    }
}

fn selected_reference_urls(references: &[String], indexes: &[usize]) -> Vec<String> {
    indexes
        .iter()
        .filter_map(|index| references.get(index.saturating_sub(1)).cloned())
        .collect()
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
    theme.chrome.right_sidebar_background = Some(theme.tokens.input_background.clone());
    theme.chrome.right_sidebar_foreground = Some(accessible_foreground_for_surface(
        &theme.tokens.foreground,
        &theme.tokens.input_background,
        4.5,
    )?);
    theme.chrome.right_sidebar_muted_foreground = Some(accessible_foreground_for_surface(
        &theme.tokens.muted_foreground,
        &theme.tokens.input_background,
        3.0,
    )?);
    theme.chrome.right_sidebar_icon = theme.chrome.right_sidebar_foreground.clone();
    theme.chrome.right_sidebar_border = Some(border_for_surface(
        &theme.tokens.border,
        &theme.tokens.input_background,
        &theme.tokens.foreground,
    )?);
    theme.chrome.right_sidebar_hover = Some(safe_hover_color(
        &theme.tokens.input_background,
        &theme.tokens.foreground,
    )?);
    theme.chrome.right_sidebar_active = Some(theme.tokens.active_background.clone());
    theme.chrome.right_sidebar_active_foreground = Some(accessible_foreground_for_surface(
        &theme.tokens.foreground,
        &theme.tokens.active_background,
        4.5,
    )?);
    theme.chrome.right_sidebar_shortcut_background = Some(theme.tokens.elevated_background.clone());
    theme.chrome.right_sidebar_shortcut_foreground = Some(accessible_foreground_for_surface(
        &theme.tokens.muted_foreground,
        &theme.tokens.elevated_background,
        3.0,
    )?);
    theme.layout.density = blueprint.style.density.clamp(0.75, 1.35);
    theme.layout.sidebar_width = blueprint
        .style
        .sidebar_width
        .clamp(180, MAX_GENERATED_SIDEBAR_WIDTH);
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

fn current_resource_paths(theme: &ThemeManifest, slot: GeneratedSlot) -> Vec<String> {
    let background_path = |background: &RegionBackground| match &background.source {
        BackgroundSource::Image { path, .. } => Some(path.clone()),
        _ => None,
    };
    let resource = |value: &Option<String>| value.iter().cloned().collect::<Vec<_>>();
    match slot {
        GeneratedSlot::BackgroundFullscreen => background_path(&theme.background.fullscreen)
            .into_iter()
            .collect(),
        GeneratedSlot::BackgroundContent => background_path(&theme.background.content)
            .into_iter()
            .collect(),
        GeneratedSlot::BackgroundSidebar => background_path(&theme.background.sidebar)
            .into_iter()
            .collect(),
        GeneratedSlot::Logo => resource(&theme.skin.resources.logo),
        GeneratedSlot::SidebarWatermark => resource(&theme.skin.resources.sidebar_watermark),
        GeneratedSlot::HeroImage => resource(&theme.skin.resources.hero_image),
        GeneratedSlot::HeroBadge => resource(&theme.skin.resources.hero_badge),
        GeneratedSlot::Avatar => resource(&theme.skin.resources.avatar),
        GeneratedSlot::Sticker => resource(&theme.skin.resources.sticker),
        GeneratedSlot::ComposerDecoration => resource(&theme.skin.resources.composer_decoration),
        GeneratedSlot::Decoration(index) => theme
            .skin
            .decorations
            .get(index)
            .map(|decoration| decoration.asset.clone())
            .filter(|path| !path.is_empty())
            .into_iter()
            .collect(),
        GeneratedSlot::ActionIconAtlas => ICON_ACTIONS
            .iter()
            .filter_map(|(action, _)| theme.skin.icons.mappings.get(*action).cloned())
            .collect(),
        GeneratedSlot::ActionIcon(action) => {
            let mut paths = Vec::new();
            if let Some(path) = theme.skin.icons.mappings.get(action) {
                paths.push(path.clone());
            }
            for (candidate, _) in ICON_ACTIONS {
                if paths.len() >= 6 {
                    break;
                }
                if candidate == action {
                    continue;
                }
                if let Some(path) = theme.skin.icons.mappings.get(candidate)
                    && !paths.contains(path)
                {
                    paths.push(path.clone());
                }
            }
            paths
        }
        GeneratedSlot::HomeCardIcon(index) => theme
            .skin
            .home
            .cards
            .get(index)
            .and_then(|card| card.icon.clone())
            .into_iter()
            .collect(),
    }
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
    pub reference_indexes: Vec<usize>,
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
        "prompt": format!("Create a polished visual asset for a desktop coding application theme. The supplied user art direction is mandatory: preserve every named subject, palette, material, mood, composition, decoration, and negative constraint; do not replace it with a generic interpretation. Target canvas dimensions: {target_dimensions} pixels. Keep the composition at a {aspect_ratio} aspect ratio. Background and UI ambience assets must be crisp and inspectable, not blurred or foggy; use clear shapes, readable contrast, and low haze. Do not design or cover the app title bar, native task header, send button, settings controls, or window controls. No readable text, no UI screenshot, no watermark. Mandatory user art direction: {prompt}. Mandatory output constraint: {background_direction}"),
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

fn multipart_image_form(body: &Value, reference_urls: &[String]) -> anyhow::Result<Form> {
    let mut form = Form::new();
    for key in [
        "model",
        "prompt",
        "size",
        "background",
        "output_format",
        "response_format",
    ] {
        if let Some(value) = body.get(key).and_then(Value::as_str) {
            form = form.text(key.to_string(), value.to_string());
        }
    }
    form = form.text("n", "1");
    for (index, reference_url) in reference_urls.iter().enumerate() {
        let (mime, bytes) = decode_reference_data_url(reference_url)?;
        let extension = match mime {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            "image/bmp" => "bmp",
            _ => bail!("不支持的参考图格式"),
        };
        let part = Part::bytes(bytes)
            .file_name(format!("reference-{}.{}", index + 1, extension))
            .mime_str(mime)
            .context("参考图 MIME 类型无效")?;
        form = if reference_urls.len() == 1 {
            form.part("image", part)
        } else {
            form.part("image[]", part)
        };
    }
    Ok(form)
}

fn decode_reference_data_url(value: &str) -> anyhow::Result<(&'static str, Vec<u8>)> {
    let (metadata, encoded) = value
        .strip_prefix("data:")
        .and_then(|value| value.split_once(";base64,"))
        .context("参考图数据格式无效")?;
    let mime = match metadata {
        "image/png" => "image/png",
        "image/jpeg" => "image/jpeg",
        "image/webp" => "image/webp",
        "image/gif" => "image/gif",
        "image/bmp" => "image/bmp",
        _ => bail!("不支持的参考图格式"),
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("参考图无法解码")?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        bail!("参考图为空或超过 15 MB");
    }
    Ok((mime, bytes))
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
        EndpointKind::ImageEdits => "images/edits",
    };
    let path = if current.ends_with("/responses")
        || current.ends_with("/chat/completions")
        || current.ends_with("/images/generations")
        || current.ends_with("/images/edits")
    {
        let prefix = current
            .trim_end_matches("/responses")
            .trim_end_matches("/chat/completions")
            .trim_end_matches("/images/generations")
            .trim_end_matches("/images/edits");
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

fn retryable_image_request_error(error: &ApiRequestError) -> bool {
    error.transient
        || matches!(
            error.http_status,
            Some(
                StatusCode::REQUEST_TIMEOUT
                    | StatusCode::TOO_MANY_REQUESTS
                    | StatusCode::INTERNAL_SERVER_ERROR
                    | StatusCode::BAD_GATEWAY
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::GATEWAY_TIMEOUT
            )
        )
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
  "style":{"radius":0-40,"density":0.75-1.35,"sidebarWidth":180-360,"contentMaxWidth":480-2400,"heroHeight":160-720,"surfaceOpacity":0.25-0.68,"blur":0-12},
  "background":"none|solid|gradient|image|per-region",
  "iconStyle":"max 500 chars, concise English direction for a cohesive transparent-background IDE icon set",
  "assets":[{"slot":"allowed slot","prompt":"English visual prompt, no text or watermark","referenceIndexes":[1]}]
}
Allowed image slots: background.fullscreen, background.content, background.sidebar, skin.logo, skin.heroImage, skin.heroBadge, skin.avatar, skin.sticker, skin.composerDecoration. Do not include skin.sidebarWatermark unless the user explicitly asks for a watermark; the application owns that opt-in.
Treat the user's original wording as a binding specification. Before composing JSON, internally enumerate every explicit requested theme, subject, color, material, mood, layout, decoration, and prohibition. Preserve those requirements in the palette and structure, and repeat the relevant concrete visual constraints inside every assets.prompt. When references are supplied, derive their actual dominant/supporting colors, material language, composition, focal placement, whitespace, decoration density, and lighting; do not substitute a generic genre-adjacent design. User text wins only where it conflicts with a reference; otherwise satisfy both. The supplied references are numbered in order starting at 1. For every image asset, assign the specific source references that should guide that asset in referenceIndexes; never leave this field out when references are supplied. Use an empty array only when an asset must intentionally avoid all references.
Use strict valid JSON syntax with double-quoted property names and strings. Every palette value must be exactly seven ASCII characters in #RRGGBB form; never emit shorthand, named colors, rgb(), alpha, #RRGGBBAA, transparent, or gradients in palette fields. Use the language explicitly requested by the user for every user-facing name, description, brand string, home title, subtitle, and card string. Use at most 4 cards and at most 6 assets. Prefer skin.heroImage plus only assets that materially improve the requested design. Never design, describe, or restyle the operating-system title bar, Codex task header, window controls, settings controls, send button, permission controls, or other native interaction chrome; those remain native and must stay clearly recognizable. System action icons and sidebar watermarks are controlled by explicit application options; never add them to assets automatically. Background and Hero images must be clear, crisp, and recognizable, not heavily blurred ambience; keep the lower-left text area relatively calm and low-detail, and prefer blur 0-4 and surfaceOpacity 0.25-0.44 so imagery remains visible while protected text surfaces stay readable.
Treat readability as a hard constraint, not an aesthetic suggestion. Calculate WCAG 2 relative-luminance contrast before returning JSON: palette.foreground must be at least 4.5:1 against app, sidebar, content, elevated, and input; muted must be at least 3:1 against all five surfaces; accentForeground must be at least 4.5:1 against accent; codeForeground must be at least 7:1 against codeBackground. The accent/accentForeground pair is used by primary controls including the send button, so its icon must remain unmistakable in default, hover, active, and disabled states. Title-bar labels, the Codex task-header title and controls, sidebar labels, terminal text, editor text, and code-review text must never use the same or a near-identical color as their background. Code-review additions, deletions, unchanged rows, inline highlights, and selected rows must retain at least 4.5:1 text contrast and avoid competing saturated red/green fills. Borders, icons, focus indicators, and essential controls should reach 3:1 against adjacent surfaces. Verify both default and hover/active states, and keep native light settings cards readable even when the requested theme is dark. Do not imitate a living artist, include copyrighted logos, or request readable text inside generated images."##
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
fn default_pet_generation_phase() -> String {
    "complete".into()
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

    fn test_generate_request(prompt: &str) -> GenerateRequest {
        GenerateRequest {
            prompt: prompt.into(),
            theme_name: "指定主题".into(),
            theme_description: "指定介绍".into(),
            language: "zh-CN".into(),
            generate_images: true,
            generate_sidebar_watermark: false,
            generate_system_icons: false,
            image_concurrency: 2,
            reference_session: None,
            reference_path: None,
            references: Vec::new(),
            resource_plans: Vec::new(),
            resource_plans_only: false,
            theme: None,
        }
    }

    fn test_pet_request(prompt: &str) -> GeneratePetRequest {
        GeneratePetRequest {
            theme: ThemeManifest::neutral_dark(),
            name: "Checkpoint Pet".into(),
            description: "Checkpoint test".into(),
            prompt: prompt.into(),
            action_prompt: String::new(),
            action_prompts: BTreeMap::new(),
            references: Vec::new(),
            image_concurrency: 3,
            resume: false,
            phase: "complete".into(),
            lock_first_reference_identity: false,
            use_first_reference_as_base: false,
        }
    }

    fn pet_test_strip(frames: u32) -> Vec<u8> {
        let width = frames * 128;
        let mut strip = RgbaImage::from_pixel(width, 256, image::Rgba([0, 255, 0, 255]));
        for column in 0..frames {
            let color = image::Rgba([
                40 + (column * 20) as u8,
                30,
                180u8.saturating_sub((column * 10) as u8),
                255,
            ]);
            for y in 48..224 {
                for x in column * 128 + 24..column * 128 + 104 {
                    strip.put_pixel(x, y, color);
                }
            }
        }
        encode_png(&strip).unwrap()
    }

    fn pet_test_grid(frames: u32) -> Vec<u8> {
        let layout = pet_strip_layout(frames);
        let cell_width = 256;
        let cell_height = 256;
        let mut sheet = RgbaImage::from_pixel(
            layout.columns * cell_width,
            layout.rows * cell_height,
            image::Rgba([0, 255, 0, 255]),
        );
        for index in 0..frames {
            let column = index % layout.columns;
            let row = index / layout.columns;
            let color = image::Rgba([
                40 + (index * 20) as u8,
                30,
                180u8.saturating_sub((index * 10) as u8),
                255,
            ]);
            for y in row * cell_height + 24..row * cell_height + 232 {
                for x in column * cell_width + 40..column * cell_width + 216 {
                    sheet.put_pixel(x, y, color);
                }
            }
        }
        encode_png(&sheet).unwrap()
    }

    fn pet_test_shifted_grid() -> Vec<u8> {
        let mut sheet = RgbaImage::from_pixel(512, 512, image::Rgba([0, 255, 0, 255]));
        for y in 40..216 {
            for x in 40..216 {
                sheet.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
            for x in 296..472 {
                sheet.put_pixel(x, y, image::Rgba([40, 100, 220, 255]));
            }
        }
        for y in 296..472 {
            for x in 40..270 {
                sheet.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
            for x in 290..472 {
                sheet.put_pixel(x, y, image::Rgba([40, 100, 220, 255]));
            }
        }
        encode_png(&sheet).unwrap()
    }

    #[test]
    fn pet_prompts_use_final_cell_geometry_not_reference_dimensions() {
        let jobs = pet_generation_jobs();
        let four_frame = jobs.iter().find(|job| job.frames == 4).unwrap();
        let eight_frame = jobs.iter().find(|job| job.frames == 8).unwrap();
        let four_prompt = pet_row_prompt("keep identity", four_frame);
        let eight_prompt = pet_row_prompt("keep identity", eight_frame);
        assert!(four_prompt.contains("fixed at 192x208 pixels"));
        assert!(four_prompt.contains("2 column by 2 row grid"));
        assert!(eight_prompt.contains("4 column by 2 row grid"));
        assert!(four_prompt.contains("359x389 production box"));
        assert!(eight_prompt.contains("291x315 production box"));
        assert!(four_prompt.contains("minimum 12% uninterrupted pure-green safety zone"));
        assert!(four_prompt.contains("must never control the output geometry"));
    }

    #[test]
    fn pet_base_prompt_locks_explicit_reference_identity() {
        let theme = ThemeManifest::neutral_light();
        let locked = pet_base_prompt(&theme, "生成同一只猫的全身形象", true);
        assert!(locked.contains("REFERENCE IDENTITY LOCK"));
        assert!(locked.contains("Reference image 1 is the primary authority"));
        assert!(locked.contains("infer only the unseen body"));
        assert!(locked.contains("Never substitute a generic member of the same species"));
        assert!(locked.contains("recolor it with the theme palette"));

        let style_only = pet_base_prompt(&theme, "生成一只主题宠物", false);
        assert!(!style_only.contains("REFERENCE IDENTITY LOCK"));
        assert!(style_only.contains("broad visual and style references"));
    }

    #[test]
    fn pet_idle_prompt_keeps_motion_compact_and_prop_free() {
        let idle = pet_generation_jobs()
            .into_iter()
            .find(|job| job.slot == "pet.idle")
            .unwrap();
        let prompt = pet_row_prompt("keep identity", &idle);
        assert!(prompt.contains("Idle-specific rule"));
        assert!(prompt.contains("Do not walk, wave, jump"));
        assert!(prompt.contains("introduce any new handheld or floating prop"));
        assert!(prompt.contains("Frame 6 must reconnect naturally to frame 1"));
        assert!(prompt.contains("Total body displacement must stay under 4%"));
    }

    #[test]
    fn pet_stationary_action_prompts_define_semantics_and_stable_anchors() {
        let jobs = pet_generation_jobs();
        for (slot, required) in [
            ("pet.waving", "Only one chosen paw, hand, wing or limb"),
            ("pet.waiting", "polite request for user input"),
            ("pet.running", "not locomotion"),
            ("pet.review", "small confirming nod"),
        ] {
            let job = jobs.iter().find(|job| job.slot == slot).unwrap();
            let prompt = pet_row_prompt("keep identity", job);
            assert!(prompt.contains(required), "missing state rule for {slot}");
            assert!(prompt.contains("keep the feet and torso anchor fixed"));
        }
    }

    #[test]
    fn pet_generation_uses_only_nine_real_codex_actions() {
        let jobs = pet_generation_jobs();
        assert_eq!(jobs.len(), 9);
        assert!(jobs.iter().all(|job| job.row < 9));
        assert!(!jobs.iter().any(|job| job.slot.contains("look")));
        for job in jobs {
            let prompt = pet_row_prompt("keep identity", &job);
            assert!(prompt.contains(&format!("exactly {} frames", job.frames)));
            assert!(prompt.contains(job.trigger));
            assert!(prompt.contains(&format!(
                "Frame {} must reconnect naturally to frame 1",
                job.frames
            )));
        }
    }

    #[test]
    fn pet_animation_cells_keep_fixed_camera_scale() {
        let mut narrow = RgbaImage::from_pixel(384, 416, image::Rgba([0, 255, 0, 255]));
        for y in 80..336 {
            for x in 144..240 {
                narrow.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
        }
        let mut wide = narrow.clone();
        for y in 160..240 {
            for x in 96..288 {
                wide.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
        }
        let narrow = normalize_pet_animation_cell(narrow).unwrap();
        let wide = normalize_pet_animation_cell(wide).unwrap();
        let narrow_bounds = alpha_bounds(&narrow).unwrap();
        let wide_bounds = alpha_bounds(&wide).unwrap();
        assert_eq!(
            narrow_bounds.3 - narrow_bounds.1,
            wide_bounds.3 - wide_bounds.1
        );
        assert!(wide_bounds.2 - wide_bounds.0 > narrow_bounds.2 - narrow_bounds.0);
    }

    #[test]
    fn pet_animation_cells_stabilize_scale_anchor_and_baseline() {
        let mut cells = Vec::new();
        for (left, top, width, height) in
            [(28, 64, 72, 144), (178, 112, 108, 216), (92, 84, 90, 180)]
        {
            let mut cell = RgbaImage::from_pixel(384, 416, image::Rgba([0, 255, 0, 255]));
            for y in top..top + height {
                for x in left..left + width {
                    cell.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
                }
            }
            cells.push(cell);
        }

        let frames = normalize_pet_animation_cells(cells, 384, 416, false).unwrap();
        let bounds = frames
            .iter()
            .map(alpha_bounds)
            .collect::<Option<Vec<_>>>()
            .unwrap();
        let heights = bounds
            .iter()
            .map(|bounds| bounds.3 - bounds.1 + 1)
            .collect::<Vec<_>>();
        let baselines = bounds.iter().map(|bounds| bounds.3).collect::<Vec<_>>();
        let anchors = frames
            .iter()
            .zip(&bounds)
            .map(|(frame, bounds)| pet_body_anchor_x(frame, *bounds).round() as i32)
            .collect::<Vec<_>>();

        assert!(
            heights.iter().max().unwrap() - heights.iter().min().unwrap() <= 2,
            "normalized heights drifted: {heights:?}"
        );
        assert!(baselines.iter().max().unwrap() - baselines.iter().min().unwrap() <= 1);
        assert!(anchors.iter().max().unwrap() - anchors.iter().min().unwrap() <= 1);
    }

    #[test]
    fn pet_frame_scale_rejects_rows_that_would_visibly_shrink() {
        let normal = RgbaImage::from_pixel(192, 208, image::Rgba([0, 0, 0, 0]));
        let mut small = normal.clone();
        for y in 70..190 {
            for x in 66..126 {
                small.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
        }
        let error = validate_pet_frame_scale(&vec![small; 6]).unwrap_err();
        assert!(format!("{error:#}").contains("明显小于统一目标尺寸"));

        let mut consistent = normal;
        for y in 25..190 {
            for x in 56..136 {
                consistent.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
        }
        validate_pet_frame_scale(&vec![consistent; 6]).unwrap();
    }

    #[test]
    fn pet_animation_resize_does_not_leave_visible_green_edges() {
        let mut cell = RgbaImage::from_pixel(384, 416, image::Rgba([0, 255, 0, 255]));
        for y in 70..350 {
            for x in 105..279 {
                cell.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
            cell.put_pixel(105, y, image::Rgba([32, 208, 12, 255]));
        }

        let mut frame = normalize_pet_animation_cell(cell).unwrap();
        despill_pet_edges(&mut frame);
        assert!(frame.pixels().all(|pixel| {
            let [red, green, blue, alpha] = pixel.0;
            alpha <= 18 || green.saturating_sub(red.max(blue)) <= 18
        }));
        assert!(
            frame
                .pixels()
                .all(|pixel| pixel.0[3] > 18 || pixel.0[..3] == [0, 0, 0])
        );
    }

    #[test]
    fn pet_chroma_cleanup_removes_enclosed_green_seams_after_webp_round_trip() {
        let mut frame = RgbaImage::new(192, 208);
        for y in 30..170 {
            for x in 40..150 {
                frame.put_pixel(x, y, image::Rgba([222, 145, 78, 255]));
            }
        }
        for y in 150..170 {
            for x in 84..=92 {
                frame.put_pixel(x, y, image::Rgba([38, 227, 12, 255]));
            }
        }
        for y in 170..198 {
            for x in 40..84 {
                frame.put_pixel(x, y, image::Rgba([222, 145, 78, 255]));
            }
            for x in 93..150 {
                frame.put_pixel(x, y, image::Rgba([222, 145, 78, 255]));
            }
        }
        frame.put_pixel(60, 80, image::Rgba([80, 130, 50, 255]));

        despill_pet_edges(&mut frame);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cleaned.webp");
        DynamicImage::ImageRgba8(frame)
            .save_with_format(&path, ImageFormat::WebP)
            .unwrap();
        let decoded = image::open(path).unwrap().to_rgba8();

        assert!(decoded.pixels().all(|pixel| {
            let [red, green, blue, alpha] = pixel.0;
            alpha <= 18 || green < 140 || green.saturating_sub(red.max(blue)) <= 18
        }));
        assert_eq!(decoded.get_pixel(60, 80).0, [80, 130, 50, 255]);
    }

    #[test]
    fn pet_chroma_cleanup_handles_external_atlas_when_configured() {
        let Ok(path) = std::env::var("THEME_INJECT_TEST_PET_CHROMA") else {
            return;
        };
        let mut atlas = image::open(path).unwrap().to_rgba8();
        let before = visible_pet_chroma_pixels(&atlas);
        despill_pet_atlas(&mut atlas);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cleaned-atlas.webp");
        DynamicImage::ImageRgba8(atlas)
            .save_with_format(&path, ImageFormat::WebP)
            .unwrap();
        let decoded = image::open(path).unwrap().to_rgba8();
        let after = visible_pet_chroma_pixels(&decoded);

        assert!(
            before > 0,
            "external atlas did not contain a chroma fixture"
        );
        assert_eq!(after, 0, "visible chroma pixels remain after cleanup");
    }

    fn visible_pet_chroma_pixels(image: &RgbaImage) -> usize {
        image
            .pixels()
            .filter(|pixel| {
                let [red, green, blue, alpha] = pixel.0;
                alpha > 18 && green >= 140 && green.saturating_sub(red.max(blue)) > 18
            })
            .count()
    }

    #[test]
    fn pet_strip_repair_prompt_includes_failure_and_stricter_spacing() {
        let prompt = pet_strip_repair_prompt("base prompt", "第 2 格主体或道具贴住边缘", 6);
        assert!(prompt.contains("第 2 格主体或道具贴住边缘"));
        assert!(prompt.contains("all 6 poses"));
        assert!(prompt.contains("restore the same head and torso size"));
        assert!(prompt.contains("continuous pure-green corridor"));
        assert!(prompt.contains("with natural foreshortening"));
        assert!(prompt.contains("Never aim a long prop toward a neighboring cell"));
    }

    #[test]
    fn uploaded_pet_identity_is_fitted_to_canonical_cell_ratio() {
        let source = RgbaImage::from_pixel(1600, 400, image::Rgba([220, 40, 80, 255]));
        let normalized = normalize_pet_identity_reference(&encode_png(&source).unwrap()).unwrap();
        let image = image::load_from_memory(&normalized).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (768, 832));
        let bounds = alpha_bounds(&image).unwrap();
        assert!(bounds.0 > 0 && bounds.1 > 0);
        assert!(bounds.2 < image.width() - 1 && bounds.3 < image.height() - 1);
    }

    #[test]
    fn pet_strips_split_supported_frame_counts() {
        for frames in [4, 5, 6, 8] {
            let cells = extract_pet_strip(&pet_test_strip(frames), frames).unwrap();
            assert_eq!(cells.len(), frames as usize);
            assert!(cells.iter().all(|cell| cell.dimensions() == (192, 208)));
            assert!(
                cells
                    .iter()
                    .all(|cell| cell.pixels().any(|pixel| pixel[3] > 0))
            );
        }
    }

    #[test]
    fn pet_grids_split_supported_frame_counts() {
        for frames in [4, 5, 6, 8] {
            let cells = extract_pet_strip(&pet_test_grid(frames), frames).unwrap();
            assert_eq!(cells.len(), frames as usize);
            assert!(cells.iter().all(|cell| cell.dimensions() == (192, 208)));
            assert!(
                cells
                    .iter()
                    .all(|cell| cell.pixels().any(|pixel| pixel[3] > 0))
            );
        }
    }

    #[test]
    fn pet_horizontal_strip_accepts_narrow_safe_green_gaps() {
        let frames = 4;
        let width = frames * 128;
        let mut strip = RgbaImage::from_pixel(width, 256, image::Rgba([0, 255, 0, 255]));
        for column in 0..frames {
            for y in 48..224 {
                for x in column * 128 + 4..column * 128 + 124 {
                    strip.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
                }
            }
        }
        let cells = extract_pet_strip(&encode_png(&strip).unwrap(), frames).unwrap();
        assert_eq!(cells.len(), frames as usize);
    }

    #[test]
    fn pet_grid_rejects_subject_touching_cell_boundary() {
        let frames = 4;
        let mut sheet = image::load_from_memory(&pet_test_grid(frames))
            .unwrap()
            .to_rgba8();
        let cell_width = sheet.width() / 2;
        for y in 40..sheet.height() / 2 - 40 {
            sheet.put_pixel(cell_width - 1, y, image::Rgba([220, 40, 80, 255]));
        }
        assert!(extract_pet_grid(&sheet, frames).is_err());
    }

    #[test]
    fn pet_adaptive_grid_uses_each_rows_real_green_corridors() {
        let frames = 4;
        let sheet = image::load_from_memory(&pet_test_shifted_grid())
            .unwrap()
            .to_rgba8();

        assert!(extract_pet_grid(&sheet, frames).is_err());
        let cells = extract_pet_adaptive_grid(&sheet, frames).unwrap();
        assert_eq!(cells.len(), frames as usize);
        assert!(cells.iter().all(|cell| cell.dimensions() == (192, 208)));
    }

    #[test]
    fn pet_grid_rejects_extra_pose_in_unused_cell() {
        let frames = 5;
        let mut sheet = image::load_from_memory(&pet_test_grid(frames))
            .unwrap()
            .to_rgba8();
        let layout = pet_strip_layout(frames);
        let cell_width = sheet.width() / layout.columns;
        let cell_height = sheet.height() / layout.rows;
        for y in cell_height + 80..cell_height + 176 {
            for x in cell_width * 2 + 80..cell_width * 2 + 176 {
                sheet.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
        }
        assert!(extract_pet_grid(&sheet, frames).is_err());
    }

    #[test]
    fn pet_strip_uses_real_green_gaps_instead_of_fixed_boundaries() {
        let frames = 4;
        let width = frames * 128;
        let mut strip = RgbaImage::from_pixel(width, 256, image::Rgba([0, 255, 0, 255]));
        for y in 48..224 {
            for x in 24..133 {
                strip.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
            for x in 148..232 {
                strip.put_pixel(x, y, image::Rgba([40, 100, 220, 255]));
            }
        }
        let cuts = pet_strip_frame_cuts(&strip, frames).unwrap();
        assert!(cuts[1] > 133 && cuts[1] < 148);
    }

    #[test]
    fn pet_strip_rejects_overlapping_pose_groups() {
        let frames = 4;
        let width = frames * 128;
        let mut strip = RgbaImage::from_pixel(width, 256, image::Rgba([0, 255, 0, 255]));
        for y in 48..224 {
            for x in 24..232 {
                strip.put_pixel(x, y, image::Rgba([220, 40, 80, 255]));
            }
        }
        assert!(pet_strip_frame_cuts(&strip, frames).is_err());
    }

    #[test]
    fn pet_strip_rejects_an_empty_slot() {
        let mut strip = image::load_from_memory(&pet_test_strip(4))
            .unwrap()
            .to_rgba8();
        for y in 0..strip.height() {
            for x in strip.width() / 2..strip.width() * 3 / 4 {
                strip.put_pixel(x, y, image::Rgba([0, 255, 0, 255]));
            }
        }
        assert!(extract_pet_strip(&encode_png(&strip).unwrap(), 4).is_err());
    }

    #[test]
    fn generated_pet_atlas_leaves_unused_slots_transparent() {
        let jobs = pet_generation_jobs();
        let mut atlas = RgbaImage::new(crate::pet::PET_WIDTH, crate::pet::PET_V2_HEIGHT);
        for job in &jobs {
            for column in 0..job.frames {
                let frame = RgbaImage::from_pixel(192, 208, image::Rgba([20, 40, 80, 255]));
                image::imageops::overlay(
                    &mut atlas,
                    &frame,
                    i64::from(column * crate::pet::PET_CELL_WIDTH),
                    i64::from(job.row * crate::pet::PET_CELL_HEIGHT),
                );
            }
            for column in job.frames..8 {
                assert_eq!(
                    atlas.get_pixel(
                        column * crate::pet::PET_CELL_WIDTH,
                        job.row * crate::pet::PET_CELL_HEIGHT
                    )[3],
                    0
                );
            }
        }
    }

    #[test]
    fn unique_pet_id_avoids_existing_library_entry() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        assert_eq!(unique_pet_id(&paths, "My Pet"), "my-pet");
        fs::create_dir_all(paths.pets.join("my-pet")).unwrap();
        let next = unique_pet_id(&paths, "My Pet");
        assert!(next.starts_with("my-pet-"));
        assert_ne!(next, "my-pet");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn image_postprocess_queue_serializes_jobs() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service = std::sync::Arc::new(
            AiThemeService::new(paths, "http://127.0.0.1:1".into(), "token".into()).unwrap(),
        );
        let active = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let peak = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for index in 0..4 {
            let service = service.clone();
            let active = active.clone();
            let peak = peak.clone();
            tasks.push(tokio::spawn(async move {
                service
                    .run_image_postprocess(format!("测试任务 {index}"), move || {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(current, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(20));
                        active.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    })
                    .await
                    .unwrap();
            }));
        }

        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn pet_checkpoint_round_trip_preserves_completed_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        let mut request = test_pet_request("blue robot");
        request.action_prompt = "动作保持克制".into();
        let mut checkpoint = PetGenerationCheckpoint {
            fingerprint: "pet-fingerprint".into(),
            request,
            pet_id: "checkpoint-pet".into(),
            session: uuid::Uuid::new_v4().simple().to_string(),
            completed: BTreeSet::new(),
        };
        fs::create_dir_all(paths.staging.join(&checkpoint.session)).unwrap();
        service
            .commit_pet_checkpoint_artifact(&mut checkpoint, "pet.base", &pet_test_strip(4))
            .unwrap();

        let restored = service
            .load_pet_checkpoint("pet-fingerprint")
            .unwrap()
            .unwrap();
        assert!(restored.completed.contains("pet.base"));
        assert!(
            service
                .pet_checkpoint_artifact(&restored, "pet.base")
                .is_file()
        );
        let preview = service
            .pet_checkpoint_generated_asset(&restored, "pet.base")
            .unwrap();
        assert_eq!(preview.path, "assets/pet-base.png");
        assert_eq!(preview.preview_path, "assets/previews/base.png");
        assert!(preview.preview_url.contains("/staging/"));
        assert!(preview.preview_url.contains("/assets/previews/base.png"));
        let thumbnail =
            image::open(service.pet_checkpoint_preview_artifact(&restored, "pet.base")).unwrap();
        assert!(thumbnail.width() <= 384 && thumbnail.height() <= 256);
        assert!(
            service
                .load_active_pet_checkpoint(&restored.request.theme.id)
                .unwrap()
                .is_some()
        );
        assert!(
            service
                .load_active_pet_checkpoint("builtin.neutral-light")
                .unwrap()
                .is_none()
        );
        let generation = service
            .restore_pet_generation(&restored.request.theme.id)
            .unwrap()
            .unwrap();
        assert_eq!(generation.stage, "base_ready");
        assert_eq!(generation.action_prompt, "动作保持克制");
        assert_eq!(generation.base.path, "assets/pet-base.png");
        assert!(
            !service
                .discard_pet_generation("builtin.neutral-light")
                .unwrap()
        );
        assert!(
            service
                .discard_pet_generation(&restored.request.theme.id)
                .unwrap()
        );
        assert!(!paths.ai_pet_generation.exists());
        assert!(!paths.staging.join(restored.session).exists());
    }

    #[test]
    fn pet_validation_source_is_previewable_without_completing_action() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        let checkpoint = PetGenerationCheckpoint {
            fingerprint: "pet-fingerprint".into(),
            request: test_pet_request("blue robot"),
            pet_id: "checkpoint-pet".into(),
            session: uuid::Uuid::new_v4().simple().to_string(),
            completed: BTreeSet::new(),
        };
        fs::create_dir_all(paths.staging.join(&checkpoint.session)).unwrap();

        let asset = service
            .stage_pet_validation_source(&checkpoint, "pet.running-right", &pet_test_grid(8))
            .unwrap();
        assert!(asset.preview_url.contains("/staging/"));
        assert!(
            paths
                .staging
                .join(&checkpoint.session)
                .join(&asset.path)
                .is_file()
        );
        assert!(!checkpoint.completed.contains("pet.running-right"));

        service.clear_pet_validation_source(&checkpoint, "pet.running-right");
        assert!(
            !service
                .pet_validation_source_artifact(&checkpoint, "pet.running-right")
                .exists()
        );
    }

    #[tokio::test]
    async fn pet_resume_reuses_cached_candidate_with_shifted_row_gap() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        let checkpoint = PetGenerationCheckpoint {
            fingerprint: "pet-fingerprint".into(),
            request: test_pet_request("blue robot"),
            pet_id: "checkpoint-pet".into(),
            session: uuid::Uuid::new_v4().simple().to_string(),
            completed: BTreeSet::new(),
        };
        fs::create_dir_all(paths.staging.join(&checkpoint.session)).unwrap();
        service
            .stage_pet_validation_source(&checkpoint, "pet.running-right", &pet_test_shifted_grid())
            .unwrap();

        let reused = service
            .reuse_pet_validation_source(
                &checkpoint,
                "pet.running-right",
                4,
                "宠物动作：向右移动",
                "向右移动动作行无法切分",
            )
            .await;
        assert!(reused.is_some());
    }

    #[test]
    fn pet_fingerprint_allows_new_concurrency_but_rejects_changed_content() {
        let credentials = AiCredentials {
            base_url: "https://api.example.com/v1".into(),
            image_base_url: String::new(),
            text_model: "text".into(),
            vision_model: "vision".into(),
            image_model: "gpt-image-2".into(),
            image_size: "1:1".into(),
            api_key: "base".into(),
            image_api_key: Some("image".into()),
        };
        let original = test_pet_request("blue robot");
        let mut resumed = original.clone();
        resumed.image_concurrency = 1;
        resumed.resume = true;
        assert_eq!(
            pet_generation_fingerprint(&original, &[], &credentials).unwrap(),
            pet_generation_fingerprint(&resumed, &[], &credentials).unwrap()
        );
        resumed.prompt = "red robot".into();
        assert_ne!(
            pet_generation_fingerprint(&original, &[], &credentials).unwrap(),
            pet_generation_fingerprint(&resumed, &[], &credentials).unwrap()
        );
        assert_ne!(
            pet_generation_fingerprint(
                &original,
                &["data:image/png;base64,AA".into()],
                &credentials
            )
            .unwrap(),
            pet_generation_fingerprint(
                &original,
                &["data:image/png;base64,BB".into()],
                &credentials
            )
            .unwrap()
        );
    }

    #[test]
    fn pet_action_generation_uses_no_supplemental_reference_images() {
        let references = vec!["base".into(), "pose".into()];
        assert!(pet_supplemental_references("actions", &references, false).is_empty());
        assert!(pet_supplemental_references("actions", &references, true).is_empty());
        assert_eq!(
            pet_supplemental_references("complete", &references, true),
            vec!["pose".to_string()]
        );
    }

    #[test]
    fn image_retry_covers_transport_and_temporary_server_failures() {
        assert!(retryable_image_request_error(&ApiRequestError::transient(
            anyhow::anyhow!("connection timed out")
        )));
        for status in [
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
        ] {
            assert!(retryable_image_request_error(&ApiRequestError {
                retry_without_json_mode: false,
                http_status: Some(status),
                retry_after: None,
                transient: false,
                error: anyhow::anyhow!("temporary"),
            }));
        }
        assert!(!retryable_image_request_error(&ApiRequestError {
            retry_without_json_mode: false,
            http_status: Some(StatusCode::BAD_REQUEST),
            retry_after: None,
            transient: false,
            error: anyhow::anyhow!("invalid request"),
        }));
    }

    #[test]
    fn planning_resets_previous_progress_and_caps_image_concurrency() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths, "http://127.0.0.1:1".into(), "token".into()).unwrap();
        service
            .generation_progress
            .lock()
            .unwrap()
            .items
            .push(GenerationProgressItem {
                slot: "skin.resources.heroImage".into(),
                label: "Hero 图片".into(),
                prompt: "旧主题资源".into(),
                status: "completed".into(),
                preview_url: String::new(),
                session: String::new(),
                path: String::new(),
                message: String::new(),
                reference_indexes: Vec::new(),
            });
        service.set_generation_state("planning", "开始新主题");
        assert!(service.generation_progress().items.is_empty());
        assert_eq!(default_image_concurrency(), 4);
        assert_eq!(123usize.clamp(1, MAX_IMAGE_GENERATION_CONCURRENCY), 4);
    }

    #[test]
    fn requested_metadata_overrides_theme_and_brand() {
        let mut theme = theme_from_blueprint(&blueprint()).unwrap();
        let request = test_generate_request("粉丝主题");
        apply_requested_metadata(&mut theme, &request).unwrap();
        assert_eq!(theme.name, "指定主题");
        assert_eq!(theme.description, "指定介绍");
        assert_eq!(theme.skin.brand.title, "指定主题");
        assert_eq!(theme.skin.brand.subtitle, "指定介绍");
        assert!(prompt_with_requested_metadata(&request).contains("指定主题"));
    }

    #[test]
    fn generation_fingerprint_changes_with_user_metadata() {
        let credentials = AiCredentials {
            base_url: "https://example.com/v1".into(),
            image_base_url: String::new(),
            text_model: "text".into(),
            vision_model: "vision".into(),
            image_model: "image".into(),
            image_size: "3:2".into(),
            api_key: "key".into(),
            image_api_key: Some("image-key".into()),
        };
        let original = test_generate_request("同一提示词");
        let mut renamed = test_generate_request("同一提示词");
        renamed.theme_name = "另一个主题".into();
        assert_ne!(
            generation_fingerprint(&original, &[], &credentials),
            generation_fingerprint(&renamed, &[], &credentials)
        );
    }

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

    #[test]
    fn generated_theme_clamps_sidebar_width() {
        let mut blueprint = blueprint();
        blueprint.style.sidebar_width = 520;
        let theme = theme_from_blueprint(&blueprint).unwrap();
        assert_eq!(theme.layout.sidebar_width, MAX_GENERATED_SIDEBAR_WIDTH);
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
            theme.chrome.right_sidebar_foreground.as_deref().unwrap(),
            theme.chrome.right_sidebar_background.as_deref().unwrap(),
            4.5,
        );
        assert_contrast(
            theme
                .chrome
                .right_sidebar_muted_foreground
                .as_deref()
                .unwrap(),
            theme.chrome.right_sidebar_background.as_deref().unwrap(),
            3.0,
        );
        assert_contrast(
            theme
                .chrome
                .right_sidebar_shortcut_foreground
                .as_deref()
                .unwrap(),
            theme
                .chrome
                .right_sidebar_shortcut_background
                .as_deref()
                .unwrap(),
            3.0,
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
    fn resource_prompt_keeps_original_user_constraints() {
        let prompt = bind_user_art_direction(
            "paint a dark coding background",
            "必须有红色机甲，禁止模糊，不要蓝色",
        );
        assert!(prompt.contains("必须有红色机甲，禁止模糊，不要蓝色"));
        assert!(prompt.contains("must be followed in full"));
        assert!(prompt.contains("do not omit, generalize, or reinterpret"));
    }

    #[test]
    fn reference_assignments_are_one_based_and_never_empty() {
        assert_eq!(normalize_reference_indexes(&[3, 1, 1, 9], 3), vec![1, 3]);
        assert_eq!(normalize_reference_indexes(&[], 2), vec![1, 2]);
        assert_eq!(normalize_reference_indexes(&[0, 8], 2), vec![1, 2]);
        let references = vec!["one".into(), "two".into(), "three".into()];
        assert_eq!(
            selected_reference_urls(&references, &[3, 1]),
            vec!["three".to_string(), "one".to_string()]
        );
    }

    #[test]
    fn blueprint_assets_preserve_reference_indexes() {
        let blueprint = serde_json::from_value::<ThemeBlueprint>(json!({
            "name": "Reference plan",
            "palette": {
                "app": "#101010", "sidebar": "#121212", "content": "#141414",
                "elevated": "#181818", "input": "#1A1A1A", "foreground": "#F5F5F5",
                "muted": "#BBBBBB", "border": "#555555", "accent": "#22CCCC",
                "accentForeground": "#001010", "codeBackground": "#080808", "codeForeground": "#EEEEEE"
            },
            "assets": [{ "slot": "skin.heroImage", "prompt": "use the subject", "referenceIndexes": [2] }]
        })).unwrap();
        assert_eq!(blueprint.assets[0].reference_indexes, vec![2]);
    }

    #[test]
    fn fallback_resource_plan_exists_without_blueprint_assets() {
        let mut blueprint = blueprint();
        blueprint.assets.clear();
        blueprint.home.cards.clear();
        let plans = fallback_resource_plans(&blueprint);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].0, GeneratedSlot::HeroImage);
        assert!(plans[0].1.contains("Aurora Studio"));
    }

    #[test]
    fn multipart_reference_data_url_is_decoded_safely() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([1_u8, 2, 3]);
        let (mime, bytes) =
            decode_reference_data_url(&format!("data:image/png;base64,{encoded}")).unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, vec![1, 2, 3]);
        assert!(decode_reference_data_url("data:text/plain;base64,QQ==").is_err());
    }

    #[test]
    fn current_resource_references_follow_theme_icon_mappings() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        let mut theme = ThemeManifest::neutral_dark();
        theme.id = "local.current-reference".into();
        theme.skin.icons.mode = IconMode::Custom;
        theme
            .skin
            .icons
            .mappings
            .insert("sidebar-toggle".into(), "assets/sidebar-toggle.png".into());
        theme
            .skin
            .icons
            .mappings
            .insert("search".into(), "assets/search.png".into());
        let assets = paths.themes.join(&theme.id).join("assets");
        fs::create_dir_all(&assets).unwrap();
        let toggle_image = encode_png(&RgbaImage::from_pixel(
            32,
            32,
            image::Rgba([32, 96, 224, 255]),
        ))
        .unwrap();
        let search_image = encode_png(&RgbaImage::from_pixel(
            32,
            32,
            image::Rgba([224, 96, 32, 255]),
        ))
        .unwrap();
        fs::write(assets.join("sidebar-toggle.png"), &toggle_image).unwrap();
        fs::write(assets.join("search.png"), &search_image).unwrap();

        assert_eq!(
            current_resource_paths(&theme, GeneratedSlot::ActionIcon("search")),
            vec!["assets/search.png", "assets/sidebar-toggle.png"]
        );
        assert_eq!(
            current_resource_paths(&theme, GeneratedSlot::ActionIcon("plugins")),
            vec!["assets/sidebar-toggle.png", "assets/search.png"]
        );
        let references = service
            .current_resource_reference_data_urls(&theme, GeneratedSlot::ActionIconAtlas)
            .unwrap();
        assert_eq!(references.len(), 2);
        assert!(
            references
                .iter()
                .all(|reference| reference.starts_with("data:image/png;base64,"))
        );
    }

    #[test]
    fn current_resource_reference_flag_is_backward_compatible() {
        let request: GenerateResourceRequest = serde_json::from_value(json!({
            "theme": ThemeManifest::neutral_dark(),
            "slot": "skin.icons.search",
            "prompt": "regenerate search icon",
            "references": []
        }))
        .unwrap();
        assert!(!request.use_current_resource_reference);
        assert_eq!(
            GeneratedSlot::from_asset_slot(&request.slot),
            Some(GeneratedSlot::ActionIcon("search"))
        );
    }

    #[test]
    fn resource_processing_retry_only_covers_generated_image_failures() {
        assert!(retryable_resource_processing_error(&anyhow::anyhow!(
            "绿幕抠图后没有检测到有效主体"
        )));
        assert!(retryable_resource_processing_error(&anyhow::anyhow!(
            "系统图标图集尺寸过小"
        )));
        assert!(!retryable_resource_processing_error(&anyhow::anyhow!(
            "无法写入暂存目录"
        )));
    }

    #[test]
    fn single_resource_progress_and_local_errors_remain_visible() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths, "http://127.0.0.1:1".into(), "token".into()).unwrap();
        service
            .generation_progress
            .lock()
            .unwrap()
            .items
            .push(GenerationProgressItem {
                slot: "skin.logo".into(),
                label: "Logo".into(),
                prompt: "existing".into(),
                status: "completed".into(),
                preview_url: String::new(),
                session: String::new(),
                path: String::new(),
                message: String::new(),
                reference_indexes: Vec::new(),
            });

        let slot = GeneratedSlot::ActionIcon("plugins");
        service.start_resource_generation(slot, "missing icon", 4);
        let progress = service.generation_progress();
        assert_eq!(progress.state, "generating");
        assert_eq!(progress.items.len(), 2);
        assert_eq!(progress.items[1].slot, "skin.icons.plugins");
        assert!(progress.items[1].message.contains("4 张参考图"));

        service.fail_resource_generation(slot, &anyhow::anyhow!("绿幕抠图失败"));
        let progress = service.generation_progress();
        assert_eq!(progress.state, "failed");
        assert_eq!(progress.items[1].status, "failed");
        let logs = service.request_log();
        assert_eq!(logs.last().unwrap().outcome, "error");
        assert!(logs.last().unwrap().message.contains("绿幕抠图失败"));
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
        assert_eq!(
            endpoint("https://api.example.com/v1", EndpointKind::ImageEdits)
                .unwrap()
                .as_str(),
            "https://api.example.com/v1/images/edits"
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
                    let png = encode_png(&RgbaImage::from_pixel(
                        64,
                        32,
                        image::Rgba([30, 120, 180, 255]),
                    ))
                    .unwrap();
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
                &[],
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
        assert_eq!(
            GeneratedSlot::ActionIconAtlas
                .image_spec("16:9", "gpt-image-2")
                .unwrap(),
            ("1536x1280".into(), "6:5".into())
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
                ("1536x1024".into(), "6:5".into())
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
    fn opaque_generated_assets_are_resized_and_compressed() {
        let source = DynamicImage::ImageRgb8(image::RgbImage::from_fn(1774, 887, |x, y| {
            image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8])
        }));
        let mut encoded = std::io::Cursor::new(Vec::new());
        source.write_to(&mut encoded, ImageFormat::Png).unwrap();

        let hero =
            post_process_generated_asset(GeneratedSlot::HeroImage, encoded.get_ref()).unwrap();
        assert!(hero.starts_with(&[0xFF, 0xD8, 0xFF]));
        assert_eq!(
            image::load_from_memory(&hero).unwrap().dimensions(),
            (1200, 600)
        );

        let background =
            post_process_generated_asset(GeneratedSlot::BackgroundFullscreen, encoded.get_ref())
                .unwrap();
        assert!(background.starts_with(&[0xFF, 0xD8, 0xFF]));
        assert_eq!(
            image::load_from_memory(&background).unwrap().dimensions(),
            (1600, 800)
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
        assert!(prompt.contains("6 column by 5 row"));
        for (_, meaning) in ICON_ACTIONS {
            assert!(prompt.contains(meaning));
        }
        assert!(prompt.contains("#00FF00"));
    }

    #[test]
    fn resource_icon_atlas_prompt_always_restores_grid_constraints() {
        let theme = ThemeManifest::neutral_dark();
        let prompt = resource_generation_prompt(
            GeneratedSlot::ActionIconAtlas,
            "match the supplied style",
            &theme,
        );
        assert!(prompt.contains("exact 6 column by 5 row sprite atlas"));
        assert!(prompt.contains("cell 30: project-app"));
        assert!(prompt.contains("#00FF00"));
        assert!(prompt.contains("match the supplied style"));
    }

    #[test]
    fn icon_atlas_retry_keeps_thirty_cells() {
        let prompt = resource_processing_retry_prompt(
            GeneratedSlot::ActionIconAtlas,
            "original atlas prompt",
        );
        assert!(prompt.contains("all thirty icons"));
        assert!(prompt.contains("one centered icon in every cell"));
        assert!(prompt.contains("no single large subject"));
        assert!(!prompt.contains("draw exactly one large"));
    }

    #[test]
    fn thin_icon_cell_passes_atlas_subject_threshold() {
        let mut cell = RgbaImage::from_pixel(256, 256, image::Rgba([0, 255, 0, 255]));
        for x in 96..160 {
            cell.put_pixel(x, 128, image::Rgba([245, 245, 245, 255]));
        }
        let output = normalize_transparent_asset_with_min_subject(cell, 512, 512, 8).unwrap();
        assert!(output.pixels().any(|pixel| pixel.0[3] == 255));
        assert!(output.pixels().any(|pixel| pixel.0[3] == 0));
    }

    #[test]
    fn system_icon_atlas_is_split_into_thirty_transparent_assets() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path().join("ThemeInject"));
        paths.ensure().unwrap();
        let service =
            AiThemeService::new(paths.clone(), "http://127.0.0.1:1".into(), "token".into())
                .unwrap();
        let mut atlas = RgbaImage::from_pixel(1536, 1280, image::Rgba([0, 255, 0, 255]));
        for index in 0..ICON_ACTIONS.len() as u32 {
            let left = index % ICON_ATLAS_COLUMNS * 256 + 72;
            let top = index / ICON_ATLAS_COLUMNS * 256 + 72;
            let color = image::Rgba([80 + (index % 14) as u8 * 12, 24 + index as u8, 180, 255]);
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
        let mut atlas = RgbaImage::from_pixel(1280, 1280, image::Rgba([245, 245, 245, 255]));
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
    fn resource_plan_only_generation_preserves_current_theme() {
        let current = ThemeManifest::neutral_dark();
        let mut request = test_generate_request("只生成装饰资源");
        request.resource_plans_only = true;
        request.theme = Some(current.clone());
        request.resource_plans = vec![ResourcePlan {
            slot: "skin.decorations.0.asset".into(),
            prompt: "transparent corner ornament".into(),
        }];

        let (theme, plans, references) = resource_plan_generation_context(&request, 2).unwrap();

        assert_eq!(theme, current);
        assert_eq!(plans.len(), 1);
        assert!(matches!(plans[0].0, GeneratedSlot::Decoration(0)));
        assert_eq!(references["skin.decorations.0.asset"], vec![1, 2]);
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
        let request = test_generate_request("checkpoint theme");
        service
            .save_checkpoint(
                "fingerprint",
                &request,
                &theme,
                &[(slot, "hero prompt".into())],
                &BTreeMap::new(),
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
        let request = test_generate_request("restored theme");
        service
            .save_checkpoint(
                "unavailable-original-fingerprint",
                &request,
                &theme,
                &[(slot, "hero prompt".into())],
                &BTreeMap::new(),
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
        let request = test_generate_request("missing asset");
        service
            .save_checkpoint(
                "fingerprint",
                &request,
                &theme,
                &[],
                &BTreeMap::new(),
                std::slice::from_ref(&staged),
            )
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
                    let png = encode_png(&RgbaImage::from_pixel(
                        64,
                        32,
                        image::Rgba([30, 120, 180, 255]),
                    ))
                    .unwrap();
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
                theme_name: String::new(),
                theme_description: String::new(),
                language: "zh-CN".into(),
                generate_images: true,
                generate_sidebar_watermark: false,
                generate_system_icons: false,
                image_concurrency: 2,
                reference_session: None,
                reference_path: None,
                references: Vec::new(),
                resource_plans: Vec::new(),
                resource_plans_only: false,
                theme: None,
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
