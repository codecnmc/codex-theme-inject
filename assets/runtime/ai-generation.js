  // Runtime module: ai-generation
  function generationIsActive(epoch) { return !destroyed && epoch === generationEpoch && busy; }

  function resetGenerationWorkspace() {
    const previousAssets = stagingAssets;
    stagingAssets = [];
    stagedUrls.clear();
    previousAssets.forEach(asset => thumbnailUrls.delete(asset.path));
    if (previousAssets.length) void cancelPreviewSessions(previousAssets);
    const baseline = studioBaseline?.draft || snapshot;
    if (baseline) {
      draft = clone(baseline);
      draftCustomCss = studioBaseline?.customCss || state?.customCss || "";
      if (studioBaseline) state.customCssTrusted = studioBaseline.customCssTrusted;
      applyTheme(draft, state?.customCssTrusted ? draftCustomCss : "");
    }
    dirty = false;
    studioProgressSignature = "";
    statusMessage = "";
    statusKind = "";
  }
  async function generateAiTheme(epoch) {
    const resourcePlansOnly = aiResourcePlansOnly;
    const draftBeforeGeneration = clone(draft);
    const stagingBeforeGeneration = clone(stagingAssets);
    await saveAiSettings();
    if (!generationIsActive(epoch)) return;
    if (!state.ai?.hasApiKey) throw new Error("请先填写并保存基础 Key");
    if (aiGenerateImages && !state.ai?.hasImageApiKey) throw new Error("启用生图时请先填写并保存生图 Key");
    if (!aiPrompt.trim() && !aiReferences.length && !aiResourcePlans.length) throw new Error("请填写主题描述、选择参考图或添加批量资源计划");
    aiRequestLog = [];
    let result;
    try {
      result = await call("ai.generate", currentGenerationRequest());
    } catch (error) {
      const logs = await call("ai.log.get").catch(() => []);
      if (!generationIsActive(epoch)) return;
      aiRequestLog = logs;
      throw error;
    }
    if (!generationIsActive(epoch)) return;
    aiGenerationProgress = { ...aiGenerationProgress, state: "finalizing", message: "图片资源已完成，正在整理预览和草稿" };
    renderStudio(false);
    const logs = await call("ai.log.get").catch(() => []);
    if (!generationIsActive(epoch)) return;
    aiRequestLog = logs;
    const complete = await restoreCompleteGeneration(result, () => generationIsActive(epoch), 3, currentGenerationRequest());
    if (!complete || !generationIsActive(epoch)) throw new Error("生成资源尚未完整加载，请稍后重试");
    result = complete.result;
    if (!resourcePlansOnly) {
      const nextKeys = new Set((result.assets || []).map(asset => `${asset.session}:${asset.path}`));
      const previousAssets = stagingAssets.filter(asset => !nextKeys.has(`${asset.session}:${asset.path}`));
      await cancelPreviewSessions(previousAssets);
      if (!generationIsActive(epoch)) return;
    }
    await clearAiReference(false);
    if (!generationIsActive(epoch)) return;
    if (!studioOpen) return;
    if (!resourcePlansOnly) {
      if (aiStudioMode === "edit") mergeGeneratedAssets(result.theme, result.assets);
      else {
        draft = clone(result.theme);
        draftCustomCss = "";
        state.customCssTrusted = false;
      }
      stagingAssets = (result.assets || []).map(asset => ({ slot: asset.slot, session: asset.session, path: asset.path }));
      stagedUrls.clear();
      complete.urls.forEach((url, path) => stagedUrls.set(path, url));
    }
    if (resourcePlansOnly) {
      for (const asset of result.assets || []) {
        if (studioResourceCandidates.some(candidate => candidate.asset?.session === asset.session && candidate.asset?.path === asset.path)) continue;
        const progress = (aiGenerationProgress.items || []).find(item => item.slot === asset.slot);
        const candidate = {
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          slot: asset.slot,
          prompt: progress?.prompt || aiResourcePlans.find(plan => plan.slot === asset.slot)?.prompt || "批量生成资源",
          theme: clone(result.theme),
          asset,
          assets: [asset],
          previewUrl: thumbnailUrls.get(previewCacheKey(asset)) || thumbnailUrls.get(asset.path) || "",
          state: "candidate",
        };
        studioResourceCandidates.unshift(candidate);
        void previewDataUrl(asset, 2, false).then(url => {
          if (!url || !studioResourceCandidates.includes(candidate)) return;
          candidate.previewUrl = url;
          if (studioOpen) renderStudio(false);
        });
      }
      pendingStudioResource = studioResourceCandidates[0] || null;
      draft = draftBeforeGeneration;
      stagingAssets = stagingBeforeGeneration;
      setStatus(`已生成 ${result.assets?.length || 0} 项候选资源，请逐项应用到草稿`, "success");
    } else {
      dirty = true;
      aiGeneratedDraft = aiStudioMode === "generate";
    }
  }

  async function regeneratePaletteOnly() {
    if (busy) return;
    generationEpoch += 1;
    clearTimeout(studioTimer);
    busy = true;
    activeGenerationKind = "theme";
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
      activeGenerationKind = "";
      renderStudio(true);
    }
  }

  function currentGenerationRequest() {
    const planOnlyPrompt = aiResourcePlans.length
      ? `为当前 Codex 主题生成已配置的 ${aiResourcePlans.length} 项图片资源。保持现有主题结构，仅按自定义资源计划执行。`
      : "";
    const editContext = aiStudioMode === "edit"
      ? `\n\n编辑模式：只修改当前主题“${draft.name}”。保持现有主题身份、布局结构和未明确要求替换的资源；基于当前配色 ${draft.tokens?.foreground || ""} / ${draft.tokens?.contentBackground || ""} / ${draft.tokens?.accent || ""} 做增量调整。`
      : "";
    return {
      prompt: `${aiPrompt.trim() || planOnlyPrompt}${editContext}`,
      themeName: aiThemeName,
      themeDescription: aiThemeDescription,
      language: aiLanguage,
      generateImages: aiGenerateImages,
      generateSidebarWatermark: aiGenerateSidebarWatermark,
      generateSystemIcons: aiGenerateSystemIcons,
      imageConcurrency: aiImageConcurrency,
      references: aiReferences.map(({ session, path }) => ({ session, path })),
      resourcePlans: aiResourcePlans,
      resourcePlansOnly: aiResourcePlansOnly,
      theme: aiResourcePlansOnly ? draft : undefined,
    };
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
      const hasCurrentRequirements = Boolean(aiPrompt.trim() || aiReferences.length || aiResourcePlans.length);
      const restored = await restoreCompleteGeneration(null, () => !destroyed && lifecycle === lifecycleEpoch && operationEpoch === generationEpoch, 4, hasCurrentRequirements ? currentGenerationRequest() : null);
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
      studioPreviewRefreshPending = true;
      studioPreviewReady = false;
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
    activateAiWorkspace(draft);
    if (!studioOpen) {
      aiThemeName = draft.name || "";
      aiThemeDescription = draft.description || "";
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
    else if (!studioPreviewReady) void activateStudioPreview();
  }

  async function regenerateIconAtlas() {
    await runAction("正在生成并切割完整图标集…", async () => {
      if (!state.ai?.hasImageApiKey) throw new Error("请先在“AI 生成”中保存生图 Key");
      const order = skinIconFields.map(([action, label], index) => `cell ${index + 1}: ${action} (${label})`).join("; ");
      const prompt = `Create a precise 6 column by 5 row sprite atlas for a cohesive desktop IDE action icon set. Use exactly thirty equal cells in row-major order: ${order}. Theme colors: foreground ${draft.tokens.foreground}, accent ${draft.tokens.accent}, background ${draft.tokens.sidebarBackground}. Every cell contains one centered high-contrast icon with identical scale, stroke weight and generous padding. The entire canvas and every gap must be solid pure chroma green #00FF00. No text, labels, dividers, frames, shadows, gradients, checkerboard, button plates or extra objects.`;
      const result = await call("ai.resource.generate", { theme: draft, slot: "skin.icons.atlas", prompt, useCurrentResourceReference: true, references: studioResourceReferences("skin.icons.atlas", true) });
      const icons = (result.assets || []).filter(asset => asset.slot.startsWith("skin.icons.") && asset.slot !== "skin.icons.atlas");
      if (icons.length !== skinIconFields.length) throw new Error(`图标集切割不完整：预期 ${skinIconFields.length} 项，实际 ${icons.length} 项`);
      const previous = stagingAssets.filter(asset => asset.slot.startsWith("skin.icons."));
      await cancelPreviewSessions(previous);
      previous.forEach(asset => { stagedUrls.delete(asset.path); thumbnailUrls.delete(asset.path); });
      stagingAssets = stagingAssets.filter(asset => !asset.slot.startsWith("skin.icons."));
      draft = clone(result.theme);
      draft.skin.enabled = true;
      draft.skin.icons.mode = "custom";
      icons.forEach(asset => {
        stagingAssets.push({ slot: asset.slot, session: asset.session, path: asset.path });
        stagedUrls.set(asset.path, asset.previewUrl || "");
      });
      const atlas = (result.assets || []).find(asset => asset.slot === "skin.icons.atlas");
      if (atlas) await call("theme.preview.cancel", { session: atlas.session }).catch(() => {});
      markDirty();
      applyTheme(draft, state?.customCssTrusted ? draftCustomCss : "");
    }, "完整图标集已生成并切割为 30 项，点击应用后保存");
  }
