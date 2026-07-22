  // Runtime module: lifecycle
  async function replaceStagedAsset(slot, result) {
    const previous = stagingAssets.find(asset => asset.slot === slot);
    if (previous) { try { await call("theme.preview.cancel", { session: previous.session }); } catch {} stagedUrls.delete(previous.path); thumbnailUrls.delete(previous.path); }
    stagingAssets = stagingAssets.filter(asset => asset.slot !== slot);
    stagingAssets.push({ slot, session: result.stagingSession, path: result.path });
    stagedUrls.set(result.path, result.previewUrl || "");
  }

  function setStatus(message, kind = "") { if (destroyed) return; statusMessage = message; statusKind = kind; const node = document.querySelector(`#${PANEL_ID} .ti-status`); if (node) { node.dataset.kind = kind; node.replaceChildren(t(message)); } }
  function applyTriggerAppearance(override) { const trigger = document.getElementById(TRIGGER_ID); if (!trigger || !state) return; const appearance = override || state.settings?.triggerAppearance || { shape:"rounded", size:40, right:18, bottom:18, backgroundOpacity:.88, shadowStrength:.35 }; trigger.style.cssText = triggerStyle(appearance); trigger.style.right = `${appearance.right}px`; trigger.style.bottom = `${appearance.bottom}px`; const icon = trigger.querySelector("img"); if (icon) icon.src = state.triggerIcon || TOOLBAR_ICON; }
  function setBusy(value) { busy = value; const panel = document.getElementById(PANEL_ID); if (!panel) return; panel.dataset.busy = String(value); panel.setAttribute("aria-busy", String(value)); panel.querySelectorAll("button,input,select,textarea").forEach(control => { if (!control.matches("[data-ai-workbench]")) control.disabled = value; }); }
  async function runAction(message, action, success) { if (busy) return; setBusy(true); setStatus(message); try { await action(); statusMessage = success; statusKind = "success"; } catch (error) { statusMessage = error?.message || "操作失败"; statusKind = "error"; } finally { setBusy(false); const panel = document.getElementById(PANEL_ID); if (panel?.dataset.open === "true") renderPanel(); } }
  function markDirty() { dirty = true; statusMessage = ""; statusKind = ""; scheduleDraftPreview(); document.querySelector(".ti-status")?.replaceChildren(t("有未应用修改")); }
  async function applyDraft(render = true) { draft.skin.decorations = (draft.skin.decorations || []).filter(item => item.asset); const referenced = collectAssetPaths(draft); const unused = stagingAssets.filter(asset => !referenced.has(asset.path)); await cancelPreviewSessions(unused); stagingAssets = stagingAssets.filter(asset => referenced.has(asset.path)); const result = await call("theme.apply", { theme: draft, customCss: draftCustomCss, enableCustomCss: Boolean(state.customCssTrusted), stagingAssets: stagingAssets.map(({ session, path }) => ({ session, path })) }); stagingAssets = []; stagedUrls.clear(); await clearAiReference(false); state = result.state; draft = clone(result.theme); snapshot = clone(result.theme); draftCustomCss = state.customCss || ""; dirty = false; await hydrateAssets(draft); applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""); if (render) renderPanel(); }
  function collectAssetPaths(theme) { const paths = new Set(); [theme.background?.fullscreen, theme.background?.content, theme.background?.sidebar].forEach(background => { if (background?.kind === "image" && background.path) paths.add(background.path); }); Object.values(theme.skin?.resources || {}).forEach(path => typeof path === "string" && path && paths.add(path)); Object.values(theme.skin?.icons?.mappings || {}).forEach(path => path && paths.add(path)); (theme.skin?.decorations || []).forEach(item => item.asset && paths.add(item.asset)); (theme.skin?.home?.cards || []).forEach(item => item.icon && paths.add(item.icon)); return paths; }
  async function cancelStaging() { const pending = stagingAssets; const references = aiReferences; stagingAssets = []; aiReferences = []; stagedUrls.clear(); thumbnailUrls.clear(); await cancelPreviewSessions([...pending, ...references]); }
  function cancelPreview() { void cancelStaging().then(async () => { draft = clone(snapshot); draftCustomCss = state?.customCss || ""; dirty = false; if (snapshot) { await hydrateAssets(snapshot); applyTheme(snapshot, state?.customCssTrusted ? state.customCss || "" : ""); } renderPanel(); }); }
  async function restorePreview() { await cancelStaging(); draft = clone(snapshot); draftCustomCss = state?.customCss || ""; dirty = false; if (snapshot) { await hydrateAssets(snapshot); applyTheme(snapshot, state?.customCssTrusted ? state.customCss || "" : ""); } }
  async function discardAndClose() { if (busy) return; setBusy(true); try { await restorePreview(); modal = null; hidePanel(); } catch (error) { modal = null; setStatus(error?.message || "放弃修改失败", "error"); renderPanel(); } finally { setBusy(false); } }
  async function applyAndClose() { if (busy) return; setBusy(true); setStatus("正在应用主题…"); try { await applyDraft(false); modal = null; statusMessage = "主题已应用"; statusKind = "success"; hidePanel(); } catch (error) { modal = null; setStatus(error?.message || "应用主题失败", "error"); renderPanel(); } finally { setBusy(false); } }
  function closePanel() { if (dirty) { modal = { kind: "close" }; renderPanel(); return; } void cancelStaging().then(hidePanel); }
  function hidePanel() { aiApiKeyDraft = ""; aiImageApiKeyDraft = ""; triggerAppearanceDraft = state?.settings?.triggerAppearance ? clone(state.settings.triggerAppearance) : null; applyTriggerAppearance(); const panel = document.getElementById(PANEL_ID); if (panel) panel.dataset.open = "false"; }
  async function reloadThemes() { state.themes = await call("theme.package.list"); }
  async function reloadState() { const lifecycle = lifecycleEpoch; await cancelStaging(); if (destroyed || lifecycle !== lifecycleEpoch) return; state = await call("theme.state.get"); if (destroyed || lifecycle !== lifecycleEpoch) return; setUiLanguage(state.settings?.uiLanguage); aiSettings = clone(state.ai); triggerAppearanceDraft = clone(state.settings.triggerAppearance); draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); draftCustomCss = state.customCss || ""; dirty = false; activateAiWorkspace(draft); await hydrateAssets(draft); if (destroyed || lifecycle !== lifecycleEpoch) return; applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""); }
  async function openPanel() { if (destroyed) return; const lifecycle = lifecycleEpoch; if (!state) await reloadState(); if (destroyed || lifecycle !== lifecycleEpoch || !state) return; triggerAppearanceDraft = clone(state.settings.triggerAppearance); applyTriggerAppearance(); draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); draftCustomCss = state.customCss || ""; dirty = false; renderPanel(); }
  function ensureTrigger() { if (destroyed) return; ensurePanelStyles(); if (document.getElementById(TRIGGER_ID) || !document.body) { applyTriggerAppearance(); return; } const button = document.createElement("button"); button.id = TRIGGER_ID; button.type = "button"; button.title = t("打开 Codex 主题"); button.setAttribute("aria-label", t("打开 Codex 主题")); const icon = document.createElement("img"); icon.src = state?.triggerIcon || TOOLBAR_ICON; icon.alt = ""; button.append(icon); button.addEventListener("click", () => void openPanel()); document.body.appendChild(button); applyTriggerAppearance(); }
  function cleanupCssInspector() { cssInspectorCleanup?.(); cssInspectorCleanup = null; }
  function dismissHomeForConversation(records) {
    const home = document.getElementById(HOME_ID);
    if (!home || !Array.isArray(records)) return;
    const selector = '[data-turn-key],[data-testid="conversation-turn"],[data-message-author-role]';
    const conversationAdded = records.some(record => [...(record.addedNodes || [])].some(node =>
      node instanceof Element && (node.matches(selector) || node.querySelector(selector))
    ));
    if (!conversationAdded) return;
    home.remove();
    homeSignature = "";
    document.querySelectorAll('[data-theme-inject-native-home="hidden"]').forEach(node => node.removeAttribute("data-theme-inject-native-home"));
  }
  function scheduleRefresh(records) {
    if (destroyed) return;
    dismissHomeForConversation(records);
    if (Array.isArray(records)) records.forEach(record => record.addedNodes?.forEach(node => {
      const root = node instanceof Element ? node : node.parentElement;
      if (root) pendingIconRoots.add(root);
    }));
    clearTimeout(mutationTimer);
    mutationTimer = setTimeout(() => {
      if (destroyed) return;
      const iconRoots = [...pendingIconRoots];
      pendingIconRoots.clear();
      ensureTrigger();
      if (draft) { patchTitlebar(); syncRightSidebar(); mountBackgrounds(draft); syncSkin(draft); syncIcons(draft, iconRoots); syncNativeSurfaceContrast(); syncNativeButtonContrast(); patchDiffRoots(draft); patchTerminal(draft); }
    }, 60);
  }
  function destroy() { if (destroyed) return; restoreAccountName(); restoreAccountAvatar(); destroyed = true; lifecycleEpoch += 1; generationEpoch += 1; assetHydrationEpoch += 1; windowChromeEpoch += 1; busy = false; studioOpen = false; studioSaveOpen = false; modal = null; stopAmbientEffect(); clearTimeout(windowChromeTimer); windowChromeTimer = 0; activeDecorationDragCleanup?.(); activeDecorationDragCleanup = null; layoutResizeObserver.disconnect(); layoutResizeTarget = null; clearAssetRetry(); observer.disconnect(); window.removeEventListener("resize", scheduleRefresh); clearTimeout(mutationTimer); clearTimeout(studioTimer); clearTimeout(studioDomTimer); mutationTimer = 0; studioTimer = 0; studioDomTimer = 0; pendingIconRoots.clear(); iconPreloads.clear(); iconPreloadThemeId = ""; Object.values(BACKDROP_IDS).forEach(id => document.getElementById(id)?.remove()); document.getElementById(BRAND_ID)?.remove(); document.getElementById(HOME_ID)?.remove(); document.getElementById(DECORATIONS_ID)?.remove(); document.getElementById(PANEL_ID)?.remove(); document.getElementById(TRIGGER_ID)?.remove(); document.getElementById(PANEL_STYLE_ID)?.remove(); document.querySelector(".ti-studio-backdrop")?.remove(); document.querySelectorAll("[data-theme-inject-custom-icon]").forEach(node => { delete node.dataset.themeInjectCustomIcon; node.style.removeProperty("--ti-custom-icon"); }); const attachShadow = Element.prototype.attachShadow; if (attachShadow.__themeInjectOriginal) Element.prototype.attachShadow = attachShadow.__themeInjectOriginal; callbacks.forEach(callback => { clearTimeout(callback.timer); callback.reject(new Error("Theme Inject reloaded")); }); callbacks.clear(); }

  installAttachShadowHook();
  const layoutResizeObserver = new ResizeObserver(scheduleRefresh);
  const observer = new MutationObserver(scheduleRefresh);
  observer.observe(document.documentElement, { childList: true, subtree: true });
  window.addEventListener("resize", scheduleRefresh);
  window.__themeInjectRuntime = { version: VERSION, locale: uiLocale, openPanel, ambientEffectStatus: () => ({ mode: ambientEffectWorker ? "worker" : ambientEffectFrame ? "main" : "off", metrics: ambientEffectMetrics }), destroy: () => { cleanupCssInspector(); destroy(); } };
  ensurePanelStyles();
  const start = async () => { const lifecycle = lifecycleEpoch; await reloadState(); if (destroyed || lifecycle !== lifecycleEpoch) return; ensureTrigger(); };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", () => { if (!destroyed) void start(); }, { once: true }); else void start();
})();
