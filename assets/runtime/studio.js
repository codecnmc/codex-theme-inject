  // Runtime module: studio
  function syncRightSidebar() {
    document.querySelectorAll("[data-theme-inject-right-sidebar],[data-theme-inject-right-sidebar-item],[data-theme-inject-right-sidebar-shortcut]").forEach(node => {
      delete node.dataset.themeInjectRightSidebar;
      delete node.dataset.themeInjectRightSidebarItem;
      delete node.dataset.themeInjectRightSidebarShortcut;
    });
    const labels = [
      "文件", "檔案", "Files", "ファイル", "파일",
      "新任务", "新建任务", "新增任務", "New task", "新しいタスク", "새 작업",
      "浏览器", "瀏覽器", "Browser", "ブラウザー", "브라우저",
      "终端", "終端機", "Terminal", "ターミナル", "터미널",
    ];
    const main = document.querySelector(".app-shell-main-content-viewport,.main-surface");
    if (!main) return;
    const items = [...main.querySelectorAll("button,[role=button],a")].filter(node => {
      if (node.closest(".app-shell-left-panel,[data-codex-composer-root],[data-codex-composer=true],#theme-inject-home")) return false;
      const bounds = node.getBoundingClientRect();
      if (bounds.width < 160 || bounds.height < 28 || bounds.height > 84) return false;
      const label = [node.getAttribute("aria-label"), node.getAttribute("title"), node.innerText]
        .filter(Boolean).join(" ").trim().replace(/\s+/g, " ");
      return labels.some(candidate => label === candidate || label.startsWith(`${candidate} `));
    });
    if (items.length < 2) return;
    const ancestorCounts = new Map();
    items.forEach(item => {
      let ancestor = item.parentElement;
      for (let depth = 0; ancestor && depth < 6 && ancestor !== main; depth += 1, ancestor = ancestor.parentElement) {
        ancestorCounts.set(ancestor, (ancestorCounts.get(ancestor) || 0) + 1);
      }
    });
    const group = [...ancestorCounts]
      .filter(([, count]) => count === items.length)
      .sort(([left], [right]) => left.querySelectorAll("button,[role=button],a").length - right.querySelectorAll("button,[role=button],a").length)[0]?.[0] || null;
    if (!group) return;
    group.dataset.themeInjectRightSidebar = "true";
    const groupItems = [...group.querySelectorAll("button,[role=button],a")].filter(node => {
      const bounds = node.getBoundingClientRect();
      return bounds.width >= 160 && bounds.height >= 28 && bounds.height <= 84;
    });
    groupItems.forEach(item => {
      item.dataset.themeInjectRightSidebarItem = "true";
      item.querySelectorAll("kbd,code,span,small").forEach(node => {
        const text = node.textContent?.trim() || "";
        if (/^(Ctrl|Alt|Shift|⌘|⌥|⇧|⌃|Cmd)[+\s-]/i.test(text) || /Ctrl\+/i.test(text)) node.dataset.themeInjectRightSidebarShortcut = "true";
      });
    });
  }

  async function interruptGeneration() {
    if (!busy) return;
    const interruptedEpoch = generationEpoch;
    const interruptedKind = activeGenerationKind;
    const interruptedPlansOnly = aiResourcePlansOnly;
    clearTimeout(studioTimer);
    const interruptButton = document.querySelector("[data-studio-interrupt]");
    if (interruptButton) { interruptButton.disabled = true; interruptButton.replaceChildren(t("正在中断…")); }
    setStatus("正在中断生成…", "warning");
    try {
      await call("ai.generation.cancel");
    } catch (error) {
      const message = error?.message || "中断请求未送达";
      if (interruptButton) { interruptButton.disabled = false; interruptButton.replaceChildren(t("重新中断")); }
      setStatus(`中断请求失败：${message}`, "error");
      studioTimer = setTimeout(() => void pollGenerationProgress(interruptedEpoch), 300);
      return;
    }
    generationEpoch += 1;
    busy = false;
    activeGenerationKind = "";
    const [progress, restored] = await Promise.all([
      call("ai.progress.get").catch(() => ({ state: "cancelled", message: "生成已中断", items: aiGenerationProgress.items || [] })),
      interruptedKind === "theme" && !interruptedPlansOnly ? call("ai.generation.restore", currentGenerationRequest()).catch(() => null) : Promise.resolve(null),
    ]);
    if (destroyed || interruptedEpoch + 1 !== generationEpoch) return;
    aiGenerationProgress = progress;
    if (restored) {
      const urls = await loadStagedAssetUrls(restored.assets || []);
      if (urls) {
        draft = clone(restored.theme);
        stagingAssets = (restored.assets || []).map(asset => ({ slot: asset.slot, session: asset.session, path: asset.path }));
        stagedUrls.clear();
        urls.forEach((url, path) => stagedUrls.set(path, url));
        dirty = true;
        aiGeneratedDraft = aiStudioMode === "generate";
      }
    }
    aiResourcePlansOnly = false;
    setStatus("生成已中断；已完成素材已保留，可在资源页补充缺失项", "warning");
    renderStudio(false);
  }

  function closeStudio() {
    studioOpen = false;
    clearTimeout(studioTimer);
    clearTimeout(studioDomTimer);
    studioDomTimer = 0;
    document.querySelector(".ti-studio-backdrop")?.remove();
    if (studioBaseline) {
      const discardedAssets = stagingAssets;
      const baselineKeys = new Set(studioBaseline.stagingAssets.map(asset => `${asset.session}:${asset.path}`));
      void cancelPreviewSessions(discardedAssets.filter(asset => !baselineKeys.has(`${asset.session}:${asset.path}`)));
      discardedAssets.forEach(asset => {
        if (!baselineKeys.has(`${asset.session}:${asset.path}`)) {
          stagedUrls.delete(asset.path);
          thumbnailUrls.delete(asset.path);
        }
      });
      draft = clone(studioBaseline.draft);
      draftCustomCss = studioBaseline.customCss;
      stagingAssets = clone(studioBaseline.stagingAssets);
      dirty = studioBaseline.dirty;
      state.customCssTrusted = studioBaseline.customCssTrusted;
      studioBaseline = null;
      aiGeneratedDraft = false;
      studioSaveOpen = false;
      void hydrateAssets(draft).then(() => applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""));
    }
    studioBaseline = null;
    const panel = document.getElementById(PANEL_ID);
    if (panel) panel.dataset.open = "true";
    renderPanel();
  }

  function renderStudio(refreshDom = false) {
    if (destroyed || !studioOpen || !draft) return;
    if (aiStudioMode === "edit" && studioTab === "generate") studioTab = "resources";
    const decorationCount = draft.skin?.decorations?.length || 0;
    if (decorationCount && (studioSelectedDecoration < 0 || studioSelectedDecoration >= decorationCount)) studioSelectedDecoration = 0;
    const previous = document.querySelector(".ti-studio-backdrop");
    const previousFrame = previous?.querySelector(".ti-studio-frame");
    const previousPane = previous?.querySelector(".ti-studio-pane");
    if (previousPane) studioPaneScrollTop = previousPane.scrollTop;
    const root = document.createElement("div");
    root.innerHTML = studioHtml();
    const studio = root.firstElementChild;
    const closeButton = studio.querySelector("[data-studio-close]");
    if (closeButton) { closeButton.disabled = false; closeButton.removeAttribute("title"); closeButton.textContent = busy ? "关闭（后台继续）" : "关闭"; }
    const generateButton = studio.querySelector("[data-studio-generate]");
    if (generateButton && generateButton.dataset.readonly !== "true") generateButton.title = "生成结果会先进入工作台草稿；需要保存并启用，或关闭工作台后在外层点击应用，才会写入当前主题";
    const studioTabs = studio.querySelector(".ti-studio-tabs");
    if (studioTabs) {
      if (aiStudioMode === "edit") studioTabs.querySelector('[data-studio-tab="generate"]')?.remove();
      const modeDescription = aiStudioMode === "generate" ? "从需求创建一套新主题，会重建配色与选中的图片资源。" : "保留当前主题结构，只把你确认的配色或资源合并进草稿。";
      studioTabs.insertAdjacentHTML("beforebegin", `<div class="ti-studio-mode-tabs" role="tablist" aria-label="工作台模式"><button class="ti-studio-tab" role="tab" data-studio-mode="generate" title="从需求创建新主题并生成所选资源" data-active="${aiStudioMode === "generate"}" ${busy ? "disabled" : ""}>生成模式</button><button class="ti-studio-tab" role="tab" data-studio-mode="edit" title="保留当前主题，只编辑确认的配色或资源" data-active="${aiStudioMode === "edit"}" ${busy ? "disabled" : ""}>编辑模式</button><div class="ti-studio-mode-help">${modeDescription}</div></div>`);
    }
    const failedCount = (aiGenerationProgress.items || []).filter(item => item.status === "failed").length;
    if (generateButton && !busy && failedCount) generateButton.textContent = `继续生成 ${failedCount} 个缺失素材`;
    else if (generateButton && !busy && aiResourcePlans.length) generateButton.textContent = `批量生成 ${aiResourcePlans.length} 项资源`;
    const prompt = studio.querySelector("[data-studio-prompt]");
    if (prompt && !studio.querySelector("[data-studio-language]")) prompt.insertAdjacentHTML("beforebegin", `<label class="ti-field-label">界面语言</label><select class="ti-control" data-studio-language ${busy ? "disabled" : ""}>${themeLanguageOptions.map(([value, label]) => `<option value="${value}" ${aiLanguage === value ? "selected" : ""}>${label}</option>`).join("")}</select>`);
    if (prompt) {
      prompt.setAttribute("aria-invalid", String(Boolean(aiPromptError)));
      prompt.insertAdjacentHTML("afterend", `<div class="ti-description" role="alert" data-studio-prompt-error ${aiPromptError ? "" : "hidden"}>${escapeHtml(aiPromptError)}</div>`);
    }
    const generateImagesCopy = studio.querySelector("[data-studio-generate-images]")?.nextElementSibling;
    if (generateImagesCopy) generateImagesCopy.textContent = "生成背景、Hero、装饰和卡片图标";
    const generateImagesLabel = studio.querySelector("[data-studio-generate-images]")?.closest("label");
    if (generateImagesLabel && !studio.querySelector("[data-studio-image-concurrency]")) generateImagesLabel.insertAdjacentHTML("afterend", `<label class="ti-ai-checkbox"><input type="checkbox" data-studio-generate-watermark ${aiGenerateSidebarWatermark ? "checked" : ""} ${busy || !aiGenerateImages ? "disabled" : ""}><span>生成左侧导航水印（默认关闭，避免遮挡导航）</span></label><label class="ti-ai-checkbox"><input type="checkbox" data-studio-generate-system-icons ${aiGenerateSystemIcons ? "checked" : ""} ${busy || !aiGenerateImages ? "disabled" : ""}><span>生成完整主题图标（6×5 图集自动切割为 30 项）</span></label><div class="ti-row ti-compact-row"><div class="ti-label">图片并发</div><div><select class="ti-control" data-studio-image-concurrency ${busy ? "disabled" : ""}>${[1,2,3,4].map(value => `<option value="${value}" ${aiImageConcurrency === value ? "selected" : ""}>${value}</option>`).join("")}</select></div></div><button class="ti-button" type="button" data-studio-regenerate-palette ${busy ? "disabled" : ""}>只重新生成配色</button>`);
    const headerActions = studio.querySelector(".ti-studio-header .ti-inline");
    if (headerActions) headerActions.insertAdjacentHTML("afterbegin", aiGeneratedDraft ? `<button class="ti-button" data-primary="true" data-studio-save title="将生成结果保存为新的主题，并立即切换到该主题">保存为新主题</button>` : dirty ? `<button class="ti-button" data-primary="true" data-studio-apply title="将当前工作台草稿写入当前主题并立即启用">保存并启用</button>` : `<button class="ti-button" data-studio-restore title="恢复当前主题最近一次未完成的生成结果">恢复上次生成</button>`);
    const decorationSection = studio.querySelector('[data-studio-section="decorations"]');
    if (decorationSection && !decorationSection.querySelector("[data-studio-decoration-save]")) {
      const saveWrap = document.createElement("div");
      saveWrap.className = "ti-inline ti-section-gap";
      saveWrap.innerHTML = `<button class="ti-button" type="button" data-studio-decoration-save title="只保存装饰层的位置和关联资源，不应用其他工作台草稿">保存装饰位置</button><span class="ti-description" data-studio-decoration-save-status>只保存装饰层，不会应用未确认的配色。</span>`;
      decorationSection.insertBefore(saveWrap, decorationSection.querySelector(".ti-resource-list"));
    }
    const resourceForm = studio.querySelector("[data-studio-plan-form]");
    if (resourceForm) {
      resourceForm.querySelector("strong").textContent = "添加批量生成计划";
      const addPlanButton = resourceForm.querySelector('button[type="submit"]');
      if (addPlanButton) {
        addPlanButton.textContent = "加入批量计划";
        addPlanButton.title = "保存当前资源类型和提示词；可继续添加其他资源，最后统一生成";
        addPlanButton.insertAdjacentHTML("afterend", `<button class="ti-button" type="button" data-studio-generate-resource title="只生成当前表单中的一个候选资源，不加入批量计划">立即生成候选资源</button>`);
      }
      const resourceSlotControl = resourceForm.querySelector("[data-studio-plan-slot]");
      if (resourceSlotControl?.querySelector(`option[value="${studioResourceSlot}"]`)) resourceSlotControl.value = studioResourceSlot;
      else if (resourceSlotControl) studioResourceSlot = resourceSlotControl.value;
      const resourcePromptControl = resourceForm.querySelector("[data-studio-plan-prompt]");
      if (resourcePromptControl) {
        resourcePromptControl.value = studioResourcePrompt;
        resourcePromptControl.setAttribute("aria-invalid", String(Boolean(studioResourceError)));
        resourcePromptControl.insertAdjacentHTML("afterend", `<div class="ti-description" role="alert" data-studio-plan-error ${studioResourceError ? "" : "hidden"}>${escapeHtml(studioResourceError)}</div>`);
      }
      const planTitle = resourceForm.nextElementSibling;
      const planList = planTitle?.nextElementSibling;
      if (planTitle) planTitle.textContent = `批量生成计划 · ${aiResourcePlans.length}/16`;
      if (planList && !aiResourcePlans.length) planList.textContent = "添加多个资源计划后，可一次提交并按图片并发设置生成。";
      if (planList && aiResourcePlans.length) planList.insertAdjacentHTML("afterend", `<button class="ti-button ti-section-gap" data-primary="true" type="button" data-studio-generate-plans title="一次提交全部计划，按图片并发设置自动生成">批量生成 ${aiResourcePlans.length} 项资源</button>`);
      resourceForm.insertAdjacentHTML("afterbegin", `<div class="ti-notice">先把多个资源加入计划，再统一生成；当前上传的参考图会用于计划中的每个资源。单个候选资源仍需应用到草稿或保存并启用。</div><div data-studio-current-resource-reference></div><div class="ti-section-title">计划共用参考图</div><div data-studio-resource-reference-list>${studioReferenceListHtml()}</div><button class="ti-button" type="button" data-studio-resource-reference-select title="添加参考图不会清空当前资源类型、提示词或批量计划" ${busy ? "disabled" : ""}>上传计划参考图</button>`);
      const candidateHistory = studioCandidateHistoryHtml();
      if (candidateHistory) resourceForm.insertAdjacentHTML("afterend", candidateHistory);
    }
    if (prompt) prompt.insertAdjacentHTML("afterend", `<div class="ti-studio-metadata">${row("主题名称", `<input class="ti-control" data-studio-meta="name" maxlength="80" value="${escapeHtml(aiThemeName)}" ${busy ? "disabled" : ""}>`)}${row("主题介绍", `<textarea class="ti-control ti-small-textarea" data-studio-meta="description" maxlength="240" ${busy ? "disabled" : ""}>${escapeHtml(aiThemeDescription)}</textarea>`)}</div>`);
    if (studioSaveOpen) studio.insertAdjacentHTML("beforeend", `<div class="ti-studio-modal-backdrop"><form class="ti-modal" data-studio-save-form><div class="ti-modal-title">保存为新主题</div><div class="ti-modal-copy">生成结果会复制到主题库并立即启用；保存成功后才会清理恢复检查点。</div><label class="ti-field-label" for="ti-generated-theme-name">主题名称</label><input id="ti-generated-theme-name" class="ti-control" data-studio-save-name maxlength="80" value="${escapeHtml(draft.name)}" autocomplete="off"><div class="ti-modal-actions"><button class="ti-button" type="button" data-studio-save-cancel>取消</button><button class="ti-button" data-primary="true" type="submit">保存并启用</button></div></form></div>`);
    if (studioAssetPreview) studio.insertAdjacentHTML("beforeend", `<div class="ti-studio-modal-backdrop" data-studio-preview-backdrop><div class="ti-studio-asset-preview"><div class="ti-inline ti-between"><strong>${escapeHtml(studioAssetPreview.title)}</strong><button class="ti-icon-button" type="button" data-studio-preview-close title="关闭大图">关闭</button></div><img src="${escapeHtml(studioAssetPreview.url)}" alt="${escapeHtml(studioAssetPreview.title)}"><div class="ti-description" data-studio-preview-status>${studioAssetPreview.loading ? "正在加载原图…" : "点击背景或关闭按钮返回"}</div></div></div>`);
    if (busy && generateButton) generateButton.insertAdjacentHTML("beforebegin", `<button class="ti-button" data-danger="true" type="button" data-studio-interrupt title="立即停止当前后台生成任务">中断生成</button>`);
    if (previousFrame) studio.querySelector(".ti-studio-frame")?.replaceWith(previousFrame);
    previous?.replaceWith(studio) || document.body.appendChild(studio);
    bindStudio(studio);
    const frame = studio.querySelector(".ti-studio-frame");
    if (frame) frame.hidden = !studioPreviewReady;
    if (!studioPreviewReady && !studioPreviewRefreshPending) {
      const frameWrap = studio.querySelector(".ti-studio-frame-wrap");
      if (frameWrap) {
        const placeholder = document.createElement("div");
        placeholder.className = "ti-studio-preview-placeholder";
        placeholder.textContent = "正在准备预览 DOM";
        frameWrap.appendChild(placeholder);
      }
    }
    if (studioPreviewRefreshPending && !busy) {
      const frameWrap = studio.querySelector(".ti-studio-frame-wrap");
      if (frameWrap) {
        const overlay = document.createElement("div");
        overlay.className = "ti-studio-preview-refresh-overlay";
        overlay.innerHTML = `<strong>生成已完成</strong><span>预览 DOM 可能仍是生成前的副本，请刷新后查看最新主题。</span><button class="ti-button" data-studio-preview-refresh type="button" data-primary="true">刷新预览 DOM</button>`;
        frameWrap.appendChild(overlay);
        overlay.querySelector("[data-studio-preview-refresh]")?.addEventListener("click", () => void activateStudioPreview());
      }
    }
    localizeElement(studio);
    if (busy) studio.querySelectorAll("button,input,textarea,select").forEach(control => {
      if (!control.matches("[data-studio-tab],[data-studio-close],[data-studio-interrupt],[data-studio-refresh],[data-studio-candidate-preview],[data-studio-preview-close]")) control.disabled = true;
    });
    const pane = studio.querySelector(".ti-studio-pane");
    if (pane) {
      pane.scrollTop = studioPaneScrollTop;
      pane.addEventListener("scroll", () => { studioPaneScrollTop = pane.scrollTop; }, { passive: true });
    }
    scheduleStudioDomRefresh(studio, refreshDom);
    renderStudioDecorations(studio);
    frame?.addEventListener("load", () => {
      if (!destroyed && studio.isConnected) renderStudioDecorations(studio);
    }, { once:true });
  }

  function staticDomSnapshot() {
    const copy = document.documentElement.cloneNode(true);
    copy.querySelectorAll("script,.ti-studio-backdrop,#theme-inject-panel,#theme-inject-trigger,#theme-inject-decorations").forEach(node => node.remove());
    copy.querySelector(`#${STYLE_ID}`)?.remove();
    const previewStyle = document.createElement("style");
    previewStyle.id = STYLE_ID;
    previewStyle.textContent = `${themeCss(draft, state?.customCssTrusted ? draftCustomCss : "")}
button,[role="button"]{opacity:1!important;}
.main-surface :is(button,[role="button"]){color:var(--ti-content-fg)!important;}
:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"]),:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"]) *{color:var(--ti-accent-foreground)!important;}
:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"]) svg{fill:currentColor!important;stroke:currentColor!important;opacity:1!important;}`;
    copy.querySelector("head")?.appendChild(previewStyle);
    copy.querySelectorAll("[contenteditable],input,textarea,select,button,a").forEach(node => { node.removeAttribute("contenteditable"); node.removeAttribute("href"); node.removeAttribute("disabled"); node.removeAttribute("aria-disabled"); node.style.removeProperty("opacity"); node.setAttribute("tabindex", "-1"); });
    copy.querySelectorAll("button svg,[role=button] svg").forEach(node => { node.style.color = "currentColor"; node.style.opacity = "1"; });
    copy.querySelectorAll("iframe").forEach(node => node.replaceWith(document.createElement("div")));
    const base = copy.querySelector("head")?.appendChild(document.createElement("base"));
    if (base) base.href = location.href;
    let html = copy.outerHTML;
    for (const asset of stagingAssets) {
      const source = stagedUrls.get(asset.path);
      const thumbnail = thumbnailUrls.get(previewCacheKey(asset)) || thumbnailUrls.get(asset.path);
      if (source && thumbnail) html = html.split(source).join(thumbnail);
    }
    return `<!doctype html>${html}`;
  }

  async function prepareStudioPreviewAssets() {
    const pending = stagingAssets.filter(asset => !thumbnailUrls.has(previewCacheKey(asset)) && !thumbnailUrls.has(asset.path));
    await mapConcurrent(pending, 1, async asset => {
      const url = await previewDataUrl(asset, 2, false);
      if (url) {
        thumbnailUrls.set(previewCacheKey(asset), url);
        thumbnailUrls.set(asset.path, url);
      }
    });
  }

  async function activateStudioPreview() {
    if (destroyed || !studioOpen || busy || studioPreviewPreparing) return;
    studioPreviewPreparing = true;
    studioPreviewRefreshPending = false;
    studioPreviewReady = false;
    try {
      await prepareStudioPreviewAssets();
      if (destroyed || !studioOpen || busy) return;
      studioPreviewReady = true;
      applyTheme(draft, "");
      renderStudio(true);
    } finally {
      studioPreviewPreparing = false;
    }
  }

  function refreshStudioDom(studio, force) {
    const frame = studio.querySelector(".ti-studio-frame");
    if (!studioPreviewReady || !frame || (!force && frame.dataset.loaded === "true")) return;
    frame.srcdoc = staticDomSnapshot();
    frame.dataset.loaded = "true";
  }

  function scheduleStudioDomRefresh(studio, force) {
    clearTimeout(studioDomTimer);
    if (!studioPreviewReady) return;
    const refresh = () => {
      studioDomTimer = 0;
      if (destroyed || !studioOpen || !studio?.isConnected) return;
      try {
        refreshStudioDom(studio, force);
      } catch (error) {
        const frame = studio.querySelector(".ti-studio-frame");
        if (frame) {
          frame.dataset.loaded = "true";
          frame.dataset.previewError = "true";
          frame.srcdoc = `<!doctype html><html><body style="margin:0;display:grid;place-items:center;min-height:100vh;background:#111722;color:#A9B8C8;font:14px Segoe UI,sans-serif;text-align:center"><div>静态预览暂时不可用<br><small>工作台仍可继续使用，稍后可点击“刷新 DOM 副本”重试</small></div></body></html>`;
        }
        console.error("Theme Inject preview failed", error);
      }
    };
    studioDomTimer = setTimeout(refresh, 0);
  }

  function renderStudioDecorations(studio) {
    if (!studio) return;
    const host = studio.querySelector(".ti-studio-decorations");
    if (!host) return;
    host.replaceChildren();
    const resources = draft.skin?.resources || {};
    const builtins = [
      resources.heroBadge ? { asset: resources.heroBadge, region: "content", anchor: "top-right", offsetX: 24, offsetY: 64, width: 180, opacity: 1, layer: 2, previewKind: "hero-badge" } : null,
      resources.sticker ? { asset: resources.sticker, region: "content", anchor: "bottom-right", offsetX: 24, offsetY: 120, width: 180, opacity: 1, layer: 2, previewKind: "sticker" } : null,
      resources.composerDecoration ? { asset: resources.composerDecoration, region: "composer", anchor: "bottom-right", offsetX: 24, offsetY: 24, width: 220, opacity: .86, layer: 1, previewKind: "composer-decoration" } : null,
    ].filter(Boolean);
    const decorations = [...builtins, ...(draft.skin?.decorations || []).map((decoration, index) => ({ ...decoration, previewIndex: index }))];
    decorations.forEach(decoration => {
      const url = resolvedAsset(draft, decoration.asset);
      if (!url) return;
      const image = document.createElement("img");
      image.className = "ti-studio-decoration";
      image.src = url;
      image.alt = "";
      image.dataset.region = decoration.region || "content";
      if (Number.isInteger(decoration.previewIndex)) {
        image.dataset.decorationIndex = String(decoration.previewIndex);
        image.dataset.selected = String(decoration.previewIndex === studioSelectedDecoration);
        image.addEventListener("pointerdown", beginDecorationDrag);
      } else {
        image.dataset.previewBuiltin = decoration.previewKind;
        image.dataset.selected = "false";
      }
      positionStudioDecoration(image, decoration, host);
      host.appendChild(image);
    });
  }

  function studioDecorationBounds(host, decoration) {
    const width = host.clientWidth || 1600;
    const height = host.clientHeight || 900;
    const frame = host.parentElement?.querySelector(".ti-studio-frame");
    const previewDocument = frame?.contentDocument;
    const region = decoration.region || "content";
    let left = 0, right = width, top = 0, bottom = height;
    if (region === "sidebar") {
      const sidebar = previewDocument?.querySelector(".app-shell-left-panel");
      const bounds = sidebar?.getBoundingClientRect();
      if (bounds?.width > 0) right = Math.max(left, Math.min(width, bounds.right));
      else right = Math.max(left, Math.min(width, Number(draft.layout?.sidebarWidth) || 300));
    } else if (region === "content") {
      const content = previewDocument?.querySelector(".main-surface,.app-shell-main-content-viewport");
      const bounds = content?.getBoundingClientRect();
      if (bounds?.width > 0) left = Math.max(0, Math.min(width, bounds.left));
    } else if (region === "composer") {
      bottom = Math.max(top, height - 18);
    }
    return { left, right, top, bottom, width: Math.max(0, right - left), height: Math.max(0, bottom - top) };
  }

  function positionStudioDecoration(node, decoration, host) {
    const bounds = studioDecorationBounds(host, decoration);
    const width = Math.min(decoration.width || 160, bounds.width || host.clientWidth || 1600);
    node.style.width = `${width}px`;
    node.style.opacity = String(decoration.opacity ?? 1);
    node.style.zIndex = String(decoration.layer ?? 0);
    const right = decoration.anchor?.endsWith("right");
    const bottom = decoration.anchor?.startsWith("bottom");
    node.style.left = right ? "auto" : `${bounds.left + (decoration.offsetX || 0)}px`;
    node.style.right = right ? `${Math.max(0, (host.clientWidth || bounds.right) - bounds.right) + (decoration.offsetX || 0)}px` : "auto";
    node.style.top = bottom ? "auto" : `${bounds.top + (decoration.offsetY || 0)}px`;
    node.style.bottom = bottom ? `${Math.max(0, (host.clientHeight || bounds.bottom) - bounds.bottom) + (decoration.offsetY || 0)}px` : "auto";
  }

  function bindStudio(studio) {
    studio.querySelector("[data-studio-close]")?.addEventListener("click", closeStudio);
    studio.querySelector("[data-studio-restore]")?.addEventListener("click", () => void restoreGeneratedTheme());
    studio.querySelector("[data-studio-save]")?.addEventListener("click", () => { studioSaveOpen = true; renderStudio(); queueMicrotask(() => document.querySelector("[data-studio-save-name]")?.select()); });
    studio.querySelector("[data-studio-save-cancel]")?.addEventListener("click", () => { studioSaveOpen = false; renderStudio(); });
    studio.querySelector("[data-studio-save-form]")?.addEventListener("submit", event => { event.preventDefault(); const name = studio.querySelector("[data-studio-save-name]")?.value.trim(); if (!name) return; void saveGeneratedTheme(name); });
    studio.querySelector("[data-studio-apply]")?.addEventListener("click", () => void persistStudioDraft());
    studio.querySelector("[data-studio-decoration-save]")?.addEventListener("click", () => void persistStudioDecorations());
    studio.querySelector("[data-studio-refresh]")?.addEventListener("click", () => void activateStudioPreview());
    studio.querySelectorAll("[data-studio-tab]").forEach(button => button.addEventListener("click", () => {
      studioTab = button.dataset.studioTab;
      studio.querySelectorAll("[data-studio-tab]").forEach(tab => { tab.dataset.active = String(tab === button); });
      studio.querySelectorAll("[data-studio-section]").forEach(section => { section.dataset.active = String(section.dataset.studioSection === studioTab); });
    }));
    studio.querySelectorAll("[data-studio-mode]").forEach(button => button.addEventListener("click", () => { if (busy) return; aiStudioMode = button.dataset.studioMode; if (aiStudioMode === "edit" && studioTab === "generate") studioTab = "resources"; renderStudio(); }));
    studio.querySelector("[data-studio-interrupt]")?.addEventListener("click", () => void interruptGeneration());
    studio.querySelector("[data-studio-prompt]")?.addEventListener("input", event => {
      aiPrompt = event.target.value;
      if (!aiPromptError) return;
      aiPromptError = "";
      event.target.setAttribute("aria-invalid", "false");
      const error = studio.querySelector("[data-studio-prompt-error]");
      if (error) { error.hidden = true; error.replaceChildren(); }
    });
    studio.querySelector("[data-studio-language]")?.addEventListener("change", event => { aiLanguage = event.target.value; });
    studio.querySelector("[data-studio-generate-images]")?.addEventListener("change", event => { aiGenerateImages = event.target.checked; renderStudio(); });
    studio.querySelector("[data-studio-generate-watermark]")?.addEventListener("change", event => { aiGenerateSidebarWatermark = event.target.checked; });
    studio.querySelector("[data-studio-generate-system-icons]")?.addEventListener("change", event => { aiGenerateSystemIcons = event.target.checked; });
    studio.querySelector("[data-studio-image-concurrency]")?.addEventListener("change", event => { aiImageConcurrency = Math.max(1, Math.min(4, Number(event.target.value) || 4)); });
    studio.querySelector("[data-studio-regenerate-palette]")?.addEventListener("click", () => void regeneratePaletteOnly());
    studio.querySelector("[data-studio-reference-select]")?.addEventListener("click", () => void chooseStudioReference());
    studio.querySelector("[data-studio-resource-reference-select]")?.addEventListener("click", () => void chooseStudioReference());
    studio.querySelectorAll("[data-studio-candidate-apply]").forEach(button => button.addEventListener("click", () => void applyStudioResource(false, button.dataset.studioCandidateApply)));
    studio.querySelectorAll("[data-studio-candidate-undo]").forEach(button => button.addEventListener("click", () => undoStudioResource(button.dataset.studioCandidateUndo)));
    studio.querySelectorAll("[data-studio-candidate-delete]").forEach(button => button.addEventListener("click", () => void deleteStudioResourceCandidate(button.dataset.studioCandidateDelete)));
    studio.querySelectorAll("[data-studio-candidate-preview]").forEach(button => button.addEventListener("click", () => void openStudioAssetPreview(button.dataset.studioCandidatePreview)));
    studio.querySelector("[data-studio-preview-close]")?.addEventListener("click", closeStudioAssetPreview);
    studio.querySelector("[data-studio-preview-backdrop]")?.addEventListener("click", event => { if (event.target === event.currentTarget) closeStudioAssetPreview(); });
    studio.querySelector("[data-studio-reference-clear]")?.addEventListener("click", () => void clearStudioReference());
    studio.querySelectorAll("[data-studio-reference-remove]").forEach(button => button.addEventListener("click", () => void removeAiReference(Number(button.dataset.studioReferenceRemove))));
    studio.addEventListener("paste", event => void pasteStudioReferences(event));
    const resourceSlot = studio.querySelector("[data-studio-plan-slot]");
    resourceSlot?.addEventListener("change", event => {
      studioResourceSlot = event.target.value;
      studioResourceError = "";
      renderStudioCurrentResourceReference(studio, studioResourceSlot);
    });
    studio.querySelector("[data-studio-plan-prompt]")?.addEventListener("input", event => {
      studioResourcePrompt = event.target.value;
      if (!studioResourceError) return;
      studioResourceError = "";
      event.target.setAttribute("aria-invalid", "false");
      const error = studio.querySelector("[data-studio-plan-error]");
      if (error) { error.hidden = true; error.replaceChildren(); }
    });
    if (resourceSlot) renderStudioCurrentResourceReference(studio, resourceSlot.value);
    studio.querySelector("[data-studio-plan-form]")?.addEventListener("submit", event => {
      event.preventDefault();
      const slot = studio.querySelector("[data-studio-plan-slot]")?.value;
      const promptControl = studio.querySelector("[data-studio-plan-prompt]");
      const prompt = promptControl?.value.trim() || "";
      studioResourceSlot = slot || studioResourceSlot;
      studioResourcePrompt = promptControl?.value || "";
      if (!slot || !prompt) {
        studioResourceError = !slot ? "请选择要生成的资源类型" : "请输入资源提示词，说明构图、颜色、留白或用途";
        promptControl?.setAttribute("aria-invalid", "true");
        const error = studio.querySelector("[data-studio-plan-error]");
        if (error) { error.hidden = false; error.replaceChildren(studioResourceError); }
        setStatus(studioResourceError, "error");
        promptControl?.focus();
        return;
      }
      studioResourceError = "";
      const existing = aiResourcePlans.findIndex(plan => plan.slot === slot);
      const plan = { slot, prompt };
      if (existing >= 0) aiResourcePlans[existing] = plan;
      else if (aiResourcePlans.length < 16) aiResourcePlans.push(plan);
      else {
        studioResourceError = "批量生成计划最多添加 16 个资源";
        setStatus(studioResourceError, "error");
        renderStudio(false);
        return;
      }
      studioResourcePrompt = "";
      setStatus(existing >= 0 ? `${resourceSlotLabel(slot)}计划已更新` : `${resourceSlotLabel(slot)}已加入批量计划`, "success");
      renderStudio(false);
    });
    studio.querySelector("[data-studio-generate-resource]")?.addEventListener("click", () => void generateStudioResource(studioResourceSlot, studioResourcePrompt));
    studio.querySelector("[data-studio-generate-plans]")?.addEventListener("click", () => {
      if (busy || !aiResourcePlans.length) return;
      aiGenerateImages = true;
      const generateButton = studio.querySelector("[data-studio-generate]");
      if (generateButton) {
        generateButton.dataset.resourcePlansOnly = "true";
        generateButton.click();
      }
    });
    studio.querySelectorAll("[data-studio-edit-plan]").forEach(button => button.addEventListener("click", () => {
      const index = Number(button.dataset.studioEditPlan);
      const plan = aiResourcePlans[index];
      if (!plan) return;
      studioResourceSlot = plan.slot;
      studioResourcePrompt = plan.prompt;
      aiResourcePlans.splice(index, 1);
      studioResourceError = "";
      renderStudio(false);
      queueMicrotask(() => document.querySelector("[data-studio-plan-prompt]")?.focus());
    }));
    studio.querySelectorAll("[data-studio-remove-plan]").forEach(button => button.addEventListener("click", () => { aiResourcePlans.splice(Number(button.dataset.studioRemovePlan), 1); renderStudio(); }));
    studio.querySelectorAll("[data-studio-regenerate-resource]").forEach(button => button.addEventListener("click", () => void generateStudioResource(button.dataset.studioRegenerateResource, button.dataset.resourcePrompt || "")));
    studio.querySelector("[data-studio-generate]")?.addEventListener("click", event => {
      if (busy) return;
      if (event.currentTarget.dataset.readonly === "true" && event.isTrusted) return;
      aiResourcePlansOnly = event.currentTarget.dataset.resourcePlansOnly === "true";
      delete event.currentTarget.dataset.resourcePlansOnly;
      if (aiStudioMode === "generate" && !aiPrompt.trim() && !aiResourcePlans.length) {
        aiPromptError = "请输入主题需求后再开始生成";
        setStatus(aiPromptError, "error");
        renderStudio(false);
        queueMicrotask(() => document.querySelector("[data-studio-prompt]")?.focus());
        return;
      }
      const epoch = ++generationEpoch;
      if (aiStudioMode === "generate" && !aiResourcePlansOnly) resetGenerationWorkspace();
      busy = true;
      activeGenerationKind = "theme";
      aiGenerationProgress = { state: "planning", items: [] };
      aiRequestLog = [];
      aiGeneratedDraft = false;
      studioPreviewRefreshPending = false;
      studioPreviewReady = false;
      renderStudio();
      void pollGenerationProgress(epoch);
      void generateAiTheme(epoch).then(async () => {
        if (!generationIsActive(epoch)) return;
        const [progress, logs] = await Promise.all([call("ai.progress.get").catch(() => aiGenerationProgress), call("ai.log.get").catch(() => aiRequestLog)]);
        if (!generationIsActive(epoch)) return;
        const completedPlansOnly = aiResourcePlansOnly;
        busy = false;
        activeGenerationKind = "";
        aiResourcePlansOnly = false;
        aiGenerationProgress = progress;
        aiRequestLog = logs;
        studioPreviewRefreshPending = !completedPlansOnly;
        studioPreviewReady = false;
        clearTimeout(studioTimer);
        renderStudio(false);
      }).catch(async error => {
        if (!generationIsActive(epoch)) return;
        const progress = await call("ai.progress.get").catch(() => ({ ...aiGenerationProgress, state:"failed", message:error?.message || "生成失败" }));
        if (!generationIsActive(epoch)) return;
        const logs = await call("ai.log.get").catch(() => aiRequestLog);
        if (!generationIsActive(epoch)) return;
        busy = false;
        activeGenerationKind = "";
        aiResourcePlansOnly = false;
        aiGenerationProgress = { ...progress, state: "failed", message: progress.message || error?.message || "生成失败" };
        aiRequestLog = logs;
        clearTimeout(studioTimer);
        setStatus(error?.message || "生成失败", "error");
        renderStudio();
      });
    });
    studio.querySelectorAll("[data-color-path]").forEach(input => input.addEventListener("input", () => { setPath(draft, input.dataset.colorPath, input.value.toUpperCase()); markStudioDirty(); scheduleStudioPreview(studio); }));
    studio.querySelectorAll("[data-color-text]").forEach(input => input.addEventListener("change", () => { if (/^#[0-9a-f]{6}$/i.test(input.value.trim())) { setPath(draft, input.dataset.colorText, input.value.trim().toUpperCase()); markStudioDirty(); refreshStudioDom(studio, true); } }));
    studio.querySelectorAll("[data-path]").forEach(input => input.addEventListener(input.tagName === "SELECT" || input.type === "checkbox" ? "change" : "input", () => {
      const value = input.type === "checkbox" ? input.checked : input.type === "number" || input.type === "range" ? Number(input.value) : input.value;
      setPath(draft, input.dataset.path, value);
      markStudioDirty();
      applyTheme(draft, "");
      if (input.type === "checkbox" || input.tagName === "SELECT") renderStudio();
      else scheduleStudioPreview(studio);
    }));
    studio.querySelectorAll("[data-studio-meta]").forEach(input => input.addEventListener("input", () => {
      if (input.dataset.studioMeta === "name") aiThemeName = input.value;
      else aiThemeDescription = input.value;
    }));
    studio.querySelectorAll("[data-studio-select-decoration]").forEach(button => button.addEventListener("click", () => {
      studioSelectedDecoration = Number(button.dataset.studioSelectDecoration);
      syncStudioDecorationEditor(studio, studioSelectedDecoration);
    }));
    studio.querySelectorAll("[data-studio-decoration-path]").forEach(input => input.addEventListener(input.tagName === "SELECT" ? "change" : "input", () => {
      const decoration = draft.skin.decorations[studioSelectedDecoration];
      if (!decoration) return;
      decoration[input.dataset.studioDecorationPath] = input.tagName === "SELECT" ? input.value : Number(input.value);
      markStudioDirty();
      renderStudioDecorations(studio);
      syncStudioDecorationEditor(studio, studioSelectedDecoration);
    }));
  }

  async function refreshReferencePreview(item) {
    const url = await Promise.race([
      previewDataUrl(item, 2),
      new Promise(resolve => setTimeout(() => resolve(""), 8000)),
    ]);
    if (destroyed || !aiReferences.includes(item)) return;
    item.previewUrl = url || "";
    item.previewState = url ? "loaded" : "failed";
    if (studioOpen) refreshStudioReferenceList();
    else if (document.getElementById(PANEL_ID)?.dataset.open === "true") renderPanel();
  }

  async function generateStudioResource(slot, prompt) {
    if (busy) return;
    slot ||= studioResourceSlot;
    prompt = String(prompt || "").trim();
    if (!slot || !prompt) {
      studioResourceError = !slot ? "请选择要生成的资源类型" : "请输入资源提示词，说明构图、颜色、留白或用途";
      studioTab = "resources";
      setStatus(studioResourceError, "error");
      renderStudio(false);
      return;
    }
    studioResourceSlot = slot;
    studioResourcePrompt = prompt;
    studioResourceError = "";
    const epoch = ++generationEpoch;
    clearTimeout(studioTimer);
    busy = true;
    activeGenerationKind = "resource";
    const label = resourceSlotLabel(slot);
    const existingItem = (aiGenerationProgress.items || []).find(item => item.slot === slot);
    const failedPreview = existingItem?.status === "failed" && existingItem.session && existingItem.path
      ? { session: existingItem.session, path: existingItem.path }
      : null;
    const generatingItem = { slot, label, prompt, status: "generating", previewUrl: "", session: "", path: "", message: "正在准备参考图并请求图片模型", referenceIndexes: [] };
    if (existingItem) Object.assign(existingItem, generatingItem); else (aiGenerationProgress.items ||= []).push(generatingItem);
    aiGenerationProgress.state = "generating";
    aiGenerationProgress.message = `正在生成${label}`;
    aiRequestLog = [];
    studioTab = "resources";
    renderStudio();
    try {
      if (failedPreview) await cancelPreviewSessions([failedPreview]);
      await saveAiSettings();
      if (!generationIsActive(epoch)) return;
      if (!state.ai?.hasImageApiKey) throw new Error("请先保存生图 Key");
      const useCurrentResourceReference = aiStudioMode === "edit" && aiUseCurrentResourceReference && currentResourcePaths(draft, slot).length > 0;
      studioTimer = setTimeout(() => void pollGenerationProgress(epoch), 300);
      const result = await call("ai.resource.generate", { theme: draft, slot, prompt, useCurrentResourceReference, references: studioResourceReferences(slot, useCurrentResourceReference) });
      if (!generationIsActive(epoch)) return;
      const previewUrl = await previewDataUrl(result.asset);
      if (!generationIsActive(epoch)) return;
      if (!previewUrl && !result.asset.previewUrl) throw new Error("候选资源预览加载失败，请重新生成");
      pendingStudioResource = { id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`, slot, prompt, theme: result.theme, asset: result.asset, assets: result.assets || [result.asset], previewUrl: previewUrl || result.asset.previewUrl, state: "candidate" };
      studioResourceCandidates.unshift(pendingStudioResource);
      while (studioResourceCandidates.length > 12) {
        const removed = studioResourceCandidates.pop();
        if (removed?.state !== "applied") void cancelPreviewSessions(removed.assets || [removed.asset]);
      }
      aiGenerationProgress.state = "completed";
      aiGenerationProgress.message = `${resourceSlotLabel(slot)}候选资源已生成，等待确认`;
      aiRequestLog = await call("ai.log.get").catch(() => aiRequestLog);
      setStatus(`${resourceSlotLabel(slot)}候选资源已生成，请选择应用或放弃`, "success");
    } catch (error) {
      const [progress, logs] = await Promise.all([
        call("ai.progress.get").catch(() => aiGenerationProgress),
        call("ai.log.get").catch(() => aiRequestLog),
      ]);
      if (destroyed || epoch !== generationEpoch) return;
      const message = error?.message || "资源生成失败";
      aiGenerationProgress = { ...progress, state: "failed", message: progress.message || message };
      let failedItem = (aiGenerationProgress.items || []).find(item => item.slot === slot);
      if (!failedItem) {
        failedItem = { ...generatingItem };
        (aiGenerationProgress.items ||= []).push(failedItem);
      }
      failedItem.status = "failed";
      failedItem.message = failedItem.message && failedItem.message !== generatingItem.message ? failedItem.message : message;
      aiRequestLog = logs.length ? logs : [{ timestampMs: Date.now(), stage: `生成${label}`, endpoint: "local://theme-inject/workbench", model: "工作台", outcome: "error", httpStatus: null, durationMs: 0, message }];
      studioTab = "logs";
      setStatus(message, "error");
    } finally {
      if (epoch === generationEpoch) {
        clearTimeout(studioTimer);
        studioTimer = 0;
        busy = false;
        activeGenerationKind = "";
        renderStudio(false);
      }
    }
  }

  function studioResourceCandidate(id) {
    return studioResourceCandidates.find(candidate => candidate.id === id) || null;
  }

  async function openStudioAssetPreview(id) {
    const candidate = studioResourceCandidate(id);
    if (!candidate) return;
    studioAssetPreview = { id, title: resourceSlotLabel(candidate.slot), url: candidate.fullPreviewUrl || candidate.previewUrl, loading: !candidate.fullPreviewUrl };
    renderStudio(false);
    if (candidate.fullPreviewUrl) return;
    try {
      const result = await call("theme.preview.data", { session: candidate.asset.session, path: candidate.asset.path });
      if (!isPreviewDataUrl(result.url) || studioAssetPreview?.id !== id) return;
      candidate.fullPreviewUrl = result.url;
      studioAssetPreview.url = result.url;
      studioAssetPreview.loading = false;
      const studio = document.querySelector(".ti-studio-backdrop");
      const image = studio?.querySelector(".ti-studio-asset-preview img");
      const status = studio?.querySelector("[data-studio-preview-status]");
      if (image) image.src = result.url;
      status?.replaceChildren(t("点击背景或关闭按钮返回"));
    } catch {
      document.querySelector("[data-studio-preview-status]")?.replaceChildren(t("原图加载失败，当前显示缩略图"));
    }
  }

  function closeStudioAssetPreview() {
    studioAssetPreview = null;
    document.querySelector("[data-studio-preview-backdrop]")?.remove();
  }

  async function deleteStudioResourceCandidate(id) {
    const candidate = studioResourceCandidate(id);
    if (!candidate) return;
    if (candidate.state === "applied") undoStudioResource(id);
    studioResourceCandidates = studioResourceCandidates.filter(item => item !== candidate);
    if (pendingStudioResource === candidate) pendingStudioResource = studioResourceCandidates[0] || null;
    await cancelPreviewSessions(candidate.assets || [candidate.asset]);
    renderStudio(false);
  }

  function undoStudioResource(id) {
    const candidate = studioResourceCandidate(id);
    if (!candidate?.rollbackDraft) return;
    draft = clone(candidate.rollbackDraft);
    stagingAssets = clone(candidate.rollbackStagingAssets || []);
    dirty = Boolean(candidate.rollbackDirty);
    candidate.state = "candidate";
    candidate.rollbackDraft = null;
    candidate.rollbackStagingAssets = null;
    applyTheme(draft, state?.customCssTrusted ? draftCustomCss : "");
    commitStudioBaseline();
    setStatus(`${resourceSlotLabel(candidate.slot)}已从草稿撤销，候选记录仍保留`, "success");
    renderStudioDecorations(document.querySelector(".ti-studio-backdrop"));
    renderStudio(false);
  }

  async function applyStudioResource(save = false, id = pendingStudioResource?.id) {
    const pending = studioResourceCandidate(id) || pendingStudioResource;
    if (!pending) return;
    const assets = (pending.assets || [pending.asset]).filter(asset => asset.slot !== "skin.icons.atlas");
    const decorationAssets = assets.filter(asset => asset.slot.startsWith("skin.decorations."));
    const urls = await loadStagedAssetDataUrls(decorationAssets);
    const unavailable = decorationAssets.filter(asset => !urls.has(asset.path));
    if (unavailable.length) {
      setStatus(`${resourceSlotLabel(unavailable[0].slot)}原图加载失败，候选暂未应用`, "error");
      return;
    }
    pending.rollbackDraft = clone(draft);
    pending.rollbackStagingAssets = clone(stagingAssets);
    pending.rollbackDirty = dirty;
    const slots = new Set(assets.map(asset => asset.slot));
    mergeGeneratedAssets(pending.theme, assets);
    stagingAssets = stagingAssets.filter(asset => !slots.has(asset.slot));
    assets.forEach(asset => {
      stagingAssets.push({ slot: asset.slot, session: asset.session, path: asset.path });
      stagedUrls.set(asset.path, urls.get(asset.path) || asset.previewUrl || (asset.path === pending.asset.path ? pending.previewUrl : ""));
    });
    pending.fullPreviewUrl = urls.get(pending.asset.path) || pending.fullPreviewUrl;
    const atlas = (pending.assets || []).find(asset => asset.slot === "skin.icons.atlas");
    if (atlas) void call("theme.preview.cancel", { session: atlas.session }).catch(() => {});
    const existingItem = (aiGenerationProgress.items || []).find(item => item.slot === pending.slot);
    const item = { slot: pending.slot, label: resourceSlotLabel(pending.slot), prompt: pending.prompt, status: "completed", previewUrl: pending.previewUrl, session: pending.asset.session, path: pending.asset.path, message: "已应用到当前草稿" };
    if (existingItem) Object.assign(existingItem, item); else (aiGenerationProgress.items ||= []).push(item);
    studioResourceCandidates.filter(candidate => candidate !== pending && candidate.slot === pending.slot && candidate.state === "applied").forEach(candidate => { candidate.state = "candidate"; });
    pending.state = "applied";
    pendingStudioResource = pending;
    if (pending.slot.startsWith("skin.decorations.")) studioTab = "decorations";
    markStudioDirty();
    applyTheme(draft, state?.customCssTrusted ? draftCustomCss : "");
    commitStudioBaseline();
    studioPreviewRefreshPending = true;
    setStatus(`${resourceSlotLabel(item.slot)}已应用`, "success");
    renderStudio(false);
    if (save) void persistStudioDraft();
  }

  async function persistStudioDraft() {
    if (busy || !dirty) return;
    busy = true;
    renderStudio(false);
    try {
      await applyDraft(false);
      studioBaseline = { draft: clone(draft), customCss: draftCustomCss, stagingAssets: [], dirty: false, customCssTrusted: Boolean(state.customCssTrusted) };
      setStatus("主题资源已保存并启用", "success");
      studioPreviewRefreshPending = true;
      studioPreviewReady = false;
    } catch (error) {
      setStatus(error?.message || "保存并启用失败", "error");
    } finally {
      busy = false;
      renderStudio(true);
    }
  }

  async function persistStudioDecorations() {
    if (busy || !draft?.skin) return;
    const button = document.querySelector("[data-studio-decoration-save]");
    const status = document.querySelector("[data-studio-decoration-save-status]");
    const base = clone(snapshot || state?.activeTheme || draft);
    const decorations = clone((draft.skin.decorations || []).filter(item => item?.asset));
    const next = clone(base);
    next.skin ||= {};
    next.skin.enabled = Boolean(draft.skin.enabled);
    next.skin.decorations = decorations;
    const referenced = new Set(decorations.map(item => item.asset).filter(Boolean));
    const assets = stagingAssets.filter(asset => referenced.has(asset.path));
    busy = true;
    if (button) { button.disabled = true; button.textContent = "正在保存位置…"; }
    if (status) status.textContent = "正在保存装饰层位置…";
    try {
      const result = await call("theme.apply", {
        theme: next,
        customCss: state.customCss || "",
        enableCustomCss: Boolean(state.customCssTrusted),
        stagingAssets: assets.map(({ session, path }) => ({ session, path })),
      });
      const previousDraft = draft;
      state = result.state;
      snapshot = clone(result.theme);
      const preserved = clone(previousDraft);
      preserved.skin = clone(preserved.skin || {});
      preserved.skin.enabled = result.theme.skin.enabled;
      preserved.skin.decorations = clone(result.theme.skin.decorations || []);
      draft = preserved;
      stagingAssets = stagingAssets.filter(asset => !assets.some(saved => saved.session === asset.session && saved.path === asset.path));
      assets.forEach(asset => { stagedUrls.delete(asset.path); thumbnailUrls.delete(asset.path); });
      dirty = JSON.stringify(draft) !== JSON.stringify(snapshot) || Boolean(draftCustomCss !== (state.customCss || ""));
      studioBaseline = { draft: clone(draft), customCss: draftCustomCss, stagingAssets: clone(stagingAssets), dirty, customCssTrusted: Boolean(state.customCssTrusted) };
      applyTheme(draft, state.customCssTrusted ? draftCustomCss : "");
      if (status) status.textContent = dirty ? "装饰层已保存，其他工作台草稿仍未应用。" : "装饰层位置已保存。";
      if (button) button.textContent = "已保存装饰位置";
      markStudioDirtyIfNeeded();
    } catch (error) {
      if (status) status.textContent = error?.message || "保存装饰位置失败";
      if (button) button.textContent = "保存装饰位置";
    } finally {
      busy = false;
      if (button) button.disabled = false;
    }
  }

  function markStudioDirtyIfNeeded() {
    if (JSON.stringify(draft) !== JSON.stringify(snapshot) || draftCustomCss !== (state?.customCss || "")) {
      dirty = true;
      statusMessage = "装饰层已保存，工作台仍有其他未应用修改";
      statusKind = "";
    }
  }

  function commitStudioBaseline() {
    studioBaseline = {
      draft: clone(draft),
      customCss: draftCustomCss,
      stagingAssets: clone(stagingAssets),
      dirty,
      customCssTrusted: Boolean(state.customCssTrusted),
    };
  }

  function syncStudioDecorationEditor(studio, index) {
    const decoration = draft.skin?.decorations?.[index];
    if (!studio || !decoration) return;
    studio.querySelector('[data-studio-section="decorations"] .ti-studio-form > strong')?.replaceChildren(t(`装饰 ${index + 1}`));
    studio.querySelectorAll("[data-studio-decoration-path]").forEach(input => {
      const value = decoration[input.dataset.studioDecorationPath];
      if (String(input.value) !== String(value)) input.value = value;
    });
    studio.querySelectorAll("[data-studio-select-decoration]").forEach(button => {
      const selected = Number(button.dataset.studioSelectDecoration) === index;
      button.dataset.status = selected ? "completed" : "configured";
      const item = draft.skin.decorations[Number(button.dataset.studioSelectDecoration)];
      button.querySelector(".ti-resource-info span")?.replaceChildren(`${item.region} · ${item.offsetX}, ${item.offsetY}`);
    });
    studio.querySelectorAll(".ti-studio-decoration[data-decoration-index]").forEach(image => {
      image.dataset.selected = String(Number(image.dataset.decorationIndex) === index);
    });
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
      syncStudioDecorationEditor(node.closest(".ti-studio-backdrop"), index);
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
      syncStudioDecorationEditor(document.querySelector(".ti-studio-backdrop"), index);
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
      refreshStudioProgress();
    }
    if (generationIsActive(epoch) && studioOpen) studioTimer = setTimeout(() => void pollGenerationProgress(epoch), 500);
  }

  function refreshStudioProgress() {
    const studio = document.querySelector(".ti-studio-backdrop");
    if (!studio) return;
    const root = document.createElement("div");
    root.innerHTML = studioHtml();
    const next = root.firstElementChild;
    const pairs = [
      [studio.querySelector(".ti-generation-overview"), next.querySelector(".ti-generation-overview")],
      [studio.querySelector("[data-studio-active-resources]"), next.querySelector("[data-studio-active-resources]")],
      [studio.querySelector('[data-studio-section="logs"] .ti-ai-log'), next.querySelector('[data-studio-section="logs"] .ti-ai-log')],
    ];
    const currentResources = studio.querySelector('[data-studio-section="resources"]');
    const nextResources = next.querySelector('[data-studio-section="resources"]');
    pairs.push([currentResources?.querySelectorAll(".ti-resource-list")?.item(currentResources.querySelectorAll(".ti-resource-list").length - 1), nextResources?.querySelectorAll(".ti-resource-list")?.item(nextResources.querySelectorAll(".ti-resource-list").length - 1)]);
    pairs.forEach(([current, replacement]) => { if (current && replacement) current.replaceWith(localizeElement(replacement)); });
    const footer = studio.querySelector(".ti-studio-footer");
    const nextFooter = next.querySelector(".ti-studio-footer");
    if (footer && nextFooter) {
      const currentCopy = footer.querySelector("div");
      const nextCopy = nextFooter.querySelector("div");
      if (currentCopy && nextCopy) currentCopy.replaceChildren(...[...nextCopy.childNodes].map(node => node.cloneNode(true)));
      const currentGenerate = footer.querySelector("[data-studio-generate]");
      const nextGenerate = nextFooter.querySelector("[data-studio-generate]");
      if (currentGenerate && nextGenerate) {
        if (currentGenerate.dataset.readonly === "true") currentGenerate.replaceChildren(...[...nextGenerate.childNodes].map(node => node.cloneNode(true)));
        else currentGenerate.textContent = nextGenerate.textContent;
      }
    }
    localizeElement(footer);
    bindProgressResourceButtons(studio);
  }

  function bindProgressResourceButtons(studio) {
    studio.querySelectorAll("[data-studio-regenerate-resource]").forEach(button => {
      if (button.dataset.bound === "true") return;
      button.dataset.bound = "true";
      button.addEventListener("click", () => void generateStudioResource(button.dataset.studioRegenerateResource, button.dataset.resourcePrompt || ""));
    });
  }

  async function chooseStudioReference() {
    if (busy) return;
    try {
      const result = await call("ai.reference.import");
      await addAiReferences(result.references || [result]);
      renderStudio(false);
    } catch (error) {
      setStatus(error?.message || "参考图加载失败", "error");
      renderStudio(false);
    }
  }

  async function clearStudioReference() {
    await clearAiReference(false);
    renderStudio(false);
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
      renderStudio(false);
    } catch (error) {
      setStatus(error?.message || "粘贴参考图失败", "error");
      renderStudio(false);
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
    aiGeneratedDraft = aiStudioMode === "generate";
    statusMessage = "工作台中有未应用修改";
    statusKind = "";
  }
