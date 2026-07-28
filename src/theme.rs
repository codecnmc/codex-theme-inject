use std::collections::BTreeMap;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const THEME_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub base_mode: BaseMode,
    #[serde(default)]
    pub compatibility: Compatibility,
    pub tokens: ThemeTokens,
    #[serde(default)]
    pub background: BackgroundLayout,
    #[serde(default)]
    pub chrome: ChromeTheme,
    #[serde(default)]
    pub skin: SkinTheme,
    #[serde(default)]
    pub typography: Typography,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub effects: Effects,
    #[serde(default)]
    pub terminal: TerminalTheme,
    #[serde(default)]
    pub custom_css: Option<String>,
    #[serde(default, flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BaseMode {
    Light,
    #[default]
    Dark,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Compatibility {
    #[serde(default = "default_engine_requirement")]
    pub engine: String,
    #[serde(default)]
    pub tested_codex_versions: Vec<String>,
    #[serde(default)]
    pub repository: Option<String>,
}

fn default_engine_requirement() -> String {
    ">=0.1.0".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeTokens {
    pub app_background: String,
    pub sidebar_background: String,
    pub content_background: String,
    pub elevated_background: String,
    pub input_background: String,
    pub hover_background: String,
    pub active_background: String,
    pub foreground: String,
    pub muted_foreground: String,
    pub subtle_foreground: String,
    pub border: String,
    pub accent: String,
    pub accent_foreground: String,
    pub selection: String,
    pub link: String,
    pub success: String,
    pub warning: String,
    pub danger: String,
    pub code_background: String,
    pub code_foreground: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundLayout {
    #[serde(default)]
    pub mode: BackgroundMode,
    #[serde(default = "default_fullscreen_background")]
    pub fullscreen: RegionBackground,
    #[serde(default = "default_content_background")]
    pub content: RegionBackground,
    #[serde(default = "default_sidebar_background")]
    pub sidebar: RegionBackground,
}

impl Default for BackgroundLayout {
    fn default() -> Self {
        Self {
            mode: BackgroundMode::Fullscreen,
            fullscreen: default_fullscreen_background(),
            content: default_content_background(),
            sidebar: default_sidebar_background(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BackgroundMode {
    #[default]
    Fullscreen,
    PerRegion,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RegionBackground {
    #[serde(flatten)]
    pub source: BackgroundSource,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub blur: u16,
    #[serde(default = "default_saturation")]
    pub saturation: f32,
    #[serde(default = "default_surface_opacity")]
    pub surface_opacity: f32,
    #[serde(default = "default_background_position")]
    pub position_x: u8,
    #[serde(default = "default_background_position")]
    pub position_y: u8,
}

impl Default for RegionBackground {
    fn default() -> Self {
        Self {
            source: BackgroundSource::None,
            opacity: 1.0,
            blur: 0,
            saturation: 1.0,
            surface_opacity: default_surface_opacity(),
            position_x: 50,
            position_y: 50,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BackgroundSource {
    #[default]
    None,
    Solid {
        color: String,
    },
    LinearGradient {
        angle: u16,
        from: String,
        to: String,
    },
    RadialGradient {
        from: String,
        to: String,
    },
    Image {
        path: String,
        #[serde(default = "default_background_fit")]
        fit: String,
    },
}

fn default_fullscreen_background() -> RegionBackground {
    RegionBackground {
        surface_opacity: 0.72,
        ..RegionBackground::default()
    }
}

fn default_content_background() -> RegionBackground {
    RegionBackground {
        surface_opacity: 0.68,
        ..RegionBackground::default()
    }
}

fn default_sidebar_background() -> RegionBackground {
    RegionBackground {
        surface_opacity: 0.82,
        ..RegionBackground::default()
    }
}

fn default_background_fit() -> String {
    "cover".to_string()
}

fn default_opacity() -> f32 {
    1.0
}

fn default_saturation() -> f32 {
    1.0
}

fn default_surface_opacity() -> f32 {
    0.72
}

fn default_background_position() -> u8 {
    50
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChromeTheme {
    #[serde(default)]
    pub titlebar_background: Option<String>,
    #[serde(default)]
    pub titlebar_foreground: Option<String>,
    #[serde(default)]
    pub titlebar_muted_foreground: Option<String>,
    #[serde(default)]
    pub titlebar_border: Option<String>,
    #[serde(default)]
    pub titlebar_hover: Option<String>,
    #[serde(default)]
    pub sidebar_foreground: Option<String>,
    #[serde(default)]
    pub sidebar_muted_foreground: Option<String>,
    #[serde(default)]
    pub sidebar_icon: Option<String>,
    #[serde(default)]
    pub sidebar_border: Option<String>,
    #[serde(default)]
    pub sidebar_hover: Option<String>,
    #[serde(default)]
    pub sidebar_active: Option<String>,
    #[serde(default)]
    pub sidebar_active_foreground: Option<String>,
    #[serde(default)]
    pub right_sidebar_background: Option<String>,
    #[serde(default)]
    pub right_sidebar_foreground: Option<String>,
    #[serde(default)]
    pub right_sidebar_muted_foreground: Option<String>,
    #[serde(default)]
    pub right_sidebar_icon: Option<String>,
    #[serde(default)]
    pub right_sidebar_border: Option<String>,
    #[serde(default)]
    pub right_sidebar_hover: Option<String>,
    #[serde(default)]
    pub right_sidebar_active: Option<String>,
    #[serde(default)]
    pub right_sidebar_active_foreground: Option<String>,
    #[serde(default)]
    pub right_sidebar_shortcut_background: Option<String>,
    #[serde(default)]
    pub right_sidebar_shortcut_foreground: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkinTheme {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub brand: BrandStyle,
    #[serde(default)]
    pub resources: SkinResources,
    #[serde(default)]
    pub surfaces: SkinSurfaces,
    #[serde(default)]
    pub icons: IconTheme,
    #[serde(default)]
    pub decorations: Vec<Decoration>,
    #[serde(default)]
    pub home: HomeSkin,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrandStyle {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub account_name: String,
    #[serde(default)]
    pub font: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkinResources {
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub account_avatar: Option<String>,
    #[serde(default)]
    pub sidebar_watermark: Option<String>,
    #[serde(default)]
    pub hero_image: Option<String>,
    #[serde(default)]
    pub hero_badge: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub sticker: Option<String>,
    #[serde(default)]
    pub composer_decoration: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkinSurfaces {
    #[serde(default)]
    pub titlebar: SurfaceStyle,
    #[serde(default)]
    pub sidebar: SurfaceStyle,
    #[serde(default)]
    pub content: SurfaceStyle,
    #[serde(default)]
    pub hero: SurfaceStyle,
    #[serde(default)]
    pub card: SurfaceStyle,
    #[serde(default)]
    pub composer: SurfaceStyle,
    #[serde(default)]
    pub popover: SurfaceStyle,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStyle {
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub blur: u16,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default)]
    pub border_width: u8,
    #[serde(default = "default_surface_radius")]
    pub radius: u16,
    #[serde(default)]
    pub shadow_color: Option<String>,
    #[serde(default = "default_surface_shadow_blur")]
    pub shadow_blur: u16,
    #[serde(default = "default_surface_shadow_opacity")]
    pub shadow_opacity: f32,
}

impl Default for SurfaceStyle {
    fn default() -> Self {
        Self {
            fill: None,
            opacity: 1.0,
            blur: 0,
            border_color: None,
            border_width: 0,
            radius: default_surface_radius(),
            shadow_color: None,
            shadow_blur: default_surface_shadow_blur(),
            shadow_opacity: default_surface_shadow_opacity(),
        }
    }
}

fn default_surface_radius() -> u16 {
    12
}

fn default_surface_shadow_blur() -> u16 {
    24
}

fn default_surface_shadow_opacity() -> f32 {
    0.0
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IconTheme {
    #[serde(default)]
    pub mode: IconMode,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub active_color: Option<String>,
    #[serde(default)]
    pub mappings: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum IconMode {
    #[default]
    Native,
    Recolor,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Decoration {
    pub asset: String,
    pub region: DecorationRegion,
    #[serde(default)]
    pub anchor: DecorationAnchor,
    #[serde(default)]
    pub offset_x: i16,
    #[serde(default)]
    pub offset_y: i16,
    #[serde(default = "default_decoration_width")]
    pub width: u16,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub layer: i8,
    #[serde(default)]
    pub hidden_below: u16,
}

fn default_decoration_width() -> u16 {
    160
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DecorationRegion {
    Titlebar,
    Sidebar,
    Content,
    Composer,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DecorationAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HomeSkin {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_home_title")]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default = "default_hero_height")]
    pub hero_height: u16,
    #[serde(default = "default_card_columns")]
    pub card_columns: u8,
    #[serde(default)]
    pub cards: Vec<HomeCard>,
}

impl Default for HomeSkin {
    fn default() -> Self {
        Self {
            enabled: false,
            title: default_home_title(),
            subtitle: String::new(),
            hero_height: default_hero_height(),
            card_columns: default_card_columns(),
            cards: Vec::new(),
        }
    }
}

fn default_home_title() -> String {
    "今天想构建什么？".to_string()
}

fn default_hero_height() -> u16 {
    320
}

fn default_card_columns() -> u8 {
    4
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HomeCard {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Typography {
    pub ui_font: String,
    pub mono_font: String,
    pub scale: f32,
    pub line_height: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            ui_font: "Inter, Segoe UI, sans-serif".to_string(),
            mono_font: "Cascadia Code, Consolas, monospace".to_string(),
            scale: 1.0,
            line_height: 1.5,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub density: f32,
    pub sidebar_width: u16,
    pub content_max_width: u16,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            density: 1.0,
            sidebar_width: 300,
            content_max_width: 960,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub radius: u16,
    pub border_width: u8,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            radius: 10,
            border_width: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Effects {
    pub shadow_strength: f32,
    pub background_blur: u16,
    pub saturation: f32,
    pub panel_opacity: f32,
    #[serde(default)]
    pub ambient_enabled: bool,
    #[serde(default = "default_ambient_effect")]
    pub ambient_effect: String,
    #[serde(default = "default_ambient_intensity")]
    pub ambient_intensity: f32,
    #[serde(default = "default_ambient_speed")]
    pub ambient_speed: f32,
    #[serde(default = "default_ambient_opacity")]
    pub ambient_opacity: f32,
    #[serde(default)]
    pub ambient_color: Option<String>,
}

fn default_ambient_effect() -> String {
    "snow".into()
}
fn default_ambient_intensity() -> f32 {
    0.5
}
fn default_ambient_speed() -> f32 {
    1.0
}
fn default_ambient_opacity() -> f32 {
    0.65
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            shadow_strength: 0.25,
            background_blur: 0,
            saturation: 1.0,
            panel_opacity: 1.0,
            ambient_enabled: false,
            ambient_effect: default_ambient_effect(),
            ambient_intensity: default_ambient_intensity(),
            ambient_speed: default_ambient_speed(),
            ambient_opacity: default_ambient_opacity(),
            ambient_color: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTheme {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
    pub ansi: Vec<String>,
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self {
            background: "#101114".to_string(),
            foreground: "#E8E9ED".to_string(),
            cursor: "#7C9CFF".to_string(),
            selection: "#334A7D".to_string(),
            ansi: vec![
                "#1B1D22", "#FF6B7A", "#67D391", "#E5C07B", "#6FA8FF", "#C792EA", "#5CCFE6",
                "#D8DEE9", "#60646C", "#FF8792", "#7BE0A0", "#F0D18C", "#87B7FF", "#D7A6F2",
                "#79D9EB", "#FFFFFF",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        }
    }
}

impl ThemeManifest {
    pub fn companion_pet_id(&self) -> Option<&str> {
        self.extensions
            .get("themeInject")
            .and_then(Value::as_object)
            .and_then(|extension| extension.get("companionPetId"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
    }

    pub fn set_companion_pet_id(&mut self, id: Option<&str>) -> anyhow::Result<()> {
        if let Some(id) = id {
            crate::pet::validate_pet_id(id)?;
            let extension = self
                .extensions
                .entry("themeInject".into())
                .or_insert_with(|| json!({}));
            let extension = extension
                .as_object_mut()
                .context("themeInject 扩展必须是对象")?;
            extension.insert("companionPetId".into(), json!(id));
        } else if let Some(extension) = self
            .extensions
            .get_mut("themeInject")
            .and_then(Value::as_object_mut)
        {
            extension.remove("companionPetId");
            if extension.is_empty() {
                self.extensions.remove("themeInject");
            }
        }
        Ok(())
    }

    pub fn from_slice_migrated(bytes: &[u8]) -> anyhow::Result<Self> {
        let value: Value = serde_json::from_slice(bytes).context("theme.json 格式无效")?;
        Self::from_value_migrated(value)
    }

    pub fn from_value_migrated(mut value: Value) -> anyhow::Result<Self> {
        migrate_theme_value(&mut value)?;
        let theme: Self = serde_json::from_value(value).context("theme.json 结构无效")?;
        theme.validate()?;
        Ok(theme)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.schema_version != THEME_SCHEMA_VERSION {
            bail!("不支持的主题 schemaVersion：{}", self.schema_version);
        }
        validate_theme_id(&self.id)?;
        semver::Version::parse(&self.version).context("主题 version 必须是 semver")?;
        semver::VersionReq::parse(&self.compatibility.engine)
            .context("compatibility.engine 必须是 semver requirement")?;
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            bail!("主题名称不能为空且不能超过 80 个字符");
        }
        for color in self
            .tokens
            .colors()
            .into_iter()
            .chain(self.terminal.colors())
            .chain(self.effects.ambient_color.as_deref())
        {
            validate_color(color)?;
        }
        if !(0.8..=1.4).contains(&self.typography.scale)
            || !(1.1..=2.0).contains(&self.typography.line_height)
            || !(0.75..=1.35).contains(&self.layout.density)
            || !(180..=520).contains(&self.layout.sidebar_width)
            || !(480..=2400).contains(&self.layout.content_max_width)
            || self.shape.radius > 40
            || self.shape.border_width > 4
            || !(0.0..=1.0).contains(&self.effects.shadow_strength)
            || self.effects.background_blur > 80
            || !(0.5..=2.0).contains(&self.effects.saturation)
            || !(0.25..=1.0).contains(&self.effects.panel_opacity)
            || !matches!(
                self.effects.ambient_effect.as_str(),
                "meteor" | "rain" | "snow" | "stars" | "fireflies" | "petals"
            )
            || !(0.1..=1.0).contains(&self.effects.ambient_intensity)
            || !(0.25..=2.5).contains(&self.effects.ambient_speed)
            || !(0.1..=1.0).contains(&self.effects.ambient_opacity)
        {
            bail!("主题数值超出允许范围");
        }
        validate_font_stack(&self.typography.ui_font)?;
        validate_font_stack(&self.typography.mono_font)?;
        if self.terminal.ansi.len() != 16 {
            bail!("terminal.ansi 必须包含 16 种颜色");
        }
        if let Some(path) = &self.custom_css {
            validate_relative_asset_path(path)?;
        }
        if let Some(id) = self.companion_pet_id() {
            crate::pet::validate_pet_id(id)?;
        }
        self.background.validate()?;
        self.chrome.validate()?;
        self.skin.validate()?;
        Ok(())
    }

    pub fn asset_paths(&self) -> Vec<&str> {
        let mut paths = self.background.image_paths();
        paths.extend(self.skin.resources.paths());
        paths.extend(self.skin.icons.mappings.values().map(String::as_str));
        paths.extend(
            self.skin
                .decorations
                .iter()
                .map(|decoration| decoration.asset.as_str()),
        );
        paths.extend(
            self.skin
                .home
                .cards
                .iter()
                .filter_map(|card| card.icon.as_deref()),
        );
        paths
    }

    pub fn neutral_dark() -> Self {
        serde_json::from_value(json!({
            "schemaVersion": 2,
            "id": "builtin.neutral-dark",
            "name": "Neutral Dark",
            "version": "1.0.0",
            "author": "Theme Inject",
            "description": "克制、清晰的深色主题",
            "baseMode": "dark",
            "compatibility": { "engine": ">=0.1.0", "testedCodexVersions": ["26.707.9981.0"] },
            "tokens": {
                "appBackground": "#18191C", "sidebarBackground": "#202125", "contentBackground": "#18191C",
                "elevatedBackground": "#24262A", "inputBackground": "#212328", "hoverBackground": "#2A2C31",
                "activeBackground": "#30343D", "foreground": "#ECEDEF", "mutedForeground": "#B5B8BF",
                "subtleForeground": "#878B93", "border": "#34363C", "accent": "#6E8FF5",
                "accentForeground": "#FFFFFF", "selection": "#354D82", "link": "#86A6FF",
                "success": "#57C785", "warning": "#E6B85C", "danger": "#F06D7A",
                "codeBackground": "#141518", "codeForeground": "#E4E6EA"
            }
        })).expect("builtin theme must be valid")
    }

    pub fn neutral_light() -> Self {
        let mut theme = Self::neutral_dark();
        theme.id = "builtin.neutral-light".into();
        theme.name = "Neutral Light".into();
        theme.description = "柔和、低眩光的浅色主题".into();
        theme.base_mode = BaseMode::Light;
        theme.tokens = ThemeTokens {
            app_background: "#F4F4F2".into(),
            sidebar_background: "#ECECE9".into(),
            content_background: "#F8F8F6".into(),
            elevated_background: "#FFFFFF".into(),
            input_background: "#FFFFFF".into(),
            hover_background: "#E4E5E1".into(),
            active_background: "#DCDDE2".into(),
            foreground: "#202124".into(),
            muted_foreground: "#5E626A".into(),
            subtle_foreground: "#858990".into(),
            border: "#D4D5D2".into(),
            accent: "#4C6FC7".into(),
            accent_foreground: "#FFFFFF".into(),
            selection: "#C9D8FF".into(),
            link: "#315FC9".into(),
            success: "#2B8A57".into(),
            warning: "#A76A12".into(),
            danger: "#C83E4D".into(),
            code_background: "#F0F1F3".into(),
            code_foreground: "#24262B".into(),
        };
        theme.terminal = TerminalTheme {
            background: "#F4F5F6".into(),
            foreground: "#24262B".into(),
            cursor: "#496FD8".into(),
            selection: "#C9D8FF".into(),
            ..TerminalTheme::default()
        };
        theme
    }

    pub fn midnight_glass() -> Self {
        let mut theme = Self::neutral_dark();
        theme.id = "builtin.midnight-glass".into();
        theme.name = "Midnight Glass".into();
        theme.description = "蓝紫渐变与半透明玻璃质感".into();
        theme.tokens.app_background = "#0B0F1A".into();
        theme.tokens.sidebar_background = "#151A29".into();
        theme.tokens.content_background = "#101522".into();
        theme.tokens.elevated_background = "#1B2232".into();
        theme.tokens.input_background = "#171D2A".into();
        theme.tokens.border = "#30384C".into();
        theme.tokens.active_background = "#2B3245".into();
        theme.tokens.accent = "#887CF2".into();
        theme.tokens.link = "#A49BFF".into();
        theme.background.fullscreen = RegionBackground {
            source: BackgroundSource::LinearGradient {
                angle: 135,
                from: "#10182C".into(),
                to: "#281C3A".into(),
            },
            surface_opacity: 0.72,
            ..RegionBackground::default()
        };
        theme.effects = Effects {
            shadow_strength: 0.45,
            background_blur: 12,
            saturation: 1.05,
            panel_opacity: 0.9,
            ..Effects::default()
        };
        theme.shape.radius = 14;
        theme
    }
}

impl ThemeTokens {
    fn colors(&self) -> Vec<&str> {
        vec![
            &self.app_background,
            &self.sidebar_background,
            &self.content_background,
            &self.elevated_background,
            &self.input_background,
            &self.hover_background,
            &self.active_background,
            &self.foreground,
            &self.muted_foreground,
            &self.subtle_foreground,
            &self.border,
            &self.accent,
            &self.accent_foreground,
            &self.selection,
            &self.link,
            &self.success,
            &self.warning,
            &self.danger,
            &self.code_background,
            &self.code_foreground,
        ]
    }
}

impl BackgroundLayout {
    pub fn image_paths(&self) -> Vec<&str> {
        [&self.fullscreen, &self.content, &self.sidebar]
            .into_iter()
            .filter_map(RegionBackground::image_path)
            .collect()
    }

    fn validate(&self) -> anyhow::Result<()> {
        for background in [&self.fullscreen, &self.content, &self.sidebar] {
            background.validate()?;
        }
        Ok(())
    }
}

impl RegionBackground {
    pub fn image_path(&self) -> Option<&str> {
        match &self.source {
            BackgroundSource::Image { path, .. } => Some(path),
            _ => None,
        }
    }

    fn validate(&self) -> anyhow::Result<()> {
        if !(0.0..=1.0).contains(&self.opacity)
            || self.blur > 80
            || !(0.5..=2.0).contains(&self.saturation)
            || !(0.0..=1.0).contains(&self.surface_opacity)
            || self.position_x > 100
            || self.position_y > 100
        {
            bail!("背景区域数值超出允许范围");
        }
        match &self.source {
            BackgroundSource::None => {}
            BackgroundSource::Solid { color } => validate_color(color)?,
            BackgroundSource::LinearGradient { angle, from, to } => {
                if *angle > 360 {
                    bail!("渐变角度必须在 0..=360");
                }
                validate_color(from)?;
                validate_color(to)?;
            }
            BackgroundSource::RadialGradient { from, to } => {
                validate_color(from)?;
                validate_color(to)?;
            }
            BackgroundSource::Image { path, fit } => {
                validate_image_asset_path(path)?;
                if !matches!(fit.as_str(), "cover" | "contain" | "fill" | "none") {
                    bail!("不支持的背景 fit");
                }
            }
        }
        Ok(())
    }
}

impl ChromeTheme {
    fn validate(&self) -> anyhow::Result<()> {
        for color in [
            &self.titlebar_background,
            &self.titlebar_foreground,
            &self.titlebar_muted_foreground,
            &self.titlebar_border,
            &self.titlebar_hover,
            &self.sidebar_foreground,
            &self.sidebar_muted_foreground,
            &self.sidebar_icon,
            &self.sidebar_border,
            &self.sidebar_hover,
            &self.sidebar_active,
            &self.sidebar_active_foreground,
            &self.right_sidebar_background,
            &self.right_sidebar_foreground,
            &self.right_sidebar_muted_foreground,
            &self.right_sidebar_icon,
            &self.right_sidebar_border,
            &self.right_sidebar_hover,
            &self.right_sidebar_active,
            &self.right_sidebar_active_foreground,
            &self.right_sidebar_shortcut_background,
            &self.right_sidebar_shortcut_foreground,
        ]
        .into_iter()
        .flatten()
        {
            validate_color(color)?;
        }
        Ok(())
    }
}

impl SkinTheme {
    fn validate(&self) -> anyhow::Result<()> {
        validate_short_text(&self.brand.title, 80, "品牌名称")?;
        validate_short_text(&self.brand.subtitle, 160, "品牌副标题")?;
        validate_short_text(&self.brand.account_name, 80, "左下角名称")?;
        if !self.brand.font.is_empty() {
            validate_font_stack(&self.brand.font)?;
        }
        self.resources.validate()?;
        self.surfaces.validate()?;
        self.icons.validate()?;
        if self.decorations.len() > 16 {
            bail!("装饰层不能超过 16 个");
        }
        for decoration in &self.decorations {
            decoration.validate()?;
        }
        self.home.validate()
    }
}

impl SkinResources {
    fn validate(&self) -> anyhow::Result<()> {
        for path in self.paths() {
            validate_image_asset_path(path)?;
        }
        Ok(())
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        [
            self.logo.as_deref(),
            self.account_avatar.as_deref(),
            self.sidebar_watermark.as_deref(),
            self.hero_image.as_deref(),
            self.hero_badge.as_deref(),
            self.avatar.as_deref(),
            self.sticker.as_deref(),
            self.composer_decoration.as_deref(),
        ]
        .into_iter()
        .flatten()
    }
}

impl SkinSurfaces {
    fn validate(&self) -> anyhow::Result<()> {
        for surface in [
            &self.titlebar,
            &self.sidebar,
            &self.content,
            &self.hero,
            &self.card,
            &self.composer,
            &self.popover,
        ] {
            surface.validate()?;
        }
        Ok(())
    }
}

impl SurfaceStyle {
    fn validate(&self) -> anyhow::Result<()> {
        for color in [&self.fill, &self.border_color, &self.shadow_color]
            .into_iter()
            .flatten()
        {
            validate_color(color)?;
        }
        if !(0.0..=1.0).contains(&self.opacity)
            || self.blur > 80
            || self.border_width > 4
            || self.radius > 48
            || self.shadow_blur > 80
            || !(0.0..=1.0).contains(&self.shadow_opacity)
        {
            bail!("高级表面数值超出允许范围");
        }
        Ok(())
    }
}

impl IconTheme {
    fn validate(&self) -> anyhow::Result<()> {
        for color in [&self.color, &self.active_color].into_iter().flatten() {
            validate_color(color)?;
        }
        if self.mappings.len() > 32 {
            bail!("图标映射不能超过 32 项");
        }
        for (action, path) in &self.mappings {
            if action.is_empty()
                || action.len() > 64
                || !action.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '.')
                })
            {
                bail!("图标动作 ID 无效");
            }
            validate_image_asset_path(path)?;
        }
        Ok(())
    }
}

impl Decoration {
    fn validate(&self) -> anyhow::Result<()> {
        validate_image_asset_path(&self.asset)?;
        if !(-2000..=2000).contains(&self.offset_x)
            || !(-2000..=2000).contains(&self.offset_y)
            || !(16..=1600).contains(&self.width)
            || !(0.0..=1.0).contains(&self.opacity)
            || !(-1..=6).contains(&self.layer)
        {
            bail!("装饰层数值超出允许范围");
        }
        Ok(())
    }
}

impl HomeSkin {
    fn validate(&self) -> anyhow::Result<()> {
        validate_short_text(&self.title, 120, "欢迎页标题")?;
        validate_short_text(&self.subtitle, 240, "欢迎页副标题")?;
        if !(160..=720).contains(&self.hero_height)
            || !(1..=4).contains(&self.card_columns)
            || self.cards.len() > 4
        {
            bail!("欢迎页配置超出允许范围");
        }
        for card in &self.cards {
            validate_short_text(&card.title, 80, "快捷卡片标题")?;
            validate_short_text(&card.description, 160, "快捷卡片说明")?;
            if card.prompt.len() > 4000 {
                bail!("快捷卡片 prompt 不能超过 4000 字节");
            }
            if let Some(path) = &card.icon {
                validate_image_asset_path(path)?;
            }
        }
        Ok(())
    }
}

impl TerminalTheme {
    fn colors(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.background.as_str())
            .chain(std::iter::once(self.foreground.as_str()))
            .chain(std::iter::once(self.cursor.as_str()))
            .chain(std::iter::once(self.selection.as_str()))
            .chain(self.ansi.iter().map(String::as_str))
    }
}

pub fn validate_theme_id(id: &str) -> anyhow::Result<()> {
    if id.is_empty()
        || id.len() > 96
        || !id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '-')
        })
    {
        bail!("主题 id 只能包含小写字母、数字、点和连字符");
    }
    Ok(())
}

pub fn validate_color(color: &str) -> anyhow::Result<()> {
    let valid = matches!(color.len(), 7 | 9)
        && color.starts_with('#')
        && color[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if !valid {
        bail!("颜色必须使用 #RRGGBB 或 #RRGGBBAA：{color}");
    }
    Ok(())
}

pub fn validate_relative_asset_path(path: &str) -> anyhow::Result<()> {
    let path = std::path::Path::new(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        bail!("资源路径必须位于主题包内");
    }
    Ok(())
}

pub fn validate_image_asset_path(path: &str) -> anyhow::Result<()> {
    validate_relative_asset_path(path)?;
    if !path.starts_with("assets/") {
        bail!("图片资源必须位于 assets/ 目录");
    }
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(
        extension.as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp")
    ) {
        bail!("不支持的图片资源格式");
    }
    Ok(())
}

fn validate_short_text(value: &str, max: usize, label: &str) -> anyhow::Result<()> {
    if value.chars().count() > max || value.contains(['\0', '\r']) {
        bail!("{label}格式无效或超过 {max} 个字符");
    }
    Ok(())
}

fn migrate_theme_value(value: &mut Value) -> anyhow::Result<()> {
    let object = value
        .as_object_mut()
        .context("theme.json 根节点必须是对象")?;
    let version = object
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if version > u64::from(THEME_SCHEMA_VERSION) {
        bail!("不支持的主题 schemaVersion：{version}");
    }
    if version < 2 {
        let legacy = object
            .remove("background")
            .unwrap_or_else(|| json!({ "kind": "none" }));
        object.insert("background".into(), migrate_legacy_background(legacy)?);
        object.insert("schemaVersion".into(), json!(THEME_SCHEMA_VERSION));
    }
    Ok(())
}

fn migrate_legacy_background(mut legacy: Value) -> anyhow::Result<Value> {
    let object = legacy
        .as_object_mut()
        .context("旧版 background 必须是对象")?;
    let region = object
        .remove("region")
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "fullscreen".to_string());
    let opacity = object
        .remove("opacity")
        .and_then(|value| value.as_f64())
        .unwrap_or(1.0);
    let mut config = object.clone();
    config.insert("opacity".into(), json!(opacity));
    config.insert("blur".into(), json!(0));
    config.insert("saturation".into(), json!(1.0));
    config.insert("positionX".into(), json!(50));
    config.insert("positionY".into(), json!(50));
    let none = json!({ "kind": "none" });
    Ok(match region.as_str() {
        "content" => json!({
            "mode": "per-region",
            "fullscreen": none,
            "content": with_surface_opacity(config, 0.68),
            "sidebar": { "kind": "none", "surfaceOpacity": 0.82 }
        }),
        "sidebar" => json!({
            "mode": "per-region",
            "fullscreen": none,
            "content": { "kind": "none", "surfaceOpacity": 0.68 },
            "sidebar": with_surface_opacity(config, 0.82)
        }),
        _ => json!({
            "mode": "fullscreen",
            "fullscreen": with_surface_opacity(config, 0.72),
            "content": { "kind": "none", "surfaceOpacity": 0.68 },
            "sidebar": { "kind": "none", "surfaceOpacity": 0.82 }
        }),
    })
}

fn with_surface_opacity(mut value: serde_json::Map<String, Value>, opacity: f32) -> Value {
    value.insert("surfaceOpacity".into(), json!(opacity));
    Value::Object(value)
}

fn validate_font_stack(value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() || value.len() > 200 || value.contains([';', '{', '}', '\n', '\r']) {
        bail!("字体栈格式无效");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_validate() {
        for theme in [
            ThemeManifest::neutral_dark(),
            ThemeManifest::neutral_light(),
            ThemeManifest::midnight_glass(),
        ] {
            theme.validate().unwrap();
        }
    }

    #[test]
    fn companion_pet_binding_round_trips_in_extensions() {
        let mut theme = ThemeManifest::neutral_dark();
        theme.set_companion_pet_id(Some("theme-pet")).unwrap();
        assert_eq!(theme.companion_pet_id(), Some("theme-pet"));
        let value = serde_json::to_value(&theme).unwrap();
        assert_eq!(value["themeInject"]["companionPetId"], "theme-pet");
        let restored = ThemeManifest::from_value_migrated(value).unwrap();
        assert_eq!(restored.companion_pet_id(), Some("theme-pet"));

        theme.set_companion_pet_id(None).unwrap();
        assert_eq!(theme.companion_pet_id(), None);
        assert!(!theme.extensions.contains_key("themeInject"));
        assert!(theme.set_companion_pet_id(Some("INVALID PET")).is_err());
    }

    #[test]
    fn ambient_effect_defaults_and_validation_are_stable() {
        let theme = ThemeManifest::neutral_dark();
        assert!(!theme.effects.ambient_enabled);
        assert_eq!(theme.effects.ambient_effect, "snow");
        assert_eq!(theme.effects.ambient_color, None);
        let mut invalid = theme;
        invalid.effects.ambient_effect = "unknown".into();
        assert!(invalid.validate().is_err());

        let mut invalid_color = ThemeManifest::neutral_dark();
        invalid_color.effects.ambient_color = Some("currentColor".into());
        assert!(invalid_color.validate().is_err());
    }

    #[test]
    fn right_sidebar_colors_round_trip_and_validate() {
        let mut theme = ThemeManifest::neutral_dark();
        theme.chrome.right_sidebar_background = Some("#101820".into());
        theme.chrome.right_sidebar_foreground = Some("#F4F7FB".into());
        theme.chrome.right_sidebar_muted_foreground = Some("#A9B7CA".into());
        theme.chrome.right_sidebar_icon = Some("#F4F7FB".into());
        theme.chrome.right_sidebar_border = Some("#60728A".into());
        theme.chrome.right_sidebar_hover = Some("#24324A".into());
        theme.chrome.right_sidebar_active = Some("#33445D".into());
        theme.chrome.right_sidebar_active_foreground = Some("#FFFFFF".into());
        theme.chrome.right_sidebar_shortcut_background = Some("#172235".into());
        theme.chrome.right_sidebar_shortcut_foreground = Some("#B8C6D8".into());
        let value = serde_json::to_value(&theme).unwrap();
        let restored = ThemeManifest::from_value_migrated(value).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.chrome, theme.chrome);

        theme.chrome.right_sidebar_background = Some("transparent".into());
        assert!(theme.validate().is_err());
    }

    #[test]
    fn account_branding_round_trips_and_defaults_for_older_themes() {
        let mut theme = ThemeManifest::neutral_dark();
        theme.skin.brand.account_name = "主题账户".into();
        theme.skin.resources.account_avatar = Some("assets/account-avatar.png".into());
        let value = serde_json::to_value(&theme).unwrap();
        assert_eq!(value["skin"]["brand"]["accountName"], "主题账户");
        assert_eq!(
            value["skin"]["resources"]["accountAvatar"],
            "assets/account-avatar.png"
        );
        let restored = ThemeManifest::from_value_migrated(value).unwrap();
        assert_eq!(restored.skin.brand.account_name, "主题账户");
        assert_eq!(
            restored.skin.resources.account_avatar.as_deref(),
            Some("assets/account-avatar.png")
        );

        let mut legacy = serde_json::to_value(ThemeManifest::neutral_dark()).unwrap();
        legacy["skin"]["brand"]
            .as_object_mut()
            .unwrap()
            .remove("accountName");
        legacy["skin"]["resources"]
            .as_object_mut()
            .unwrap()
            .remove("accountAvatar");
        let restored = ThemeManifest::from_value_migrated(legacy).unwrap();
        assert!(restored.skin.brand.account_name.is_empty());
        assert!(restored.skin.resources.account_avatar.is_none());

        let mut invalid = ThemeManifest::neutral_dark();
        invalid.skin.brand.account_name = "名".repeat(81);
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn rejects_invalid_values() {
        let mut theme = ThemeManifest::neutral_dark();
        theme.tokens.accent = "red".into();
        assert!(theme.validate().is_err());
        theme = ThemeManifest::neutral_dark();
        theme.custom_css = Some("../escape.css".into());
        assert!(theme.validate().is_err());
    }

    #[test]
    fn migrates_legacy_fullscreen_background() {
        let mut value = serde_json::to_value(ThemeManifest::neutral_dark()).unwrap();
        value["schemaVersion"] = json!(1);
        value["background"] = json!({
            "kind": "image", "path": "assets/background.png", "fit": "cover", "opacity": 0.8
        });
        let theme = ThemeManifest::from_value_migrated(value).unwrap();
        assert_eq!(theme.schema_version, 2);
        assert_eq!(theme.background.mode, BackgroundMode::Fullscreen);
        assert_eq!(
            theme.background.fullscreen.image_path(),
            Some("assets/background.png")
        );
        assert_eq!(theme.background.fullscreen.opacity, 0.8);
    }

    #[test]
    fn migrates_legacy_region_backgrounds() {
        for (region, active) in [("content", "content"), ("sidebar", "sidebar")] {
            let mut value = serde_json::to_value(ThemeManifest::neutral_dark()).unwrap();
            value["schemaVersion"] = json!(1);
            value["background"] = json!({
                "kind": "solid", "color": "#112233", "region": region
            });
            let theme = ThemeManifest::from_value_migrated(value).unwrap();
            assert_eq!(theme.background.mode, BackgroundMode::PerRegion);
            let configured = if active == "content" {
                &theme.background.content
            } else {
                &theme.background.sidebar
            };
            assert!(matches!(configured.source, BackgroundSource::Solid { .. }));
        }
    }

    #[test]
    fn validates_skin_limits_and_assets() {
        let mut theme = ThemeManifest::neutral_light();
        theme.skin.enabled = true;
        theme.skin.resources.hero_image = Some("assets/hero.webp".into());
        theme.skin.home.enabled = true;
        theme.skin.home.cards.push(HomeCard {
            title: "解释代码".into(),
            description: "读取并说明当前代码".into(),
            prompt: "请解释当前代码".into(),
            icon: Some("assets/explain.png".into()),
        });
        theme.validate().unwrap();
        theme.skin.decorations = (0..17)
            .map(|_| Decoration {
                asset: "assets/star.png".into(),
                region: DecorationRegion::Content,
                anchor: DecorationAnchor::BottomRight,
                offset_x: 0,
                offset_y: 0,
                width: 100,
                opacity: 1.0,
                layer: 0,
                hidden_below: 0,
            })
            .collect();
        assert!(theme.validate().is_err());
    }
}
