  // Runtime module: panel-ui
  function ensurePanelStyles() {
    if (destroyed) return;
    let style = document.getElementById(PANEL_STYLE_ID);
    if (!style) { style = document.createElement("style"); style.id = PANEL_STYLE_ID; document.documentElement.appendChild(style); }
    if (style.textContent !== PANEL_CSS) style.textContent = PANEL_CSS;
  }

  function panelHtml() {
    return `<div class="ti-header"><div class="ti-header-brand"><img class="ti-brand-watermark" src="${TOOLBAR_ICON}" alt="Theme Inject" title="Theme Inject"><div><h2 class="ti-title">Codex 主题</h2><div class="ti-subtitle">实时定制 Codex 视觉系统</div></div></div><div class="ti-header-actions">${state?.development ? `<button class="ti-icon-button" data-action="reinject" title="构建备用开发版本，重启后重新注入">重新注入</button>` : ""}<button class="ti-icon-button" data-action="close">关闭</button></div></div><div class="ti-tabs">${tabs.map(([id, label]) => `<button class="ti-tab" data-tab="${id}" data-active="${id === activeTab}">${label}</button>`).join("")}</div><div class="ti-body">${tabs.map(([id]) => `<section class="ti-section" data-section="${id}" data-active="${id === activeTab}">${sectionHtml(id)}</section>`).join("")}</div><div class="ti-footer"><div class="ti-status" data-kind="${escapeHtml(statusKind)}">${escapeHtml(statusMessage || (dirty ? "有未应用修改" : "已同步"))}</div><div class="ti-footer-actions"><button class="ti-button" data-action="cancel">放弃</button><button class="ti-button" data-primary="true" data-action="apply">应用</button></div></div>${modalHtml()}`;
  }

  function modalHtml() {
    if (!modal) return "";
    if (modal.kind === "create") return `<div class="ti-modal-backdrop"><form class="ti-modal" data-modal-form="create"><div class="ti-modal-title">新建主题</div><div class="ti-modal-copy">以“${escapeHtml(draft.name)}”为基础创建一个可编辑副本。</div><label class="ti-field-label" for="ti-theme-name">主题名称</label><input id="ti-theme-name" class="ti-control" data-modal-name maxlength="80" value="${escapeHtml(modal.defaultValue)}" autocomplete="off"><div class="ti-modal-actions"><button class="ti-button" type="button" data-modal-cancel>取消</button><button class="ti-button" data-primary="true" type="submit">创建并切换</button></div></form></div>`;
    if (modal.kind === "delete") return `<div class="ti-modal-backdrop"><div class="ti-modal"><div class="ti-modal-title">删除主题</div><div class="ti-modal-copy">确定删除“${escapeHtml(modal.theme.name)}”？此操作无法撤销。</div><div class="ti-modal-actions"><button class="ti-button" type="button" data-modal-cancel>取消</button><button class="ti-button" data-danger="true" type="button" data-modal-confirm="delete">删除</button></div></div></div>`;
    if (modal.kind === "edit-theme") return `<div class="ti-modal-backdrop"><form class="ti-modal" data-modal-form="edit-theme"><div class="ti-modal-title">编辑主题信息</div><div class="ti-modal-copy">修改主题库中显示的名称和介绍。</div><label class="ti-field-label" for="ti-edit-theme-name">主题名称</label><input id="ti-edit-theme-name" class="ti-control" data-modal-theme-name maxlength="80" value="${escapeHtml(modal.theme.name)}" autocomplete="off"><label class="ti-field-label ti-section-gap" for="ti-edit-theme-description">主题介绍</label><textarea id="ti-edit-theme-description" class="ti-control ti-modal-textarea" data-modal-theme-description maxlength="240">${escapeHtml(modal.theme.description || "")}</textarea><div class="ti-modal-actions"><button class="ti-button" type="button" data-modal-cancel>取消</button><button class="ti-button" data-primary="true" type="submit">保存</button></div></form></div>`;
    return `<div class="ti-modal-backdrop"><div class="ti-modal"><div class="ti-modal-title">未应用的修改</div><div class="ti-modal-copy">关闭前要应用当前主题修改吗？</div><div class="ti-modal-actions ti-modal-actions-split"><button class="ti-button" type="button" data-modal-cancel>继续编辑</button><button class="ti-button" type="button" data-modal-confirm="discard">放弃并关闭</button><button class="ti-button" data-primary="true" type="button" data-modal-confirm="apply">应用并关闭</button></div></div></div>`;
  }

  function syncNativeSurfaceContrast() {
    document.querySelectorAll('[data-theme-inject-light-surface="true"]').forEach(node => node.removeAttribute("data-theme-inject-light-surface"));
    document.querySelectorAll(".main-surface section,.main-surface form,.main-surface [role=group],.main-surface div").forEach(node => {
      if (node.matches("[data-theme-inject-right-sidebar],[data-theme-inject-right-sidebar-item]") || node.closest("[data-theme-inject-right-sidebar-item=true]")) return;
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
      if (node.closest(`#${PANEL_ID},.ti-studio-backdrop,[data-theme-inject-right-sidebar=true],[data-theme-inject-project-actions=true]`) || node.matches('[data-theme-inject-custom-icon],[data-theme-inject-right-sidebar-item=true],[data-theme-inject-project-action=true]')) return;
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
      if (studioPreviewReady && !destroyed && studioOpen && studio?.isConnected) {
        applyTheme(draft, "");
        refreshStudioDom(studio, true);
        renderStudioDecorations(studio);
      }
    }, delay);
  }

  function studioCandidateHistoryHtml() {
    if (!studioResourceCandidates.length) return "";
    return `<div class="ti-resource-candidate"><div class="ti-section-title">候选记录 · ${studioResourceCandidates.length}</div><div class="ti-resource-list">${studioResourceCandidates.map(candidate => {
      const applied = candidate.state === "applied";
      return `<div class="ti-resource-item" data-status="${applied ? "completed" : "configured"}"><button class="ti-resource-preview ti-resource-preview-button" type="button" data-studio-candidate-preview="${escapeHtml(candidate.id)}" data-empty="${!candidate.previewUrl}" style="${candidate.previewUrl ? `background-image:url(&quot;${escapeHtml(candidate.previewUrl)}&quot;)` : ""}" title="查看大图"></button><div class="ti-resource-info"><strong>${escapeHtml(resourceSlotLabel(candidate.slot))}</strong><span>${escapeHtml(candidate.prompt)}</span><span>${applied ? "已应用到当前草稿" : "候选已保留，可再次使用"}</span></div><div class="ti-inline">${applied ? `<button class="ti-button" type="button" data-studio-candidate-undo="${escapeHtml(candidate.id)}">撤销应用</button>` : `<button class="ti-button" type="button" data-studio-candidate-apply="${escapeHtml(candidate.id)}">应用到草稿</button>`}<button class="ti-button" type="button" data-studio-candidate-preview="${escapeHtml(candidate.id)}">查看大图</button><button class="ti-icon-button" type="button" data-studio-candidate-delete="${escapeHtml(candidate.id)}" title="删除候选记录和临时资源">删除</button></div></div>`;
    }).join("")}</div></div>`;
  }

  function studioHtml() {
    const progressItems = aiGenerationProgress.items || [];
    const completedCount = progressItems.filter(item => item.status === "completed").length;
    const failedCount = progressItems.filter(item => item.status === "failed").length;
    const activeItems = progressItems.filter(item => item.status === "generating");
    const totalCount = progressItems.length;
    const settling = busy && ["completed", "finalizing"].includes(aiGenerationProgress.state);
    const progressPercent = totalCount ? (settling ? Math.min(98, Math.round(completedCount / totalCount * 100)) : Math.round(completedCount / totalCount * 100)) : aiGenerationProgress.state === "planning" ? 8 : 0;
    const phase = generationPhase(aiGenerationProgress.state, aiGenerationProgress.message, activeItems, completedCount, totalCount, failedCount, settling);
    const generationControl = aiStudioMode === "generate"
      ? `<button class="ti-button" data-primary="true" data-studio-generate ${busy ? "disabled" : ""}>${busy ? `${progressPercent}% 生成中` : "开始生成"}</button>`
      : `<div class="ti-studio-generation-state" data-studio-generate data-readonly="true" aria-live="polite"><strong>${escapeHtml(busy ? phase.title : "编辑模式")}</strong><span>${escapeHtml(busy ? phase.footer : "仅调整当前主题，不会重新生成主题")}</span></div>`;
    const latestActivity = aiRequestLog.at(-1);
    const plans = aiResourcePlans.map((plan, index) => `<div class="ti-resource-item" data-status="configured"><div class="ti-resource-preview"></div><div class="ti-resource-info"><strong>${escapeHtml(resourceSlotLabel(plan.slot))}</strong><span title="${escapeHtml(plan.prompt)}">${escapeHtml(plan.prompt)}</span><span>${aiReferences.length ? `随计划使用当前 ${aiReferences.length} 张参考图` : "不使用额外参考图"}</span></div><div class="ti-inline"><button class="ti-icon-button" type="button" data-studio-edit-plan="${index}" title="载入此计划以修改资源类型或提示词">编辑</button><button class="ti-icon-button" type="button" data-studio-remove-plan="${index}" title="从批量计划中移除此资源">删除</button></div></div>`).join("");
    const progress = progressItems.map(item => { const previewUrl = thumbnailUrls.get(item.path) || item.previewUrl || "", canGenerate = studioResourceSlotOptions().some(([slot]) => slot === item.slot), references = Array.isArray(item.referenceIndexes) && item.referenceIndexes.length ? ` · 参考图 ${item.referenceIndexes.join("、")}` : ""; return `<div class="ti-resource-item" data-status="${escapeHtml(item.status)}"><div class="ti-resource-preview" data-empty="${!previewUrl}" style="${previewUrl ? `background-image:url(&quot;${escapeHtml(previewUrl)}&quot;)` : ""}"></div><div class="ti-resource-info"><strong>${escapeHtml(item.label || resourceSlotLabel(item.slot))}</strong><span title="${escapeHtml(item.message || item.prompt)}">${escapeHtml(item.message || item.prompt)}${escapeHtml(references)}</span></div>${canGenerate ? `<button class="ti-button" data-studio-regenerate-resource="${escapeHtml(item.slot)}" data-resource-prompt="${escapeHtml(item.prompt)}" title="重新生成候选资源；确认应用到草稿后，仍需保存并启用或在外层点击应用" ${busy ? "disabled" : ""}>${item.status === "completed" ? "重新生成" : "生成"}</button>` : `<div class="ti-resource-state">${escapeHtml(resourceStatusLabel(item.status))}</div>`}</div>`; }).join("");
    const reference = `<div data-studio-reference-list>${studioReferenceListHtml()}</div>`;
    const logs = aiRequestLog.length ? aiRequestLog.slice().reverse().map(item => { const local = String(item.endpoint || "").startsWith("local://"); const result = local ? (item.outcome === "error" ? "本地处理失败" : item.outcome === "retrying" ? "自动重试" : item.outcome === "processing" ? "本地处理中" : "本地处理完成") : `${item.httpStatus || "网络错误"} · ${item.durationMs} ms`; return `<div class="ti-ai-log-item" data-outcome="${escapeHtml(item.outcome)}"><div><strong>${escapeHtml(item.stage)}</strong><span>${escapeHtml(item.model)} · ${escapeHtml(result)}</span></div><code>${escapeHtml(item.endpoint)}</code>${item.message && item.message !== "请求成功" ? `<p>${escapeHtml(item.message)}</p>` : ""}</div>`; }).join("") : `<div class="ti-studio-empty">开始生成后，这里会实时显示图片请求、绿幕抠图和资源保存进度。</div>`;
    const decorations = draft?.skin?.decorations || [];
    const selected = decorations[studioSelectedDecoration];
    const decorationEditor = selected ? `<div class="ti-studio-form"><strong>装饰 ${studioSelectedDecoration + 1}</strong>${row("区域", `<select class="ti-control" data-studio-decoration-path="region"><option value="sidebar" ${selected.region === "sidebar" ? "selected" : ""}>左侧导航</option><option value="content" ${selected.region === "content" || selected.region === "titlebar" ? "selected" : ""}>内容区</option><option value="composer" ${selected.region === "composer" ? "selected" : ""}>输入区</option></select>`)}${row("锚点", `<select class="ti-control" data-studio-decoration-path="anchor"><option value="top-left" ${selected.anchor === "top-left" ? "selected" : ""}>左上</option><option value="top-right" ${selected.anchor === "top-right" ? "selected" : ""}>右上</option><option value="bottom-left" ${selected.anchor === "bottom-left" ? "selected" : ""}>左下</option><option value="bottom-right" ${selected.anchor === "bottom-right" ? "selected" : ""}>右下</option></select>`)}${row("水平偏移", `<input class="ti-control" type="number" min="-2000" max="2000" data-studio-decoration-path="offsetX" value="${selected.offsetX}">`)}${row("垂直偏移", `<input class="ti-control" type="number" min="-2000" max="2000" data-studio-decoration-path="offsetY" value="${selected.offsetY}">`)}${row("宽度", `<input class="ti-control" type="number" min="16" max="1600" data-studio-decoration-path="width" value="${selected.width}">`)}${row("透明度", `<input class="ti-control" type="range" min="0" max="1" step=".01" data-studio-decoration-path="opacity" value="${selected.opacity}">`)}</div>` : `<div class="ti-studio-empty">在预览中点击装饰，或从下方列表选择。</div>`;
    return `<div class="ti-studio-backdrop"><div class="ti-studio-header"><div><div class="ti-studio-title">AI 主题生成工作台</div><div class="ti-studio-copy">需求、参考图、资源、日志与静态预览都在这里完成</div></div><div class="ti-inline"><button class="ti-button" data-studio-refresh>刷新 DOM 副本</button><button class="ti-button" data-studio-close ${busy ? "disabled title=\"生成完成后可关闭\"" : ""}>关闭</button></div></div><div class="ti-studio-main"><div class="ti-studio-canvas"><div class="ti-studio-frame-wrap"><iframe class="ti-studio-frame" sandbox="allow-same-origin" referrerpolicy="no-referrer" title="Codex 静态 DOM 预览"></iframe><div class="ti-studio-decorations"></div></div></div><aside class="ti-studio-sidebar"><div class="ti-studio-tabs"><button class="ti-studio-tab" data-studio-tab="generate" data-active="${studioTab === "generate"}">生成</button><button class="ti-studio-tab" data-studio-tab="resources" data-active="${studioTab === "resources"}">资源</button><button class="ti-studio-tab" data-studio-tab="logs" data-active="${studioTab === "logs"}">日志</button><button class="ti-studio-tab" data-studio-tab="theme" data-active="${studioTab === "theme"}">主题</button><button class="ti-studio-tab" data-studio-tab="decorations" data-active="${studioTab === "decorations"}">装饰</button></div><div class="ti-studio-pane"><section data-studio-section="generate" data-active="${studioTab === "generate"}"><div class="ti-studio-form"><strong>主题需求</strong><textarea class="ti-control ti-ai-prompt" data-studio-prompt maxlength="6000" placeholder="描述主题风格、颜色、氛围、组件和使用场景" ${busy ? "disabled" : ""}>${escapeHtml(aiPrompt)}</textarea><div class="ti-section-title">参考图</div>${reference}<button class="ti-button" type="button" data-studio-reference-select ${busy ? "disabled" : ""}>选择参考图</button><label class="ti-ai-checkbox"><input type="checkbox" data-studio-generate-images ${aiGenerateImages ? "checked" : ""} ${busy ? "disabled" : ""}><span>生成背景、Hero、装饰和动作/卡片图标</span></label></div><div class="ti-generation-overview" data-state="${escapeHtml(aiGenerationProgress.state || "idle")}"><div class="ti-generation-phase"><strong>${escapeHtml(phase.title)}</strong><span>${escapeHtml(phase.detail)}</span>${latestActivity ? `<span class="ti-generation-activity">最近活动：${escapeHtml(latestActivity.stage)} · ${escapeHtml(latestActivity.outcome)}${latestActivity.httpStatus ? ` · HTTP ${latestActivity.httpStatus}` : ""}</span>` : ""}</div><div class="ti-progress-track"><span style="width:${progressPercent}%"></span></div><div class="ti-generation-counts"><span>${completedCount}/${totalCount || "—"} 已完成</span>${failedCount ? `<span>${failedCount} 失败</span>` : ""}</div></div><div data-studio-active-resources>${activeItems.length ? `<div class="ti-section-title ti-section-gap">正在处理</div><div class="ti-resource-list">${activeItems.map(item => `<div class="ti-resource-item" data-status="generating"><div class="ti-resource-preview"></div><div class="ti-resource-info"><strong>${escapeHtml(item.label)}</strong><span>${escapeHtml(item.prompt)}</span></div><div class="ti-resource-state">生成中</div></div>`).join("")}</div>` : ""}</div></section><section data-studio-section="resources" data-active="${studioTab === "resources"}"><form class="ti-studio-form" data-studio-plan-form><strong>添加资源配置</strong><select class="ti-control" data-studio-plan-slot>${studioResourceSlotOptions().map(([value, label]) => `<option value="${value}">${escapeHtml(label)}</option>`).join("")}</select><textarea class="ti-control" data-studio-plan-prompt maxlength="2000" placeholder="描述这个资源的构图、颜色、留白和用途"></textarea><button class="ti-button" data-primary="true" type="submit">添加到生成计划</button></form><div class="ti-section-title">自定义计划</div><div class="ti-resource-list">${plans || `<div class="ti-studio-empty">尚未添加自定义资源，AI 仍会按蓝图生成默认资源。</div>`}</div><div class="ti-section-title ti-section-gap">全部资源 · ${completedCount}/${totalCount || "—"}</div><div class="ti-resource-list">${progress || `<div class="ti-studio-empty">开始生成后逐项显示状态。</div>`}</div></section><section data-studio-section="logs" data-active="${studioTab === "logs"}"><div class="ti-section-title">实时请求日志 · ${aiRequestLog.length}</div><div class="ti-ai-log">${logs}</div></section><section data-studio-section="theme" data-active="${studioTab === "theme"}"><div class="ti-studio-form"><strong>快速主题配置</strong>${row("内容背景", colorInput("tokens.contentBackground", draft.tokens.contentBackground))}${row("主要文字", colorInput("tokens.foreground", draft.tokens.foreground))}${row("强调色", colorInput("tokens.accent", draft.tokens.accent))}${row("圆角", rangeInput("shape.radius", draft.shape.radius, 0, 40, 1))}${row("侧栏宽度", numberInput("layout.sidebarWidth", draft.layout.sidebarWidth, 180, 520))}</div></section><section data-studio-section="decorations" data-active="${studioTab === "decorations"}">${decorationEditor}<div class="ti-resource-list">${decorations.map((item, index) => `<button class="ti-resource-item" data-studio-select-decoration="${index}" data-status="${index === studioSelectedDecoration ? "completed" : "configured"}"><span class="ti-resource-preview" style="background-image:url(&quot;${escapeHtml(resolvedAsset(draft, item.asset))}&quot;)"></span><span class="ti-resource-info"><strong>装饰 ${index + 1}</strong><span>${escapeHtml(item.region)} · ${item.offsetX}, ${item.offsetY}</span></span></button>`).join("") || `<div class="ti-studio-empty">先生成或添加装饰层。</div>`}</div></section></div><div class="ti-studio-footer"><div><strong>${escapeHtml(phase.title)}</strong><span class="ti-studio-copy">${escapeHtml(phase.footer)}</span></div>${generationControl}</div></aside></div></div>`;
  }

  function resourceSlotLabel(slot) { return resourceSlotOptions.find(([value]) => value === slot)?.[1] || slot; }
  function resourceStatusLabel(status) { return ({ queued:"等待", generating:"生成中", completed:"已完成", failed:"失败" })[status] || status; }
  function currentResourcePaths(theme, slot) {
    const one = value => typeof value === "string" && value ? [value] : [];
    if (slot === "background.fullscreen") return theme.background?.fullscreen?.kind === "image" ? one(theme.background.fullscreen.path) : [];
    if (slot === "background.content") return theme.background?.content?.kind === "image" ? one(theme.background.content.path) : [];
    if (slot === "background.sidebar") return theme.background?.sidebar?.kind === "image" ? one(theme.background.sidebar.path) : [];
    if (slot === "skin.icons.atlas") return skinIconFields.map(([action]) => theme.skin?.icons?.mappings?.[action]).filter(Boolean);
    if (slot.startsWith("skin.icons.")) {
      const action = slot.slice("skin.icons.".length), mappings = theme.skin?.icons?.mappings || {};
      return [mappings[action], ...skinIconFields.filter(([candidate]) => candidate !== action).map(([candidate]) => mappings[candidate])].filter((path, index, paths) => path && paths.indexOf(path) === index).slice(0, 6);
    }
    if (slot.startsWith("skin.decorations.")) {
      const index = Number(slot.split(".")[2]);
      return one(theme.skin?.decorations?.[index]?.asset);
    }
    const resourceKey = ({
      "skin.logo":"logo", "skin.sidebarWatermark":"sidebarWatermark", "skin.heroImage":"heroImage",
      "skin.heroBadge":"heroBadge", "skin.avatar":"avatar", "skin.accountAvatar":"accountAvatar", "skin.sticker":"sticker",
      "skin.composerDecoration":"composerDecoration",
    })[slot];
    return resourceKey ? one(theme.skin?.resources?.[resourceKey]) : [];
  }

  function studioResourceReferences(slot, useCurrent) {
    const references = useCurrent ? currentResourcePaths(draft, slot).map(path => {
      const staged = stagingAssets.find(asset => asset.path === path);
      return staged ? { session: staged.session, path: staged.path } : null;
    }).filter(Boolean) : [];
    references.push(...aiReferences.map(({ session, path }) => ({ session, path })));
    const unique = new Map(references.map(reference => [`${reference.session}:${reference.path}`, reference]));
    return [...unique.values()].slice(0, 6);
  }

  function renderStudioCurrentResourceReference(studio, slot) {
    const host = studio.querySelector("[data-studio-current-resource-reference]");
    if (!host) return;
    if (aiStudioMode !== "edit") { host.replaceChildren(); return; }
    const paths = currentResourcePaths(draft, slot);
    const previews = paths.map(path => resolvedAsset(draft, path)).filter(Boolean).slice(0, 4);
    const available = paths.length > 0;
    const detail = slot === "skin.icons.atlas" && paths.length
      ? `将从当前 ${paths.length} 个动作图标中选取代表图保持线条、配色和比例一致。`
      : slot.startsWith("skin.icons.") && paths.length
        ? `目标图标缺失时会从当前 ${paths.length} 个同组图标中提取线条、配色和比例，保持整套风格一致。`
      : available ? "生成时会将当前图片作为第一参考，优先保持视觉语言与构图。" : "当前槽位没有可用图片，将仅使用手动上传的参考图。";
    host.innerHTML = `<div class="ti-section-title">当前资源参考</div><label class="ti-ai-checkbox"><input type="checkbox" data-studio-use-current-resource ${aiUseCurrentResourceReference && available ? "checked" : ""} ${busy || !available ? "disabled" : ""}><span>使用当前资源保持风格一致（推荐）</span></label><div class="ti-resource-item" data-status="${available ? "completed" : "configured"}"><div class="ti-resource-preview" data-empty="${!previews.length}" style="${previews[0] ? `background-image:url(&quot;${escapeHtml(previews[0])}&quot;)` : ""}"></div><div class="ti-resource-info"><strong>${escapeHtml(resourceSlotLabel(slot))}</strong><span>${escapeHtml(detail)}</span></div>${previews.length > 1 ? `<div class="ti-resource-state">${previews.length} 张预览</div>` : ""}</div>`;
    host.querySelector("[data-studio-use-current-resource]")?.addEventListener("change", event => { aiUseCurrentResourceReference = event.target.checked; });
  }

  function studioReferenceListHtml() {
    return aiReferences.length ? `<div class="ti-reference-grid">${aiReferences.map((item, index) => { const loading = item.previewState === "loading"; const failed = item.previewState === "failed"; return `<div class="ti-ai-reference"><div class="ti-ai-reference-image" data-loading="${loading}" data-failed="${failed}" style="${item.previewUrl ? `background-image:url(&quot;${escapeHtml(item.previewUrl)}&quot;)` : ""}"></div><div><strong>${escapeHtml(item.path?.split("/").pop() || `参考图 ${index + 1}`)}</strong><span>${loading ? "正在读取预览…" : failed ? "预览读取失败，生成时仍会使用原图" : `参考图 ${index + 1}`}</span><button class="ti-button" type="button" data-studio-reference-remove="${index}">移除</button></div></div>`; }).join("")}</div>` : `<div class="ti-studio-empty">可选：添加参考图分析配色、氛围和层级。也可以 Ctrl+V 直接粘贴图片。</div>`;
  }

  function refreshStudioReferenceList() {
    document.querySelectorAll("[data-studio-reference-list],[data-studio-resource-reference-list]").forEach(host => {
      host.innerHTML = studioReferenceListHtml();
      host.querySelectorAll("[data-studio-reference-remove]").forEach(button => button.addEventListener("click", () => void removeAiReference(Number(button.dataset.studioReferenceRemove))));
    });
  }

  function generationPhase(state, message, active, completed, total, failed, settling = false) {
    if (settling || state === "finalizing") return { title:"正在整理生成结果", detail:message || "图片资源已完成，正在整理预览和草稿，马上就好。", footer:"正在收尾，不会重复生成图片" };
    if (state === "planning") return { title:"正在分析主题需求", detail:message || "基础/视觉模型正在生成主题蓝图，资源清单将在蓝图完成后出现。", footer:"主题蓝图分析中" };
    if (state === "generating") return { title:active.length ? `正在生成：${active.map(item => item.label).join("、")}` : "正在调度图片资源", detail:message ? `${message}；已完成 ${completed}/${total}` : `已完成 ${completed}/${total}，最多 4 路并发；预览 DOM 会在收尾后单独刷新。`, footer:`图片资源 ${completed}/${total}` };
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
    if (id === "effects") return effectsHtml();
    if (id === "terminal") return terminalHtml();
    if (id === "skin") return skinHtml();
    if (id === "ai") return aiHtml();
    if (id === "advanced") return languageHtml() + advancedHtml();
    return "";
  }

  function languageHtml() {
    const language = state.settings?.uiLanguage || "system";
    return `<details class="ti-card ti-config-card" open><summary>界面语言</summary>${row("显示语言", `<select class="ti-control" data-ui-language><option value="system" ${language === "system" ? "selected" : ""}>跟随系统</option><option value="zh-CN" ${language === "zh-CN" ? "selected" : ""}>简体中文</option><option value="en-US" ${language === "en-US" ? "selected" : ""}>English</option></select>`)}</details>`;
  }

  function advancedHtml() {
    const appearance = triggerAppearanceDraft || state.settings?.triggerAppearance || { shape:"rounded", size:40, right:18, bottom:18, backgroundOpacity:.88, shadowStrength:.35 };
    return `<details class="ti-card ti-config-card" open><summary>右下角入口</summary><div class="ti-trigger-preview"><span style="${triggerStyle(appearance)}"><img src="${escapeHtml(state.triggerIcon || TOOLBAR_ICON)}" alt=""></span><div><strong>入口预览</strong><div class="ti-description">标题中的品牌水印固定不变；这里只调整右下角入口。</div></div></div>${row("形状", `<select class="ti-control" data-trigger-setting="shape"><option value="rounded" ${appearance.shape === "rounded" ? "selected" : ""}>圆角方形</option><option value="circle" ${appearance.shape === "circle" ? "selected" : ""}>圆形</option><option value="square" ${appearance.shape === "square" ? "selected" : ""}>方形</option></select>`)}${triggerRangeRow("尺寸", "size", appearance.size, 32, 72, 1)}${triggerRangeRow("右侧距离", "right", appearance.right, 0, 160, 1)}${triggerRangeRow("底部距离", "bottom", appearance.bottom, 0, 160, 1)}${triggerRangeRow("背景透明度", "backgroundOpacity", appearance.backgroundOpacity, .35, 1, .01)}${triggerRangeRow("阴影强度", "shadowStrength", appearance.shadowStrength, 0, 1, .01)}<div class="ti-action-group"><button class="ti-button" type="button" data-trigger-icon-import>选择自定义图标</button>${state.triggerIcon ? `<button class="ti-button" type="button" data-trigger-icon-reset>恢复默认图标</button>` : ""}<button class="ti-button" data-primary="true" type="button" data-trigger-save>保存入口设置</button></div></details><div class="ti-notice ti-section-gap"><strong>如何捕捉 CSS 位置</strong><ol class="ti-css-guide"><li>按 F12 或 Ctrl+Shift+I 打开开发者工具。</li><li>按 Ctrl+Shift+C 后点击要修改的位置。</li><li>在 Elements 面板右键节点，选择 Copy → Copy selector，再粘贴到下方。</li></ol><button class="ti-button ti-section-gap" type="button" data-css-inspector>直接在页面捕捉元素</button><div class="ti-description" data-css-selector-output>也可点击上方按钮，再点击 Codex 页面中的目标位置，选择器会自动写入 CSS。</div></div><div class="ti-notice ti-section-gap">自定义 CSS 可以隐藏或伪造页面内容。只有你完全信任的本地主题才应启用。</div><label class="ti-inline" style="margin:12px 0"><input type="checkbox" data-custom-css-enabled ${state.customCssTrusted ? "checked" : ""}>启用并信任自定义 CSS</label><textarea class="ti-textarea" data-custom-css spellcheck="false" placeholder="/* 示例：粘贴捕捉到的选择器 */&#10;.your-selector {&#10;  /* 在此添加样式 */&#10;}">${escapeHtml(draftCustomCss)}</textarea>`;
  }

  function triggerRangeRow(label, key, value, min, max, step) { return row(label, `<div class="ti-inline"><input class="ti-control" type="range" data-trigger-setting="${key}" value="${value}" min="${min}" max="${max}" step="${step}"><output>${rangeValue(value, step)}</output></div>`); }
  function triggerStyle(appearance) { const radius = appearance.shape === "circle" ? "50%" : appearance.shape === "square" ? "4px" : "12px"; return `width:${appearance.size}px;height:${appearance.size}px;border-radius:${radius};background:color-mix(in srgb,var(--ti-elevated) ${Math.round(appearance.backgroundOpacity * 100)}%,transparent);box-shadow:0 8px 24px rgba(0,0,0,${(.36 * appearance.shadowStrength).toFixed(2)})`; }

  function libraryHtml() {
    const actions = `<div class="ti-inline" style="margin-bottom:10px;flex-wrap:wrap"><button class="ti-button" data-action="create">新建主题</button><button class="ti-button" data-action="import">安装 ZIP</button><button class="ti-button" data-action="open-ai">AI 生成主题</button></div>`;
    const cards = state.themes.map(theme => {
      const mode = theme.skin?.enabled ? "高级皮肤" : theme.background?.mode === "per-region" ? "分区背景" : activeBackgrounds(theme).some(([, background]) => hasBackground(background)) ? "全屏背景" : "";
      const switching = theme.id === switchingThemeId;
      return `<div class="ti-card ti-theme-card" data-theme-activate="${escapeHtml(theme.id)}" data-selected="${theme.id === draft.id}" data-switching="${switching}" aria-busy="${switching}">${themeCardThumbnailHtml(theme)}<div><div class="ti-name">${escapeHtml(theme.name)}${mode ? `<span class="ti-badge">${mode}</span>` : ""}</div><div class="ti-description">${switching ? "正在切换…" : escapeHtml(theme.description || theme.id)}</div></div><details class="ti-theme-menu"><summary title="主题操作" aria-label="主题操作">•••</summary><div class="ti-theme-menu-popover"><button type="button" data-theme-edit="${escapeHtml(theme.id)}">编辑名称和介绍</button><button type="button" data-export="${escapeHtml(theme.id)}">导出主题</button><button type="button" data-theme-delete="${escapeHtml(theme.id)}" ${theme.id.startsWith("builtin.") ? "disabled title=\"内置主题不能删除\"" : ""}>删除主题</button></div></details></div>`;
    }).join("");
    return `<div class="ti-notice" style="margin-bottom:10px">点击主题卡片即可切换；右侧菜单可编辑名称和介绍、导出或删除主题。</div>${actions}<div class="ti-grid">${cards}</div><div class="ti-inline" style="margin-top:10px"><button class="ti-button" data-action="clone">复制当前</button></div>`;
  }

  function themeCardThumbnailHtml(theme) {
    const fallback = `linear-gradient(135deg,${theme.tokens.sidebarBackground},${theme.tokens.accent})`;
    const heroPath = theme.skin?.resources?.heroImage;
    if (heroPath) {
      const key = `${theme.id}:${heroPath}`;
      const thumbnailUrl = themeThumbnailUrls.get(key) || "";
      return `<div class="ti-swatch ti-theme-thumbnail" data-loading="${!thumbnailUrl}" data-thumbnail-key="${escapeHtml(key)}" data-theme-id="${escapeHtml(theme.id)}" data-theme-hero="${escapeHtml(heroPath)}" style="background:${fallback}"><img ${thumbnailUrl ? `src="${escapeHtml(thumbnailUrl)}"` : ""} alt="" loading="lazy"></div>`;
    }
    const backgrounds = theme.background?.mode === "per-region"
      ? [theme.background?.content, theme.background?.sidebar, theme.background?.fullscreen]
      : [theme.background?.fullscreen, theme.background?.content, theme.background?.sidebar];
    const background = backgrounds.find(item => item?.kind === "image" && item.path);
    const path = background?.path;
    if (!path) return `<div class="ti-swatch" style="background:${fallback}"></div>`;
    const url = resolvedAsset(theme, path);
    const positionX = Number.isFinite(background?.positionX) ? background.positionX : 50;
    const positionY = Number.isFinite(background?.positionY) ? background.positionY : 50;
    return `<div class="ti-swatch ti-theme-thumbnail" data-loading="true" style="background:${fallback}"><img src="${escapeHtml(url)}" alt="" loading="lazy" data-fit="cover" style="object-position:${positionX}% ${positionY}%"></div>`;
  }

  function colorsHtml() {
    const warning = colorContrast(draft.tokens.border, draft.tokens.inputBackground) < 1.35 ? `<div class="ti-notice ti-notice-danger">边框与输入区颜色过于接近，运行时会自动提高边框对比度。</div>` : "";
    return `${warning}<div class="ti-notice">左侧导航是项目与任务列表；右侧边栏是空白任务页中的文件、新任务、浏览器和终端快捷操作列表。</div><div class="ti-section-title ti-section-gap">基础颜色</div><div class="ti-card">${colorFields.map(([key, label]) => row(label, colorInput(`tokens.${key}`, draft.tokens[key]))).join("")}</div><div class="ti-section-title ti-section-gap">标题栏与左侧导航</div><div class="ti-card">${chromeFields.map(([key, label]) => row(label, optionalColorInput(`chrome.${key}`, draft.chrome?.[key]))).join("")}</div><div class="ti-section-title ti-section-gap">右侧边栏快捷列表</div><div class="ti-card">${rightSidebarFields.map(([key, label]) => row(label, optionalColorInput(`chrome.${key}`, draft.chrome?.[key]))).join("")}</div>`;
  }

  function backgroundHtml() {
    const mode = draft.background?.mode || "fullscreen";
    const selector = row("配置模式", `<select class="ti-control" data-background-mode><option value="fullscreen" ${mode === "fullscreen" ? "selected" : ""}>全屏统一</option><option value="per-region" ${mode === "per-region" ? "selected" : ""}>分区独立</option></select>`);
    const body = mode === "per-region" ? backgroundCard("content", "内容区域") + backgroundCard("sidebar", "左侧导航区域") : backgroundCard("fullscreen", "全屏背景");
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
    return `<div class="ti-card">${[["background", "背景"], ["foreground", "文字"], ["cursor", "光标"], ["selection", "选择区域"]].map(([key, label]) => row(label, colorInput(`terminal.${key}`, draft.terminal[key]))).join("")}<div class="ti-section-title" style="margin-top:12px">ANSI 颜色</div><div class="ti-inline" style="flex-wrap:wrap">${draft.terminal.ansi.map((value, index) => `<input class="ti-color-swatch" type="color" title="ANSI ${index}" data-ansi="${index}" value="${normalizeColor(value)}">`).join("")}</div></div>`;
  }

  function skinHtml() {
    const skin = draft.skin;
    const enabled = Boolean(skin.enabled);
    return `<div class="ti-notice">高级皮肤可添加品牌、插画、组件表面和空白首页。关闭时不会改变 Codex 原有结构。</div><div class="ti-card ti-section-gap">${row("高级皮肤", `<label class="ti-switch"><input type="checkbox" data-path="skin.enabled" ${enabled ? "checked" : ""}><span>${enabled ? "已启用" : "已关闭"}</span></label>`)}</div>${enabled ? skinDetailsHtml() : ""}`;
  }

  function skinDetailsHtml() {
    const skin = draft.skin;
    return `<details class="ti-card ti-config-card" open><summary>品牌信息</summary>${row("品牌名称", textInput("skin.brand.title", skin.brand.title || ""))}${row("副标题", textInput("skin.brand.subtitle", skin.brand.subtitle || ""))}${row("左下角名称", textInput("skin.brand.accountName", skin.brand.accountName || "", "留空时沿用 Codex 账户名称"))}${row("左下角头像", accountAvatarFieldHtml())}${row("品牌字体", textInput("skin.brand.font", skin.brand.font || draft.typography.uiFont))}</details><details class="ti-card ti-config-card" open><summary>图片资源</summary><div class="ti-assets">${skinResourceFields.map(([key, label, purpose]) => assetField(key, label, purpose)).join("")}</div></details>${surfaceEditorHtml()}${iconEditorHtml()}${decorationEditorHtml()}${homeEditorHtml()}`;
  }

  function assetField(key, label, purpose) {
    const path = draft.skin.resources[key], url = resolvedAsset(draft, path);
    return `<div class="ti-asset"><div class="ti-asset-preview" style="background-image:url(&quot;${escapeHtml(url)}&quot;)"></div><div class="ti-asset-copy"><strong>${label}</strong><span>${escapeHtml(path?.split("/").pop() || "未设置")}</span><div class="ti-inline"><button class="ti-button" data-skin-asset="${key}" data-purpose="${purpose}">选择</button>${path ? `<button class="ti-button" data-clear-skin-asset="${key}">清除</button>` : ""}</div></div></div>`;
  }

  function accountAvatarFieldHtml() {
    const path = draft.skin.resources.accountAvatar, url = resolvedAsset(draft, path);
    return `<div class="ti-account-avatar-control"><span class="ti-account-avatar-preview" data-empty="${!url}" style="${url ? `background-image:url(&quot;${escapeHtml(url)}&quot;)` : ""}"></span><div class="ti-account-avatar-copy"><span class="ti-account-avatar-file" title="${escapeHtml(path?.split("/").pop() || "未设置，使用 Codex 原生头像")}">${escapeHtml(path?.split("/").pop() || "未设置，使用 Codex 原生头像")}</span><div class="ti-inline ti-account-avatar-actions"><button class="ti-button" data-skin-asset="accountAvatar" data-purpose="account-avatar">选择</button>${path ? `<button class="ti-button" data-clear-skin-asset="accountAvatar">清除</button>` : ""}</div></div></div>`;
  }

  function surfaceEditorHtml() {
    const items = [["titlebar", "标题栏"], ["sidebar", "侧边栏"], ["content", "内容区"], ["hero", "Hero"], ["card", "快捷卡片"], ["composer", "输入区"], ["popover", "浮层"]];
    return `<details class="ti-card ti-config-card"><summary>组件表面</summary>${items.map(([key, label]) => { const prefix = `skin.surfaces.${key}`, surface = draft.skin.surfaces[key]; return `<div class="ti-subcard"><strong>${label}</strong>${row("填充", optionalColorInput(`${prefix}.fill`, surface.fill))}${row("透明度", rangeInput(`${prefix}.opacity`, surface.opacity, 0, 1, .01))}${row("模糊", rangeInput(`${prefix}.blur`, surface.blur, 0, 80, 1))}${row("边框颜色", optionalColorInput(`${prefix}.borderColor`, surface.borderColor))}${row("边框宽度", rangeInput(`${prefix}.borderWidth`, surface.borderWidth, 0, 4, 1))}${row("圆角", rangeInput(`${prefix}.radius`, surface.radius, 0, 48, 1))}${row("阴影", rangeInput(`${prefix}.shadowOpacity`, surface.shadowOpacity, 0, 1, .01))}</div>`; }).join("")}</details>`;
  }

  function iconEditorHtml() {
    const icons = draft.skin.icons;
    const modeHelp = {
      native: "沿用 Codex 原图标，不改颜色也不替换资源。",
      recolor: "保留 Codex 原图标，只应用默认颜色与激活颜色。",
      custom: "优先使用下面的自定义映射，缺失项自动回退原图标。",
    }[icons.mode] || "";
    const mappings = icons.mode === "custom" ? `<div class="ti-section-title ti-section-gap">动作图标</div>${skinIconFields.map(([action, label]) => { const path = icons.mappings[action], url = resolvedAsset(draft, path); return row(label, `<div class="ti-inline">${url ? `<span class="ti-mini-asset" style="background-image:url(&quot;${escapeHtml(url)}&quot;)"></span>` : ""}<button class="ti-button" data-icon-asset="${action}">选择</button>${path ? `<button class="ti-button" data-clear-icon-asset="${action}">清除</button>` : ""}</div>`); }).join("")}` : "";
    return `<details class="ti-card ti-config-card"><summary>图标系统</summary>${row("模式", `<select class="ti-control" data-path="skin.icons.mode" title="沿用原生：不处理；仅重着色：保留形状并改色；自定义图标包：用映射资源替换"><option value="native" title="不处理 Codex 原图标" ${icons.mode === "native" ? "selected" : ""}>沿用原生</option><option value="recolor" title="保留图标形状，仅调整颜色" ${icons.mode === "recolor" ? "selected" : ""}>仅重着色</option><option value="custom" title="优先使用映射图片，缺失项回退原生" ${icons.mode === "custom" ? "selected" : ""}>自定义图标包</option></select>`)}<div class="ti-description ti-mode-description">${modeHelp}</div>${row("默认颜色", optionalColorInput("skin.icons.color", icons.color))}${row("激活颜色", optionalColorInput("skin.icons.activeColor", icons.activeColor))}<button class="ti-button ti-icon-atlas-generate" type="button" data-regenerate-icon-atlas title="AI 生成一张 6×5 图集并自动切割、覆盖全部 30 个动作图标">AI 重新生成完整图标集</button><div class="ti-notice">生成图标集需要已配置生图 Key；生成后先作为草稿预览，点击底部“应用”才会保存。</div>${mappings}</details>`;
  }

  function decorationEditorHtml() {
    const decorations = draft.skin.decorations || [];
    const regionOptions = [["sidebar", "左侧导航"], ["content", "内容区"], ["composer", "输入区"]];
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
    return `<div class="ti-notice">主题需求、参考图、资源计划、生成进度和日志已统一放入生成工作台。此处只管理连接设置。</div><details class="ti-card ti-config-card" open><summary>连接设置 <span class="ti-badge">${settings.hasApiKey ? "基础已配置" : "基础未配置"}</span></summary>${row("基础 API 地址", `<input class="ti-control" type="url" data-ai-setting="baseUrl" value="${escapeHtml(settings.baseUrl)}" placeholder="https://api.openai.com/v1">`)}${row("基础 Key", `<input class="ti-control" type="password" data-ai-api-key value="${escapeHtml(aiApiKeyDraft)}" placeholder="${settings.hasApiKey ? "留空则继续使用已保存的基础 Key" : "输入基础/视觉模型使用的 Key"}" autocomplete="new-password">`)}<div class="ti-secret-status">${baseKeyState}。基础地址允许 HTTP/HTTPS。</div>${row("基础模型", `<input class="ti-control" data-ai-setting="textModel" value="${escapeHtml(settings.textModel)}" placeholder="gpt-4.1-mini">`)}${row("视觉模型", `<input class="ti-control" data-ai-setting="visionModel" value="${escapeHtml(settings.visionModel)}" placeholder="支持图片输入的模型名称">`)}${row("生图 API 地址", `<input class="ti-control" type="url" data-ai-setting="imageBaseUrl" value="${escapeHtml(settings.imageBaseUrl || "")}" placeholder="">`)}${row("生图 Key", `<input class="ti-control" type="password" data-ai-image-api-key value="${escapeHtml(aiImageApiKeyDraft)}" placeholder="${settings.hasImageApiKey ? "留空则继续使用已保存的生图 Key" : "输入生图模型单独使用的 Key"}" autocomplete="new-password">`)}<div class="ti-secret-status">${imageKeyState}。生图地址允许 HTTP/HTTPS，留空时自动使用基础地址；密钥不会进入日志或主题 ZIP。</div>${row("生图模型", `<input class="ti-control" data-ai-setting="imageModel" value="${escapeHtml(settings.imageModel)}" placeholder="gpt-image-1">`)}${row("图片比例", `<input class="ti-control" data-ai-setting="imageSize" value="${escapeHtml(settings.imageSize)}" placeholder="3:2 或 16:9">`)}<div class="ti-inline ti-section-gap ti-modal-actions-split"><button class="ti-button" type="button" data-ai-settings-save>保存连接设置</button>${settings.hasApiKey ? `<button class="ti-button" type="button" data-ai-key-clear>清除基础 Key</button>` : ""}${settings.hasImageApiKey ? `<button class="ti-button" type="button" data-ai-image-key-clear>清除生图 Key</button>` : ""}</div></details><button class="ti-button ti-ai-generate" data-primary="true" type="button" data-ai-workbench>打开 AI 生成工作台</button><div class="ti-description">在工作台内填写需求、添加参考图、查看每项资源与实时日志。</div>`;
  }

  function row(label, control) { return `<div class="ti-row"><div class="ti-label">${label}</div><div>${control}</div></div>`; }
  function textInput(path, value, placeholder = "") { return `<input class="ti-control" type="text" data-path="${path}" value="${escapeHtml(value)}"${placeholder ? ` placeholder="${escapeHtml(placeholder)}"` : ""}>`; }
  function numberInput(path, value, min, max) { return `<input class="ti-control" type="number" data-path="${path}" value="${value}" min="${min}" max="${max}">`; }
  function rangeValue(value, step) { const precision = (String(step).split(".")[1] || "").length; const normalized = Math.round(Number(value) / Number(step)) * Number(step); return Number(normalized.toFixed(precision)).toString(); }
  function rangeInput(path, value, min, max, step) { return `<div class="ti-inline"><input class="ti-control" type="range" data-path="${path}" value="${value}" min="${min}" max="${max}" step="${step}"><output>${rangeValue(value, step)}</output></div>`; }
  function colorInput(path, value) { const color = normalizeColor(value); return `<div class="ti-color-control"><input class="ti-color-swatch" type="color" data-color-path="${path}" value="${color}"><input class="ti-color-hex" type="text" data-color-text="${path}" value="${color.toUpperCase()}" maxlength="7" spellcheck="false"></div>`; }
  function optionalColorInput(path, value) { const color = normalizeColor(value || "#000000"); return `<div class="ti-color-control ti-color-optional"><input class="ti-color-swatch" type="color" data-color-path="${path}" value="${color}"><input class="ti-color-hex" type="text" data-color-text="${path}" value="${value ? color.toUpperCase() : ""}" maxlength="7" placeholder="继承" spellcheck="false"><button class="ti-color-reset" type="button" data-clear-path="${path}" title="恢复继承">×</button></div>`; }

  function capturePanelState(panel) {
    const body = panel?.querySelector(".ti-body");
    const active = document.activeElement instanceof HTMLElement && panel?.contains(document.activeElement) ? document.activeElement : null;
    const focusAttributes = ["data-path", "data-color-path", "data-color-text", "data-ai-setting", "data-trigger-setting"];
    const focusAttribute = active ? focusAttributes.find(attribute => active.hasAttribute(attribute)) : null;
    return {
      scrollTop: body?.scrollTop || 0,
      details: Object.fromEntries([...panel?.querySelectorAll(".ti-section") || []].map(section => [section.dataset.section, [...section.querySelectorAll("details")].map(detail => detail.open)])),
      focus: focusAttribute ? {
        attribute: focusAttribute,
        value: active.getAttribute(focusAttribute),
        start: "selectionStart" in active ? active.selectionStart : null,
        end: "selectionEnd" in active ? active.selectionEnd : null,
      } : null,
    };
  }

  function restorePanelState(panel, previous) {
    if (!previous) return;
    Object.entries(previous.details).forEach(([sectionId, details]) => {
      panel.querySelectorAll(`.ti-section[data-section="${sectionId}"] details`).forEach((detail, index) => { if (index < details.length) detail.open = details[index]; });
    });
    const body = panel.querySelector(".ti-body");
    const restoreScroll = () => { if (body?.isConnected) body.scrollTop = previous.scrollTop; };
    restoreScroll();
    requestAnimationFrame(restoreScroll);
    if (!previous.focus) return;
    queueMicrotask(() => {
      const control = [...panel.querySelectorAll(`[${previous.focus.attribute}]`)].find(node => node.getAttribute(previous.focus.attribute) === previous.focus.value);
      if (!(control instanceof HTMLElement)) return;
      control.focus({ preventScroll: true });
      if ("setSelectionRange" in control && previous.focus.start !== null && previous.focus.end !== null) control.setSelectionRange(previous.focus.start, previous.focus.end);
    });
  }

  function removeInlineAssetControl(button) {
    button.closest(".ti-inline")?.querySelector(".ti-mini-asset")?.remove();
    button.remove();
  }

  function renderPanel() {
    if (destroyed) return;
    activateAiWorkspace(draft);
    ensurePanelStyles();
    let panel = document.getElementById(PANEL_ID);
    const previous = capturePanelState(panel);
    panelScrollTop = previous.scrollTop;
    if (!panel) { panel = document.createElement("aside"); panel.id = PANEL_ID; document.body.appendChild(panel); }
    panel.innerHTML = panelHtml();
    localizeElement(panel);
    panel.dataset.open = "true";
    panel.dataset.busy = String(busy);
    panel.setAttribute("aria-busy", String(busy));
    bindPanel(panel);
    const body = panel.querySelector(".ti-body");
    if (body) {
      body.addEventListener("scroll", () => { panelScrollTop = body.scrollTop; }, { passive: true });
    }
    restorePanelState(panel, previous);
    if (modal?.kind === "create") queueMicrotask(() => panel.querySelector("[data-modal-name]")?.select());
    if (modal?.kind === "edit-theme") queueMicrotask(() => panel.querySelector("[data-modal-theme-name]")?.select());
  }

  function bindPanel(panel) {
    panel.querySelectorAll(".ti-theme-thumbnail img").forEach(image => {
      const thumbnail = image.parentElement;
      const settle = loaded => {
        thumbnail.dataset.loading = "false";
        thumbnail.dataset.loaded = String(loaded);
        image.hidden = !loaded;
      };
      image.addEventListener("load", () => settle(true), { once:true });
      image.addEventListener("error", () => settle(false), { once:true });
      if (image.hasAttribute("src") && image.complete) settle(image.naturalWidth > 0);
    });
    panel.querySelectorAll(".ti-theme-thumbnail[data-theme-hero]").forEach(thumbnail => void loadThemeThumbnail(thumbnail));
    panel.querySelectorAll("[data-tab]").forEach(button => button.onclick = () => { activeTab = button.dataset.tab; renderPanel(); });
    panel.querySelector('[data-action="close"]')?.addEventListener("click", closePanel);
    panel.querySelector('[data-action="reinject"]')?.addEventListener("click", () => void runAction("正在构建备用开发版本…", () => call("runtime.reinject"), "构建完成，正在重启并重新注入…"));
    panel.querySelector('[data-action="cancel"]')?.addEventListener("click", cancelPreview);
    panel.querySelector('[data-action="apply"]')?.addEventListener("click", () => void runAction("正在应用主题…", applyDraft, "主题已应用"));
    panel.querySelector('[data-action="import"]')?.addEventListener("click", () => void runAction("等待选择主题包…", async () => { await call("theme.package.import"); await reloadState(); }, "主题包已安装"));
    panel.querySelector('[data-action="create"]')?.addEventListener("click", () => { modal = { kind: "create", defaultValue: `${draft.name} 自定义` }; renderPanel(); });
    panel.querySelector('[data-action="open-ai"]')?.addEventListener("click", () => { activeTab = "ai"; renderPanel(); });
    panel.querySelector('[data-action="clone"]')?.addEventListener("click", () => void runAction("正在复制主题…", async () => { draft = await call("theme.package.clone", { id: draft.id }); dirty = true; await hydrateAssets(draft); applyTheme(draft, draftCustomCss); await reloadThemes(); }, "副本已创建，点击应用以保存"));
    panel.querySelectorAll("[data-theme-activate]").forEach(card => card.onclick = event => { if (event.target.closest("details,button")) return; void switchTheme(card.dataset.themeActivate); });
    panel.querySelectorAll(".ti-theme-menu").forEach(menu => menu.onclick = event => event.stopPropagation());
    panel.querySelectorAll(".ti-theme-menu > summary").forEach(summary => summary.addEventListener("click", () => {
      const currentMenu = summary.parentElement;
      panel.querySelectorAll(".ti-theme-menu[open]").forEach(menu => { if (menu !== currentMenu) menu.open = false; });
      setTimeout(() => {
        if (!currentMenu.open) return;
        const popover = currentMenu.querySelector(".ti-theme-menu-popover");
        if (!popover) return;
        const summaryRect = summary.getBoundingClientRect();
        const panelRect = panel.getBoundingClientRect();
        const width = 168;
        const left = Math.max(8, Math.min(summaryRect.right - width - panelRect.left, panelRect.width - width - 8));
        const top = Math.max(8, summaryRect.bottom - panelRect.top + 6);
        popover.style.left = `${left}px`;
        popover.style.top = `${top}px`;
      }, 0);
    }));
    panel.querySelector(".ti-body")?.addEventListener("scroll", () => panel.querySelectorAll(".ti-theme-menu[open]").forEach(menu => { menu.open = false; }), { passive: true });
    panel.addEventListener("click", event => { if (event.target.closest(".ti-theme-menu")) return; panel.querySelectorAll(".ti-theme-menu[open]").forEach(menu => { menu.open = false; }); });
    panel.querySelectorAll("[data-theme-edit]").forEach(button => button.onclick = event => { event.stopPropagation(); const theme = state.themes.find(item => item.id === button.dataset.themeEdit); if (theme) { modal = { kind:"edit-theme", theme:clone(theme) }; renderPanel(); } });
    panel.querySelectorAll("[data-theme-delete]").forEach(button => button.onclick = event => { event.stopPropagation(); if (button.disabled) return; const theme = state.themes.find(item => item.id === button.dataset.themeDelete); if (theme) { modal = { kind:"delete", theme:clone(theme) }; renderPanel(); } });
    panel.querySelectorAll("[data-export]").forEach(button => button.onclick = event => { event.stopPropagation(); void runAction("等待选择导出位置…", () => call("theme.package.export", { id: button.dataset.export }), "主题已导出"); });
    panel.querySelector("[data-modal-cancel]")?.addEventListener("click", () => { modal = null; renderPanel(); });
    panel.querySelector('[data-modal-form="create"]')?.addEventListener("submit", event => { event.preventDefault(); const name = panel.querySelector("[data-modal-name]")?.value.trim(); if (!name) { setStatus("请输入主题名称", "error"); return; } void runAction("正在创建主题…", async () => { const theme = await call("theme.package.create", { sourceId: draft.id, name }); state = await call("theme.activate", { id: theme.id }); draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); draftCustomCss = state.customCss || ""; dirty = false; modal = null; await hydrateAssets(draft); applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""); }, "主题已创建并切换"); });
    panel.querySelector('[data-modal-form="edit-theme"]')?.addEventListener("submit", event => { event.preventDefault(); const name = panel.querySelector("[data-modal-theme-name]")?.value.trim(); const description = panel.querySelector("[data-modal-theme-description]")?.value.trim() || ""; if (!name) { setStatus("请输入主题名称", "error"); return; } void runAction("正在保存主题信息…", async () => { const updated = await call("theme.package.metadata", { id:modal.theme.id, name, description }); state.themes = state.themes.map(theme => theme.id === updated.id ? updated : theme); if (draft.id === updated.id) { draft.name = updated.name; draft.description = updated.description; snapshot.name = updated.name; snapshot.description = updated.description; state.activeTheme = clone(updated); } modal = null; }, "主题信息已保存"); });
    panel.querySelector('[data-modal-confirm="delete"]')?.addEventListener("click", () => void runAction("正在删除主题…", async () => { state = await call("theme.package.delete", { id: modal.theme.id }); draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); draftCustomCss = state.customCss || ""; dirty = false; modal = null; await hydrateAssets(draft); applyTheme(draft, state.customCssTrusted ? draftCustomCss : ""); }, "主题已删除"));
    panel.querySelector('[data-modal-confirm="discard"]')?.addEventListener("click", () => void discardAndClose());
    panel.querySelector('[data-modal-confirm="apply"]')?.addEventListener("click", () => void applyAndClose());
    panel.querySelectorAll("[data-path]").forEach(input => input.addEventListener(input.tagName === "SELECT" || input.type === "checkbox" ? "change" : "input", () => { const value = input.type === "checkbox" ? input.checked : input.type === "number" || input.type === "range" ? Number(input.value) : input.value; setPath(draft, input.dataset.path, value); input.parentElement.querySelector("output")?.replaceChildren(input.type === "range" ? rangeValue(value, input.step) : input.value); markDirty(); if (input.type === "checkbox" || input.tagName === "SELECT") renderPanel(); }));
    panel.querySelectorAll("[data-color-path]").forEach(input => input.addEventListener("input", () => { const path = input.dataset.colorPath; setPath(draft, path, input.value.toUpperCase()); const text = panel.querySelector(`[data-color-text="${path}"]`); if (text) text.value = input.value.toUpperCase(); markDirty(); }));
    panel.querySelectorAll("[data-color-text]").forEach(input => input.addEventListener("change", () => { const value = input.value.trim(); const path = input.dataset.colorText; if (!value) { setPath(draft, path, null); markDirty(); return; } if (!/^#[0-9a-f]{6}$/i.test(value)) { input.value = normalizeColor(getPath(draft, path)).toUpperCase(); setStatus("颜色必须使用 #RRGGBB", "error"); return; } setPath(draft, path, value.toUpperCase()); const swatch = panel.querySelector(`[data-color-path="${path}"]`); if (swatch) swatch.value = value; markDirty(); }));
    panel.querySelectorAll("[data-clear-path]").forEach(button => button.addEventListener("click", () => { const path = button.dataset.clearPath; setPath(draft, path, null); const control = button.closest(".ti-color-control"); const swatch = control?.querySelector(`[data-color-path="${path}"]`); const text = control?.querySelector(`[data-color-text="${path}"]`); if (swatch) swatch.value = "#000000"; if (text) text.value = ""; markDirty(); }));
    panel.querySelectorAll("[data-ansi]").forEach(input => input.addEventListener("input", () => { draft.terminal.ansi[Number(input.dataset.ansi)] = input.value; markDirty(); }));
    panel.querySelector("[data-background-mode]")?.addEventListener("change", event => { draft.background.mode = event.target.value; markDirty(); renderPanel(); });
    panel.querySelectorAll("[data-bg-kind]").forEach(select => select.addEventListener("change", () => { const key = select.dataset.bgKind, kind = select.value; if (kind === "image") { void chooseBackground(key); return; } draft.background[key] = defaultBackground(kind, key); markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-bg-image]").forEach(button => button.addEventListener("click", () => void chooseBackground(button.dataset.bgImage)));
    panel.querySelectorAll("[data-bg-clear]").forEach(button => button.addEventListener("click", () => { draft.background[button.dataset.bgClear] = defaultBackground("none", button.dataset.bgClear); markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-skin-asset]").forEach(button => button.addEventListener("click", () => void chooseSkinAsset(button.dataset.skinAsset, button.dataset.purpose)));
    panel.querySelectorAll("[data-clear-skin-asset]").forEach(button => button.addEventListener("click", () => { draft.skin.resources[button.dataset.clearSkinAsset] = null; const asset = button.closest(".ti-asset"); asset?.querySelector(".ti-asset-preview")?.style.removeProperty("background-image"); asset?.querySelector(".ti-asset-copy > span")?.replaceChildren(t("未设置")); removeInlineAssetControl(button); markDirty(); }));
    panel.querySelectorAll("[data-icon-asset]").forEach(button => button.addEventListener("click", () => void chooseIconAsset(button.dataset.iconAsset)));
    panel.querySelectorAll("[data-clear-icon-asset]").forEach(button => button.addEventListener("click", () => { delete draft.skin.icons.mappings[button.dataset.clearIconAsset]; removeInlineAssetControl(button); markDirty(); }));
    panel.querySelector("[data-regenerate-icon-atlas]")?.addEventListener("click", () => void regenerateIconAtlas());
    panel.querySelector("[data-add-decoration]")?.addEventListener("click", () => { if (draft.skin.decorations.length < 16) draft.skin.decorations.push({ asset: "", region: "content", anchor: "bottom-right", offsetX: 24, offsetY: 24, width: 160, opacity: 1, layer: 0, hiddenBelow: 0 }); markDirty(); renderPanel(); });
    panel.querySelectorAll("[data-remove-decoration]").forEach(button => button.addEventListener("click", () => { draft.skin.decorations.splice(Number(button.dataset.removeDecoration), 1); markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-decoration-asset]").forEach(button => button.addEventListener("click", () => void chooseDecorationAsset(Number(button.dataset.decorationAsset))));
    panel.querySelector("[data-add-home-card]")?.addEventListener("click", () => { if (draft.skin.home.cards.length < 4) draft.skin.home.cards.push({ title: "新快捷卡片", description: "", prompt: "", icon: null }); markDirty(); renderPanel(); });
    panel.querySelectorAll("[data-remove-home-card]").forEach(button => button.addEventListener("click", () => { draft.skin.home.cards.splice(Number(button.dataset.removeHomeCard), 1); markDirty(); renderPanel(); }));
    panel.querySelectorAll("[data-home-card-icon]").forEach(button => button.addEventListener("click", () => void chooseHomeCardIcon(Number(button.dataset.homeCardIcon))));
    panel.querySelectorAll("[data-clear-home-card-icon]").forEach(button => button.addEventListener("click", () => { draft.skin.home.cards[Number(button.dataset.clearHomeCardIcon)].icon = null; removeInlineAssetControl(button); markDirty(); }));
    panel.querySelectorAll("[data-ai-setting]").forEach(input => input.addEventListener("input", () => { aiSettings ||= clone(state.ai); aiSettings[input.dataset.aiSetting] = input.value; }));
    panel.querySelector("[data-ai-api-key]")?.addEventListener("input", event => { aiApiKeyDraft = event.target.value; });
    panel.querySelector("[data-ai-image-api-key]")?.addEventListener("input", event => { aiImageApiKeyDraft = event.target.value; });
    panel.querySelector("[data-ai-settings-save]")?.addEventListener("click", () => void runAction("正在保存 AI 设置…", () => saveAiSettings(), "AI 连接设置已保存"));
    panel.querySelector("[data-ai-key-clear]")?.addEventListener("click", () => void runAction("正在清除基础 Key…", () => saveAiSettings(true), "已清除保存的基础 Key"));
    panel.querySelector("[data-ai-image-key-clear]")?.addEventListener("click", () => void runAction("正在清除生图 Key…", () => saveAiSettings(false, true), "已清除保存的生图 Key"));
    panel.querySelector("[data-ai-workbench]")?.addEventListener("click", openStudio);
    panel.querySelector("[data-custom-css]")?.addEventListener("input", event => { draftCustomCss = event.target.value; markDirty(); });
    panel.querySelector("[data-custom-css-enabled]")?.addEventListener("change", event => { state.customCssTrusted = event.target.checked; markDirty(); });
    panel.querySelector("[data-css-inspector]")?.addEventListener("click", startCssInspector);
    panel.querySelectorAll("[data-trigger-setting]").forEach(input => input.addEventListener(input.tagName === "SELECT" ? "change" : "input", () => { triggerAppearanceDraft ||= clone(state.settings.triggerAppearance); triggerAppearanceDraft[input.dataset.triggerSetting] = input.tagName === "SELECT" ? input.value : Number(input.value); input.parentElement.querySelector("output")?.replaceChildren(input.value); applyTriggerAppearance(triggerAppearanceDraft); const preview = panel.querySelector(".ti-trigger-preview > span"); if (preview) preview.style.cssText = triggerStyle(triggerAppearanceDraft); }));
    panel.querySelector("[data-trigger-save]")?.addEventListener("click", () => void runAction("正在保存入口设置…", async () => { state = await call("app.trigger.save", { appearance:triggerAppearanceDraft || state.settings.triggerAppearance }); triggerAppearanceDraft = clone(state.settings.triggerAppearance); applyTriggerAppearance(); }, "入口设置已保存"));
    panel.querySelector("[data-ui-language]")?.addEventListener("change", event => void runAction("正在保存语言设置…", async () => { state = await call("app.language.save", { language:event.target.value }); setUiLanguage(state.settings.uiLanguage); }, "语言设置已保存"));
    panel.querySelector("[data-trigger-icon-import]")?.addEventListener("click", () => void runAction("等待选择入口图标…", async () => { state = await call("app.trigger.icon.import"); triggerAppearanceDraft = clone(state.settings.triggerAppearance); applyTriggerAppearance(); }, "入口图标已更新"));
    panel.querySelector("[data-trigger-icon-reset]")?.addEventListener("click", () => void runAction("正在恢复默认图标…", async () => { state = await call("app.trigger.icon.reset"); triggerAppearanceDraft = clone(state.settings.triggerAppearance); applyTriggerAppearance(); }, "已恢复默认入口图标"));
  }

  async function switchTheme(id) {
    if (busy || !id || id === draft.id) return;
    switchingThemeId = id;
    busy = true;
    statusMessage = "正在切换主题…";
    statusKind = "";
    renderPanel();
    try {
      await cancelStaging();
      state = await call("theme.activate", { id });
      draft = clone(state.activeTheme);
      snapshot = clone(state.activeTheme);
      draftCustomCss = state.customCss || "";
      dirty = false;
      await hydrateAssets(draft);
      applyTheme(draft, state.customCssTrusted ? draftCustomCss : "");
      statusMessage = "主题切换成功";
      statusKind = "success";
    } catch (error) {
      statusMessage = error?.message || "主题切换失败";
      statusKind = "error";
    } finally {
      switchingThemeId = "";
      busy = false;
      renderPanel();
    }
  }

  async function loadThemeThumbnail(thumbnail) {
    const key = thumbnail.dataset.thumbnailKey;
    const themeId = thumbnail.dataset.themeId;
    const path = thumbnail.dataset.themeHero;
    if (!key || !themeId || !path) return;
    let url = themeThumbnailUrls.get(key) || "";
    if (!url) {
      let request = themeThumbnailRequests.get(key);
      if (!request) {
        request = call("theme.thumbnail", { id:themeId, path })
          .then(result => isPreviewDataUrl(result.url) ? result.url : "")
          .catch(() => "")
          .finally(() => themeThumbnailRequests.delete(key));
        themeThumbnailRequests.set(key, request);
      }
      url = await request;
      if (url) themeThumbnailUrls.set(key, url);
    }
    if (!thumbnail.isConnected || thumbnail.dataset.thumbnailKey !== key) return;
    const image = thumbnail.querySelector("img");
    if (url && image) image.src = url;
    else {
      thumbnail.dataset.loading = "false";
      thumbnail.dataset.loaded = "false";
    }
  }

  function startCssInspector() {
    cssInspectorCleanup?.();
    const panel = document.getElementById(PANEL_ID);
    if (panel) panel.dataset.open = "false";
    let highlighted = null;
    let previousOutline = null;
    const clearHighlight = () => {
      if (!highlighted) return;
      for (const [property, value, priority] of previousOutline || []) {
        if (value) highlighted.style.setProperty(property, value, priority);
        else highlighted.style.removeProperty(property);
      }
      highlighted = null;
      previousOutline = null;
    };
    const move = event => {
      const target = event.target instanceof Element ? event.target : null;
      if (!target || target.closest(`#${PANEL_ID},.ti-studio-backdrop,#${TRIGGER_ID}`)) return;
      if (target === highlighted) return;
      clearHighlight();
      highlighted = target;
      previousOutline = ["outline", "outline-offset"].map(property => [property, target.style.getPropertyValue(property), target.style.getPropertyPriority(property)]);
      target.style.setProperty("outline", "2px solid #8b5cf6", "important");
      target.style.setProperty("outline-offset", "2px", "important");
    };
    const cleanup = () => {
      clearHighlight();
      document.removeEventListener("mousemove", move, true);
      document.removeEventListener("click", pick, true);
      document.removeEventListener("keydown", cancel, true);
      cssInspectorCleanup = null;
    };
    const finish = selector => {
      cleanup();
      if (panel) panel.dataset.open = "true";
      if (!selector) return;
      draftCustomCss += `${draftCustomCss.trim() ? "\n\n" : ""}${selector} {\n  \n}`;
      markDirty();
      renderPanel();
      const textarea = document.querySelector("[data-custom-css]");
      textarea?.focus();
      textarea?.setSelectionRange(Math.max(0, draftCustomCss.length - 2), Math.max(0, draftCustomCss.length - 2));
    };
    const pick = event => {
      const target = event.target instanceof Element ? event.target : null;
      if (!target || target.closest(`#${PANEL_ID},.ti-studio-backdrop,#${TRIGGER_ID}`)) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      finish(cssSelectorFor(target));
    };
    const cancel = event => { if (event.key === "Escape") finish(""); };
    cssInspectorCleanup = cleanup;
    document.addEventListener("mousemove", move, true);
    document.addEventListener("click", pick, true);
    document.addEventListener("keydown", cancel, true);
  }

  function cssSelectorFor(element) {
    if (element.id) return `#${CSS.escape(element.id)}`;
    const path = [];
    let node = element;
    while (node instanceof Element && node !== document.body && path.length < 5) {
      let part = node.localName;
      const stableAttribute = [...node.attributes].find(attribute => /^(data-(testid|app-action|codex|composer)|aria-label)$/.test(attribute.name) && attribute.value);
      if (stableAttribute) {
        part += `[${stableAttribute.name}="${CSS.escape(stableAttribute.value)}"]`;
        path.unshift(part);
        break;
      }
      const classes = [...node.classList].filter(name => !name.startsWith("ti-") && !/^[a-z]{1,2}\d/i.test(name)).slice(0, 2);
      if (classes.length) part += classes.map(name => `.${CSS.escape(name)}`).join("");
      const siblings = node.parentElement ? [...node.parentElement.children].filter(candidate => candidate.localName === node.localName) : [];
      if (siblings.length > 1) part += `:nth-of-type(${siblings.indexOf(node) + 1})`;
      path.unshift(part);
      node = node.parentElement;
    }
    return path.join(" > ");
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
    previous.forEach(item => { thumbnailUrls.delete(previewCacheKey(item)); thumbnailUrls.delete(item.path); });
    await cancelPreviewSessions(previous);
    if (render) renderPanel();
  }

  async function addAiReferences(items) {
    const existing = new Set(aiReferences.map(item => `${item.session}:${item.path}`));
    for (const item of items.filter(Boolean)) {
      if (!item.session || !item.path || existing.has(`${item.session}:${item.path}`)) continue;
      if (aiReferences.length >= 6) break;
      item.previewUrl = "";
      item.previewState = "loading";
      aiReferences.push(item);
      existing.add(`${item.session}:${item.path}`);
      void refreshReferencePreview(item);
    }
    refreshStudioReferenceList();
  }

  async function removeAiReference(index, render = true) {
    const [removed] = aiReferences.splice(index, 1);
    if (removed) { thumbnailUrls.delete(previewCacheKey(removed)); thumbnailUrls.delete(removed.path); await call("theme.preview.cancel", { session: removed.session }).catch(() => {}); }
    if (render) refreshStudioReferenceList();
  }
