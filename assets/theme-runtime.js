(() => {
  "use strict";
  const VERSION = 2;
  const STYLE_ID = "theme-inject-style";
  const PANEL_STYLE_ID = "theme-inject-panel-style";
  const BACKDROP_IDS = {
    fullscreen: "theme-inject-backdrop",
    content: "theme-inject-backdrop-content",
    sidebar: "theme-inject-backdrop-sidebar",
  };
  const PANEL_ID = "theme-inject-panel";
  const TRIGGER_ID = "theme-inject-trigger";
  const GENERATION_ISLAND_ID = "theme-inject-generation-island";
  const BRAND_ID = "theme-inject-brand";
  const HOME_ID = "theme-inject-home";
  const DECORATIONS_ID = "theme-inject-decorations";
  const AMBIENT_EFFECT_ID = "theme-inject-ambient-effect";
  const AMBIENT_MAX_PIXELS = 1600000;
  const AMBIENT_FRAME_INTERVAL = 1000 / 60;
  const DIFF_STYLE_ID = "theme-inject-diff-style";
  const TERMINAL_ANSI_NAMES = ["Black", "Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White", "BrightBlack", "BrightRed", "BrightGreen", "BrightYellow", "BrightBlue", "BrightMagenta", "BrightCyan", "BrightWhite"];
  const TERMINAL_TOKEN_NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "bright-black", "bright-red", "bright-green", "bright-yellow", "bright-blue", "bright-magenta", "bright-cyan", "bright-white"];
  const PANEL_CSS = __THEME_INJECT_PANEL_CSS_JSON__;
  const TOOLBAR_ICON = __THEME_INJECT_ICON_DATA_URL_JSON__;
  const LOCALES = __THEME_INJECT_LOCALES_JSON__;
  const nonce = window.__THEME_INJECT_NONCE__;
  const callbacks = new Map();
  let assetUrls = new Map();
  const stagedUrls = new Map();
  const thumbnailUrls = new Map();
  const iconPreloads = new Map();
  let iconPreloadThemeId = "";
  const ASSET_RETRY_DELAYS = [250, 750, 1500, 3000, 6000, 12000, 30000];
  let requestSequence = 0;
  let destroyed = false;
  let lifecycleEpoch = 0;
  let state = null;
  let draft = null;
  let snapshot = null;
  let draftCustomCss = "";
  let stagingAssets = [];
  let dirty = false;
  let activeTab = "library";
  let mutationTimer = 0;
  const pendingIconRoots = new Set();
  const pendingRefresh = { layout: false, sidebar: false, icons: false, code: false };
  let draftPreviewTimer = 0;
  let draftPreviewDeadline = 0;
  let studioPreviewTimer = 0;
  let studioPreviewDeadline = 0;
  let studioDomTimer = 0;
  let modal = null;
  let busy = false;
  let switchingThemeId = "";
  let statusMessage = "";
  let statusKind = "";
  let aiSettings = null;
  let aiApiKeyDraft = "";
  let aiImageApiKeyDraft = "";
  let aiPrompt = "";
  let aiPromptError = "";
  let aiThemeName = "";
  let aiThemeDescription = "";
  let aiGenerateImages = true;
  let aiStudioMode = "generate";
  let aiUseCurrentResourceReference = true;
  let studioResourceSlot = "background.fullscreen";
  let studioResourcePrompt = "";
  let studioResourceError = "";
  let aiResourcePlansOnly = false;
  let pendingStudioResource = null;
  let studioResourceCandidates = [];
  let studioAssetPreview = null;
  let aiGenerateSidebarWatermark = false;
  let aiGenerateSystemIcons = false;
  let aiPetName = "";
  let aiPetDescription = "";
  let aiPetPrompt = "";
  let aiPetActionPrompt = "";
  let aiPetActionPrompts = {};
  let aiPetReferences = [];
  let aiPetGenerationStage = "idle";
  let aiPetBaseAsset = null;
  let aiPetBasePreviewUrl = "";
  let aiPetBasePreviewLoading = false;
  let aiPetBasePreviewAttemptedKey = "";
  let aiPetActionAssets = [];
  let aiPetActionPreviewsLoading = false;
  let petCreatorOpen = false;
  let petCreatorMode = "";
  let petCreatorRestoring = false;
  let petCreatorRestoreEpoch = 0;
  let petActionUseUploadedBase = false;
  let petCreatorTimer = 0;
  let petCreatorProgress = { state: "idle", items: [] };
  let petCreatorRequestLog = [];
  let petActionEditor = null;
  let petActionPromptSuggestSlot = "";
  let aiImageConcurrency = 4;
  let aiLanguage = "zh-CN";
  let aiReferences = [];
  let aiRequestLog = [];
  let aiResourcePlans = [];
  let aiGenerationProgress = { state: "idle", items: [] };
  let aiWorkspaceThemeId = "";
  const aiWorkspaceByTheme = new Map();
  let studioOpen = false;
  let studioTab = "generate";
  let studioTimer = 0;
  let studioProgressSignature = "";
  let generationEpoch = 0;
  let activeGenerationKind = "";
  let generationIslandKind = "";
  let generationIslandSignature = "";
  let generationIslandDismissTimer = 0;
  let assetHydrationEpoch = 0;
  let assetCacheThemeId = "";
  let assetRetryTimer = 0;
  let assetRetryThemeId = "";
  let assetRetrySignature = "";
  let assetRetryPaths = new Set();
  const themeThumbnailUrls = new Map();
  const themeThumbnailRequests = new Map();
  let studioSelectedDecoration = -1;
  let studioSaveOpen = false;
  let aiGeneratedDraft = false;
  let studioPreviewRefreshPending = false;
  let studioPreviewReady = false;
  let studioPreviewPreparing = false;
  let studioPreviewSignature = "";
  let studioBaseline = null;
  let panelScrollTop = 0;
  let studioPaneScrollTop = 0;
  let activeDecorationDragCleanup = null;
  let ambientEffectFrame = 0;
  let ambientEffectWorker = null;
  let ambientEffectWorkerDisabled = false;
  let ambientEffectMetrics = null;
  let ambientEffectVisibilityCleanup = null;
  let ambientEffectSignature = "";
  let windowChromeTimer = 0;
  let windowChromeEpoch = 0;
  let codexAppearanceSetterPromise = null;
  let layoutResizeTarget = null;
  let brandSignature = "";
  let accountNameNode = null;
  let originalAccountName = "";
  let decorationsSignature = "";
  let homeSignature = "";
  let cssInspectorCleanup = null;
  let triggerAppearanceDraft = null;
  let updateStatus = null;
  let updateStatusSignature = "";
  let updateStatusLoading = false;
  let updatePollTimer = 0;
  let petPreviewAction = "idle";
  const petAssetUrls = new Map();
  const petAssetRequests = new Map();
  let rendererErrorReportAt = 0;

  if (window.__themeInjectRuntime?.destroy) window.__themeInjectRuntime.destroy();

  window.__themeInjectResolve = (id, response) => {
    const callback = callbacks.get(String(id));
    if (!callback) return;
    callbacks.delete(String(id));
    clearTimeout(callback.timer);
    if (response?.ok) callback.resolve(response.result);
    else callback.reject(new Error(response?.error?.message || "Theme Inject 请求失败"));
  };

  const call = (method, params = {}) => {
    if (destroyed) return Promise.reject(new Error("Theme Inject reloaded"));
    return new Promise((resolve, reject) => {
      const id = `${Date.now()}-${++requestSequence}`;
      const timeout = ["ai.generate", "ai.resource.generate", "ai.pet.generate", "ai.pet.base.generate", "ai.pet.actions.generate", "ai.pet.publish", "ai.pet.action.generate", "ai.pet.action.prompts.suggest", "runtime.reinject"].includes(method)
        ? 25 * 60 * 1000
        : method === "app.update.check"
          ? 90 * 1000
        : ["theme.preview.data", "theme.preview.thumbnail", "theme.background.data"].includes(method)
          ? 2 * 60 * 1000
          : ["theme.package.import", "theme.package.export", "pet.library.import", "pet.library.export", "theme.background.import", "theme.asset.import", "ai.reference.import", "ai.reference.upload", "app.trigger.icon.import"].includes(method)
            ? 10 * 60 * 1000
            : 16000;
      const timer = setTimeout(() => {
        callbacks.delete(id);
        reject(new Error(`${method} 请求超时`));
      }, timeout);
      callbacks.set(id, { resolve, reject, timer });
      window.themeInjectV1(JSON.stringify({ version: 1, id, nonce, method, params }));
    });
  };

  function reportRendererError(event, detail) {
    if (String(detail?.message || "").startsWith("ResizeObserver loop")) return;
    const now = Date.now();
    if (now - rendererErrorReportAt < 1000) return;
    rendererErrorReportAt = now;
    void call("diagnostics.report", { event, ...detail }).catch(() => {});
  }

  window.addEventListener("error", event => reportRendererError("renderer.error", {
    message: String(event?.message || event?.error?.message || "未知脚本错误").slice(0, 1000),
    source: String(event?.filename || "").slice(0, 500),
    line: event?.lineno || 0,
    column: event?.colno || 0,
    stack: String(event?.error?.stack || "").slice(0, 2000),
  }));
  window.addEventListener("unhandledrejection", event => reportRendererError("renderer.unhandled_rejection", {
    message: String(event?.reason?.message || event?.reason || "未处理的 Promise 拒绝").slice(0, 1000),
    stack: String(event?.reason?.stack || "").slice(0, 2000),
  }));

  window.themeInject = { call };

  const tabs = [
    ["library", "主题库"], ["pets", "宠物库"], ["colors", "颜色"], ["background", "背景"], ["type", "字体"],
    ["effects", "效果"], ["terminal", "终端"], ["skin", "皮肤"], ["ai", "AI 生成"], ["advanced", "高级"],
  ];
  const colorFields = [
    ["appBackground", "应用背景"], ["sidebarBackground", "左侧导航背景"], ["contentBackground", "内容区"],
    ["elevatedBackground", "浮层"], ["inputBackground", "输入区"], ["hoverBackground", "悬停"],
    ["activeBackground", "选中"], ["foreground", "主要文字"], ["mutedForeground", "次要文字"],
    ["subtleForeground", "弱化文字"], ["border", "边框"], ["accent", "强调色"],
    ["accentForeground", "强调文字"], ["selection", "选择区域"], ["link", "链接"],
    ["success", "成功"], ["warning", "警告"], ["danger", "危险"],
    ["codeBackground", "代码背景"], ["codeForeground", "代码文字"],
  ];
  const chromeFields = [
    ["titlebarBackground", "标题栏背景"], ["titlebarForeground", "标题栏文字"], ["titlebarBorder", "标题栏边框"],
    ["titlebarHover", "标题栏悬停"], ["sidebarForeground", "左侧导航文字"], ["sidebarMutedForeground", "左侧导航次要文字"],
    ["sidebarIcon", "左侧导航图标"], ["sidebarBorder", "左侧导航边框"], ["sidebarHover", "左侧导航悬停"],
    ["sidebarActive", "左侧导航选中"], ["sidebarActiveForeground", "选中文字"],
  ];
  const rightSidebarFields = [
    ["rightSidebarBackground", "右侧边栏背景"], ["rightSidebarForeground", "右侧边栏文字"],
    ["rightSidebarMutedForeground", "右侧边栏次要文字"], ["rightSidebarIcon", "右侧边栏图标"],
    ["rightSidebarBorder", "右侧边栏边框"], ["rightSidebarHover", "右侧边栏悬停"],
    ["rightSidebarActive", "右侧边栏选中"], ["rightSidebarActiveForeground", "右侧栏选中文字"],
    ["rightSidebarShortcutBackground", "快捷键标签背景"], ["rightSidebarShortcutForeground", "快捷键标签文字"],
  ];
  const skinResourceFields = [
    ["logo", "品牌 Logo", "logo"], ["sidebarWatermark", "左侧导航水印", "sidebar-watermark"],
    ["heroImage", "Hero 图片", "hero"], ["heroBadge", "Hero 徽章", "hero-badge"],
    ["avatar", "品牌区头像", "avatar"], ["sticker", "角落贴纸", "sticker"],
    ["composerDecoration", "输入区装饰", "composer-decoration"],
  ];
  const skinIconFields = [
    ["sidebar-toggle", "侧栏开关"], ["new-task", "新建任务"], ["search", "搜索"],
    ["scheduled", "已安排"], ["plugins", "插件"], ["pull-requests", "拉取请求"],
    ["settings", "设置"], ["send", "发送"], ["terminal", "终端"],
    ["files", "文件"], ["browser", "浏览器"], ["environments", "环境"],
    ["git", "Git"], ["connections", "连接"], ["worktrees", "工作树"], ["hooks", "钩子"],
    ["account", "账户"], ["general", "常规"], ["appearance", "外观"], ["voice", "语音"],
    ["configuration", "配置"], ["personalization", "个性化"], ["pets", "宠物"],
    ["keyboard-shortcuts", "键盘快捷键"], ["computer-control", "电脑操控"],
    ["project-folder", "项目文件夹"], ["project-open", "展开的项目"],
    ["section-toggle", "分组展开/收起"], ["more-actions", "更多操作"], ["project-app", "当前项目入口"],
  ];
  const iconSelectors = {
    "sidebar-toggle": '[data-app-shell-sidebar-trigger="true"]',
    "new-task": '[data-app-action-sidebar-new-task], [data-app-action-new-task]',
    search: '[data-app-action-sidebar-search], [data-app-action-search]',
    scheduled: '[data-app-action-sidebar-scheduled], [data-app-action-scheduled-tasks]',
    plugins: '[data-app-action-sidebar-plugins], [data-app-action-plugins]',
    "pull-requests": '[data-app-action-sidebar-pull-requests], [data-app-action-pull-requests]',
    settings: '[data-app-action-sidebar-settings], [data-app-action-settings]',
    send: '[data-codex-composer-submit], [data-app-action-submit], [data-composer-navigation-target="submit"]',
    terminal: '[data-app-action-terminal]',
    files: '[data-app-action-files], [data-app-action-open-files]',
    browser: '[data-app-action-browser]',
    environments: '[data-app-action-environments]',
    git: '[data-app-action-git]',
    connections: '[data-app-action-connections]',
    worktrees: '[data-app-action-worktrees]',
    hooks: '[data-app-action-hooks]',
    account: '[data-app-action-account]',
    general: '[data-app-action-settings-general]',
    appearance: '[data-app-action-settings-appearance]',
    voice: '[data-app-action-settings-voice]',
    configuration: '[data-app-action-settings-configuration]',
    personalization: '[data-app-action-settings-personalization]',
    pets: '[data-app-action-settings-pets]',
    "keyboard-shortcuts": '[data-app-action-settings-keyboard-shortcuts]',
    "computer-control": '[data-app-action-computer-control]',
    "project-folder": '[data-app-action-sidebar-project-row]',
    "project-open": '[data-app-action-sidebar-project-row][aria-expanded="true"], [data-app-action-sidebar-project-row][data-state="open"], [data-app-action-sidebar-project-row]:has([aria-expanded="true"])',
    "section-toggle": '[data-app-action-sidebar-section-heading][aria-expanded], [data-app-action-sidebar-section] button[aria-expanded]',
    "more-actions": '.app-shell-left-panel button[aria-haspopup="menu"], [data-theme-inject-project-action="true"][aria-haspopup="menu"]',
    "project-app": '[data-theme-inject-project-action="true"]:not([aria-haspopup="menu"])',
  };
  const resourceSlotOptions = [
    ["background.fullscreen", "全屏背景"], ["background.content", "内容区背景"], ["background.sidebar", "左侧导航背景"],
    ["skin.logo", "Logo"], ["skin.sidebarWatermark", "左侧导航水印"], ["skin.heroImage", "Hero 图片"], ["skin.heroBadge", "Hero 徽章"],
    ["skin.avatar", "品牌区头像"], ["skin.sticker", "角落贴纸"], ["skin.composerDecoration", "输入区装饰"],
    ["skin.decorations.0.asset", "自定义装饰层"], ["skin.icons.atlas", "完整图标集（30 项）"],
    ...skinIconFields.map(([action, label]) => [`skin.icons.${action}`, `动作图标 · ${label}`]),
  ];
  function studioResourceSlotOptions() {
    const options = [...resourceSlotOptions];
    for (let index = 0; index < (draft?.skin?.decorations?.length || 0); index += 1) {
      const slot = `skin.decorations.${index}.asset`;
      if (!options.some(([value]) => value === slot)) options.push([slot, `自定义装饰层 ${index + 1}`]);
    }
    for (let index = 0; index < (draft?.skin?.home?.cards?.length || 0); index += 1) {
      options.push([`skin.home.cards.${index}.icon`, `快捷卡片图标 · ${draft.skin.home.cards[index].title || index + 1}`]);
    }
    return options;
  }
  const themeLanguageOptions = [["zh-CN", "简体中文"], ["zh-TW", "繁體中文"], ["en-US", "English"], ["ja-JP", "日本語"], ["ko-KR", "한국어"]];
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function saveAiWorkspace() {
    if (!aiWorkspaceThemeId) return;
    aiWorkspaceByTheme.set(aiWorkspaceThemeId, clone({
      aiPrompt, aiThemeName, aiThemeDescription, aiGenerateImages, aiStudioMode, aiUseCurrentResourceReference,
      aiGenerateSidebarWatermark, aiGenerateSystemIcons, aiLanguage,
      aiRequestLog, aiResourcePlans, aiGenerationProgress, studioTab, studioResourceSlot, studioResourcePrompt, studioResourceCandidates,
    }));
  }
  function activateAiWorkspace(theme) {
    const themeId = theme?.id || "";
    if (!themeId || themeId === aiWorkspaceThemeId) return;
    saveAiWorkspace();
    const workspace = aiWorkspaceByTheme.get(themeId);
    aiWorkspaceThemeId = themeId;
    aiPrompt = workspace?.aiPrompt || "";
    aiPromptError = "";
    aiThemeName = workspace?.aiThemeName || theme.name || "";
    aiThemeDescription = workspace?.aiThemeDescription || theme.description || "";
    aiGenerateImages = workspace?.aiGenerateImages ?? true;
    aiStudioMode = workspace?.aiStudioMode || "generate";
    aiUseCurrentResourceReference = workspace?.aiUseCurrentResourceReference ?? true;
    aiGenerateSidebarWatermark = workspace?.aiGenerateSidebarWatermark ?? false;
    aiGenerateSystemIcons = workspace?.aiGenerateSystemIcons ?? false;
    aiLanguage = workspace?.aiLanguage || "zh-CN";
    aiRequestLog = clone(workspace?.aiRequestLog || []);
    aiResourcePlans = clone(workspace?.aiResourcePlans || []);
    aiGenerationProgress = clone(workspace?.aiGenerationProgress || { state: "idle", items: [] });
    studioTab = workspace?.studioTab || "generate";
    studioResourceSlot = workspace?.studioResourceSlot || "background.fullscreen";
    studioResourcePrompt = workspace?.studioResourcePrompt || "";
    studioResourceCandidates = clone(workspace?.studioResourceCandidates || []);
    studioAssetPreview = null;
    studioResourceError = "";
    studioProgressSignature = "";
    pendingStudioResource = studioResourceCandidates[0] || null;
    aiGeneratedDraft = false;
  }
  function escapeHtml(value) { return String(value ?? "").replace(/[&<>"']/g, character => ({ "&":"&amp;", "<":"&lt;", ">":"&gt;", "\"":"&quot;", "'":"&#39;" })[character]); }
  function cssValue(value, fallback) { return typeof value === "string" && value ? value : fallback; }
  function normalizeColor(value) { return /^#[0-9a-f]{6}/i.test(value || "") ? value.slice(0, 7) : "#000000"; }
  function rgbaFromHex(hex, opacity) {
    const value = normalizeColor(hex);
    const red = parseInt(value.slice(1, 3), 16), green = parseInt(value.slice(3, 5), 16), blue = parseInt(value.slice(5, 7), 16);
    return `rgba(${red},${green},${blue},${Math.max(0, Math.min(1, opacity))})`;
  }
  function colorChannels(hex) { const value = normalizeColor(hex); return [1, 3, 5].map(index => parseInt(value.slice(index, index + 2), 16)); }
  function colorLuminance(hex) { const values = colorChannels(hex).map(value => { const normalized = value / 255; return normalized <= .03928 ? normalized / 12.92 : ((normalized + .055) / 1.055) ** 2.4; }); return values[0] * .2126 + values[1] * .7152 + values[2] * .0722; }
  function colorContrast(left, right) { const values = [colorLuminance(left), colorLuminance(right)].sort((a, b) => b - a); return (values[0] + .05) / (values[1] + .05); }
  function mixColor(background, foreground, amount) { const left = colorChannels(background), right = colorChannels(foreground); return `#${left.map((value, index) => Math.round(value * (1 - amount) + right[index] * amount).toString(16).padStart(2, "0")).join("")}`; }
  function safeForeground(background, requested, minimum = 4.5) {
    const desired = normalizeColor(requested);
    if (colorContrast(background, desired) >= minimum) return desired;
    return colorContrast(background, "#FFFFFF") >= colorContrast(background, "#111318") ? "#FFFFFF" : "#111318";
  }
  function safeTint(background, requested, minimum = 4.5) {
    const desired = normalizeColor(requested);
    if (colorContrast(background, desired) >= minimum) return desired;
    const target = colorContrast(background, "#FFFFFF") >= colorContrast(background, "#111318") ? "#FFFFFF" : "#111318";
    for (const amount of [.2, .35, .5, .65, .8, 1]) {
      const candidate = mixColor(desired, target, amount);
      if (colorContrast(background, candidate) >= minimum) return candidate;
    }
    return target;
  }
  function assetUrl(theme, relative) {
    if (!relative || !state) return "";
    const path = `${encodeURIComponent(theme.id)}/${relative.split("/").map(encodeURIComponent).join("/")}`;
    return `${state.assetBase}/assets/${path}?token=${encodeURIComponent(state.assetToken)}`;
  }
  function resolvedAsset(theme, relative) { return stagedUrls.get(relative) || (assetCacheThemeId === theme?.id ? assetUrls.get(relative) : "") || assetUrl(theme, relative); }
  function isPreviewDataUrl(value) { return typeof value === "string" && value.startsWith("data:image/"); }
  function isPreviewImageUrl(value) { return isPreviewDataUrl(value) || (typeof value === "string" && /^https?:\/\//i.test(value)); }
  function previewCacheKey(asset) { return asset?.session && asset?.path ? `${asset.session}:${asset.path}` : asset?.path || ""; }
  async function previewDataUrl(asset, attempts = 4, allowFullFallback = true) {
    const key = previewCacheKey(asset);
    const cached = thumbnailUrls.get(key) || (!asset?.session ? thumbnailUrls.get(asset?.path) : "") || "";
    if (cached || !asset?.session || !asset?.path) return cached;
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      try {
        const result = await call("theme.preview.thumbnail", { session: asset.session, path: asset.path });
        const url = isPreviewDataUrl(result.url) ? result.url : "";
        if (url) {
          thumbnailUrls.set(key, url);
          thumbnailUrls.set(asset.path, url);
          return url;
        }
      } catch {}
      if (attempt + 1 < attempts) await waitFor([250, 750, 1500][Math.min(attempt, 2)]);
    }
    if (!allowFullFallback) return "";
    try {
      const result = await call("theme.preview.data", { session: asset.session, path: asset.path });
      const url = isPreviewDataUrl(result.url) ? result.url : "";
      if (url) {
        thumbnailUrls.set(key, url);
        thumbnailUrls.set(asset.path, url);
        return url;
      }
    } catch {}
    return "";
  }
  async function loadStagedAssetUrls(assets, shouldContinue = () => true) {
    const direct = (assets || [])
      .filter(asset => asset?.path && isPreviewImageUrl(asset.previewUrl))
      .map(asset => [asset.path, asset.previewUrl]);
    const pending = (assets || []).filter(asset => asset?.path && !isPreviewImageUrl(asset.previewUrl));
    const loaded = await mapConcurrent(pending, 1, async asset => {
      try {
        const result = await call("theme.preview.data", { session: asset.session, path: asset.path });
        return [asset.path, isPreviewImageUrl(result.url) ? result.url : ""];
      } catch { return [asset.path, ""]; }
    });
    if (!shouldContinue()) return null;
    const urls = new Map([...direct, ...loaded].filter(([, url]) => url));
    return urls;
  }
  async function loadStagedAssetDataUrls(assets) {
    const loaded = await mapConcurrent((assets || []).filter(asset => asset?.session && asset?.path), 2, async asset => {
      try {
        const result = await call("theme.preview.data", { session: asset.session, path: asset.path });
        return [asset.path, isPreviewDataUrl(result.url) ? result.url : ""];
      } catch { return [asset.path, ""]; }
    });
    return new Map(loaded.filter(([, url]) => url));
  }
  async function cacheStagedAssetUrls(assets, shouldContinue = () => true) {
    const urls = await loadStagedAssetUrls(assets, shouldContinue);
    if (!urls || !shouldContinue()) return false;
    for (const asset of assets || []) {
      if (!asset?.path) continue;
      if (urls.has(asset.path)) stagedUrls.set(asset.path, urls.get(asset.path));
      else stagedUrls.delete(asset.path);
    }
    return true;
  }
  async function cancelPreviewSessions(items) {
    const sessions = [...new Set((items || []).map(item => item?.session).filter(Boolean))];
    for (const session of sessions) {
      try {
        await call("theme.preview.cancel", { session });
      } catch {}
    }
  }
  function waitFor(delay) { return new Promise(resolve => setTimeout(resolve, delay)); }
  function themeAssetPaths(theme) {
    const paths = new Set();
    [theme.background?.fullscreen, theme.background?.content, theme.background?.sidebar].forEach(background => { if (background?.kind === "image" && background.path) paths.add(background.path); });
    Object.values(theme.skin?.resources || {}).forEach(path => { if (typeof path === "string" && path) paths.add(path); });
    Object.values(theme.skin?.icons?.mappings || {}).forEach(path => { if (path) paths.add(path); });
    (theme.skin?.decorations || []).forEach(item => item.asset && paths.add(item.asset));
    (theme.skin?.home?.cards || []).forEach(item => item.icon && paths.add(item.icon));
    return paths;
  }
  async function mapConcurrent(items, concurrency, mapper) {
    const results = new Array(items.length);
    let next = 0;
    const workers = Array.from({ length: Math.min(concurrency, items.length) }, async () => {
      while (next < items.length) {
        const index = next++;
        results[index] = await mapper(items[index], index);
      }
    });
    await Promise.all(workers);
    return results;
  }
  function visibleAssetPriority(theme) {
    const priority = [];
    const add = path => { if (path && !priority.includes(path)) priority.push(path); };
    (theme.skin?.decorations || []).forEach(item => add(item.asset));
    add(theme.skin?.resources?.heroBadge);
    add(theme.skin?.resources?.sticker);
    add(theme.skin?.resources?.composerDecoration);
    add(theme.skin?.resources?.logo);
    add(theme.skin?.resources?.accountAvatar);
    add(theme.skin?.resources?.avatar);
    add(theme.skin?.resources?.heroImage);
    add(theme.skin?.resources?.sidebarWatermark);
    return priority;
  }
  function clearAssetRetry() {
    clearTimeout(assetRetryTimer);
    assetRetryTimer = 0;
    assetRetryThemeId = "";
    assetRetrySignature = "";
    assetRetryPaths = new Set();
  }
  function scheduleAssetRetry(theme, attempt) {
    if (destroyed || !theme?.id) return;
    clearTimeout(assetRetryTimer);
    assetRetryThemeId = theme.id;
    assetRetrySignature = `${theme.id}|${[...themeAssetPaths(theme)].sort().join("|")}`;
    const delay = ASSET_RETRY_DELAYS[Math.min(attempt, ASSET_RETRY_DELAYS.length - 1)];
    const lifecycle = lifecycleEpoch;
    assetRetryTimer = setTimeout(() => {
      assetRetryTimer = 0;
      const currentSignature = draft ? `${draft.id}|${[...themeAssetPaths(draft)].sort().join("|")}` : "";
      if (destroyed || lifecycle !== lifecycleEpoch || currentSignature !== assetRetrySignature || assetRetryThemeId !== theme.id) return;
      void hydrateAssets(draft, { retryAttempt: attempt + 1 }).then(result => {
        if (result === null || destroyed || lifecycle !== lifecycleEpoch || draft?.id !== theme.id) return;
        applyTheme(draft, state?.customCssTrusted ? draftCustomCss : "");
        if (studioOpen) renderStudio(busy ? false : true);
        else if (document.getElementById(PANEL_ID)?.dataset.open === "true") renderPanel();
      });
    }, delay);
  }
  function expectedAtlasSlots(result) {
    if (!(result?.assets || []).some(asset => asset.slot === "skin.icons.atlas")) return [];
    return skinIconFields.map(([action]) => `skin.icons.${action}`);
  }
  function atlasRestoreComplete(result, urls) {
    const slots = expectedAtlasSlots(result);
    if (!slots.length) return true;
    const assets = new Map((result.assets || []).map(asset => [asset.slot, asset]));
    const mappings = result.theme?.skin?.icons?.mappings || {};
    const atlas = assets.get("skin.icons.atlas");
    if (!atlas?.path || !isPreviewImageUrl(urls.get(atlas.path))) return false;
    return slots.every(slot => {
      const action = slot.slice("skin.icons.".length);
      const asset = assets.get(slot);
      return asset?.path && mappings[action] === asset.path && isPreviewImageUrl(urls.get(asset.path));
    });
  }
  function generationUrlsComplete(result, urls) {
    const assets = (result?.assets || []).filter(asset => asset?.path);
    return assets.every(asset => isPreviewImageUrl(urls.get(asset.path))) && atlasRestoreComplete(result, urls);
  }
  async function loadCompleteGenerationUrls(result, shouldContinue = () => true, attempts = 1) {
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      const urls = await loadStagedAssetUrls(result.assets || [], shouldContinue);
      if (!urls || !shouldContinue()) return null;
      if (generationUrlsComplete(result, urls)) return urls;
      if (attempt + 1 < attempts) await waitFor(ASSET_RETRY_DELAYS[Math.min(attempt, ASSET_RETRY_DELAYS.length - 1)]);
      if (!shouldContinue()) return null;
    }
    return null;
  }
  async function restoreCompleteGeneration(initialResult, shouldContinue = () => true, attempts = 3, restoreParams = null) {
    let result = initialResult;
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      if (attempt > 0 || !result) {
        try {
          result = await call("ai.generation.restore", restoreParams || {});
        } catch {
          result = null;
        }
        if (!shouldContinue()) return null;
      }
      if (result) {
        const urls = await loadCompleteGenerationUrls(result, shouldContinue);
        if (!shouldContinue()) return null;
        if (urls) return { result, urls };
      }
      if (attempt + 1 < attempts) await waitFor(ASSET_RETRY_DELAYS[Math.min(attempt, 2)]);
      if (!shouldContinue()) return null;
    }
    return null;
  }
