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
  const BRAND_ID = "theme-inject-brand";
  const HOME_ID = "theme-inject-home";
  const DECORATIONS_ID = "theme-inject-decorations";
  const DIFF_STYLE_ID = "theme-inject-diff-style";
  const TERMINAL_ANSI_NAMES = ["Black", "Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White", "BrightBlack", "BrightRed", "BrightGreen", "BrightYellow", "BrightBlue", "BrightMagenta", "BrightCyan", "BrightWhite"];
  const TERMINAL_TOKEN_NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "bright-black", "bright-red", "bright-green", "bright-yellow", "bright-blue", "bright-magenta", "bright-cyan", "bright-white"];
  const PANEL_CSS = __THEME_INJECT_PANEL_CSS_JSON__;
  const nonce = window.__THEME_INJECT_NONCE__;
  const callbacks = new Map();
  let assetUrls = new Map();
  const stagedUrls = new Map();
  const thumbnailUrls = new Map();
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
  let draftPreviewTimer = 0;
  let draftPreviewDeadline = 0;
  let studioPreviewTimer = 0;
  let studioPreviewDeadline = 0;
  let modal = null;
  let busy = false;
  let statusMessage = "";
  let statusKind = "";
  let aiSettings = null;
  let aiApiKeyDraft = "";
  let aiImageApiKeyDraft = "";
  let aiPrompt = "";
  let aiGenerateImages = true;
  let aiGenerateSidebarWatermark = false;
  let aiGenerateSystemIcons = false;
  let aiImageConcurrency = 2;
  let aiLanguage = "zh-CN";
  let aiReferences = [];
  let aiRequestLog = [];
  let aiResourcePlans = [];
  let aiGenerationProgress = { state: "idle", items: [] };
  let studioOpen = false;
  let studioTab = "generate";
  let studioTimer = 0;
  let studioProgressSignature = "";
  let generationEpoch = 0;
  let assetHydrationEpoch = 0;
  let assetCacheThemeId = "";
  let assetRetryTimer = 0;
  let assetRetryThemeId = "";
  let assetRetrySignature = "";
  let assetRetryPaths = new Set();
  let studioSelectedDecoration = -1;
  let studioSaveOpen = false;
  let aiGeneratedDraft = false;
  let studioBaseline = null;
  let panelScrollTop = 0;
  let studioPaneScrollTop = 0;
  let activeDecorationDragCleanup = null;
  let layoutResizeTarget = null;
  let brandSignature = "";
  let decorationsSignature = "";
  let homeSignature = "";

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
      const timeout = method === "ai.generate"
        ? 25 * 60 * 1000
        : ["theme.preview.data", "theme.preview.thumbnail", "theme.background.data"].includes(method)
          ? 2 * 60 * 1000
          : ["theme.package.import", "theme.package.export", "theme.background.import", "theme.asset.import", "ai.reference.import", "ai.reference.upload"].includes(method)
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

  window.themeInject = { call };

  const tabs = [
    ["library", "主题库"], ["colors", "颜色"], ["background", "背景"], ["type", "字体"],
    ["layout", "布局"], ["effects", "效果"], ["terminal", "终端"], ["skin", "皮肤"], ["ai", "AI 生成"], ["advanced", "高级"],
  ];
  const colorFields = [
    ["appBackground", "应用背景"], ["sidebarBackground", "侧边栏"], ["contentBackground", "内容区"],
    ["elevatedBackground", "浮层"], ["inputBackground", "输入区"], ["hoverBackground", "悬停"],
    ["activeBackground", "选中"], ["foreground", "主要文字"], ["mutedForeground", "次要文字"],
    ["subtleForeground", "弱化文字"], ["border", "边框"], ["accent", "强调色"],
    ["accentForeground", "强调文字"], ["selection", "选择区域"], ["link", "链接"],
    ["success", "成功"], ["warning", "警告"], ["danger", "危险"],
    ["codeBackground", "代码背景"], ["codeForeground", "代码文字"],
  ];
  const chromeFields = [
    ["titlebarBackground", "标题栏背景"], ["titlebarForeground", "标题栏文字"], ["titlebarBorder", "标题栏边框"],
    ["titlebarHover", "标题栏悬停"], ["sidebarForeground", "侧栏文字"], ["sidebarMutedForeground", "侧栏次要文字"],
    ["sidebarIcon", "侧栏图标"], ["sidebarBorder", "侧栏边框"], ["sidebarHover", "侧栏悬停"],
    ["sidebarActive", "侧栏选中"], ["sidebarActiveForeground", "选中文字"],
  ];
  const skinResourceFields = [
    ["logo", "品牌 Logo", "logo"], ["sidebarWatermark", "侧栏水印", "sidebar-watermark"],
    ["heroImage", "Hero 图片", "hero"], ["heroBadge", "Hero 徽章", "hero-badge"],
    ["avatar", "侧栏头像", "avatar"], ["sticker", "角落贴纸", "sticker"],
    ["composerDecoration", "输入区装饰", "composer-decoration"],
  ];
  const skinIconFields = [
    ["sidebar-toggle", "侧栏开关"], ["new-task", "新建任务"], ["search", "搜索"],
    ["scheduled", "已安排"], ["plugins", "插件"], ["pull-requests", "拉取请求"],
    ["settings", "设置"], ["send", "发送"], ["terminal", "终端"],
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
  };
  const resourceSlotOptions = [
    ["background.fullscreen", "全屏背景"], ["background.content", "内容区背景"], ["background.sidebar", "侧栏背景"],
    ["skin.logo", "Logo"], ["skin.sidebarWatermark", "侧栏水印"], ["skin.heroImage", "Hero 图片"], ["skin.heroBadge", "Hero 徽章"],
    ["skin.avatar", "头像"], ["skin.sticker", "角落贴纸"], ["skin.composerDecoration", "输入区装饰"],
    ["skin.decorations.0.asset", "自定义装饰层"],
  ];
  const themeLanguageOptions = [["zh-CN", "简体中文"], ["zh-TW", "繁體中文"], ["en-US", "English"], ["ja-JP", "日本語"], ["ko-KR", "한국어"]];
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
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
  async function previewDataUrl(asset) {
    const cached = thumbnailUrls.get(asset?.path) || "";
    if (cached || !asset?.session || !asset?.path) return cached;
    try {
      const result = await call("theme.preview.thumbnail", { session: asset.session, path: asset.path });
      const url = isPreviewDataUrl(result.url) ? result.url : "";
      if (url && asset?.path) thumbnailUrls.set(asset.path, url);
      return url;
    } catch {
      return "";
    }
  }
  async function loadStagedAssetUrls(assets, shouldContinue = () => true) {
    const loaded = await mapConcurrent((assets || []).filter(asset => asset?.path), 3, async asset => {
      try {
        const result = await call("theme.preview.data", { session: asset.session, path: asset.path });
        return [asset.path, isPreviewDataUrl(result.url) ? result.url : ""];
      } catch { return [asset.path, ""]; }
    });
    if (!shouldContinue()) return null;
    const urls = new Map(loaded.filter(([, url]) => url));
    return urls;
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
        if (studioOpen) renderStudio(true);
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
    if (!atlas?.path || !isPreviewDataUrl(urls.get(atlas.path))) return false;
    return slots.every(slot => {
      const action = slot.slice("skin.icons.".length);
      const asset = assets.get(slot);
      return asset?.path && mappings[action] === asset.path && isPreviewDataUrl(urls.get(asset.path));
    });
  }
  function generationUrlsComplete(result, urls) {
    const assets = (result?.assets || []).filter(asset => asset?.path);
    return assets.every(asset => isPreviewDataUrl(urls.get(asset.path))) && atlasRestoreComplete(result, urls);
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
  async function restoreCompleteGeneration(initialResult, shouldContinue = () => true, attempts = 3) {
    let result = initialResult;
    for (let attempt = 0; attempt < attempts; attempt += 1) {
      if (attempt > 0 || !result) {
        try {
          result = await call("ai.generation.restore");
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
  function getPath(root, path) { return path.split(".").reduce((target, key) => target?.[key], root); }
  function setPath(root, path, value) { const keys = path.split("."); const last = keys.pop(); let target = root; for (const key of keys) target = target[key]; target[last] = value; }
  function hasBackground(config) { return config && config.kind !== "none" && (config.kind !== "image" || Boolean(config.path)); }
  function activeBackgrounds(theme) {
    const layout = theme.background || {};
    return layout.mode === "per-region"
      ? [["content", layout.content], ["sidebar", layout.sidebar]]
      : [["fullscreen", layout.fullscreen]];
  }
  function backgroundCss(theme, config) {
    if (!config || config.kind === "none") return "none";
    if (config.kind === "solid") return `linear-gradient(${config.color},${config.color})`;
    if (config.kind === "linear-gradient") return `linear-gradient(${config.angle || 0}deg, ${config.from}, ${config.to})`;
    if (config.kind === "radial-gradient") return `radial-gradient(circle at ${config.positionX ?? 40}% ${config.positionY ?? 20}%, ${config.from}, ${config.to})`;
    if (config.kind === "image" && config.path) return `url("${resolvedAsset(theme, config.path).replace(/"/g, "%22")}")`;
    return "none";
  }
  function surfaceCss(surface, fallbackFill, fallbackBorder, fallbackRadius, options = {}) {
    const fill = surface?.fill || fallbackFill;
    const border = surface?.borderColor || fallbackBorder;
    const radius = surface?.radius ?? fallbackRadius;
    const shadow = surface?.shadowColor || "#000000";
    const requestedOpacity = surface?.opacity ?? 1;
    const opacity = Math.max(options.minOpacity ?? 0, options.maxOpacity == null ? requestedOpacity : Math.min(requestedOpacity, options.maxOpacity));
    let css = surface?.fill || opacity !== 1 ? `background-color:${rgbaFromHex(fill, opacity)}!important;` : "";
    if ((surface?.borderWidth ?? 0) > 0) css += `border:${surface.borderWidth}px solid ${border}!important;`;
    if (options.radius !== false) css += `border-radius:${radius}px!important;`;
    if ((surface?.shadowOpacity ?? 0) > 0) css += `box-shadow:0 10px ${surface?.shadowBlur ?? 24}px ${rgbaFromHex(shadow, surface.shadowOpacity)}!important;`;
    if (options.blur !== false && (surface?.blur ?? 0) > 0) css += `backdrop-filter:blur(${surface.blur}px)!important;`;
    return css;
  }

  function terminalPalette(theme) {
    const fallback = ["#1B1D22", "#FF6B7A", "#67D391", "#E5C07B", "#6FA8FF", "#C792EA", "#5CCFE6", "#D8DEE9", "#60646C", "#FF8792", "#7BE0A0", "#F0D18C", "#87B7FF", "#D7A6F2", "#79D9EB", "#FFFFFF"];
    return fallback.map((color, index) => normalizeColor(theme.terminal?.ansi?.[index] || color));
  }

  function terminalVariableCss(theme) {
    return terminalPalette(theme).map((color, index) => `--vscode-terminal-ansi${TERMINAL_ANSI_NAMES[index]}:${color};--color-token-terminal-ansi-${TERMINAL_TOKEN_NAMES[index]}:${color};`).join("");
  }

  function terminalClassCss(theme) {
    return terminalPalette(theme).map((color, index) => `.xterm .xterm-rows .xterm-fg-${index}{color:${color}!important;}.xterm .xterm-rows .xterm-fg-${index}.xterm-dim{color:${rgbaFromHex(color, .55)}!important;}.xterm .xterm-rows .xterm-bg-${index}{background-color:${color}!important;}`).join("");
  }

  function diffShadowCss(theme) {
    const background = normalizeColor(theme.terminal?.background || theme.tokens.codeBackground);
    const foreground = safeForeground(background, theme.terminal?.foreground || theme.tokens.codeForeground, 7);
    const muted = safeForeground(background, theme.tokens.mutedForeground, 3.2);
    const addition = safeTint(background, theme.tokens.success, 4.5);
    const deletion = safeTint(background, theme.tokens.danger, 4.5);
    const additionBackground = rgbaFromHex(mixColor(background, addition, .24), .84);
    const deletionBackground = rgbaFromHex(mixColor(background, deletion, .24), .84);
    const elevated = mixColor(background, foreground, theme.baseMode === "dark" ? .09 : .06);
    return `
:host{color-scheme:${theme.baseMode}!important;color:${foreground}!important;background:${background}!important;--diffs-fg:${foreground}!important;--diffs-bg:${background}!important;--diffs-addition-color:${addition}!important;--diffs-deletion-color:${deletion}!important;}
:is([data-diff],[data-file],pre[data-diff],code[data-code]){color:${foreground}!important;background:${background}!important;}
[data-line-type="context"]{color:${muted}!important;background:${background}!important;}
[data-line-type="change-addition"]{color:${foreground}!important;background:${additionBackground}!important;}
[data-line-type="change-deletion"]{color:${foreground}!important;background:${deletionBackground}!important;}
[data-line-type="change-addition"][data-column-number]{color:${addition}!important;}
[data-line-type="change-deletion"][data-column-number]{color:${deletion}!important;}
[data-gutter]{border-color:${mixColor(background, foreground, .18)}!important;}
[data-separator="line-info"],[data-separator-content],[data-unmodified-lines]{color:${muted}!important;background:${elevated}!important;border-color:${mixColor(background, foreground, .18)}!important;}
`;
  }

  function themeCss(theme, customCss = "") {
    const tokens = theme.tokens, type = theme.typography, layout = theme.layout, shape = theme.shape, effects = theme.effects;
    const chrome = theme.chrome || {}, skin = theme.skin || {}, surfaces = skin.surfaces || {};
    const fullscreen = theme.background?.fullscreen || {}, contentBackground = theme.background?.content || {}, sidebarBackground = theme.background?.sidebar || {};
    const fullscreenActive = theme.background?.mode !== "per-region" && hasBackground(fullscreen);
    const contentActive = theme.background?.mode === "per-region" && hasBackground(contentBackground);
    const sidebarActive = theme.background?.mode === "per-region" && hasBackground(sidebarBackground);
    const contentOpacity = fullscreenActive ? fullscreen.surfaceOpacity : contentActive ? contentBackground.surfaceOpacity : 1;
    const sidebarOpacity = fullscreenActive ? Math.min(fullscreen.surfaceOpacity ?? .72, .48) : sidebarActive ? sidebarBackground.surfaceOpacity : 1;
    const effectiveContentOpacity = fullscreenActive ? Math.min(contentOpacity ?? .72, .36) : contentOpacity;
    const content = rgbaFromHex(tokens.contentBackground, effectiveContentOpacity ?? 1);
    const sidebar = rgbaFromHex(tokens.sidebarBackground, sidebarOpacity ?? 1);
    const elevated = rgbaFromHex(tokens.elevatedBackground, (fullscreenActive || contentActive) ? Math.max(.78, effects.panelOpacity ?? 1) : 1);
    const titlebarBase = normalizeColor(chrome.titlebarBackground || (skin.enabled && surfaces.titlebar?.fill) || tokens.appBackground);
    const titlebarForeground = safeForeground(titlebarBase, chrome.titlebarForeground || tokens.foreground);
    const titlebarMuted = safeForeground(titlebarBase, chrome.titlebarMutedForeground || tokens.mutedForeground, 3.2);
    const titlebarBorder = chrome.titlebarBorder || mixColor(titlebarBase, titlebarForeground, .18);
    const titlebarHover = chrome.titlebarHover || mixColor(titlebarBase, titlebarForeground, theme.baseMode === "dark" ? .13 : .08);
    const sidebarBase = normalizeColor((skin.enabled && surfaces.sidebar?.fill) || tokens.sidebarBackground);
    const sidebarForeground = safeForeground(sidebarBase, chrome.sidebarForeground || tokens.foreground);
    const sidebarMuted = safeForeground(sidebarBase, chrome.sidebarMutedForeground || tokens.mutedForeground, 3.2);
    const sidebarIcon = safeForeground(sidebarBase, chrome.sidebarIcon || sidebarForeground, 3.2);
    const sidebarBorder = chrome.sidebarBorder || mixColor(sidebarBase, sidebarForeground, .18);
    const sidebarHover = chrome.sidebarHover || mixColor(sidebarBase, sidebarForeground, theme.baseMode === "dark" ? .13 : .08);
    const sidebarSelected = chrome.sidebarActive || mixColor(sidebarBase, tokens.accent, .24);
    const sidebarSelectedForeground = safeForeground(sidebarSelected, chrome.sidebarActiveForeground || sidebarForeground);
    const contentBase = normalizeColor((skin.enabled && surfaces.content?.fill) || tokens.contentBackground);
    const contentForeground = safeForeground(contentBase, tokens.foreground);
    const contentMuted = safeForeground(contentBase, tokens.mutedForeground, 3.2);
    const heroBase = normalizeColor((skin.enabled && surfaces.hero?.fill) || tokens.elevatedBackground);
    const heroForeground = safeForeground(heroBase, tokens.foreground);
    const heroMuted = safeForeground(heroBase, tokens.mutedForeground, 3.2);
    const heroUsesLightText = colorLuminance(heroForeground) > .5;
    const heroProtection = heroUsesLightText ? "3,8,18" : "255,255,255";
    const cardBase = normalizeColor((skin.enabled && surfaces.card?.fill) || tokens.elevatedBackground);
    const cardForeground = safeForeground(cardBase, tokens.foreground);
    const cardMuted = safeForeground(cardBase, tokens.mutedForeground, 3.2);
    const composerBase = normalizeColor((skin.enabled && surfaces.composer?.fill) || tokens.inputBackground);
    const composerForeground = safeForeground(composerBase, tokens.foreground);
    const composerMuted = safeForeground(composerBase, tokens.subtleForeground, 3.2);
    const popoverBase = normalizeColor((skin.enabled && surfaces.popover?.fill) || tokens.elevatedBackground);
    const popoverForeground = safeForeground(popoverBase, tokens.foreground);
    const popoverMuted = safeForeground(popoverBase, tokens.mutedForeground, 4.5);
    const codeBase = normalizeColor(theme.terminal?.background || tokens.codeBackground);
    const codeForeground = safeForeground(codeBase, theme.terminal?.foreground || tokens.codeForeground, 7);
    const terminalCursor = safeForeground(codeBase, theme.terminal?.cursor || tokens.accent, 3);
    const diffSuccess = safeTint(codeBase, tokens.success, 4.5);
    const diffDanger = safeTint(codeBase, tokens.danger, 4.5);
    const diffInserted = rgbaFromHex(mixColor(codeBase, diffSuccess, .24), .84);
    const diffRemoved = rgbaFromHex(mixColor(codeBase, diffDanger, .24), .84);
    const diffInsertedText = rgbaFromHex(mixColor(codeBase, diffSuccess, .34), .9);
    const diffRemovedText = rgbaFromHex(mixColor(codeBase, diffDanger, .34), .9);
    const lineNumber = safeForeground(codeBase, tokens.mutedForeground, 3.2);
    const density = layout.density || 1, baseFontSize = 14 * (type.scale || 1);
    const shadowAlpha = Math.max(0, Math.min(.48, (effects.shadowStrength || 0) * .48));
    const effectiveBorder = colorContrast(tokens.border, tokens.inputBackground) < 1.35 ? mixColor(tokens.inputBackground, tokens.foreground, .28) : tokens.border;
    const skinEnabled = Boolean(skin.enabled);
    const headerBottom = document.documentElement.style.getPropertyValue("--ti-app-header-bottom") || "36px";
    const contentHeaderBottom = document.documentElement.style.getPropertyValue("--ti-content-header-bottom") || headerBottom;
    return `
:root {
  color-scheme:${theme.baseMode};
  --ti-app:${tokens.appBackground};--ti-sidebar:${sidebar};--ti-content:${content};--ti-elevated:${elevated};--ti-input:${tokens.inputBackground};
  --ti-hover:${tokens.hoverBackground};--ti-active:${tokens.activeBackground};--ti-foreground:${tokens.foreground};--ti-muted:${tokens.mutedForeground};
  --ti-subtle:${tokens.subtleForeground};--ti-border:${effectiveBorder};--ti-accent:${tokens.accent};--ti-accent-foreground:${tokens.accentForeground};
  --ti-selection:${tokens.selection};--ti-link:${tokens.link};--ti-success:${tokens.success};--ti-warning:${tokens.warning};--ti-danger:${tokens.danger};
  --ti-code-bg:${codeBase};--ti-code-fg:${codeForeground};--ti-ui-font:${type.uiFont};--ti-mono-font:${type.monoFont};
  --ti-radius:${shape.radius}px;--ti-border-width:${shape.borderWidth}px;--ti-shadow:0 12px 34px rgba(0,0,0,${shadowAlpha});--ti-app-header-bottom:${headerBottom};--ti-content-header-bottom:${contentHeaderBottom};
  --ti-titlebar-bg:${titlebarBase};--ti-titlebar-fg:${titlebarForeground};--ti-titlebar-muted:${titlebarMuted};--ti-titlebar-border:${titlebarBorder};--ti-titlebar-hover:${titlebarHover};
  --ti-sidebar-fg:${sidebarForeground};--ti-sidebar-muted:${sidebarMuted};--ti-sidebar-icon:${sidebarIcon};--ti-sidebar-border:${sidebarBorder};--ti-sidebar-hover:${sidebarHover};--ti-sidebar-selected:${sidebarSelected};--ti-sidebar-selected-fg:${sidebarSelectedForeground};--ti-skin-sidebar-fill:${surfaces.sidebar?.fill || tokens.sidebarBackground};
  --ti-content-fg:${contentForeground};--ti-content-muted:${contentMuted};--ti-hero-fg:${heroForeground};--ti-hero-muted:${heroMuted};--ti-hero-protection:rgba(${heroProtection},.88);--ti-hero-protection-soft:rgba(${heroProtection},.58);--ti-card-fg:${cardForeground};--ti-card-muted:${cardMuted};--ti-composer-fg:${composerForeground};--ti-composer-muted:${composerMuted};--ti-popover-fg:${popoverForeground};--ti-popover-muted:${popoverMuted};
  --ti-skin-icon:${skin.icons?.color || sidebarIcon};--ti-skin-icon-active:${skin.icons?.activeColor || tokens.accent};
  --spacing-token-sidebar:${layout.sidebarWidth}px;--thread-content-max-width:${layout.contentMaxWidth}px;--conversation-item-gap:${Math.round(16 * density)}px;
  --token-foreground:${tokens.foreground};--token-text-primary:${tokens.foreground};--token-text-secondary:${tokens.mutedForeground};--token-text-tertiary:${tokens.subtleForeground};
  --token-border:${effectiveBorder};--token-bg-fog:${tokens.elevatedBackground};--token-list-hover-background:${tokens.hoverBackground};--token-button-tertiary-foreground:${tokens.mutedForeground};
  --color-token-foreground:${tokens.foreground};--color-token-border:${effectiveBorder};--color-token-text-primary:${tokens.foreground};--color-token-text-secondary:${tokens.mutedForeground};
  --color-token-text-tertiary:${tokens.subtleForeground};--color-token-input-foreground:${tokens.foreground};--color-token-side-bar-foreground:${sidebarForeground};--color-token-link-foreground:${tokens.link};
  --color-token-button-primary-foreground:${tokens.accentForeground};--color-token-button-secondary-foreground:${tokens.foreground};--color-token-button-tertiary-foreground:${tokens.mutedForeground};
  --color-token-border-default:${effectiveBorder};--color-token-border-subtle:${rgbaFromHex(effectiveBorder,.66)};--color-token-border-heavy:${effectiveBorder};--color-token-description-foreground:${tokens.mutedForeground};
  --color-token-conversation-header:${tokens.mutedForeground};--color-token-bg-fog:${elevated};--color-token-list-hover-background:${tokens.hoverBackground};
  --color-token-bg-primary:${tokens.elevatedBackground};--color-token-bg-secondary:${tokens.inputBackground};--color-token-bg-tertiary:${tokens.hoverBackground};--color-token-primary:${contentForeground};
  --color-token-icon-foreground:${contentForeground};--color-token-disabled-foreground:${contentMuted};--color-token-input-placeholder-foreground:${composerMuted};--color-token-dropdown-foreground:${popoverForeground};
  --color-token-menu-background:${tokens.elevatedBackground};--color-token-menu-border:${effectiveBorder};--color-token-button-background:${tokens.inputBackground};--color-token-button-border:${effectiveBorder};--color-token-button-foreground:${composerForeground};
  --color-token-checkbox-background:${tokens.inputBackground};--color-token-checkbox-border:${effectiveBorder};--color-token-checkbox-foreground:${composerForeground};--color-token-list-active-selection-foreground:${sidebarSelectedForeground};--color-token-list-active-selection-icon-foreground:${sidebarSelectedForeground};
  --color-token-main-surface-primary:${content};--color-token-side-bar-background:${sidebar};--color-token-dropdown-background:${elevated};--color-token-input-background:${tokens.inputBackground};
  --color-token-editor-widget-background:${elevated};--color-token-diff-surface:${elevated};--color-token-diff-editor-inserted-line-background:${diffInserted};--color-token-diff-editor-inserted-text-background:${diffInsertedText};--color-token-diff-editor-removed-line-background:${diffRemoved};--color-token-diff-editor-removed-text-background:${diffRemovedText};--color-background-elevated-primary-opaque:${tokens.elevatedBackground};--color-background-elevated-secondary:${elevated};
  --color-token-list-active-selection-background:${tokens.activeBackground};--color-token-button-secondary-hover-background:${tokens.hoverBackground};
  --vscode-foreground:${contentForeground};--vscode-editor-background:${codeBase};--vscode-editor-foreground:${codeForeground};--vscode-focusBorder:${tokens.accent};
  --vscode-descriptionForeground:${tokens.mutedForeground};--vscode-disabledForeground:${tokens.subtleForeground};--vscode-input-foreground:${tokens.foreground};--vscode-dropdown-foreground:${tokens.foreground};
  --vscode-menu-foreground:${tokens.foreground};--vscode-editorWidget-foreground:${tokens.foreground};--vscode-sideBarTitle-foreground:${sidebarForeground};--vscode-commandCenter-foreground:${titlebarForeground};
  --vscode-input-border:${effectiveBorder};--vscode-dropdown-border:${effectiveBorder};--vscode-menu-border:${effectiveBorder};--vscode-contrastBorder:${effectiveBorder};--vscode-widget-border:${effectiveBorder};
  --vscode-titleBar-activeBackground:${titlebarBase};--vscode-titleBar-inactiveBackground:${titlebarBase};--vscode-titleBar-activeForeground:${titlebarForeground};--vscode-titleBar-inactiveForeground:${titlebarMuted};
  --vscode-menubar-selectionForeground:${titlebarForeground};--vscode-menubar-selectionBackground:${titlebarHover};--vscode-font-size:${baseFontSize}px;--vscode-editor-font-family:${type.monoFont};
  --vscode-list-hoverBackground:${tokens.hoverBackground};--vscode-list-activeSelectionBackground:${tokens.activeBackground};--vscode-sideBar-background:${sidebar};--vscode-sideBar-foreground:${sidebarForeground};
  --vscode-input-background:${tokens.inputBackground};--vscode-dropdown-background:${tokens.elevatedBackground};--vscode-menu-background:${tokens.elevatedBackground};--vscode-editorWidget-background:${tokens.elevatedBackground};
  --vscode-terminal-background:${codeBase};--vscode-terminal-foreground:${codeForeground};--vscode-terminalCursor-foreground:${terminalCursor};--vscode-terminal-border:${effectiveBorder};--vscode-terminal-selectionBackground:${theme.terminal?.selection || tokens.selection};
  --vscode-editorLineNumber-foreground:${lineNumber};--vscode-editorLineNumber-activeForeground:${codeForeground};--vscode-multiDiffEditor-background:${codeBase};--vscode-multiDiffEditor-headerBackground:${tokens.elevatedBackground};--vscode-multiDiffEditor-border:${effectiveBorder};
  --vscode-diffEditor-insertedLineBackground:${diffInserted};--vscode-diffEditor-insertedTextBackground:${diffInsertedText};--vscode-diffEditor-removedLineBackground:${diffRemoved};--vscode-diffEditor-removedTextBackground:${diffRemovedText};--vscode-editorGutter-addedBackground:${tokens.success};--vscode-editorGutter-deletedBackground:${tokens.danger};
  --color-token-terminal-background:${codeBase};--color-token-terminal-foreground:${codeForeground};${terminalVariableCss(theme)}
}
html{background:var(--ti-titlebar-bg)!important;}body{background:transparent!important;color:var(--ti-foreground)!important;font-family:var(--ti-ui-font)!important;font-size:${baseFontSize}px!important;}
body>#root,body>[data-reactroot],#__next{position:relative;z-index:1;background:transparent!important;}
.app-header-tint,[data-theme-inject-titlebar-surface="true"]{background:var(--ti-titlebar-bg)!important;color:var(--ti-titlebar-fg)!important;border:0!important;border-bottom:1px solid var(--ti-titlebar-border)!important;border-radius:0!important;box-shadow:none!important;backdrop-filter:none!important;filter:none!important;color-scheme:${theme.baseMode}!important;}
.app-header-tint button,.app-header-tint [role="button"],.app-header-tint [role="menuitem"],.app-header-tint [role="menubar"]>*,[data-theme-inject-titlebar-text="true"]{color:var(--ti-titlebar-muted)!important;opacity:1!important;}
.app-header-tint button{border-radius:6px!important;box-shadow:none!important;filter:none!important;}
.app-header-tint button:hover,.app-header-tint [role="button"]:hover{background:var(--ti-titlebar-hover)!important;color:var(--ti-titlebar-fg)!important;}
.app-header-tint svg,[data-theme-inject-titlebar-text="true"] svg{color:currentColor!important;stroke:currentColor!important;opacity:1!important;}
.app-header-tint svg [fill]:not([fill="none"]),[data-theme-inject-titlebar-text="true"] svg [fill]:not([fill="none"]){fill:currentColor!important;}
.app-header-tint svg [stroke]:not([stroke="none"]),[data-theme-inject-titlebar-text="true"] svg [stroke]:not([stroke="none"]){stroke:currentColor!important;}
.app-header-tint [data-testid="app-shell-header-context-menu-surface"],.app-header-tint [data-testid="app-shell-header-context-menu-surface"] *{color:var(--ti-titlebar-fg)!important;}
[data-theme-inject-task-header="true"]{left:var(--ti-main-left,0px)!important;right:0!important;width:auto!important;}
.app-shell-left-panel{position:relative!important;isolation:isolate;color:var(--ti-sidebar-fg)!important;background-color:${sidebarActive ? "transparent" : "var(--ti-sidebar)"}!important;border-color:var(--ti-sidebar-border)!important;width:${layout.sidebarWidth}px!important;min-width:${layout.sidebarWidth}px!important;--height-token-row:${Math.round(30*density)}px;--height-token-nav-row:${Math.round(30*density)}px;--padding-row-x:${Math.max(5,Math.round(8*density))}px;--token-foreground:var(--ti-sidebar-fg);--token-text-primary:var(--ti-sidebar-fg);--token-text-secondary:var(--ti-sidebar-muted);--token-text-tertiary:var(--ti-sidebar-muted);--token-muted-foreground:var(--ti-sidebar-muted);--token-list-hover-background:var(--ti-sidebar-hover);--color-token-side-bar-foreground:var(--ti-sidebar-fg);${skinEnabled ? surfaceCss(surfaces.sidebar,tokens.sidebarBackground,sidebarBorder,0,{radius:false,maxOpacity:sidebarOpacity}) : ""}}
.app-shell-left-panel>*{position:relative;z-index:1;}
.app-shell-left-panel nav,.app-shell-left-panel [data-app-action-sidebar-section]{background-color:transparent!important;}
.app-shell-left-panel svg{color:var(--ti-sidebar-icon)!important;stroke:currentColor!important;}
.app-shell-left-panel [data-app-action-sidebar-section-heading],.app-shell-left-panel .text-token-text-tertiary,.app-shell-left-panel .text-token-muted-foreground{color:var(--ti-sidebar-muted)!important;}
.app-shell-left-panel [data-app-action-sidebar-thread-id],.app-shell-left-panel [data-app-action-sidebar-project-row]{color:var(--ti-sidebar-fg)!important;}
.app-shell-left-panel :is(button,[role="button"],[data-app-action-sidebar-thread-id],[data-app-action-sidebar-project-row]){--token-foreground:var(--ti-sidebar-fg);--token-text-primary:var(--ti-sidebar-fg);--color-token-foreground:var(--ti-sidebar-fg);}
.app-shell-left-panel [data-app-action-sidebar-thread-id] :is(span,div),.app-shell-left-panel [data-app-action-sidebar-project-row] :is(span,div){color:inherit!important;}
.app-shell-left-panel [data-app-action-sidebar-thread-id]:hover,.app-shell-left-panel [data-app-action-sidebar-project-row]:hover{background-color:var(--ti-sidebar-hover)!important;}
.app-shell-left-panel [data-app-action-sidebar-thread-active="true"]{background-color:var(--ti-sidebar-selected)!important;color:var(--ti-sidebar-selected-fg)!important;}
.app-shell-left-panel [data-app-action-sidebar-thread-active="true"] *,.app-shell-left-panel [data-app-action-sidebar-thread-active="true"] svg{color:var(--ti-sidebar-selected-fg)!important;}
.main-surface{position:relative!important;isolation:isolate;background-color:${contentActive ? "transparent" : "var(--ti-content)"}!important;color:var(--ti-content-fg)!important;--token-foreground:var(--ti-content-fg);--token-text-primary:var(--ti-content-fg);--token-text-secondary:var(--ti-content-muted);--color-token-foreground:var(--ti-content-fg);--color-token-text-primary:var(--ti-content-fg);--color-token-text-secondary:var(--ti-content-muted);${skinEnabled ? surfaceCss(surfaces.content,tokens.contentBackground,effectiveBorder,shape.radius,{maxOpacity:effectiveContentOpacity,blur:false}) : ""}}
.app-shell-main-content-viewport,.thread-scroll-container{background:transparent!important;}
.main-surface,.main-surface *{--thread-content-max-width:${layout.contentMaxWidth}px!important;--composer-adjacent-max-width:calc(${layout.contentMaxWidth}px + 22px)!important;}
#${BACKDROP_IDS.fullscreen},#${BACKDROP_IDS.content},#${BACKDROP_IDS.sidebar}{pointer-events:none!important;overflow:hidden!important;}
#${BACKDROP_IDS.content},#${BACKDROP_IDS.sidebar}{z-index:-1!important;}
[data-theme-inject-custom-icon]{position:relative;}
[data-theme-inject-custom-icon] svg{display:none!important;}
[data-theme-inject-custom-icon]::before{content:"";flex:0 0 auto;width:16px;height:16px;background-image:var(--ti-custom-icon);background-size:contain;background-position:center;background-repeat:no-repeat;}
${skinEnabled && skin.icons?.mode !== "native" ? `.app-shell-left-panel button svg,.app-shell-left-panel [role="button"] svg{color:var(--ti-skin-icon)!important;stroke:currentColor!important;}.app-header-tint button svg,.app-header-tint [role="button"] svg{color:var(--ti-titlebar-muted)!important;stroke:currentColor!important;}.app-header-tint button:hover svg,.app-header-tint [role="button"]:hover svg{color:var(--ti-titlebar-fg)!important;}.app-shell-left-panel [data-app-action-sidebar-thread-active="true"] svg,.app-shell-left-panel [aria-pressed="true"] svg{color:var(--ti-skin-icon-active)!important;}` : ""}
:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"]){background:var(--ti-accent)!important;color:var(--ti-accent-foreground)!important;border-color:var(--ti-accent)!important;}
:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"]) svg{display:block!important;color:var(--ti-accent-foreground)!important;fill:currentColor!important;stroke:currentColor!important;opacity:1!important;}
:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"])[data-theme-inject-custom-icon="send"] svg{display:none!important;}
[data-theme-inject-button-contrast="true"]{color:var(--ti-native-button-fg)!important;}
[data-theme-inject-button-contrast="true"] svg{color:var(--ti-native-button-fg)!important;fill:currentColor!important;stroke:currentColor!important;opacity:1!important;}
[data-turn-key],[data-codex-composer-root],.text-size-chat{font-size:${baseFontSize}px;line-height:${type.lineHeight};}
[data-thread-scroll-footer]{--thread-content-max-width:${layout.contentMaxWidth}px;}
.composer-surface-chrome,[data-codex-composer-root] .composer-surface-chrome{background-color:var(--ti-input)!important;color:var(--ti-composer-fg)!important;border:var(--ti-border-width) solid var(--ti-border)!important;border-radius:var(--ti-radius)!important;box-shadow:var(--ti-shadow)!important;--token-foreground:var(--ti-composer-fg);--token-text-primary:var(--ti-composer-fg);--token-text-tertiary:var(--ti-composer-muted);${skinEnabled ? surfaceCss(surfaces.composer,tokens.inputBackground,effectiveBorder,shape.radius) : ""}}
.composer-surface-chrome *,[data-codex-composer="true"]{caret-color:var(--ti-composer-fg);}[data-codex-composer="true"]{color:var(--ti-composer-fg)!important;}[data-codex-composer="true"] p,[data-codex-composer="true"] span{color:inherit;}[data-codex-composer="true"][data-placeholder]:empty::before{color:var(--ti-composer-muted)!important;}
[role="dialog"],[role="menu"],[role="listbox"],[data-radix-popper-content-wrapper]>*{background-color:var(--ti-elevated)!important;color:var(--ti-popover-fg)!important;border-color:var(--ti-border)!important;--token-foreground:var(--ti-popover-fg);--token-text-primary:var(--ti-popover-fg);${skinEnabled ? surfaceCss(surfaces.popover,tokens.elevatedBackground,effectiveBorder,shape.radius) : ""}}
.rounded-3xl.bg-token-dropdown-background{background-color:var(--ti-elevated)!important;color:var(--ti-popover-fg)!important;border-color:var(--ti-border)!important;--token-foreground:var(--ti-popover-fg);--token-text-primary:var(--ti-popover-fg);--token-text-secondary:var(--ti-popover-muted);--token-text-tertiary:var(--ti-popover-muted);--color-token-bg-fog:var(--ti-elevated);--color-token-dropdown-background:var(--ti-elevated);}
[class~="group/turn-diff-header"] .bg-token-bg-fog,[role="tabpanel"][aria-label="审阅"] .bg-token-bg-fog{background-color:var(--ti-elevated)!important;color:var(--ti-popover-fg)!important;--token-foreground:var(--ti-popover-fg);--token-text-primary:var(--ti-popover-fg);--color-token-button-tertiary-foreground:var(--ti-popover-fg);}
a{color:var(--ti-link);}::selection{background:var(--ti-selection);color:var(--ti-foreground);}.monaco-editor,.monaco-editor-background,.xterm,.xterm-viewport,.xterm-screen{font-family:var(--ti-mono-font)!important;}pre,code{font-family:var(--ti-mono-font)!important;}
[id^="terminal-panel-"]{background:${codeBase}!important;color:${codeForeground}!important;color-scheme:${theme.baseMode}!important;--token-foreground:${codeForeground};--color-token-terminal-background:${codeBase};--color-token-terminal-foreground:${codeForeground};--vscode-terminal-background:${codeBase};--vscode-terminal-foreground:${codeForeground};--vscode-terminalCursor-foreground:${terminalCursor};--vscode-terminal-border:${effectiveBorder};}
.xterm,.xterm-viewport,.xterm-screen,.xterm-rows{background-color:${codeBase}!important;color:${codeForeground}!important;}.xterm .xterm-cursor-layer{color:${terminalCursor}!important;}.xterm .xterm-cursor{border-color:${terminalCursor}!important;box-shadow:1px 0 0 ${terminalCursor} inset!important;}${terminalClassCss(theme)}*{scrollbar-color:var(--ti-subtle) transparent;}
#${BRAND_ID}{font-family:${cssValue(skin.brand?.font,type.uiFont)};}
#${HOME_ID} .ti-home-hero{color:var(--ti-hero-fg)!important;${skinEnabled ? surfaceCss(surfaces.hero,tokens.elevatedBackground,effectiveBorder,shape.radius) : ""}}
#${HOME_ID} .ti-home-copy h1{color:var(--ti-hero-fg)!important;}#${HOME_ID} .ti-home-copy p{color:var(--ti-hero-muted)!important;}
#${HOME_ID} .ti-home-card{color:var(--ti-card-fg)!important;${skinEnabled ? surfaceCss(surfaces.card,tokens.elevatedBackground,effectiveBorder,shape.radius,{minOpacity:.84}) : ""}}#${HOME_ID} .ti-home-card span{color:var(--ti-card-muted)!important;}
[data-theme-inject-native-home="hidden"]{visibility:hidden!important;pointer-events:none!important;}
[data-theme-inject-light-surface="true"]{color:#172033!important;--token-foreground:#172033;--token-text-primary:#172033;--token-text-secondary:#52627a;--token-text-tertiary:#6b7b91;--color-token-foreground:#172033;--color-token-text-primary:#172033;--color-token-text-secondary:#52627a;--color-token-text-tertiary:#6b7b91;--color-token-button-foreground:#172033;--color-token-input-foreground:#172033;--color-token-icon-foreground:#334155;--vscode-foreground:#172033;--vscode-descriptionForeground:#52627a;--vscode-input-foreground:#172033;}
[data-theme-inject-light-surface="true"] :is(h1,h2,h3,h4,h5,h6,p,span,label,button,[role="button"],[role="switch"],select,input){color:inherit;}
${customCss || ""}`;
  }

  function applyTheme(theme, customCss = "") {
    if (destroyed || !theme) return;
    document.documentElement.dataset.themeInjectMode = theme.baseMode;
    document.documentElement.dataset.themeInjectSkin = String(Boolean(theme.skin?.enabled));
    document.documentElement.classList.toggle("electron-dark", theme.baseMode === "dark");
    document.documentElement.classList.toggle("electron-light", theme.baseMode !== "dark");
    let style = document.getElementById(STYLE_ID);
    if (!style) { style = document.createElement("style"); style.id = STYLE_ID; document.documentElement.appendChild(style); }
    style.textContent = themeCss(theme, customCss);
    mountBackgrounds(theme);
    patchTitlebar();
    syncSkin(theme);
    syncIcons(theme);
    syncNativeButtonContrast();
    patchShadowRoots(style.textContent);
    patchDiffRoots(theme);
    patchTerminal(theme);
    queueMicrotask(reportThemeState);
  }

  function ensureBackdrop(id) {
    let backdrop = document.getElementById(id);
    if (!backdrop) { backdrop = document.createElement("div"); backdrop.id = id; }
    return backdrop;
  }

  function configureBackdrop(theme, region, config, active) {
    const id = BACKDROP_IDS[region], backdrop = ensureBackdrop(id);
    const host = region === "fullscreen" ? document.body : region === "content" ? document.querySelector(".main-surface") : document.querySelector(".app-shell-left-panel");
    if (!host) { backdrop.remove(); return; }
    if (backdrop.parentElement !== host) host.prepend(backdrop);
    const blur = config?.kind === "image" ? Math.min(config?.blur || 0, 2) : Math.max(config?.blur || 0, Math.min(theme.effects?.backgroundBlur || 0, 6)), scoped = region !== "fullscreen";
    const source = backgroundCss(theme, config);
    const overlayBase = region === "sidebar" ? theme.tokens.sidebarBackground : theme.tokens.contentBackground;
    const overlay = rgbaFromHex(overlayBase, config?.surfaceOpacity ?? .72);
    Object.assign(backdrop.style, {
      display: active ? "block" : "none",
      position: scoped ? "absolute" : "fixed",
      top: scoped ? `${-blur}px` : "var(--ti-content-header-bottom,var(--ti-app-header-bottom,36px))",
      right: `${-blur}px`, bottom: `${-blur}px`, left: `${-blur}px`,
      zIndex: scoped ? "-1" : "0",
      background: scoped && active ? `linear-gradient(${overlay},${overlay}),${source}` : source,
      backgroundSize: config?.fit || "cover",
      backgroundPosition: `${config?.positionX ?? 50}% ${config?.positionY ?? 50}%`,
      backgroundRepeat: "no-repeat",
      opacity: String(config?.opacity ?? 1),
      filter: `blur(${blur}px) saturate(${(config?.saturation ?? 1) * (theme.effects?.saturation ?? 1)}) brightness(${config?.kind === "image" ? 1.14 : 1}) contrast(${config?.kind === "image" ? 1.06 : 1})`,
    });
    backdrop.dataset.kind = config?.kind || "none";
    backdrop.dataset.path = config?.path || "";
    backdrop.dataset.region = region;
    backdrop.dataset.loaded = String(config?.kind !== "image" || Boolean(resolvedAsset(theme, config?.path)));
  }

  function mountBackgrounds(theme) {
    const layout = theme.background || {}, unified = layout.mode !== "per-region";
    configureBackdrop(theme, "fullscreen", layout.fullscreen || {}, unified && hasBackground(layout.fullscreen));
    configureBackdrop(theme, "content", layout.content || {}, !unified && hasBackground(layout.content));
    configureBackdrop(theme, "sidebar", layout.sidebar || {}, !unified && hasBackground(layout.sidebar));
  }

  function patchTitlebar() {
    document.querySelectorAll("[data-theme-inject-titlebar-surface],[data-theme-inject-titlebar-text],[data-theme-inject-task-header]").forEach(node => {
      delete node.dataset.themeInjectTitlebarSurface;
      delete node.dataset.themeInjectTitlebarText;
      delete node.dataset.themeInjectTaskHeader;
    });
    const titlebars = [...document.querySelectorAll(".app-header-tint")];
    const systemTitlebar = titlebars
      .filter(node => node.getBoundingClientRect().top <= 1)
      .sort((left, right) => right.getBoundingClientRect().width - left.getBoundingClientRect().width)[0] || null;
    const taskTitlebar = titlebars
      .filter(node => node !== systemTitlebar && node.getBoundingClientRect().top > 1)
      .sort((left, right) => right.getBoundingClientRect().width - left.getBoundingClientRect().width)[0];
    const main = document.querySelector(".main-surface");
    if (layoutResizeTarget !== main) {
      layoutResizeObserver.disconnect();
      layoutResizeTarget = main;
      if (main) layoutResizeObserver.observe(main);
    }
    const systemBottom = systemTitlebar ? Math.max(0, Math.round(systemTitlebar.getBoundingClientRect().bottom)) : 36;
    const contentBottom = taskTitlebar ? Math.max(systemBottom, Math.round(taskTitlebar.getBoundingClientRect().bottom)) : systemBottom;
    const mainLeft = main ? Math.max(0, Math.round(main.getBoundingClientRect().left)) : 0;
    document.documentElement.style.setProperty("--ti-app-header-bottom", `${systemBottom || 36}px`);
    document.documentElement.style.setProperty("--ti-content-header-bottom", `${contentBottom || systemBottom || 36}px`);
    document.documentElement.style.setProperty("--ti-main-left", `${mainLeft}px`);
    if (taskTitlebar) taskTitlebar.dataset.themeInjectTaskHeader = "true";
    for (const titlebar of [systemTitlebar, taskTitlebar].filter(Boolean)) {
      titlebar.dataset.themeInjectTitlebarSurface = "true";
      titlebar.querySelectorAll("button,[role=button],[role=menuitem],[role=menubar]>*").forEach(node => node.dataset.themeInjectTitlebarText = "true");
    }
  }

  function syncSkin(theme) {
    if (!theme.skin?.enabled) {
      document.querySelector(".app-shell-left-panel")?.style.removeProperty("--ti-sidebar-watermark");
      document.querySelectorAll('[data-theme-inject-native-home="hidden"]').forEach(node => node.removeAttribute("data-theme-inject-native-home"));
      document.getElementById(BRAND_ID)?.remove();
      document.getElementById(HOME_ID)?.remove();
      document.getElementById(DECORATIONS_ID)?.remove();
      return;
    }
    syncBrand(theme);
    syncDecorations(theme);
    syncHome(theme);
  }

  function syncIcons(theme) {
    document.querySelectorAll("[data-theme-inject-custom-icon]").forEach(node => {
      delete node.dataset.themeInjectCustomIcon;
      node.style.removeProperty("--ti-custom-icon");
    });
    if (!theme.skin?.enabled || theme.skin.icons?.mode !== "custom") return;
    for (const [action, path] of Object.entries(theme.skin.icons.mappings || {})) {
      if (action === "send") continue;
      const url = resolvedAsset(theme, path);
      if (!url) continue;
      resolveIconNodes(action).forEach(node => {
        node.dataset.themeInjectCustomIcon = action;
        node.style.setProperty("--ti-custom-icon", `url("${url.replace(/"/g, "%22")}")`);
      });
    }
  }

  function resolveIconNodes(action) {
    const selector = iconSelectors[action];
    const stable = selector ? [...document.querySelectorAll(selector)] : [];
    if (stable.length) return stable;
    const sidebar = document.querySelector(".app-shell-left-panel");
    const composer = document.querySelector('[data-codex-composer-root],[data-codex-composer="true"]');
    const scope = action === "send" ? composer : action === "terminal" ? document : sidebar;
    const candidates = [...(scope || document).querySelectorAll("button,[role=button]")];
    const label = node => [node.getAttribute("aria-label"), node.getAttribute("title"), node.innerText]
      .filter(Boolean).join(" ").trim().replace(/\s+/g, " ");
    const isLabel = (node, values) => values.some(value => label(node) === value || label(node).startsWith(`${value} `));
    const matchers = {
      "new-task": node => isLabel(node, ["新建任务", "新增任務", "New task", "新しいタスク", "새 작업"]),
      search: node => isLabel(node, ["搜索", "搜尋", "Search", "検索", "검색"]),
      scheduled: node => isLabel(node, ["已安排", "已排程", "Scheduled", "スケジュール済み", "예약됨"]),
      plugins: node => isLabel(node, ["插件", "Plugins", "プラグイン", "플러그인"]),
      "pull-requests": node => isLabel(node, ["拉取请求", "提取要求", "Pull requests", "プルリクエスト", "풀 리퀘스트"]),
      settings: node => isLabel(node, ["设置", "設定", "Settings", "プロフィール メニューを開く", "프로필 메뉴 열기", "打开个人资料菜单", "開啟個人資料選單", "Open profile menu"]),
      send: node => isLabel(node, ["发送", "提交", "傳送", "送出", "Send", "Submit", "送信", "보내기", "제출"]),
      terminal: node => isLabel(node, ["终端", "終端機", "Terminal", "ターミナル", "터미널"]),
    };
    return matchers[action] ? candidates.filter(matchers[action]) : [];
  }

  function syncBrand(theme) {
    const sidebar = document.querySelector(".app-shell-left-panel");
    if (!sidebar) return;
    const host = sidebar.querySelector("nav") || sidebar.querySelector(".relative.flex.min-h-0.flex-1.flex-col.overflow-hidden") || sidebar;
    let brand = document.getElementById(BRAND_ID);
    if (!brand) { brand = document.createElement("section"); brand.id = BRAND_ID; }
    if (brand.parentElement !== host) host.prepend(brand);
    const logo = resolvedAsset(theme, theme.skin.resources?.logo);
    const avatar = resolvedAsset(theme, theme.skin.resources?.avatar);
    const watermark = resolvedAsset(theme, theme.skin.resources?.sidebarWatermark);
    const signature = JSON.stringify([theme.skin.brand, logo, avatar]);
    sidebar.style.setProperty("--ti-sidebar-watermark", watermark ? `url("${watermark.replace(/"/g, "%22")}")` : "none");
    if (brandSignature === signature && brand.childElementCount) return;
    brandSignature = signature;
    brand.style.removeProperty("--ti-brand-watermark");
    brand.innerHTML = `${logo ? `<img class="ti-brand-logo" src="${escapeHtml(logo)}" alt="">` : ""}<div class="ti-brand-copy"><strong>${escapeHtml(theme.skin.brand?.title || theme.name)}</strong>${theme.skin.brand?.subtitle ? `<span>${escapeHtml(theme.skin.brand.subtitle)}</span>` : ""}</div>${avatar ? `<img class="ti-brand-avatar" src="${escapeHtml(avatar)}" alt="">` : ""}`;
  }

  function syncDecorations(theme) {
    let root = document.getElementById(DECORATIONS_ID);
    if (!root) { root = document.createElement("div"); root.id = DECORATIONS_ID; document.body.appendChild(root); }
    const resources = theme.skin.resources || {};
    const builtins = [
      resources.heroBadge ? { asset: resources.heroBadge, region: "content", anchor: "top-right", offsetX: 24, offsetY: 64, width: 180, opacity: 1, layer: 2 } : null,
      resources.sticker ? { asset: resources.sticker, region: "content", anchor: "bottom-right", offsetX: 24, offsetY: 120, width: 180, opacity: 1, layer: 2 } : null,
      resources.composerDecoration ? { asset: resources.composerDecoration, region: "composer", anchor: "bottom-right", offsetX: 0, offsetY: 0, width: 220, opacity: .86, layer: 1 } : null,
    ].filter(Boolean);
    const decorations = [...builtins, ...(theme.skin.decorations || [])];
    const visible = decorations.filter(item => (item.region || "content") !== "titlebar" && (!item.hiddenBelow || window.innerWidth >= item.hiddenBelow) && Boolean(resolvedAsset(theme, item.asset)));
    const signature = JSON.stringify(visible.map(item => [item, resolvedAsset(theme, item.asset)]));
    if (decorationsSignature === signature && root.childElementCount === visible.length) return;
    decorationsSignature = signature;
    root.replaceChildren();
    for (const decoration of visible) {
      const url = resolvedAsset(theme, decoration.asset);
      if (!url) continue;
      const node = document.createElement("img");
      node.className = "ti-skin-decoration";
      node.src = url;
      node.alt = "";
      node.dataset.region = decoration.region || "content";
      node.dataset.anchor = decoration.anchor || "bottom-right";
      node.style.setProperty("--ti-decoration-x", `${decoration.offsetX || 0}px`);
      node.style.setProperty("--ti-decoration-y", `${decoration.offsetY || 0}px`);
      node.style.width = `${decoration.width || 160}px`;
      node.style.opacity = String(decoration.opacity ?? 1);
      node.style.zIndex = String(Math.max(0, Math.min(6, decoration.layer ?? 0)));
      if (decoration.hiddenBelow) node.style.setProperty("--ti-decoration-min-width", `${decoration.hiddenBelow}px`);
      root.appendChild(node);
    }
  }

  function syncHome(theme) {
    const main = document.querySelector(".app-shell-main-content-viewport") || document.querySelector(".main-surface"), homeConfig = theme.skin.home || {};
    const nativeHomeIcon = main?.querySelector('[data-testid="home-icon"]');
    const nativeHome = nativeHomeIcon?.closest('section,[data-testid="empty-thread"],[data-testid="home-view"]') || nativeHomeIcon?.parentElement?.parentElement?.parentElement?.parentElement || null;
    const composer = main?.querySelector('[data-codex-composer-root],[data-codex-composer="true"]');
    const emptyTask = Boolean(composer && !main.querySelector("[data-turn-key]") && !main.querySelector('[data-testid="conversation-turn"],[data-message-author-role]'));
    document.querySelectorAll('[data-theme-inject-native-home="hidden"]').forEach(node => node.removeAttribute("data-theme-inject-native-home"));
    if (!main || !homeConfig.enabled || !emptyTask) {
      const current = document.getElementById(HOME_ID);
      if (current && !current.classList.contains("ti-home-leaving")) {
        current.classList.add("ti-home-leaving");
        setTimeout(() => current.remove(), 90);
      }
      homeSignature = "";
      return;
    }
    if (nativeHome) nativeHome.dataset.themeInjectNativeHome = "hidden";
    let home = document.getElementById(HOME_ID);
    if (!home) { home = document.createElement("section"); home.id = HOME_ID; }
    home.classList.remove("ti-home-leaving");
    if (home.parentElement !== main) main.appendChild(home);
    const hero = resolvedAsset(theme, theme.skin.resources?.heroImage);
    const cards = (homeConfig.cards || []).slice(0, 4);
    const signature = JSON.stringify([homeConfig, hero, cards.map(card => resolvedAsset(theme, card.icon))]);
    if (homeSignature === signature && home.childElementCount) return;
    homeSignature = signature;
    home.style.setProperty("--ti-home-hero", hero ? `url("${hero.replace(/"/g, "%22")}")` : "none");
    home.style.setProperty("--ti-home-height", `${homeConfig.heroHeight || 320}px`);
    home.style.setProperty("--ti-home-columns", String(Math.max(1, Math.min(4, homeConfig.cardColumns || 4))));
    home.innerHTML = `<div class="ti-home-hero"><div class="ti-home-copy"><h1>${escapeHtml(homeConfig.title || "今天想构建什么？")}</h1>${homeConfig.subtitle ? `<p>${escapeHtml(homeConfig.subtitle)}</p>` : ""}</div><div class="ti-home-cards">${cards.map((card, index) => { const icon = resolvedAsset(theme, card.icon); return `<button type="button" class="ti-home-card" data-home-card="${index}">${icon ? `<img src="${escapeHtml(icon)}" alt="">` : ""}<strong>${escapeHtml(card.title)}</strong>${card.description ? `<span>${escapeHtml(card.description)}</span>` : ""}</button>`; }).join("")}</div></div>`;
    home.querySelectorAll("[data-home-card]").forEach(button => button.addEventListener("click", () => fillComposer(cards[Number(button.dataset.homeCard)]?.prompt || "")));
  }

  function fillComposer(prompt) {
    if (!prompt) return;
    const editor = document.querySelector('[data-codex-composer="true"][contenteditable="true"], [contenteditable="true"][data-placeholder], textarea');
    if (!editor) return;
    editor.focus();
    if (editor instanceof HTMLTextAreaElement || editor instanceof HTMLInputElement) {
      const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(editor), "value")?.set;
      setter?.call(editor, prompt);
    } else {
      editor.replaceChildren(document.createTextNode(prompt));
    }
    editor.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: prompt }));
  }

  function reportThemeState() {
    const main = document.querySelector(".main-surface"), composer = document.querySelector(".composer-surface-chrome");
    const backgrounds = Object.entries(BACKDROP_IDS).map(([region, id]) => { const node = document.getElementById(id); return { region, kind: node?.dataset.kind || "none", path: node?.dataset.path || "", active: node?.style.display !== "none", loaded: node?.dataset.loaded === "true" }; });
    void call("diagnostics.report", {
      event: "theme.applied", themeId: draft?.id || state?.activeTheme?.id || "", schemaVersion: draft?.schemaVersion || 0,
      backgrounds, titlebarMarked: document.querySelectorAll("[data-theme-inject-titlebar-surface],[data-theme-inject-titlebar-text]").length,
      mainBackground: main ? getComputedStyle(main).backgroundColor : "", composerBorder: composer ? getComputedStyle(composer).borderColor : "",
      skin: { enabled: Boolean(draft?.skin?.enabled), brand: Boolean(document.getElementById(BRAND_ID)), home: Boolean(document.getElementById(HOME_ID)) },
    }).catch(() => {});
  }

  async function hydrateAssets(theme, options = {}) {
    const epoch = ++assetHydrationEpoch;
    const lifecycle = lifecycleEpoch;
    const paths = themeAssetPaths(theme);
    const sameTheme = assetCacheThemeId === theme.id;
    const urls = new Map(sameTheme ? [...assetUrls].filter(([path, url]) => paths.has(path) && isPreviewDataUrl(url)) : []);
    const priority = visibleAssetPriority(theme);
    const pending = [...paths]
      .filter(path => !stagedUrls.has(path) && !urls.has(path))
      .sort((left, right) => {
        const leftIndex = priority.indexOf(left), rightIndex = priority.indexOf(right);
        return (leftIndex < 0 ? Number.MAX_SAFE_INTEGER : leftIndex) - (rightIndex < 0 ? Number.MAX_SAFE_INTEGER : rightIndex);
      });
    const loaded = await mapConcurrent(pending, 3, async path => {
      try {
        const result = await call("theme.background.data", { id: theme.id, path });
        return isPreviewDataUrl(result.url) ? [path, result.url] : [path, ""];
      } catch {
        return [path, ""];
      }
    });
    if (destroyed || lifecycle !== lifecycleEpoch || epoch !== assetHydrationEpoch) return null;
    const failed = [];
    loaded.forEach(([path, url]) => { if (url) urls.set(path, url); else failed.push(path); });
    assetCacheThemeId = theme.id;
    assetUrls = urls;
    assetRetryPaths = new Set(failed);
    if (failed.length) scheduleAssetRetry(theme, options.retryAttempt || 0); else clearAssetRetry();
    return failed.length === 0;
  }

  function patchShadowRoots(css) {
    document.querySelectorAll("*").forEach(node => {
      const root = node.shadowRoot;
      if (!root) return;
      let style = root.getElementById?.(STYLE_ID);
      if (!style) { style = document.createElement("style"); style.id = STYLE_ID; root.appendChild(style); }
      style.textContent = css;
    });
  }

  function patchDiffRoots(theme) {
    document.querySelectorAll("diffs-container").forEach(host => {
      const root = host.shadowRoot;
      if (!root) return;
      let style = root.getElementById?.(DIFF_STYLE_ID);
      if (!style) { style = document.createElement("style"); style.id = DIFF_STYLE_ID; root.appendChild(style); }
      style.textContent = diffShadowCss(theme);
    });
  }

  function patchTerminal(theme) {
    const palette = terminalPalette(theme);
    const background = normalizeColor(theme.terminal?.background || theme.tokens.codeBackground);
    const foreground = safeForeground(background, theme.terminal?.foreground || theme.tokens.codeForeground, 7);
    const cursor = safeForeground(background, theme.terminal?.cursor || theme.tokens.accent, 3);
    const selection = normalizeColor(theme.terminal?.selection || theme.tokens.selection);
    document.querySelectorAll('[id^="terminal-panel-"]').forEach(panel => {
      panel.style.setProperty("background", background, "important");
      panel.style.setProperty("color", foreground, "important");
      panel.style.setProperty("color-scheme", theme.baseMode, "important");
      panel.style.setProperty("--color-token-terminal-background", background);
      panel.style.setProperty("--color-token-terminal-foreground", foreground);
      panel.style.setProperty("--vscode-terminal-background", background);
      panel.style.setProperty("--vscode-terminal-foreground", foreground);
      panel.style.setProperty("--vscode-terminalCursor-foreground", cursor);
      panel.style.setProperty("--vscode-terminal-selectionBackground", selection);
      palette.forEach((color, index) => {
        panel.style.setProperty(`--vscode-terminal-ansi${TERMINAL_ANSI_NAMES[index]}`, color);
        panel.style.setProperty(`--color-token-terminal-ansi-${TERMINAL_TOKEN_NAMES[index]}`, color);
      });
    });
    document.querySelectorAll(".xterm").forEach(terminal => {
      terminal.style.setProperty("--vscode-terminal-background", background);
      terminal.style.setProperty("--vscode-terminal-foreground", foreground);
      terminal.style.setProperty("--vscode-terminalCursor-foreground", cursor);
      terminal.style.setProperty("--vscode-terminal-selectionBackground", selection);
      palette.forEach((color, index) => {
        terminal.style.setProperty(`--ti-ansi-${index}`, color);
        terminal.style.setProperty(`--vscode-terminal-ansi${TERMINAL_ANSI_NAMES[index]}`, color);
      });
    });
  }

  function installAttachShadowHook() {
    if (Element.prototype.attachShadow.__themeInjectPatched) return;
    const original = Element.prototype.attachShadow;
    function patched(init) {
      const root = original.call(this, init);
      const lifecycle = lifecycleEpoch;
      queueMicrotask(() => {
        if (destroyed || lifecycle !== lifecycleEpoch || !draft) return;
        patchShadowRoots(themeCss(draft, state?.customCssTrusted ? draftCustomCss : ""));
        patchDiffRoots(draft);
      });
      return root;
    }
    patched.__themeInjectPatched = true;
    patched.__themeInjectOriginal = original;
    Element.prototype.attachShadow = patched;
  }

  function ensurePanelStyles() {
    if (destroyed) return;
    let style = document.getElementById(PANEL_STYLE_ID);
    if (!style) { style = document.createElement("style"); style.id = PANEL_STYLE_ID; document.documentElement.appendChild(style); }
    if (style.textContent !== PANEL_CSS) style.textContent = PANEL_CSS;
  }

  function panelHtml() {
    return `<div class="ti-header"><div><h2 class="ti-title">Theme Inject</h2><div class="ti-subtitle">实时定制 Codex 视觉系统</div></div><div class="ti-header-actions"><button class="ti-icon-button" data-action="reinject">重新注入</button><button class="ti-icon-button" data-action="close">关闭</button></div></div><div class="ti-tabs">${tabs.map(([id, label]) => `<button class="ti-tab" data-tab="${id}" data-active="${id === activeTab}">${label}</button>`).join("")}</div><div class="ti-body">${tabs.map(([id]) => `<section class="ti-section" data-section="${id}" data-active="${id === activeTab}">${sectionHtml(id)}</section>`).join("")}</div><div class="ti-footer"><div class="ti-status" data-kind="${escapeHtml(statusKind)}">${escapeHtml(statusMessage || (dirty ? "有未应用修改" : "已同步"))}</div><div class="ti-footer-actions"><button class="ti-button" data-action="cancel">放弃</button><button class="ti-button" data-primary="true" data-action="apply">应用</button></div></div>${modalHtml()}`;
  }

  function modalHtml() {
    if (!modal) return "";
    if (modal.kind === "create") return `<div class="ti-modal-backdrop"><form class="ti-modal" data-modal-form="create"><div class="ti-modal-title">新建主题</div><div class="ti-modal-copy">以“${escapeHtml(draft.name)}”为基础创建一个可编辑副本。</div><label class="ti-field-label" for="ti-theme-name">主题名称</label><input id="ti-theme-name" class="ti-control" data-modal-name maxlength="80" value="${escapeHtml(modal.defaultValue)}" autocomplete="off"><div class="ti-modal-actions"><button class="ti-button" type="button" data-modal-cancel>取消</button><button class="ti-button" data-primary="true" type="submit">创建并切换</button></div></form></div>`;
    if (modal.kind === "delete") return `<div class="ti-modal-backdrop"><div class="ti-modal"><div class="ti-modal-title">删除主题</div><div class="ti-modal-copy">确定删除“${escapeHtml(draft.name)}”？此操作无法撤销。</div><div class="ti-modal-actions"><button class="ti-button" type="button" data-modal-cancel>取消</button><button class="ti-button" data-danger="true" type="button" data-modal-confirm="delete">删除</button></div></div></div>`;
    return `<div class="ti-modal-backdrop"><div class="ti-modal"><div class="ti-modal-title">未应用的修改</div><div class="ti-modal-copy">关闭前要应用当前主题修改吗？</div><div class="ti-modal-actions ti-modal-actions-split"><button class="ti-button" type="button" data-modal-cancel>继续编辑</button><button class="ti-button" type="button" data-modal-confirm="discard">放弃并关闭</button><button class="ti-button" data-primary="true" type="button" data-modal-confirm="apply">应用并关闭</button></div></div></div>`;
  }

  function syncNativeSurfaceContrast() {
    document.querySelectorAll('[data-theme-inject-light-surface="true"]').forEach(node => node.removeAttribute("data-theme-inject-light-surface"));
    document.querySelectorAll(".main-surface section,.main-surface form,.main-surface [role=group],.main-surface div").forEach(node => {
      const style = getComputedStyle(node);
      const color = style.backgroundColor.match(/[\d.]+/g)?.slice(0, 3).map(Number);
      if (!color) return;
      const luminance = (color[0] * .2126 + color[1] * .7152 + color[2] * .0722) / 255;
      const bounds = node.getBoundingClientRect();
      if (luminance > .72 && bounds.width > 280 && bounds.height > 44 && style.backgroundColor !== "rgba(0, 0, 0, 0)") node.dataset.themeInjectLightSurface = "true";
    });
  }
  function generationPlansComplete(result) {
    const slots = new Set((result?.assets || []).map(asset => asset.slot));
    return (result?.plans || []).every(plan => slots.has(plan.slot));
  }

  function syncNativeButtonContrast() {
    document.querySelectorAll('[data-theme-inject-button-contrast="true"]').forEach(node => node.removeAttribute("data-theme-inject-button-contrast"));
    document.querySelectorAll("button,[role=button]").forEach(node => {
      if (node.closest(`#${PANEL_ID},.ti-studio-backdrop`) || node.matches('[data-theme-inject-custom-icon]')) return;
      const style = getComputedStyle(node);
      const channels = style.backgroundColor.match(/[\d.]+/g)?.slice(0, 4).map(Number);
      if (!channels || (channels[3] ?? 1) < .72) return;
      const background = `#${channels.slice(0, 3).map(value => Math.round(value).toString(16).padStart(2, "0")).join("")}`;
      const foreground = colorContrast(background, "#000000") >= colorContrast(background, "#FFFFFF") ? "#000000" : "#FFFFFF";
      node.dataset.themeInjectButtonContrast = "true";
      node.style.setProperty("--ti-native-button-fg", foreground);
    });
  }

  function scheduleDraftPreview() {
    const now = Date.now();
    if (!draftPreviewDeadline) draftPreviewDeadline = now + 240;
    clearTimeout(draftPreviewTimer);
    const delay = Math.max(0, Math.min(80, draftPreviewDeadline - now));
    draftPreviewTimer = setTimeout(() => {
      draftPreviewTimer = 0;
      draftPreviewDeadline = 0;
      if (!destroyed && draft) applyTheme(draft, state?.customCssTrusted ? draftCustomCss : "");
    }, delay);
  }

  function scheduleStudioPreview(studio) {
    const now = Date.now();
    if (!studioPreviewDeadline) studioPreviewDeadline = now + 700;
    clearTimeout(studioPreviewTimer);
    const delay = Math.max(0, Math.min(180, studioPreviewDeadline - now));
    studioPreviewTimer = setTimeout(() => {
      studioPreviewTimer = 0;
      studioPreviewDeadline = 0;
      if (!destroyed && studioOpen && studio?.isConnected) {
        applyTheme(draft, "");
        refreshStudioDom(studio, true);
        renderStudioDecorations(studio);
      }
    }, delay);
  }

  function studioHtml() {
    const progressItems = aiGenerationProgress.items || [];
    const completedCount = progressItems.filter(item => item.status === "completed").length;
    const failedCount = progressItems.filter(item => item.status === "failed").length;
    const activeItems = progressItems.filter(item => item.status === "generating");
    const totalCount = progressItems.length;
    const progressPercent = totalCount ? Math.round(completedCount / totalCount * 100) : aiGenerationProgress.state === "planning" ? 8 : 0;
    const phase = generationPhase(aiGenerationProgress.state, aiGenerationProgress.message, activeItems, completedCount, totalCount, failedCount);
    const latestActivity = aiRequestLog.at(-1);
    const plans = aiResourcePlans.map((plan, index) => `<div class="ti-resource-item" data-status="configured"><div class="ti-resource-preview"></div><div class="ti-resource-info"><strong>${escapeHtml(resourceSlotLabel(plan.slot))}</strong><span title="${escapeHtml(plan.prompt)}">${escapeHtml(plan.prompt)}</span></div><button class="ti-icon-button" data-studio-remove-plan="${index}">删除</button></div>`).join("");
    const progress = progressItems.map(item => { const previewUrl = thumbnailUrls.get(item.path) || item.previewUrl || ""; return `<div class="ti-resource-item" data-status="${escapeHtml(item.status)}"><div class="ti-resource-preview" data-empty="${!previewUrl}" style="${previewUrl ? `background-image:url(&quot;${escapeHtml(previewUrl)}&quot;)` : ""}"></div><div class="ti-resource-info"><strong>${escapeHtml(item.label || resourceSlotLabel(item.slot))}</strong><span title="${escapeHtml(item.message || item.prompt)}">${escapeHtml(item.message || item.prompt)}</span></div><div class="ti-resource-state">${escapeHtml(resourceStatusLabel(item.status))}</div></div>`; }).join("");
    const reference = aiReferences.length ? `<div class="ti-reference-grid">${aiReferences.map((item, index) => `<div class="ti-ai-reference"><div class="ti-ai-reference-image" style="background-image:url(&quot;${escapeHtml(item.previewUrl)}&quot;)"></div><div><strong>${escapeHtml(item.path?.split("/").pop() || `参考图 ${index + 1}`)}</strong><span>参考图 ${index + 1}</span><button class="ti-button" type="button" data-studio-reference-remove="${index}">移除</button></div></div>`).join("")}</div>` : `<div class="ti-studio-empty">可选：添加参考图分析配色、氛围和层级。也可以 Ctrl+V 直接粘贴图片。</div>`;
    const logs = aiRequestLog.length ? aiRequestLog.slice().reverse().map(item => `<div class="ti-ai-log-item" data-outcome="${escapeHtml(item.outcome)}"><div><strong>${escapeHtml(item.stage)}</strong><span>${escapeHtml(item.model)} · ${item.httpStatus || "网络错误"} · ${item.durationMs} ms</span></div><code>${escapeHtml(item.endpoint)}</code>${item.message && item.message !== "请求成功" ? `<p>${escapeHtml(item.message)}</p>` : ""}</div>`).join("") : `<div class="ti-studio-empty">开始生成后，这里会实时显示蓝图和图片请求。</div>`;
    const decorations = draft?.skin?.decorations || [];
    const selected = decorations[studioSelectedDecoration];
    const decorationEditor = selected ? `<div class="ti-studio-form"><strong>装饰 ${studioSelectedDecoration + 1}</strong>${row("区域", `<select class="ti-control" data-studio-decoration-path="region"><option value="sidebar" ${selected.region === "sidebar" ? "selected" : ""}>侧边栏</option><option value="content" ${selected.region === "content" || selected.region === "titlebar" ? "selected" : ""}>内容区</option><option value="composer" ${selected.region === "composer" ? "selected" : ""}>输入区</option></select>`)}${row("锚点", `<select class="ti-control" data-studio-decoration-path="anchor"><option value="top-left" ${selected.anchor === "top-left" ? "selected" : ""}>左上</option><option value="top-right" ${selected.anchor === "top-right" ? "selected" : ""}>右上</option><option value="bottom-left" ${selected.anchor === "bottom-left" ? "selected" : ""}>左下</option><option value="bottom-right" ${selected.anchor === "bottom-right" ? "selected" : ""}>右下</option></select>`)}${row("水平偏移", `<input class="ti-control" type="number" min="-2000" max="2000" data-studio-decoration-path="offsetX" value="${selected.offsetX}">`)}${row("垂直偏移", `<input class="ti-control" type="number" min="-2000" max="2000" data-studio-decoration-path="offsetY" value="${selected.offsetY}">`)}${row("宽度", `<input class="ti-control" type="number" min="16" max="1600" data-studio-decoration-path="width" value="${selected.width}">`)}${row("透明度", `<input class="ti-control" type="range" min="0" max="1" step=".01" data-studio-decoration-path="opacity" value="${selected.opacity}">`)}</div>` : `<div class="ti-studio-empty">在预览中点击装饰，或从下方列表选择。</div>`;
    return `<div class="ti-studio-backdrop"><div class="ti-studio-header"><div><div class="ti-studio-title">AI 主题生成工作台</div><div class="ti-studio-copy">需求、参考图、资源、日志与静态预览都在这里完成</div></div><div class="ti-inline"><button class="ti-button" data-studio-refresh>刷新 DOM 副本</button><button class="ti-button" data-studio-close ${busy ? "disabled title=\"生成完成后可关闭\"" : ""}>关闭</button></div></div><div class="ti-studio-main"><div class="ti-studio-canvas"><div class="ti-studio-frame-wrap"><iframe class="ti-studio-frame" sandbox="" title="Codex 静态 DOM 预览"></iframe><div class="ti-studio-decorations"></div></div></div><aside class="ti-studio-sidebar"><div class="ti-studio-tabs"><button class="ti-studio-tab" data-studio-tab="generate" data-active="${studioTab === "generate"}">生成</button><button class="ti-studio-tab" data-studio-tab="resources" data-active="${studioTab === "resources"}">资源</button><button class="ti-studio-tab" data-studio-tab="logs" data-active="${studioTab === "logs"}">日志</button><button class="ti-studio-tab" data-studio-tab="theme" data-active="${studioTab === "theme"}">主题</button><button class="ti-studio-tab" data-studio-tab="decorations" data-active="${studioTab === "decorations"}">装饰</button></div><div class="ti-studio-pane"><section data-studio-section="generate" data-active="${studioTab === "generate"}"><div class="ti-studio-form"><strong>主题需求</strong><textarea class="ti-control ti-ai-prompt" data-studio-prompt maxlength="6000" placeholder="描述主题风格、颜色、氛围、组件和使用场景" ${busy ? "disabled" : ""}>${escapeHtml(aiPrompt)}</textarea><div class="ti-section-title">参考图</div>${reference}<button class="ti-button" type="button" data-studio-reference-select ${busy ? "disabled" : ""}>选择参考图</button><label class="ti-ai-checkbox"><input type="checkbox" data-studio-generate-images ${aiGenerateImages ? "checked" : ""} ${busy ? "disabled" : ""}><span>生成背景、Hero、装饰和动作/卡片图标</span></label></div><div class="ti-generation-overview" data-state="${escapeHtml(aiGenerationProgress.state || "idle")}"><div class="ti-generation-phase"><strong>${escapeHtml(phase.title)}</strong><span>${escapeHtml(phase.detail)}</span>${latestActivity ? `<span class="ti-generation-activity">最近活动：${escapeHtml(latestActivity.stage)} · ${escapeHtml(latestActivity.outcome)}${latestActivity.httpStatus ? ` · HTTP ${latestActivity.httpStatus}` : ""}</span>` : ""}</div><div class="ti-progress-track"><span style="width:${progressPercent}%"></span></div><div class="ti-generation-counts"><span>${completedCount}/${totalCount || "—"} 已完成</span>${failedCount ? `<span>${failedCount} 失败</span>` : ""}</div></div>${activeItems.length ? `<div class="ti-section-title ti-section-gap">正在处理</div><div class="ti-resource-list">${activeItems.map(item => `<div class="ti-resource-item" data-status="generating"><div class="ti-resource-preview"></div><div class="ti-resource-info"><strong>${escapeHtml(item.label)}</strong><span>${escapeHtml(item.prompt)}</span></div><div class="ti-resource-state">生成中</div></div>`).join("")}</div>` : ""}</section><section data-studio-section="resources" data-active="${studioTab === "resources"}"><form class="ti-studio-form" data-studio-plan-form><strong>添加资源配置</strong><select class="ti-control" data-studio-plan-slot>${resourceSlotOptions.map(([value, label]) => `<option value="${value}">${label}</option>`).join("")}</select><textarea class="ti-control" data-studio-plan-prompt maxlength="2000" placeholder="描述这个资源的构图、颜色、留白和用途"></textarea><button class="ti-button" data-primary="true" type="submit">添加到生成计划</button></form><div class="ti-section-title">自定义计划</div><div class="ti-resource-list">${plans || `<div class="ti-studio-empty">尚未添加自定义资源，AI 仍会按蓝图生成默认资源。</div>`}</div><div class="ti-section-title ti-section-gap">全部资源 · ${completedCount}/${totalCount || "—"}</div><div class="ti-resource-list">${progress || `<div class="ti-studio-empty">开始生成后逐项显示状态。</div>`}</div></section><section data-studio-section="logs" data-active="${studioTab === "logs"}"><div class="ti-section-title">实时请求日志 · ${aiRequestLog.length}</div><div class="ti-ai-log">${logs}</div></section><section data-studio-section="theme" data-active="${studioTab === "theme"}"><div class="ti-studio-form"><strong>快速主题配置</strong>${row("内容背景", colorInput("tokens.contentBackground", draft.tokens.contentBackground))}${row("主要文字", colorInput("tokens.foreground", draft.tokens.foreground))}${row("强调色", colorInput("tokens.accent", draft.tokens.accent))}${row("圆角", rangeInput("shape.radius", draft.shape.radius, 0, 40, 1))}${row("侧栏宽度", numberInput("layout.sidebarWidth", draft.layout.sidebarWidth, 180, 520))}</div></section><section data-studio-section="decorations" data-active="${studioTab === "decorations"}">${decorationEditor}<div class="ti-resource-list">${decorations.map((item, index) => `<button class="ti-resource-item" data-studio-select-decoration="${index}" data-status="${index === studioSelectedDecoration ? "completed" : "configured"}"><span class="ti-resource-preview" style="background-image:url(&quot;${escapeHtml(resolvedAsset(draft, item.asset))}&quot;)"></span><span class="ti-resource-info"><strong>装饰 ${index + 1}</strong><span>${escapeHtml(item.region)} · ${item.offsetX}, ${item.offsetY}</span></span></button>`).join("") || `<div class="ti-studio-empty">先生成或添加装饰层。</div>`}</div></section></div><div class="ti-studio-footer"><div><strong>${escapeHtml(phase.title)}</strong><span class="ti-studio-copy">${escapeHtml(phase.footer)}</span></div><button class="ti-button" data-primary="true" data-studio-generate ${busy ? "disabled" : ""}>${busy ? `${progressPercent}% 生成中` : "开始生成"}</button></div></aside></div></div>`;
  }

  function resourceSlotLabel(slot) { return resourceSlotOptions.find(([value]) => value === slot)?.[1] || slot; }
  function resourceStatusLabel(status) { return ({ queued:"等待", generating:"生成中", completed:"已完成", failed:"失败" })[status] || status; }
  function generationPhase(state, message, active, completed, total, failed) {
    if (state === "planning") return { title:"正在分析主题需求", detail:message || "基础/视觉模型正在生成主题蓝图，资源清单将在蓝图完成后出现。", footer:"主题蓝图分析中" };
    if (state === "generating") return { title:active.length ? `正在生成：${active.map(item => item.label).join("、")}` : "正在调度图片资源", detail:message ? `${message}；已完成 ${completed}/${total}` : `已完成 ${completed}/${total}，图片并发 2；失败资源会显示原因并保留已完成结果。`, footer:`图片资源 ${completed}/${total}` };
    if (state === "completed") return { title:"主题生成完成", detail:total ? `${completed}/${total} 个图片资源已完成，可继续调整主题和装饰。` : (message || "主题蓝图已生成，可继续调整主题。"), footer:"可以继续预览和调整" };
    if (state === "failed") return { title:"生成未完整完成", detail:`已完成 ${completed}/${total}，失败 ${failed}；查看资源和日志了解原因。`, footer:"查看日志后可再次生成续跑" };
    return { title:"等待开始生成", detail:"填写主题需求，可选参考图和自定义资源，然后开始生成。", footer:"预览无实际功能，修改只进入草稿" };
  }

  function sectionHtml(id) {
    if (!draft) return `<div class="ti-empty">正在读取主题…</div>`;
    if (id === "library") return libraryHtml();
    if (id === "colors") return colorsHtml();
    if (id === "background") return backgroundHtml();
    if (id === "type") return `<div class="ti-card">${row("界面字体", textInput("typography.uiFont", draft.typography.uiFont))}${row("代码字体", textInput("typography.monoFont", draft.typography.monoFont))}${row("字号比例", rangeInput("typography.scale", draft.typography.scale, .8, 1.4, .01))}${row("行高", rangeInput("typography.lineHeight", draft.typography.lineHeight, 1.1, 2, .05))}</div>`;
    if (id === "layout") return `<div class="ti-card">${row("密度", rangeInput("layout.density", draft.layout.density, .75, 1.35, .01))}${row("侧边栏宽度", numberInput("layout.sidebarWidth", draft.layout.sidebarWidth, 180, 520))}${row("内容最大宽度", numberInput("layout.contentMaxWidth", draft.layout.contentMaxWidth, 480, 2400))}</div>`;
    if (id === "effects") return `<div class="ti-card">${row("圆角", rangeInput("shape.radius", draft.shape.radius, 0, 40, 1))}${row("边框宽度", rangeInput("shape.borderWidth", draft.shape.borderWidth, 0, 4, 1))}${row("浮层透明度", rangeInput("effects.panelOpacity", draft.effects.panelOpacity, .25, 1, .01))}${row("全局模糊", rangeInput("effects.backgroundBlur", draft.effects.backgroundBlur, 0, 80, 1))}${row("全局饱和度", rangeInput("effects.saturation", draft.effects.saturation, .5, 2, .05))}${row("阴影强度", rangeInput("effects.shadowStrength", draft.effects.shadowStrength, 0, 1, .05))}</div>`;
    if (id === "terminal") return terminalHtml();
    if (id === "skin") return skinHtml();
    if (id === "ai") return aiHtml();
    if (id === "advanced") return `<div class="ti-notice">自定义 CSS 可以隐藏或伪造页面内容。只有你完全信任的本地主题才应启用。</div><label class="ti-inline" style="margin:12px 0"><input type="checkbox" data-custom-css-enabled ${state.customCssTrusted ? "checked" : ""}>启用并信任自定义 CSS</label><textarea class="ti-textarea" data-custom-css spellcheck="false" placeholder="/* 自定义 CSS */">${escapeHtml(draftCustomCss)}</textarea>`;
    return "";
  }

  function libraryHtml() {
    const actions = `<div class="ti-inline" style="margin-bottom:10px;flex-wrap:wrap"><button class="ti-button" data-action="create">新建主题</button><button class="ti-button" data-action="import">安装 ZIP</button><button class="ti-button" data-action="open-ai">AI 生成主题</button></div>`;
    return `<div class="ti-notice" style="margin-bottom:10px">皮肤是主题包的一部分。导出 ZIP 会包含颜色、背景、品牌、组件表面、图标、欢迎页以及主题目录中的本地图片资源；API Key 不会导出。</div>${actions}<div class="ti-grid">${state.themes.map(theme => { const mode = theme.skin?.enabled ? "高级皮肤" : theme.background?.mode === "per-region" ? "分区背景" : activeBackgrounds(theme).some(([, background]) => hasBackground(background)) ? "全屏背景" : ""; return `<div class="ti-card ti-theme-card" data-theme-id="${escapeHtml(theme.id)}" data-selected="${theme.id === draft.id}"><div class="ti-swatch" style="background:linear-gradient(135deg,${theme.tokens.sidebarBackground},${theme.tokens.accent})"></div><div><div class="ti-name">${escapeHtml(theme.name)}${mode ? `<span class="ti-badge">${mode}</span>` : ""}</div><div class="ti-description">${escapeHtml(theme.description || theme.id)}</div></div><button class="ti-icon-button" data-export="${escapeHtml(theme.id)}">导出</button></div>`; }).join("")}</div><div class="ti-inline" style="margin-top:10px"><button class="ti-button" data-action="clone">复制当前</button><button class="ti-button" data-action="delete">删除当前</button></div>`;
  }

  function colorsHtml() {
    const warning = colorContrast(draft.tokens.border, draft.tokens.inputBackground) < 1.35 ? `<div class="ti-notice ti-notice-danger">边框与输入区颜色过于接近，运行时会自动提高边框对比度。</div>` : "";
    return `${warning}<div class="ti-section-title">基础颜色</div><div class="ti-card">${colorFields.map(([key, label]) => row(label, colorInput(`tokens.${key}`, draft.tokens[key]))).join("")}</div><div class="ti-section-title ti-section-gap">标题栏与侧边栏</div><div class="ti-card">${chromeFields.map(([key, label]) => row(label, optionalColorInput(`chrome.${key}`, draft.chrome?.[key]))).join("")}</div>`;
  }

  function backgroundHtml() {
    const mode = draft.background?.mode || "fullscreen";
    const selector = row("配置模式", `<select class="ti-control" data-background-mode><option value="fullscreen" ${mode === "fullscreen" ? "selected" : ""}>全屏统一</option><option value="per-region" ${mode === "per-region" ? "selected" : ""}>分区独立</option></select>`);
    const body = mode === "per-region" ? backgroundCard("content", "内容区域") + backgroundCard("sidebar", "侧边栏区域") : backgroundCard("fullscreen", "全屏背景");
    return `<div class="ti-card">${selector}</div>${body}`;
  }

  function backgroundCard(key, label) {
    const background = draft.background?.[key] || defaultBackground("none", key);
    const kinds = [["none", "无"], ["solid", "纯色"], ["linear-gradient", "线性渐变"], ["radial-gradient", "径向渐变"], ["image", "本地图片"]];
    return `<details class="ti-card ti-config-card" open><summary><span>${label}</span><span class="ti-badge">${kinds.find(([value]) => value === background.kind)?.[1] || "无"}</span></summary>${row("类型", `<select class="ti-control" data-bg-kind="${key}">${kinds.map(([value, text]) => `<option value="${value}" ${background.kind === value ? "selected" : ""}>${text}</option>`).join("")}</select>`)}${backgroundFields(key, background)}</details>`;
  }

  function backgroundFields(key, background) {
    const prefix = `background.${key}`;
    let source = "";
    if (background.kind === "solid") source = row("颜色", colorInput(`${prefix}.color`, background.color));
    if (background.kind === "linear-gradient") source = row("起点", colorInput(`${prefix}.from`, background.from)) + row("终点", colorInput(`${prefix}.to`, background.to)) + row("角度", rangeInput(`${prefix}.angle`, background.angle ?? 135, 0, 360, 1));
    if (background.kind === "radial-gradient") source = row("中心", colorInput(`${prefix}.from`, background.from)) + row("边缘", colorInput(`${prefix}.to`, background.to));
    if (background.kind === "image") {
      const url = resolvedAsset(draft, background.path), staged = stagingAssets.some(asset => asset.slot === `background.${key}`);
      source = `<div class="ti-background-preview" data-loaded="${Boolean(url)}"><div class="ti-background-thumb" style="background-image:url(&quot;${escapeHtml(url)}&quot;)"></div><div class="ti-background-meta"><strong>${escapeHtml(background.path?.split("/").pop() || "尚未选择图片")}</strong><span>${staged ? "仅预览，点击应用后保存" : "已保存到当前主题"}</span><span>${url ? "图片数据已加载" : "图片数据未加载"}</span></div></div>` + row("图片", `<div class="ti-inline"><button class="ti-button" data-bg-image="${key}">选择图片</button><button class="ti-button" data-bg-clear="${key}">清除</button></div>`) + row("适应", `<select class="ti-control" data-path="${prefix}.fit"><option value="cover" ${background.fit === "cover" ? "selected" : ""}>cover</option><option value="contain" ${background.fit === "contain" ? "selected" : ""}>contain</option><option value="fill" ${background.fit === "fill" ? "selected" : ""}>fill</option><option value="none" ${background.fit === "none" ? "selected" : ""}>none</option></select>`) + row("水平焦点", rangeInput(`${prefix}.positionX`, background.positionX ?? 50, 0, 100, 1)) + row("垂直焦点", rangeInput(`${prefix}.positionY`, background.positionY ?? 50, 0, 100, 1));
    }
    if (background.kind === "none") return `<div class="ti-empty ti-empty-compact">此区域使用主题基础表面</div>`;
    return source + row("图片透明度", rangeInput(`${prefix}.opacity`, background.opacity ?? 1, 0, 1, .01)) + row("区域遮罩", rangeInput(`${prefix}.surfaceOpacity`, background.surfaceOpacity ?? .72, 0, 1, .01)) + row("背景模糊", rangeInput(`${prefix}.blur`, background.blur ?? 0, 0, 80, 1)) + row("背景饱和度", rangeInput(`${prefix}.saturation`, background.saturation ?? 1, .5, 2, .05));
  }

  function terminalHtml() {
    return `<div class="ti-card">${[["background", "背景"], ["foreground", "文字"], ["cursor", "光标"], ["selection", "选择"]].map(([key, label]) => row(label, colorInput(`terminal.${key}`, draft.terminal[key]))).join("")}<div class="ti-section-title" style="margin-top:12px">ANSI 颜色</div><div class="ti-inline" style="flex-wrap:wrap">${draft.terminal.ansi.map((value, index) => `<input class="ti-color-swatch" type="color" title="ANSI ${index}" data-ansi="${index}" value="${normalizeColor(value)}">`).join("")}</div></div>`;
  }

  function skinHtml() {
    const skin = draft.skin;
    const enabled = Boolean(skin.enabled);
    return `<div class="ti-notice">高级皮肤可添加品牌、插画、组件表面和空白首页。关闭时不会改变 Codex 原有结构。</div><div class="ti-card ti-section-gap">${row("高级皮肤", `<label class="ti-switch"><input type="checkbox" data-path="skin.enabled" ${enabled ? "checked" : ""}><span>${enabled ? "已启用" : "已关闭"}</span></label>`)}</div>${enabled ? skinDetailsHtml() : ""}`;
  }

  function skinDetailsHtml() {
    const skin = draft.skin;
    return `<details class="ti-card ti-config-card" open><summary>品牌信息</summary>${row("品牌名称", textInput("skin.brand.title", skin.brand.title || ""))}${row("副标题", textInput("skin.brand.subtitle", skin.brand.subtitle || ""))}${row("品牌字体", textInput("skin.brand.font", skin.brand.font || draft.typography.uiFont))}</details><details class="ti-card ti-config-card" open><summary>图片资源</summary><div class="ti-assets">${skinResourceFields.map(([key, label, purpose]) => assetField(key, label, purpose)).join("")}</div></details>${surfaceEditorHtml()}${iconEditorHtml()}${decorationEditorHtml()}${homeEditorHtml()}`;
  }

  function assetField(key, label, purpose) {
    const path = draft.skin.resources[key], url = resolvedAsset(draft, path);
    return `<div class="ti-asset"><div class="ti-asset-preview" style="background-image:url(&quot;${escapeHtml(url)}&quot;)"></div><div class="ti-asset-copy"><strong>${label}</strong><span>${escapeHtml(path?.split("/").pop() || "未设置")}</span><div class="ti-inline"><button class="ti-button" data-skin-asset="${key}" data-purpose="${purpose}">选择</button>${path ? `<button class="ti-button" data-clear-skin-asset="${key}">清除</button>` : ""}</div></div></div>`;
  }

  function surfaceEditorHtml() {
    const items = [["titlebar", "标题栏"], ["sidebar", "侧边栏"], ["content", "内容区"], ["hero", "Hero"], ["card", "快捷卡片"], ["composer", "输入区"], ["popover", "浮层"]];
    return `<details class="ti-card ti-config-card"><summary>组件表面</summary>${items.map(([key, label]) => { const prefix = `skin.surfaces.${key}`, surface = draft.skin.surfaces[key]; return `<div class="ti-subcard"><strong>${label}</strong>${row("填充", optionalColorInput(`${prefix}.fill`, surface.fill))}${row("透明度", rangeInput(`${prefix}.opacity`, surface.opacity, 0, 1, .01))}${row("模糊", rangeInput(`${prefix}.blur`, surface.blur, 0, 80, 1))}${row("边框颜色", optionalColorInput(`${prefix}.borderColor`, surface.borderColor))}${row("边框宽度", rangeInput(`${prefix}.borderWidth`, surface.borderWidth, 0, 4, 1))}${row("圆角", rangeInput(`${prefix}.radius`, surface.radius, 0, 48, 1))}${row("阴影", rangeInput(`${prefix}.shadowOpacity`, surface.shadowOpacity, 0, 1, .01))}</div>`; }).join("")}</details>`;
  }

  function iconEditorHtml() {
    const icons = draft.skin.icons;
    const mappings = icons.mode === "custom" ? `<div class="ti-section-title ti-section-gap">动作图标</div>${skinIconFields.map(([action, label]) => { const path = icons.mappings[action], url = resolvedAsset(draft, path); return row(label, `<div class="ti-inline">${url ? `<span class="ti-mini-asset" style="background-image:url(&quot;${escapeHtml(url)}&quot;)"></span>` : ""}<button class="ti-button" data-icon-asset="${action}">选择</button>${path ? `<button class="ti-button" data-clear-icon-asset="${action}">清除</button>` : ""}</div>`); }).join("")}` : "";
    return `<details class="ti-card ti-config-card"><summary>图标系统</summary>${row("模式", `<select class="ti-control" data-path="skin.icons.mode"><option value="native" ${icons.mode === "native" ? "selected" : ""}>沿用原生</option><option value="recolor" ${icons.mode === "recolor" ? "selected" : ""}>仅重着色</option><option value="custom" ${icons.mode === "custom" ? "selected" : ""}>自定义图标包</option></select>`)}${row("默认颜色", optionalColorInput("skin.icons.color", icons.color))}${row("激活颜色", optionalColorInput("skin.icons.activeColor", icons.activeColor))}<div class="ti-notice">有图标映射时替换对应原生图标；没有映射或资源加载失败时自动回退原生图标。</div>${mappings}</details>`;
  }

  function decorationEditorHtml() {
    const decorations = draft.skin.decorations || [];
    const regionOptions = [["sidebar", "侧边栏"], ["content", "内容区"], ["composer", "输入区"]];
    const anchorOptions = [["top-left", "左上"], ["top-right", "右上"], ["bottom-left", "左下"], ["bottom-right", "右下"]];
    return `<details class="ti-card ti-config-card"><summary>装饰层 <span class="ti-badge">${decorations.length}/16</span></summary><div class="ti-notice">装饰层不接收鼠标事件，不会遮挡 Codex 操作。</div>${decorations.map((item, index) => { const url = resolvedAsset(draft, item.asset); return `<div class="ti-subcard"><div class="ti-inline ti-between"><strong>装饰 ${index + 1}</strong><button class="ti-icon-button" data-remove-decoration="${index}">删除</button></div>${row("图片", `<div class="ti-inline">${url ? `<span class="ti-mini-asset" style="background-image:url(&quot;${escapeHtml(url)}&quot;)"></span>` : ""}<button class="ti-button" data-decoration-asset="${index}">选择</button></div>`)}${row("区域", `<select class="ti-control" data-path="skin.decorations.${index}.region">${regionOptions.map(([value, label]) => `<option value="${value}" ${item.region === value ? "selected" : ""}>${label}</option>`).join("")}</select>`)}${row("锚点", `<select class="ti-control" data-path="skin.decorations.${index}.anchor">${anchorOptions.map(([value, label]) => `<option value="${value}" ${item.anchor === value ? "selected" : ""}>${label}</option>`).join("")}</select>`)}${row("水平偏移", numberInput(`skin.decorations.${index}.offsetX`, item.offsetX, -2000, 2000))}${row("垂直偏移", numberInput(`skin.decorations.${index}.offsetY`, item.offsetY, -2000, 2000))}${row("宽度", numberInput(`skin.decorations.${index}.width`, item.width, 16, 1600))}${row("透明度", rangeInput(`skin.decorations.${index}.opacity`, item.opacity, 0, 1, .01))}</div>`; }).join("")}<button class="ti-button" data-add-decoration ${decorations.length >= 16 ? "disabled" : ""}>添加装饰层</button></details>`;
  }

  function homeEditorHtml() {
    const home = draft.skin.home;
    return `<details class="ti-card ti-config-card" open><summary>空白首页</summary>${row("启用", `<label class="ti-switch"><input type="checkbox" data-path="skin.home.enabled" ${home.enabled ? "checked" : ""}><span>${home.enabled ? "已启用" : "已关闭"}</span></label>`)}${row("标题", textInput("skin.home.title", home.title))}${row("副标题", textInput("skin.home.subtitle", home.subtitle))}${row("Hero 高度", numberInput("skin.home.heroHeight", home.heroHeight, 160, 720))}${row("卡片列数", numberInput("skin.home.cardColumns", home.cardColumns, 1, 4))}<div class="ti-section-title ti-section-gap">快捷卡片</div>${home.cards.map((card, index) => { const iconUrl = resolvedAsset(draft, card.icon); return `<div class="ti-subcard"><div class="ti-inline ti-between"><strong>卡片 ${index + 1}</strong><button class="ti-icon-button" data-remove-home-card="${index}">删除</button></div>${row("标题", textInput(`skin.home.cards.${index}.title`, card.title))}${row("说明", textInput(`skin.home.cards.${index}.description`, card.description))}${row("Prompt", `<textarea class="ti-control ti-small-textarea" data-path="skin.home.cards.${index}.prompt">${escapeHtml(card.prompt)}</textarea>`)}${row("图标", `<div class="ti-inline">${iconUrl ? `<span class="ti-mini-asset" style="background-image:url(&quot;${escapeHtml(iconUrl)}&quot;)"></span>` : ""}<button class="ti-button" data-home-card-icon="${index}">选择</button>${card.icon ? `<button class="ti-button" data-clear-home-card-icon="${index}">清除</button>` : ""}</div>`)}</div>`; }).join("")}<button class="ti-button" data-add-home-card ${home.cards.length >= 4 ? "disabled" : ""}>添加快捷卡片</button></details>`;
  }

  function aiHtml() {
    const settings = aiSettings || state.ai || { baseUrl: "https://api.openai.com/v1", imageBaseUrl: "", textModel: "gpt-4.1-mini", visionModel: "gpt-4.1-mini", imageModel: "gpt-image-1", imageSize: "3:2", hasApiKey: false, hasImageApiKey: false };
    const baseKeyState = settings.hasApiKey ? "基础 Key 已使用 Windows 当前用户加密保存" : "基础 Key 尚未保存";
    const imageKeyState = settings.hasImageApiKey ? "生图 Key 已使用 Windows 当前用户加密保存" : "生图 Key 尚未保存";
    return `<div class="ti-notice">主题需求、参考图、资源计划、生成进度和日志已统一放入生成工作台。此处只管理连接设置。</div><details class="ti-card ti-config-card" open><summary>连接设置 <span class="ti-badge">${settings.hasApiKey ? "基础已配置" : "基础未配置"}</span></summary>${row("基础 API 地址", `<input class="ti-control" type="url" data-ai-setting="baseUrl" value="${escapeHtml(settings.baseUrl)}" placeholder="http://192.168.1.16:8080/v1 或 https://api.openai.com/v1">`)}${row("基础 Key", `<input class="ti-control" type="password" data-ai-api-key value="${escapeHtml(aiApiKeyDraft)}" placeholder="${settings.hasApiKey ? "留空则继续使用已保存的基础 Key" : "输入基础/视觉模型使用的 Key"}" autocomplete="new-password">`)}<div class="ti-secret-status">${baseKeyState}。基础地址允许 HTTP/HTTPS。</div>${row("基础模型", `<input class="ti-control" data-ai-setting="textModel" value="${escapeHtml(settings.textModel)}" placeholder="gpt-4.1-mini">`)}${row("视觉模型", `<input class="ti-control" data-ai-setting="visionModel" value="${escapeHtml(settings.visionModel)}" placeholder="支持图片输入的模型名称">`)}${row("生图 API 地址", `<input class="ti-control" type="url" data-ai-setting="imageBaseUrl" value="${escapeHtml(settings.imageBaseUrl || "")}" placeholder="http://192.168.1.16:8080 或留空">`)}${row("生图 Key", `<input class="ti-control" type="password" data-ai-image-api-key value="${escapeHtml(aiImageApiKeyDraft)}" placeholder="${settings.hasImageApiKey ? "留空则继续使用已保存的生图 Key" : "输入生图模型单独使用的 Key"}" autocomplete="new-password">`)}<div class="ti-secret-status">${imageKeyState}。生图地址允许 HTTP/HTTPS，留空时自动使用基础地址；密钥不会进入日志或主题 ZIP。</div>${row("生图模型", `<input class="ti-control" data-ai-setting="imageModel" value="${escapeHtml(settings.imageModel)}" placeholder="gpt-image-1">`)}${row("图片比例", `<input class="ti-control" data-ai-setting="imageSize" value="${escapeHtml(settings.imageSize)}" placeholder="3:2 或 16:9">`)}<div class="ti-inline ti-section-gap ti-modal-actions-split"><button class="ti-button" type="button" data-ai-settings-save>保存连接设置</button>${settings.hasApiKey ? `<button class="ti-button" type="button" data-ai-key-clear>清除基础 Key</button>` : ""}${settings.hasImageApiKey ? `<button class="ti-button" type="button" data-ai-image-key-clear>清除生图 Key</button>` : ""}</div></details><button class="ti-button ti-ai-generate" data-primary="true" type="button" data-ai-workbench>打开 AI 生成工作台</button><div class="ti-description">在工作台内填写需求、添加参考图、查看每项资源与实时日志。</div>`;
  }

  function row(label, control) { return `<div class="ti-row"><div class="ti-label">${label}</div><div>${control}</div></div>`; }
  function textInput(path, value) { return `<input class="ti-control" type="text" data-path="${path}" value="${escapeHtml(value)}">`; }
  function numberInput(path, value, min, max) { return `<input class="ti-control" type="number" data-path="${path}" value="${value}" min="${min}" max="${max}">`; }
  function rangeValue(value, step) { const precision = (String(step).split(".")[1] || "").length; const normalized = Math.round(Number(value) / Number(step)) * Number(step); return Number(normalized.toFixed(precision)).toString(); }
  function rangeInput(path, value, min, max, step) { return `<div class="ti-inline"><input class="ti-control" type="range" data-path="${path}" value="${value}" min="${min}" max="${max}" step="${step}"><output>${rangeValue(value, step)}</output></div>`; }
  function colorInput(path, value) { const color = normalizeColor(value); return `<div class="ti-color-control"><input class="ti-color-swatch" type="color" data-color-path="${path}" value="${color}"><input class="ti-color-hex" type="text" data-color-text="${path}" value="${color.toUpperCase()}" maxlength="7" spellcheck="false"></div>`; }
  function optionalColorInput(path, value) { const color = normalizeColor(value || "#000000"); return `<div class="ti-color-control ti-color-optional"><input class="ti-color-swatch" type="color" data-color-path="${path}" value="${color}"><input class="ti-color-hex" type="text" data-color-text="${path}" value="${value ? color.toUpperCase() : ""}" maxlength="7" placeholder="继承" spellcheck="false"><button class="ti-color-reset" type="button" data-clear-path="${path}" title="恢复继承">×</button></div>`; }

  function renderPanel() {
    if (destroyed) return;
    ensurePanelStyles();
    let panel = document.getElementById(PANEL_ID);
    const previousBody = panel?.querySelector(".ti-body");
    if (previousBody) panelScrollTop = previousBody.scrollTop;
    if (!panel) { panel = document.createElement("aside"); panel.id = PANEL_ID; document.body.appendChild(panel); }
    panel.innerHTML = panelHtml();
    panel.dataset.open = "true";
    panel.dataset.busy = String(busy);
    panel.setAttribute("aria-busy", String(busy));
    bindPanel(panel);
    const body = panel.querySelector(".ti-body");
    if (body) {
      body.scrollTop = panelScrollTop;
      body.addEventListener("scroll", () => { panelScrollTop = body.scrollTop; }, { passive: true });
    }
    if (modal?.kind === "create") queueMicrotask(() => panel.querySelector("[data-modal-name]")?.select());
  }

  function bindPanel(panel) {
    panel.querySelectorAll("[data-tab]").forEach(button => button.onclick = () => { activeTab = button.dataset.tab; renderPanel(); });
    panel.querySelector('[data-action="close"]')?.addEventListener("click", closePanel);
    panel.querySelector('[data-action="reinject"]')?.addEventListener("click", () => void runAction("正在重新注入…", () => call("runtime.reinject"), "重新注入请求已发送"));
    panel.querySelector('[data-action="cancel"]')?.addEventListener("click", cancelPreview);
    panel.querySelector('[data-action="apply"]')?.addEventListener("click", () => void runAction("正在应用主题…", applyDraft, "主题已应用"));
    panel.querySelector('[data-action="import"]')?.addEventListener("click", () => void runAction("等待选择主题包…", async () => { await call("theme.package.import"); await reloadState(); }, "主题包已安装"));
    panel.querySelector('[data-action="create"]')?.addEventListener("click", () => { modal = { kind: "create", defaultValue: `${draft.name} 自定义` }; renderPanel(); });
    panel.querySelector('[data-action="open-ai"]')?.addEventListener("click", () => { activeTab = "ai"; renderPanel(); });
    panel.querySelector('[data-action="clone"]')?.addEventListener("click", () => void runAction("正在复制主题…", async () => { draft = await call("theme.package.clone", { id: draft.id }); dirty = true; await hydrateAssets(draft); applyTheme(draft, draftCustomCss); await reloadThemes(); }, "副本已创建，点击应用以保存"));
    panel.querySelector('[data-action="delete"]')?.addEventListener("click", () => { if (draft.id.startsWith("builtin.")) { setStatus("内置主题不能删除", "error"); return; } modal = { kind: "delete" }; renderPanel(); });
    panel.querySelectorAll("[data-theme-id]").forEach(card => card.onclick = event => { if (event.target.closest("[data-export]")) return; void runAction("正在切换主题…", async () => { await cancelStaging(); state = await call("theme.activate", { id: card.dataset.themeId }); draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); draftCustomCss = state.customCss || ""; dirty = false; await hydrateAssets(draft); applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""); }, "主题已切换并保存"); });
    panel.querySelectorAll("[data-export]").forEach(button => button.onclick = event => { event.stopPropagation(); void runAction("等待选择导出位置…", () => call("theme.package.export", { id: button.dataset.export }), "主题已导出"); });
    panel.querySelector("[data-modal-cancel]")?.addEventListener("click", () => { modal = null; renderPanel(); });
    panel.querySelector('[data-modal-form="create"]')?.addEventListener("submit", event => { event.preventDefault(); const name = panel.querySelector("[data-modal-name]")?.value.trim(); if (!name) { setStatus("请输入主题名称", "error"); return; } void runAction("正在创建主题…", async () => { const theme = await call("theme.package.create", { sourceId: draft.id, name }); state = await call("theme.activate", { id: theme.id }); draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); draftCustomCss = state.customCss || ""; dirty = false; modal = null; await hydrateAssets(draft); applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""); }, "主题已创建并切换"); });
    panel.querySelector('[data-modal-confirm="delete"]')?.addEventListener("click", () => void runAction("正在删除主题…", async () => { state = await call("theme.package.delete", { id: draft.id }); draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); draftCustomCss = state.customCss || ""; dirty = false; modal = null; await hydrateAssets(draft); applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""); }, "主题已删除，已切换到默认主题"));
    panel.querySelector('[data-modal-confirm="discard"]')?.addEventListener("click", () => { modal = null; cancelPreview(); hidePanel(); });
    panel.querySelector('[data-modal-confirm="apply"]')?.addEventListener("click", () => void runAction("正在应用主题…", async () => { await applyDraft(); modal = null; hidePanel(); }, "主题已应用"));
    panel.querySelectorAll("[data-path]").forEach(input => input.addEventListener(input.tagName === "SELECT" || input.type === "checkbox" ? "change" : "input", () => { const value = input.type === "checkbox" ? input.checked : input.type === "number" || input.type === "range" ? Number(input.value) : input.value; setPath(draft, input.dataset.path, value); input.parentElement.querySelector("output")?.replaceChildren(input.type === "range" ? rangeValue(value, input.step) : input.value); markDirty(); if (input.type === "checkbox" || input.tagName === "SELECT") renderPanel(); }));
    panel.querySelectorAll("[data-color-path]").forEach(input => input.addEventListener("input", () => { const path = input.dataset.colorPath; setPath(draft, path, input.value.toUpperCase()); const text = panel.querySelector(`[data-color-text="${path}"]`); if (text) text.value = input.value.toUpperCase(); markDirty(); }));
    panel.querySelectorAll("[data-color-text]").forEach(input => input.addEventListener("change", () => { const value = input.value.trim(); const path = input.dataset.colorText; if (!value) { setPath(draft, path, null); markDirty(); return; } if (!/^#[0-9a-f]{6}$/i.test(value)) { input.value = normalizeColor(getPath(draft, path)).toUpperCase(); setStatus("颜色必须使用 #RRGGBB", "error"); return; } setPath(draft, path, value.toUpperCase()); const swatch = panel.querySelector(`[data-color-path="${path}"]`); if (swatch) swatch.value = value; markDirty(); }));
    panel.querySelectorAll("[data-clear-path]").forEach(button => button.addEventListener("click", () => { setPath(draft, button.dataset.clearPath, null); markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-ansi]").forEach(input => input.addEventListener("input", () => { draft.terminal.ansi[Number(input.dataset.ansi)] = input.value; markDirty(); }));
    panel.querySelector("[data-background-mode]")?.addEventListener("change", event => { draft.background.mode = event.target.value; markDirty(); renderPanel(); });
    panel.querySelectorAll("[data-bg-kind]").forEach(select => select.addEventListener("change", () => { const key = select.dataset.bgKind, kind = select.value; if (kind === "image") { void chooseBackground(key); return; } draft.background[key] = defaultBackground(kind, key); markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-bg-image]").forEach(button => button.addEventListener("click", () => void chooseBackground(button.dataset.bgImage)));
    panel.querySelectorAll("[data-bg-clear]").forEach(button => button.addEventListener("click", () => { draft.background[button.dataset.bgClear] = defaultBackground("none", button.dataset.bgClear); markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-skin-asset]").forEach(button => button.addEventListener("click", () => void chooseSkinAsset(button.dataset.skinAsset, button.dataset.purpose)));
    panel.querySelectorAll("[data-clear-skin-asset]").forEach(button => button.addEventListener("click", () => { draft.skin.resources[button.dataset.clearSkinAsset] = null; markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-icon-asset]").forEach(button => button.addEventListener("click", () => void chooseIconAsset(button.dataset.iconAsset)));
    panel.querySelectorAll("[data-clear-icon-asset]").forEach(button => button.addEventListener("click", () => { delete draft.skin.icons.mappings[button.dataset.clearIconAsset]; markDirty(); renderPanel(); }));
    panel.querySelector("[data-add-decoration]")?.addEventListener("click", () => { if (draft.skin.decorations.length < 16) draft.skin.decorations.push({ asset: "", region: "content", anchor: "bottom-right", offsetX: 24, offsetY: 24, width: 160, opacity: 1, layer: 0, hiddenBelow: 0 }); markDirty(); renderPanel(); });
    panel.querySelectorAll("[data-remove-decoration]").forEach(button => button.addEventListener("click", () => { draft.skin.decorations.splice(Number(button.dataset.removeDecoration), 1); markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-decoration-asset]").forEach(button => button.addEventListener("click", () => void chooseDecorationAsset(Number(button.dataset.decorationAsset))));
    panel.querySelector("[data-add-home-card]")?.addEventListener("click", () => { if (draft.skin.home.cards.length < 4) draft.skin.home.cards.push({ title: "新快捷卡片", description: "", prompt: "", icon: null }); markDirty(); renderPanel(); });
    panel.querySelectorAll("[data-remove-home-card]").forEach(button => button.addEventListener("click", () => { draft.skin.home.cards.splice(Number(button.dataset.removeHomeCard), 1); markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-home-card-icon]").forEach(button => button.addEventListener("click", () => void chooseHomeCardIcon(Number(button.dataset.homeCardIcon))));
    panel.querySelectorAll("[data-clear-home-card-icon]").forEach(button => button.addEventListener("click", () => { draft.skin.home.cards[Number(button.dataset.clearHomeCardIcon)].icon = null; markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-ai-setting]").forEach(input => input.addEventListener("input", () => { aiSettings ||= clone(state.ai); aiSettings[input.dataset.aiSetting] = input.value; }));
    panel.querySelector("[data-ai-api-key]")?.addEventListener("input", event => { aiApiKeyDraft = event.target.value; });
    panel.querySelector("[data-ai-image-api-key]")?.addEventListener("input", event => { aiImageApiKeyDraft = event.target.value; });
    panel.querySelector("[data-ai-settings-save]")?.addEventListener("click", () => void runAction("正在保存 AI 设置…", () => saveAiSettings(), "AI 连接设置已保存"));
    panel.querySelector("[data-ai-key-clear]")?.addEventListener("click", () => void runAction("正在清除基础 Key…", () => saveAiSettings(true), "已清除保存的基础 Key"));
    panel.querySelector("[data-ai-image-key-clear]")?.addEventListener("click", () => void runAction("正在清除生图 Key…", () => saveAiSettings(false, true), "已清除保存的生图 Key"));
    panel.querySelector("[data-ai-workbench]")?.addEventListener("click", openStudio);
    panel.querySelector("[data-custom-css]")?.addEventListener("input", event => { draftCustomCss = event.target.value; markDirty(); });
    panel.querySelector("[data-custom-css-enabled]")?.addEventListener("change", event => { state.customCssTrusted = event.target.checked; markDirty(); });
  }

  function defaultBackground(kind, key) {
    const surfaceOpacity = key === "sidebar" ? .46 : key === "content" ? .34 : .32;
    const common = { opacity: 1, blur: 0, saturation: 1, surfaceOpacity, positionX: 50, positionY: 50 };
    if (kind === "solid") return { kind, color: key === "sidebar" ? draft.tokens.sidebarBackground : key === "fullscreen" ? draft.tokens.appBackground : draft.tokens.contentBackground, ...common };
    if (kind === "linear-gradient") return { kind, angle: 135, from: draft.tokens.appBackground, to: draft.tokens.accent, ...common };
    if (kind === "radial-gradient") return { kind, from: draft.tokens.accent, to: draft.tokens.appBackground, ...common };
    if (kind === "image") return { kind, path: "", fit: "cover", ...common };
    return { kind: "none", ...common };
  }

  async function chooseBackground(key) {
    await runAction("等待选择背景图片…", async () => {
      const result = await call("theme.background.import", { id: draft.id, purpose: `background-${key}` });
      await replaceStagedAsset(`background.${key}`, result);
      draft.background[key] = { ...defaultBackground("image", key), path: result.path };
      markDirty();
    }, "背景图片已加载，当前为预览状态");
  }

  async function chooseSkinAsset(key, purpose) {
    await runAction("等待选择皮肤图片…", async () => {
      const result = await call("theme.asset.import", { id: draft.id, purpose });
      await replaceStagedAsset(`skin.resources.${key}`, result);
      draft.skin.resources[key] = result.path;
      markDirty();
    }, "皮肤图片已加载，当前为预览状态");
  }

  async function chooseIconAsset(action) {
    await runAction("等待选择图标图片…", async () => {
      const result = await call("theme.asset.import", { id: draft.id, purpose: `icon-${action}` });
      await replaceStagedAsset(`skin.icons.${action}`, result);
      draft.skin.icons.mappings[action] = result.path;
      markDirty();
    }, "图标图片已加载，当前为预览状态");
  }

  async function chooseHomeCardIcon(index) {
    await runAction("等待选择卡片图标…", async () => {
      const result = await call("theme.asset.import", { id: draft.id, purpose: `home-card-${index + 1}` });
      await replaceStagedAsset(`skin.home.cards.${index}.icon`, result);
      draft.skin.home.cards[index].icon = result.path;
      markDirty();
    }, "卡片图标已加载，当前为预览状态");
  }

  async function chooseDecorationAsset(index) {
    await runAction("等待选择装饰图片…", async () => {
      const result = await call("theme.asset.import", { id: draft.id, purpose: `decoration-${index + 1}` });
      await replaceStagedAsset(`skin.decorations.${index}.asset`, result);
      draft.skin.decorations[index].asset = result.path;
      markDirty();
    }, "装饰图片已加载，当前为预览状态");
  }

  async function saveAiSettings(clearApiKey = false, clearImageApiKey = false) {
    const settings = clone(aiSettings || state.ai);
    const result = await call("ai.settings.save", { settings, apiKey: clearApiKey ? "" : aiApiKeyDraft, imageApiKey: clearImageApiKey ? "" : aiImageApiKeyDraft, clearApiKey, clearImageApiKey });
    state.ai = clone(result);
    aiSettings = clone(result);
    aiApiKeyDraft = "";
    aiImageApiKeyDraft = "";
  }

  async function chooseAiReference() {
    await runAction("等待选择 AI 参考图…", async () => {
      const result = await call("ai.reference.import");
      await addAiReferences(result.references || [result]);
    }, "参考图已加载");
  }

  async function clearAiReference(render = true) {
    const previous = aiReferences;
    aiReferences = [];
    await cancelPreviewSessions(previous);
    if (render) renderPanel();
  }

  async function addAiReferences(items) {
    const existing = new Set(aiReferences.map(item => `${item.session}:${item.path}`));
    for (const item of items.filter(Boolean)) {
      if (!item.session || !item.path || existing.has(`${item.session}:${item.path}`)) continue;
      if (aiReferences.length >= 6) break;
      item.previewUrl = await previewDataUrl(item);
      aiReferences.push(item);
      existing.add(`${item.session}:${item.path}`);
    }
  }

  async function removeAiReference(index, render = true) {
    const [removed] = aiReferences.splice(index, 1);
    if (removed) await call("theme.preview.cancel", { session: removed.session }).catch(() => {});
    if (render) renderStudio();
  }

  function generationIsActive(epoch) { return !destroyed && epoch === generationEpoch && busy; }

  async function generateAiTheme(epoch) {
    await saveAiSettings();
    if (!generationIsActive(epoch)) return;
    if (!state.ai?.hasApiKey) throw new Error("请先填写并保存基础 Key");
    if (aiGenerateImages && !state.ai?.hasImageApiKey) throw new Error("启用生图时请先填写并保存生图 Key");
    if (!aiPrompt.trim() && !aiReferences.length) throw new Error("请填写主题描述或选择参考图");
    aiRequestLog = [];
    let result;
    try {
      result = await call("ai.generate", {
        prompt: aiPrompt,
        language: aiLanguage,
        generateImages: aiGenerateImages,
        generateSidebarWatermark: aiGenerateSidebarWatermark,
        generateSystemIcons: aiGenerateSystemIcons,
        imageConcurrency: aiImageConcurrency,
        references: aiReferences.map(({ session, path }) => ({ session, path })),
        resourcePlans: aiResourcePlans,
      });
    } catch (error) {
      const logs = await call("ai.log.get").catch(() => []);
      if (!generationIsActive(epoch)) return;
      aiRequestLog = logs;
      throw error;
    }
    if (!generationIsActive(epoch)) return;
    const logs = await call("ai.log.get").catch(() => []);
    if (!generationIsActive(epoch)) return;
    aiRequestLog = logs;
    const complete = await restoreCompleteGeneration(result, () => generationIsActive(epoch), 3);
    if (!complete || !generationIsActive(epoch)) throw new Error("生成资源尚未完整加载，请稍后重试");
    result = complete.result;
    const nextKeys = new Set((result.assets || []).map(asset => `${asset.session}:${asset.path}`));
    const previousAssets = stagingAssets.filter(asset => !nextKeys.has(`${asset.session}:${asset.path}`));
    await cancelPreviewSessions(previousAssets);
    if (!generationIsActive(epoch)) return;
    await clearAiReference(false);
    if (!generationIsActive(epoch)) return;
    if (!studioOpen) return;
    draft = clone(result.theme);
    draftCustomCss = "";
    state.customCssTrusted = false;
    stagingAssets = (result.assets || []).map(asset => ({ slot: asset.slot, session: asset.session, path: asset.path }));
    stagedUrls.clear();
    complete.urls.forEach((url, path) => stagedUrls.set(path, url));
    dirty = true;
    aiGeneratedDraft = true;
    applyTheme(draft, "");
  }

  async function regeneratePaletteOnly() {
    if (busy) return;
    generationEpoch += 1;
    clearTimeout(studioTimer);
    busy = true;
    aiGenerationProgress = { state: "planning", message: "正在重新生成配色，不会调整图标或图片资源", items: [] };
    aiRequestLog = [];
    renderStudio();
    try {
      await saveAiSettings();
      if (!state.ai?.hasApiKey) throw new Error("请先填写并保存基础 Key");
      const result = await call("ai.generate", {
        prompt: `${aiPrompt || draft.description || draft.name}\n\n只重新生成配色、对比度、圆角、透明度、背景风格和组件表面。不要生成或调整任何图标、Logo、水印、装饰图片、Hero 图片或卡片图标。保留当前主题的资源结构。`,
        language: aiLanguage,
        generateImages: false,
        imageConcurrency: 1,
        references: aiReferences.map(({ session, path }) => ({ session, path })),
        resourcePlans: [],
      });
      mergePaletteTheme(result.theme);
      aiGenerationProgress = await call("ai.progress.get").catch(() => ({ state: "completed", message: "配色已更新", items: [] }));
      aiRequestLog = await call("ai.log.get").catch(() => aiRequestLog);
      markStudioDirty();
      applyTheme(draft, "");
      renderStudio(true);
    } catch (error) {
      aiGenerationProgress = { state: "failed", message: error?.message || "配色生成失败", items: [] };
      aiRequestLog = await call("ai.log.get").catch(() => aiRequestLog);
      setStatus(error?.message || "配色生成失败", "error");
      renderStudio();
    } finally {
      busy = false;
      renderStudio(true);
    }
  }

  function mergePaletteTheme(next) {
    const preservedSkin = clone(draft.skin || {});
    const preservedBackground = clone(draft.background || {});
    draft.tokens = clone(next.tokens);
    draft.chrome = clone(next.chrome);
    draft.terminal = clone(next.terminal);
    draft.shape = clone(next.shape);
    draft.effects = { ...clone(next.effects), backgroundBlur: Math.min(next.effects?.backgroundBlur || 0, 4) };
    draft.typography = clone(next.typography);
    draft.layout = { ...draft.layout, density: next.layout?.density ?? draft.layout.density, contentMaxWidth: next.layout?.contentMaxWidth ?? draft.layout.contentMaxWidth };
    draft.background = backgroundHasImage(preservedBackground) ? preservedBackground : clone(next.background || preservedBackground);
    draft.skin = clone(next.skin || preservedSkin);
    draft.skin.resources = preservedSkin.resources || {};
    draft.skin.icons = preservedSkin.icons || { mode: "recolor", mappings: {} };
    draft.skin.decorations = preservedSkin.decorations || [];
    draft.skin.home = preservedSkin.home || draft.skin.home;
    if (draft.background?.fullscreen?.kind === "image") draft.background.fullscreen.blur = Math.min(draft.background.fullscreen.blur || 0, 2);
    if (draft.background?.content?.kind === "image") draft.background.content.blur = Math.min(draft.background.content.blur || 0, 2);
    if (draft.background?.sidebar?.kind === "image") draft.background.sidebar.blur = Math.min(draft.background.sidebar.blur || 0, 2);
  }

  function backgroundHasImage(background) {
    return [background?.fullscreen, background?.content, background?.sidebar].some(item => item?.kind === "image" && item.path);
  }

  async function syncProgressAssets(progress, epoch) {
    let changed = false;
    for (const item of progress?.items || []) {
      if (!generationIsActive(epoch)) return false;
      if (item.status !== "completed" || !item.path || !item.session) continue;
      if (item.slot === "skin.icons.atlas") {
        continue;
      }
      const current = stagingAssets.find(asset => asset.slot === item.slot && asset.session === item.session && asset.path === item.path);
      if (current && thumbnailUrls.has(item.path)) { item.previewUrl = thumbnailUrls.get(item.path); continue; }
      const previewUrl = await previewDataUrl(item);
      if (!generationIsActive(epoch)) return false;
      item.previewUrl = previewUrl;
      if (previewUrl) thumbnailUrls.set(item.path, previewUrl);
      else thumbnailUrls.delete(item.path);
      changed = true;
    }
    return changed;
  }

  async function restoreGeneratedTheme() {
    if (busy) return;
    const operationEpoch = ++generationEpoch;
    const lifecycle = lifecycleEpoch;
    clearTimeout(studioTimer);
    busy = true;
    renderStudio();
    try {
      const restored = await restoreCompleteGeneration(null, () => !destroyed && lifecycle === lifecycleEpoch && operationEpoch === generationEpoch, 4);
      if (!restored) throw new Error("生成检查点中的图片资源尚未完整加载，请稍后重试");
      const { result, urls } = restored;
      if (destroyed || lifecycle !== lifecycleEpoch || operationEpoch !== generationEpoch) return;
      const previousAssets = stagingAssets;
      const nextKeys = new Set((result.assets || []).map(asset => `${asset.session}:${asset.path}`));
      await cancelPreviewSessions(previousAssets.filter(asset => !nextKeys.has(`${asset.session}:${asset.path}`)));
      if (destroyed || lifecycle !== lifecycleEpoch || operationEpoch !== generationEpoch) return;
      draft = clone(result.theme);
      draftCustomCss = "";
      state.customCssTrusted = false;
      stagingAssets = (result.assets || []).map(asset => ({ slot: asset.slot, session: asset.session, path: asset.path }));
      stagedUrls.clear();
      urls.forEach((url, path) => stagedUrls.set(path, url));
      aiResourcePlans = (result.plans || []).filter(plan => resourceSlotOptions.some(([slot]) => slot === plan.slot));
      const restoredAssets = new Map((result.assets || []).map(asset => [asset.slot, asset]));
      aiGenerationProgress = {
        state: "completed",
        message: result.summary || "已恢复上次生成结果",
        items: (result.plans || []).map(plan => ({ slot: plan.slot, label: resourceSlotLabel(plan.slot), prompt: plan.prompt, status: restoredAssets.has(plan.slot) ? "completed" : "queued", previewUrl: restoredAssets.get(plan.slot)?.previewUrl || "", session: restoredAssets.get(plan.slot)?.session || "", path: restoredAssets.get(plan.slot)?.path || "", message: "" })),
      };
      aiGeneratedDraft = generationPlansComplete(result);
      dirty = true;
      statusMessage = result.summary || "已恢复上次生成结果";
      statusKind = "success";
      await hydrateAssets(draft);
      if (destroyed || lifecycle !== lifecycleEpoch || operationEpoch !== generationEpoch) return;
      applyTheme(draft, "");
    } catch (error) {
      if (destroyed || lifecycle !== lifecycleEpoch || operationEpoch !== generationEpoch) return;
      setStatus(error?.message || "恢复失败", "error");
    } finally {
      if (!destroyed && lifecycle === lifecycleEpoch && operationEpoch === generationEpoch) {
        busy = false;
        renderStudio(true);
      }
    }
  }

  async function saveGeneratedTheme(name) {
    if (busy) return;
    generationEpoch += 1;
    clearTimeout(studioTimer);
    busy = true;
    renderStudio();
    try {
      draft.skin.decorations = (draft.skin.decorations || []).filter(item => item.asset);
      const referenced = collectAssetPaths(draft);
      const assets = stagingAssets.filter(asset => referenced.has(asset.path));
      const result = await call("ai.generation.save", { name, theme: draft, stagingAssets: assets.map(({ session, path }) => ({ session, path })) });
      state = result.state;
      draft = clone(result.theme);
      snapshot = clone(result.theme);
      stagingAssets = [];
      stagedUrls.clear();
      aiGeneratedDraft = false;
      studioSaveOpen = false;
      studioBaseline = null;
      dirty = false;
      statusMessage = `已保存为新主题“${result.theme.name}”`;
      statusKind = "success";
      await hydrateAssets(draft);
      applyTheme(draft, "");
    } catch (error) {
      setStatus(error?.message || "保存失败", "error");
    } finally {
      busy = false;
      renderStudio(true);
    }
  }

  function openStudio() {
    if (destroyed) return;
    if (!studioOpen) {
      studioBaseline = {
        draft: clone(draft),
        customCss: draftCustomCss,
        stagingAssets: clone(stagingAssets),
        dirty,
        customCssTrusted: Boolean(state.customCssTrusted),
      };
    }
    studioOpen = true;
    const panel = document.getElementById(PANEL_ID);
    if (panel) panel.dataset.open = "false";
    renderStudio(true);
    if (busy) void pollGenerationProgress(generationEpoch);
  }

  function closeStudio() {
    studioOpen = false;
    clearTimeout(studioTimer);
    document.querySelector(".ti-studio-backdrop")?.remove();
    if (studioBaseline) {
      draft = clone(studioBaseline.draft);
      draftCustomCss = studioBaseline.customCss;
      stagingAssets = clone(studioBaseline.stagingAssets);
      dirty = studioBaseline.dirty;
      state.customCssTrusted = studioBaseline.customCssTrusted;
      studioBaseline = null;
      aiGeneratedDraft = false;
      studioSaveOpen = false;
      stagedUrls.clear();
      void hydrateAssets(draft).then(() => applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""));
    }
    const panel = document.getElementById(PANEL_ID);
    if (panel) panel.dataset.open = "true";
    renderPanel();
  }

  function renderStudio(refreshDom = false) {
    if (destroyed || !studioOpen || !draft) return;
    const previous = document.querySelector(".ti-studio-backdrop");
    const previousFrame = !refreshDom ? previous?.querySelector(".ti-studio-frame") : null;
    const previousPane = previous?.querySelector(".ti-studio-pane");
    if (previousPane) studioPaneScrollTop = previousPane.scrollTop;
    const root = document.createElement("div");
    root.innerHTML = studioHtml();
    const studio = root.firstElementChild;
    const closeButton = studio.querySelector("[data-studio-close]");
    if (closeButton) { closeButton.disabled = false; closeButton.removeAttribute("title"); closeButton.textContent = busy ? "关闭（后台继续）" : "关闭"; }
    const generateButton = studio.querySelector("[data-studio-generate]");
    const failedCount = (aiGenerationProgress.items || []).filter(item => item.status === "failed").length;
    if (generateButton && !busy && failedCount) generateButton.textContent = `继续生成 ${failedCount} 个缺失素材`;
    const prompt = studio.querySelector("[data-studio-prompt]");
    if (prompt && !studio.querySelector("[data-studio-language]")) prompt.insertAdjacentHTML("beforebegin", `<label class="ti-field-label">界面语言</label><select class="ti-control" data-studio-language ${busy ? "disabled" : ""}>${themeLanguageOptions.map(([value, label]) => `<option value="${value}" ${aiLanguage === value ? "selected" : ""}>${label}</option>`).join("")}</select>`);
    const generateImagesCopy = studio.querySelector("[data-studio-generate-images]")?.nextElementSibling;
    if (generateImagesCopy) generateImagesCopy.textContent = "生成背景、Hero、装饰和卡片图标";
    const generateImagesLabel = studio.querySelector("[data-studio-generate-images]")?.closest("label");
    if (generateImagesLabel && !studio.querySelector("[data-studio-image-concurrency]")) generateImagesLabel.insertAdjacentHTML("afterend", `<label class="ti-ai-checkbox"><input type="checkbox" data-studio-generate-watermark ${aiGenerateSidebarWatermark ? "checked" : ""} ${busy || !aiGenerateImages ? "disabled" : ""}><span>生成侧栏水印（默认关闭，避免遮挡导航）</span></label><label class="ti-ai-checkbox"><input type="checkbox" data-studio-generate-system-icons ${aiGenerateSystemIcons ? "checked" : ""} ${busy || !aiGenerateImages ? "disabled" : ""}><span>生成同主题系统图标（3×3 图集自动切割）</span></label><div class="ti-row ti-compact-row"><div class="ti-label">图片并发</div><div><select class="ti-control" data-studio-image-concurrency ${busy ? "disabled" : ""}>${[1,2,3,4].map(value => `<option value="${value}" ${aiImageConcurrency === value ? "selected" : ""}>${value}</option>`).join("")}</select></div></div><button class="ti-button" type="button" data-studio-regenerate-palette ${busy ? "disabled" : ""}>只重新生成配色</button>`);
    const headerActions = studio.querySelector(".ti-studio-header .ti-inline");
    if (headerActions) headerActions.insertAdjacentHTML("afterbegin", aiGeneratedDraft ? `<button class="ti-button" data-primary="true" data-studio-save>保存为新主题</button>` : `<button class="ti-button" data-studio-restore>恢复上次生成</button>`);
    if (studioSaveOpen) studio.insertAdjacentHTML("beforeend", `<div class="ti-studio-modal-backdrop"><form class="ti-modal" data-studio-save-form><div class="ti-modal-title">保存为新主题</div><div class="ti-modal-copy">生成结果会复制到主题库并立即启用；保存成功后才会清理恢复检查点。</div><label class="ti-field-label" for="ti-generated-theme-name">主题名称</label><input id="ti-generated-theme-name" class="ti-control" data-studio-save-name maxlength="80" value="${escapeHtml(draft.name)}" autocomplete="off"><div class="ti-modal-actions"><button class="ti-button" type="button" data-studio-save-cancel>取消</button><button class="ti-button" data-primary="true" type="submit">保存并启用</button></div></form></div>`);
    if (previousFrame) studio.querySelector(".ti-studio-frame")?.replaceWith(previousFrame);
    previous?.replaceWith(studio) || document.body.appendChild(studio);
    bindStudio(studio);
    const pane = studio.querySelector(".ti-studio-pane");
    if (pane) {
      pane.scrollTop = studioPaneScrollTop;
      pane.addEventListener("scroll", () => { studioPaneScrollTop = pane.scrollTop; }, { passive: true });
    }
    refreshStudioDom(studio, refreshDom);
    renderStudioDecorations(studio);
  }

  function staticDomSnapshot() {
    const copy = document.documentElement.cloneNode(true);
    copy.querySelectorAll("script,.ti-studio-backdrop,#theme-inject-panel,#theme-inject-trigger,#theme-inject-decorations").forEach(node => node.remove());
    copy.querySelector(`#${STYLE_ID}`)?.remove();
    const previewStyle = document.createElement("style");
    previewStyle.id = STYLE_ID;
    previewStyle.textContent = themeCss(draft, state?.customCssTrusted ? draftCustomCss : "");
    copy.querySelector("head")?.appendChild(previewStyle);
    copy.querySelectorAll("[contenteditable],input,textarea,select,button,a").forEach(node => { node.removeAttribute("contenteditable"); node.removeAttribute("href"); node.setAttribute("tabindex", "-1"); });
    copy.querySelectorAll("iframe").forEach(node => node.replaceWith(document.createElement("div")));
    const base = copy.querySelector("head")?.appendChild(document.createElement("base"));
    if (base) base.href = location.href;
    return `<!doctype html>${copy.outerHTML}`;
  }

  function refreshStudioDom(studio, force) {
    const frame = studio.querySelector(".ti-studio-frame");
    if (!frame || (!force && frame.dataset.loaded === "true")) return;
    frame.srcdoc = staticDomSnapshot();
    frame.dataset.loaded = "true";
  }

  function renderStudioDecorations(studio) {
    const host = studio.querySelector(".ti-studio-decorations");
    if (!host) return;
    host.replaceChildren();
    (draft.skin.decorations || []).forEach((decoration, index) => {
      const url = resolvedAsset(draft, decoration.asset);
      if (!url) return;
      const image = document.createElement("img");
      image.className = "ti-studio-decoration";
      image.src = url;
      image.alt = "";
      image.dataset.decorationIndex = String(index);
      image.dataset.selected = String(index === studioSelectedDecoration);
      positionStudioDecoration(image, decoration, host);
      host.appendChild(image);
    });
  }

  function positionStudioDecoration(node, decoration, host) {
    const width = Math.min(decoration.width || 160, host.clientWidth || 1600);
    node.style.width = `${width}px`;
    node.style.opacity = String(decoration.opacity ?? 1);
    node.style.zIndex = String(decoration.layer ?? 0);
    const right = decoration.anchor?.endsWith("right");
    const bottom = decoration.anchor?.startsWith("bottom");
    node.style.left = right ? "auto" : `${decoration.offsetX || 0}px`;
    node.style.right = right ? `${decoration.offsetX || 0}px` : "auto";
    node.style.top = bottom ? "auto" : `${decoration.offsetY || 0}px`;
    node.style.bottom = bottom ? `${decoration.offsetY || 0}px` : "auto";
  }

  function bindStudio(studio) {
    studio.querySelector("[data-studio-close]")?.addEventListener("click", closeStudio);
    studio.querySelector("[data-studio-restore]")?.addEventListener("click", () => void restoreGeneratedTheme());
    studio.querySelector("[data-studio-save]")?.addEventListener("click", () => { studioSaveOpen = true; renderStudio(); queueMicrotask(() => document.querySelector("[data-studio-save-name]")?.select()); });
    studio.querySelector("[data-studio-save-cancel]")?.addEventListener("click", () => { studioSaveOpen = false; renderStudio(); });
    studio.querySelector("[data-studio-save-form]")?.addEventListener("submit", event => { event.preventDefault(); const name = studio.querySelector("[data-studio-save-name]")?.value.trim(); if (!name) return; void saveGeneratedTheme(name); });
    studio.querySelector("[data-studio-refresh]")?.addEventListener("click", () => renderStudio(true));
    studio.querySelectorAll("[data-studio-tab]").forEach(button => button.addEventListener("click", () => { studioTab = button.dataset.studioTab; renderStudio(); }));
    studio.querySelector("[data-studio-prompt]")?.addEventListener("input", event => { aiPrompt = event.target.value; });
    studio.querySelector("[data-studio-language]")?.addEventListener("change", event => { aiLanguage = event.target.value; });
    studio.querySelector("[data-studio-generate-images]")?.addEventListener("change", event => { aiGenerateImages = event.target.checked; renderStudio(); });
    studio.querySelector("[data-studio-generate-watermark]")?.addEventListener("change", event => { aiGenerateSidebarWatermark = event.target.checked; });
    studio.querySelector("[data-studio-generate-system-icons]")?.addEventListener("change", event => { aiGenerateSystemIcons = event.target.checked; });
    studio.querySelector("[data-studio-image-concurrency]")?.addEventListener("change", event => { aiImageConcurrency = Math.max(1, Math.min(4, Number(event.target.value) || 2)); });
    studio.querySelector("[data-studio-regenerate-palette]")?.addEventListener("click", () => void regeneratePaletteOnly());
    studio.querySelector("[data-studio-reference-select]")?.addEventListener("click", () => void chooseStudioReference());
    studio.querySelector("[data-studio-reference-clear]")?.addEventListener("click", () => void clearStudioReference());
    studio.querySelectorAll("[data-studio-reference-remove]").forEach(button => button.addEventListener("click", () => void removeAiReference(Number(button.dataset.studioReferenceRemove))));
    studio.addEventListener("paste", event => void pasteStudioReferences(event));
    studio.querySelector("[data-studio-plan-form]")?.addEventListener("submit", event => {
      event.preventDefault();
      const slot = studio.querySelector("[data-studio-plan-slot]")?.value;
      const prompt = studio.querySelector("[data-studio-plan-prompt]")?.value.trim();
      if (!slot || !prompt) return;
      const existing = aiResourcePlans.find(plan => plan.slot === slot);
      if (existing) existing.prompt = prompt; else aiResourcePlans.push({ slot, prompt });
      renderStudio();
    });
    studio.querySelectorAll("[data-studio-remove-plan]").forEach(button => button.addEventListener("click", () => { aiResourcePlans.splice(Number(button.dataset.studioRemovePlan), 1); renderStudio(); }));
    studio.querySelector("[data-studio-generate]")?.addEventListener("click", () => {
      if (busy) return;
      const epoch = ++generationEpoch;
      busy = true;
      aiGenerationProgress = { state: "planning", items: [] };
      aiRequestLog = [];
      aiGeneratedDraft = false;
      renderStudio();
      void pollGenerationProgress(epoch);
      void generateAiTheme(epoch).then(async () => {
        if (!generationIsActive(epoch)) return;
        const [progress, logs] = await Promise.all([call("ai.progress.get").catch(() => aiGenerationProgress), call("ai.log.get").catch(() => aiRequestLog)]);
        if (!generationIsActive(epoch)) return;
        busy = false;
        aiGenerationProgress = progress;
        aiRequestLog = logs;
        clearTimeout(studioTimer);
        renderStudio(true);
      }).catch(async error => {
        if (!generationIsActive(epoch)) return;
        const progress = await call("ai.progress.get").catch(() => ({ ...aiGenerationProgress, state:"failed", message:error?.message || "生成失败" }));
        if (!generationIsActive(epoch)) return;
        const logs = await call("ai.log.get").catch(() => aiRequestLog);
        if (!generationIsActive(epoch)) return;
        busy = false;
        aiGenerationProgress = { ...progress, state: "failed", message: progress.message || error?.message || "生成失败" };
        aiRequestLog = logs;
        clearTimeout(studioTimer);
        setStatus(error?.message || "生成失败", "error");
        renderStudio();
      });
    });
    studio.querySelectorAll("[data-color-path]").forEach(input => input.addEventListener("input", () => { setPath(draft, input.dataset.colorPath, input.value.toUpperCase()); markStudioDirty(); scheduleStudioPreview(studio); }));
    studio.querySelectorAll("[data-color-text]").forEach(input => input.addEventListener("change", () => { if (/^#[0-9a-f]{6}$/i.test(input.value.trim())) { setPath(draft, input.dataset.colorText, input.value.trim().toUpperCase()); markStudioDirty(); refreshStudioDom(studio, true); } }));
    studio.querySelectorAll("[data-path]").forEach(input => input.addEventListener("input", () => { setPath(draft, input.dataset.path, Number(input.value)); markStudioDirty(); scheduleStudioPreview(studio); }));
    studio.querySelectorAll("[data-studio-select-decoration]").forEach(button => button.addEventListener("click", () => { studioSelectedDecoration = Number(button.dataset.studioSelectDecoration); renderStudio(); }));
    studio.querySelectorAll("[data-studio-decoration-path]").forEach(input => input.addEventListener(input.tagName === "SELECT" ? "change" : "input", () => {
      const decoration = draft.skin.decorations[studioSelectedDecoration];
      if (!decoration) return;
      decoration[input.dataset.studioDecorationPath] = input.tagName === "SELECT" ? input.value : Number(input.value);
      markStudioDirty();
      renderStudioDecorations(studio);
      scheduleStudioPreview(studio);
    }));
    studio.querySelectorAll(".ti-studio-decoration").forEach(image => image.addEventListener("pointerdown", beginDecorationDrag));
  }

  function beginDecorationDrag(event) {
    event.preventDefault();
    activeDecorationDragCleanup?.();
    const node = event.currentTarget;
    const index = Number(node.dataset.decorationIndex);
    const decoration = draft.skin.decorations[index];
    const host = node.parentElement;
    if (!decoration || !host) return;
    studioSelectedDecoration = index;
    const startX = event.clientX, startY = event.clientY, originX = decoration.offsetX || 0, originY = decoration.offsetY || 0;
    const horizontalSign = decoration.anchor?.endsWith("right") ? -1 : 1;
    const verticalSign = decoration.anchor?.startsWith("bottom") ? -1 : 1;
    const move = moveEvent => {
      decoration.offsetX = Math.max(-2000, Math.min(2000, Math.round(originX + (moveEvent.clientX - startX) * horizontalSign)));
      decoration.offsetY = Math.max(-2000, Math.min(2000, Math.round(originY + (moveEvent.clientY - startY) * verticalSign)));
      positionStudioDecoration(node, decoration, host);
    };
    const cleanup = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      if (activeDecorationDragCleanup === cleanup) activeDecorationDragCleanup = null;
    };
    const end = () => {
      cleanup();
      if (destroyed) return;
      markStudioDirty();
      renderStudio();
    };
    activeDecorationDragCleanup = cleanup;
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end, { once:true });
  }

  async function pollGenerationProgress(epoch) {
    clearTimeout(studioTimer);
    if (!generationIsActive(epoch)) return;
    const [progress, logs] = await Promise.all([
      call("ai.progress.get").catch(() => aiGenerationProgress),
      call("ai.log.get").catch(() => aiRequestLog),
    ]);
    if (!generationIsActive(epoch)) return;
    aiGenerationProgress = progress;
    aiRequestLog = logs;
    const changed = await syncProgressAssets(progress, epoch);
    if (!generationIsActive(epoch)) return;
    const signature = JSON.stringify([progress.state, progress.message, (progress.items || []).map(item => [item.slot, item.status, item.path, item.message]), aiRequestLog.length]);
    if (studioOpen && (changed || signature !== studioProgressSignature)) {
      studioProgressSignature = signature;
      renderStudio(false);
    }
    if (generationIsActive(epoch) && studioOpen) studioTimer = setTimeout(() => void pollGenerationProgress(epoch), 500);
  }

  async function chooseStudioReference() {
    if (busy) return;
    try {
      const result = await call("ai.reference.import");
      await addAiReferences(result.references || [result]);
      renderStudio();
    } catch (error) {
      setStatus(error?.message || "参考图加载失败", "error");
      renderStudio();
    }
  }

  async function clearStudioReference() {
    await clearAiReference(false);
    renderStudio();
  }

  async function pasteStudioReferences(event) {
    if (busy) return;
    const files = [...(event.clipboardData?.files || [])].filter(file => file.type.startsWith("image/"));
    if (!files.length) return;
    event.preventDefault();
    try {
      const remaining = Math.max(0, 6 - aiReferences.length);
      const uploaded = [];
      for (const file of files.slice(0, remaining)) {
        uploaded.push(await uploadReferenceFile(file));
      }
      await addAiReferences(uploaded);
      setStatus(`已粘贴 ${uploaded.length} 张参考图`, "success");
      renderStudio();
    } catch (error) {
      setStatus(error?.message || "粘贴参考图失败", "error");
      renderStudio();
    }
  }

  function uploadReferenceFile(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onerror = () => reject(new Error("参考图读取失败"));
      reader.onload = async () => {
        try {
          resolve(await call("ai.reference.upload", { dataUrl: String(reader.result || "") }));
        } catch (error) {
          reject(error);
        }
      };
      reader.readAsDataURL(file);
    });
  }

  function markStudioDirty() {
    dirty = true;
    aiGeneratedDraft = true;
    statusMessage = "工作台中有未应用修改";
    statusKind = "";
  }

  async function replaceStagedAsset(slot, result) {
    const previous = stagingAssets.find(asset => asset.slot === slot);
    if (previous) { try { await call("theme.preview.cancel", { session: previous.session }); } catch {} stagedUrls.delete(previous.path); thumbnailUrls.delete(previous.path); }
    stagingAssets = stagingAssets.filter(asset => asset.slot !== slot);
    stagingAssets.push({ slot, session: result.stagingSession, path: result.path });
    stagedUrls.set(result.path, result.previewUrl || "");
  }

  function setStatus(message, kind = "") { if (destroyed) return; statusMessage = message; statusKind = kind; const node = document.querySelector(`#${PANEL_ID} .ti-status`); if (node) { node.dataset.kind = kind; node.replaceChildren(message); } }
  function setBusy(value) { busy = value; const panel = document.getElementById(PANEL_ID); if (!panel) return; panel.dataset.busy = String(value); panel.setAttribute("aria-busy", String(value)); panel.querySelectorAll("button,input,select,textarea").forEach(control => control.disabled = value); }
  async function runAction(message, action, success) { if (busy) return; setBusy(true); setStatus(message); try { await action(); statusMessage = success; statusKind = "success"; } catch (error) { statusMessage = error?.message || "操作失败"; statusKind = "error"; } finally { setBusy(false); const panel = document.getElementById(PANEL_ID); if (panel?.dataset.open === "true") renderPanel(); } }
  function markDirty() { dirty = true; statusMessage = ""; statusKind = ""; scheduleDraftPreview(); document.querySelector(".ti-status")?.replaceChildren("有未应用修改"); }
  async function applyDraft() { draft.skin.decorations = (draft.skin.decorations || []).filter(item => item.asset); const referenced = collectAssetPaths(draft); const unused = stagingAssets.filter(asset => !referenced.has(asset.path)); await cancelPreviewSessions(unused); stagingAssets = stagingAssets.filter(asset => referenced.has(asset.path)); const result = await call("theme.apply", { theme: draft, customCss: draftCustomCss, enableCustomCss: Boolean(state.customCssTrusted), stagingAssets: stagingAssets.map(({ session, path }) => ({ session, path })) }); stagingAssets = []; stagedUrls.clear(); await clearAiReference(false); state = result.state; draft = clone(result.theme); snapshot = clone(result.theme); draftCustomCss = state.customCss || ""; dirty = false; await hydrateAssets(draft); applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""); renderPanel(); }
  function collectAssetPaths(theme) { const paths = new Set(); [theme.background?.fullscreen, theme.background?.content, theme.background?.sidebar].forEach(background => { if (background?.kind === "image" && background.path) paths.add(background.path); }); Object.values(theme.skin?.resources || {}).forEach(path => typeof path === "string" && path && paths.add(path)); Object.values(theme.skin?.icons?.mappings || {}).forEach(path => path && paths.add(path)); (theme.skin?.decorations || []).forEach(item => item.asset && paths.add(item.asset)); (theme.skin?.home?.cards || []).forEach(item => item.icon && paths.add(item.icon)); return paths; }
  async function cancelStaging() { const pending = stagingAssets; const references = aiReferences; stagingAssets = []; aiReferences = []; stagedUrls.clear(); thumbnailUrls.clear(); await cancelPreviewSessions([...pending, ...references]); }
  function cancelPreview() { void cancelStaging().then(async () => { draft = clone(snapshot); draftCustomCss = state?.customCss || ""; dirty = false; if (snapshot) { await hydrateAssets(snapshot); applyTheme(snapshot, state?.customCssTrusted ? state.customCss || "" : ""); } renderPanel(); }); }
  function closePanel() { if (dirty) { modal = { kind: "close" }; renderPanel(); return; } void cancelStaging().then(hidePanel); }
  function hidePanel() { aiApiKeyDraft = ""; aiImageApiKeyDraft = ""; const panel = document.getElementById(PANEL_ID); if (panel) panel.dataset.open = "false"; }
  async function reloadThemes() { state.themes = await call("theme.package.list"); }
  async function reloadState() { const lifecycle = lifecycleEpoch; await cancelStaging(); if (destroyed || lifecycle !== lifecycleEpoch) return; state = await call("theme.state.get"); if (destroyed || lifecycle !== lifecycleEpoch) return; aiSettings = clone(state.ai); draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); draftCustomCss = state.customCss || ""; dirty = false; await hydrateAssets(draft); if (destroyed || lifecycle !== lifecycleEpoch) return; applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""); }
  async function openPanel() { if (destroyed) return; const lifecycle = lifecycleEpoch; if (!state) await reloadState(); if (destroyed || lifecycle !== lifecycleEpoch || !state) return; draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); draftCustomCss = state.customCss || ""; dirty = false; renderPanel(); }
  function ensureTrigger() { if (destroyed) return; ensurePanelStyles(); if (document.getElementById(TRIGGER_ID) || !document.body) return; const button = document.createElement("button"); button.id = TRIGGER_ID; button.type = "button"; button.textContent = "主题"; button.addEventListener("click", () => void openPanel()); document.body.appendChild(button); }
  function scheduleRefresh() { if (destroyed) return; clearTimeout(mutationTimer); if (draft) syncHome(draft); mutationTimer = setTimeout(() => { if (destroyed) return; ensureTrigger(); if (draft) { patchTitlebar(); mountBackgrounds(draft); syncSkin(draft); syncIcons(draft); syncNativeSurfaceContrast(); syncNativeButtonContrast(); patchDiffRoots(draft); patchTerminal(draft); } }, 60); }
  function destroy() { if (destroyed) return; destroyed = true; lifecycleEpoch += 1; generationEpoch += 1; assetHydrationEpoch += 1; busy = false; studioOpen = false; studioSaveOpen = false; modal = null; activeDecorationDragCleanup?.(); activeDecorationDragCleanup = null; layoutResizeObserver.disconnect(); layoutResizeTarget = null; clearAssetRetry(); observer.disconnect(); window.removeEventListener("resize", scheduleRefresh); clearTimeout(mutationTimer); clearTimeout(studioTimer); mutationTimer = 0; studioTimer = 0; Object.values(BACKDROP_IDS).forEach(id => document.getElementById(id)?.remove()); document.getElementById(BRAND_ID)?.remove(); document.getElementById(HOME_ID)?.remove(); document.getElementById(DECORATIONS_ID)?.remove(); document.getElementById(PANEL_ID)?.remove(); document.getElementById(TRIGGER_ID)?.remove(); document.getElementById(PANEL_STYLE_ID)?.remove(); document.querySelector(".ti-studio-backdrop")?.remove(); document.querySelectorAll("[data-theme-inject-custom-icon]").forEach(node => { delete node.dataset.themeInjectCustomIcon; node.style.removeProperty("--ti-custom-icon"); }); const attachShadow = Element.prototype.attachShadow; if (attachShadow.__themeInjectOriginal) Element.prototype.attachShadow = attachShadow.__themeInjectOriginal; callbacks.forEach(callback => { clearTimeout(callback.timer); callback.reject(new Error("Theme Inject reloaded")); }); callbacks.clear(); }

  installAttachShadowHook();
  const layoutResizeObserver = new ResizeObserver(scheduleRefresh);
  const observer = new MutationObserver(scheduleRefresh);
  observer.observe(document.documentElement, { childList: true, subtree: true });
  window.addEventListener("resize", scheduleRefresh);
  window.__themeInjectRuntime = { version: VERSION, openPanel, destroy };
  ensurePanelStyles();
  const start = async () => { const lifecycle = lifecycleEpoch; await reloadState(); if (destroyed || lifecycle !== lifecycleEpoch) return; ensureTrigger(); };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", () => { if (!destroyed) void start(); }, { once: true }); else void start();
})();
