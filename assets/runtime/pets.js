  // Runtime module: pets
  const petActions = [
    ["idle", "待机", 0, 6, 920, "没有活动通知时持续播放"],
    ["running-right", "向右拖动", 1, 8, 1060, "用户把宠物向右拖动时"],
    ["running-left", "向左拖动", 2, 8, 1060, "用户把宠物向左拖动时"],
    ["waving", "欢迎挥手", 3, 4, 700, "宠物首次出现或唤醒时"],
    ["jumping", "悬停反馈", 4, 5, 840, "鼠标悬停在宠物上时"],
    ["failed", "任务失败", 5, 8, 1220, "任务失败或被阻塞时"],
    ["waiting", "等待输入", 6, 6, 1010, "需要用户输入或审批时"],
    ["running", "任务处理中", 7, 6, 820, "任务正在思考或执行时"],
    ["review", "检查结果", 8, 6, 1030, "任务完成且结果尚未查看时"],
  ];

  function availablePetActions(pet) {
    return petActions;
  }

  function petSpriteHtml(pet, actionId = "idle", large = false, animated = false) {
    const action = availablePetActions(pet).find(([id]) => id === actionId) || petActions[0];
    const [, label, row, frames, duration] = action;
    const scale = large ? 1 : .5;
    const width = 192 * scale, height = 208 * scale;
    const rows = pet.spriteVersionNumber >= 2 ? 11 : 9;
    const style = `--ti-pet-width:${width}px;--ti-pet-height:${height}px;--ti-pet-rows:${rows};--ti-pet-row:${row};--ti-pet-frames:${frames};--ti-pet-duration:${duration}ms`;
    if (!pet.assetUrl) return `<div class="ti-pet-sprite ti-pet-sprite-loading" role="img" aria-busy="true" aria-label="${escapeHtml(`${pet.displayName} · 正在加载`)}" style="${style}"></div>`;
    const motionClass = animated ? "" : " ti-pet-sprite-static";
    const motionLabel = animated ? `${label}动作` : `${label}首帧`;
    return `<div class="ti-pet-sprite${motionClass}" role="img" aria-label="${escapeHtml(`${pet.displayName} · ${motionLabel}`)}" style="${style}"><img src="${escapeHtml(pet.assetUrl)}" alt="" draggable="false"></div>`;
  }

  function restoreCachedPetAssets(pets, previousPets = []) {
    const previous = new Map(previousPets.map(pet => [pet.id, pet.assetUrl || ""]));
    pets.forEach(pet => {
      const assetUrl = pet.assetUrl || previous.get(pet.id) || petAssetUrls.get(pet.id) || "";
      if (assetUrl) {
        pet.assetUrl = assetUrl;
        petAssetUrls.set(pet.id, assetUrl);
      }
    });
  }

  async function hydratePetAssets(ids = (state?.pets || []).map(pet => pet.id)) {
    const lifecycle = lifecycleEpoch;
    await mapConcurrent(ids, 1, async id => {
      const pet = findPet(id);
      if (!pet || pet.assetUrl) return;
      const cached = petAssetUrls.get(id);
      if (cached) {
        pet.assetUrl = cached;
        return;
      }
      let request = petAssetRequests.get(id);
      if (!request) {
        request = call("pet.library.data", { id }).finally(() => petAssetRequests.delete(id));
        petAssetRequests.set(id, request);
      }
      try {
        const result = await request;
        if (destroyed || lifecycle !== lifecycleEpoch) return;
        const current = findPet(id);
        if (current && result?.url?.startsWith("data:image/webp;base64,")) {
          petAssetUrls.set(id, result.url);
          current.assetUrl = result.url;
        }
      } catch (error) {
        if (!destroyed && lifecycle === lifecycleEpoch) setStatus(error?.message || "宠物预览加载失败", "error");
      }
    });
    if (!destroyed && lifecycle === lifecycleEpoch && (activeTab === "pets" || modal?.kind === "pet-preview")) renderPanel();
  }

  function invalidatePetAsset(id) {
    petAssetRequests.delete(id);
    petAssetUrls.delete(id);
    (state?.pets || []).forEach(pet => { if (pet.id === id) pet.assetUrl = ""; });
    if (modal?.pet?.id === id) modal.pet.assetUrl = "";
  }

  function petLibraryHtml() {
    const pets = state?.pets || [];
    restoreCachedPetAssets(pets);
    const boundPetId = draft?.themeInject?.companionPetId || "";
    if (petCreatorOpen) return petCreatorHtml();
    const actions = `<div class="ti-inline ti-pet-library-actions"><button class="ti-button" data-primary="true" type="button" data-pet-create>新建宠物</button><button class="ti-button" type="button" data-pet-import>导入宠物包</button></div>`;
    if (!pets.length) return `${actions}<div class="ti-empty ti-pet-empty"><strong>宠物库还是空的</strong><span>导入 Codex 或社区宠物 ZIP 后，可以预览动作并安装到 Codex。</span></div>`;
    const cards = pets.map(pet => { const bound = pet.id === boundPetId; return `<div class="ti-card ti-pet-card"><button class="ti-pet-preview-button" type="button" data-pet-preview="${escapeHtml(pet.id)}" title="预览全部动作">${petSpriteHtml(pet)}</button><div class="ti-pet-card-copy"><div class="ti-name">${escapeHtml(pet.displayName)}<span class="ti-badge">V${pet.spriteVersionNumber}</span>${pet.installed ? `<span class="ti-badge ti-pet-installed">已安装</span>` : ""}${bound ? `<span class="ti-badge ti-pet-bound">当前主题</span>` : ""}</div><div class="ti-description">${escapeHtml(pet.description || pet.id)}</div><div class="ti-pet-card-actions"><button class="ti-button" type="button" data-pet-preview="${escapeHtml(pet.id)}">预览动作</button><button class="ti-button" type="button" data-pet-install="${escapeHtml(pet.id)}">${pet.installed ? "重新安装" : "安装到 Codex"}</button><button class="ti-button" type="button" data-pet-bind="${escapeHtml(pet.id)}" data-bound="${bound}">${bound ? "取消主题绑定" : "绑定当前主题"}</button></div></div><details class="ti-theme-menu"><summary title="宠物操作" aria-label="宠物操作">•••</summary><div class="ti-theme-menu-popover"><button type="button" data-pet-edit="${escapeHtml(pet.id)}">编辑名称和介绍</button><button type="button" data-pet-export="${escapeHtml(pet.id)}">导出宠物</button><button type="button" data-pet-delete="${escapeHtml(pet.id)}">从宠物库删除</button></div></details></div>`; }).join("");
    return `<div class="ti-notice ti-pet-notice">宠物安装后，请在 Codex 的“设置 → 宠物”中选择。重新安装同名宠物时会先备份现有版本。</div>${actions}<div class="ti-grid">${cards}</div>`;
  }

  function petModalHtml() {
    const pet = modal.pet;
    if (modal.kind === "pet-delete") return `<div class="ti-modal-backdrop"><div class="ti-modal"><div class="ti-modal-title">删除宠物</div><div class="ti-modal-copy">确定从 Theme Inject 宠物库删除“${escapeHtml(pet.displayName)}”？Codex 中已安装的副本不会被删除。</div><div class="ti-modal-actions"><button class="ti-button" type="button" data-modal-cancel>取消</button><button class="ti-button" data-danger="true" type="button" data-pet-delete-confirm="${escapeHtml(pet.id)}">删除</button></div></div></div>`;
    if (modal.kind === "pet-edit") return `<div class="ti-modal-backdrop"><form class="ti-modal" data-modal-form="pet-edit"><div class="ti-modal-title">编辑宠物信息</div><div class="ti-modal-copy">修改宠物库中显示的名称和介绍，不会改变宠物动作。</div><label class="ti-field-label" for="ti-edit-pet-name">宠物名称</label><input id="ti-edit-pet-name" class="ti-control" data-modal-pet-name maxlength="80" value="${escapeHtml(pet.displayName)}" autocomplete="off"><label class="ti-field-label ti-section-gap" for="ti-edit-pet-description">宠物介绍</label><textarea id="ti-edit-pet-description" class="ti-control ti-modal-textarea" data-modal-pet-description maxlength="240">${escapeHtml(pet.description || "")}</textarea><div class="ti-modal-actions"><button class="ti-button" type="button" data-modal-cancel>取消</button><button class="ti-button" data-primary="true" type="submit">保存</button></div></form></div>`;
    const actions = availablePetActions(pet);
    const action = actions.find(([id]) => id === petPreviewAction) || actions[0];
    const backgroundGeneration = busy && activeGenerationKind === "pet";
    return `<div class="ti-modal-backdrop ti-pet-modal-backdrop"><div class="ti-pet-modal"><div class="ti-pet-modal-header"><div><div class="ti-modal-title">${escapeHtml(pet.displayName)}</div><div class="ti-description">${escapeHtml(action[1])} · ${action[3]} 帧 · 触发：${escapeHtml(action[5])}</div></div><button class="ti-icon-button" type="button" data-modal-cancel ${backgroundGeneration ? "data-busy-allow title=\"关闭；生成任务会在后台继续\"" : "title=\"关闭\""}>关闭</button></div><div class="ti-pet-stage" data-pet-stage-theme="dark">${petSpriteHtml(pet, action[0], true, true)}</div><div class="ti-pet-action-grid">${actions.map(([id, label]) => `<button class="ti-button" type="button" data-pet-action="${id}" data-active="${id === action[0]}">${escapeHtml(label)}</button>`).join("")}</div><div class="ti-pet-action-tune"><label class="ti-field-label">动作调优提示词</label><textarea class="ti-control ti-small-textarea" data-pet-action-prompt maxlength="6000" placeholder="只描述这个动作需要调整的姿态、节奏或表情；原宠物形象会作为强制身份参考">${escapeHtml(petActionEditor?.petId === pet.id && petActionEditor?.slot === action[0] ? petActionEditor.prompt || "" : "")}</textarea><div class="ti-inline ti-pet-generation-actions"><button class="ti-button" type="button" data-pet-action-reference>添加参考图</button><button class="ti-button" data-primary="true" type="button" data-pet-action-generate="${escapeHtml(action[0])}">生成动作候选</button>${petActionEditor?.petId === pet.id && petActionEditor?.slot === action[0] && petActionEditor.candidate ? `<button class="ti-button" data-primary="true" type="button" data-pet-action-apply>采用此动作</button>` : ""}</div>${petActionEditor?.petId === pet.id && petActionEditor?.slot === action[0] && petActionEditor.candidatePreviewUrl ? `<div class="ti-pet-action-candidate"><img src="${escapeHtml(petActionEditor.candidatePreviewUrl)}" alt="动作候选条带"><span>候选只会在确认后替换原动作；其他动作保持不变。</span></div>` : ""}</div><div class="ti-pet-modal-footer"><span>192 × 208 实际尺寸 · V${pet.spriteVersionNumber} 动作图集</span><div class="ti-inline"><button class="ti-button" type="button" data-pet-stage-toggle>切换背景</button><button class="ti-button" type="button" data-pet-install="${escapeHtml(pet.id)}">${pet.installed ? "重新安装" : "安装到 Codex"}</button></div></div></div></div>`;
  }

  function findPet(id) { return (state?.pets || []).find(pet => pet.id === id); }

  function openPetCreator() {
    petCreatorMode = "base";
    petCreatorOpen = true;
    petActionUseUploadedBase = false;
    aiPetName ||= `${draft?.name || "Codex"} 伙伴`;
    aiPetDescription ||= `为“${draft?.name || "Codex"}”创建的宠物`;
    void restorePetCreatorCheckpoint();
  }

  async function restorePetCreatorCheckpoint() {
    if (!draft?.id) return;
    const restoreEpoch = ++petCreatorRestoreEpoch;
    petCreatorRestoring = true;
    aiPetGenerationStage = "idle";
    aiPetBaseAsset = null;
    aiPetBasePreviewUrl = "";
    aiPetActionAssets = [];
    petCreatorProgress = { state:"planning", message:"正在读取已保存的基础形象", items:[] };
    renderPanel();
    try {
      const restored = await call("ai.pet.restore", { themeId:draft.id });
      if (restoreEpoch !== petCreatorRestoreEpoch || !petCreatorOpen) return;
      if (restored?.base) {
        aiPetName = restored.name || aiPetName;
        aiPetDescription = restored.description || aiPetDescription;
        aiPetPrompt = restored.prompt || aiPetPrompt;
        aiPetActionPrompt = restored.actionPrompt || aiPetActionPrompt;
        aiPetActionPrompts = restored.actionPrompts || (restored.actionPrompt ? Object.fromEntries(petActions.map(([id]) => [`pet.${id}`, restored.actionPrompt])) : aiPetActionPrompts);
        aiImageConcurrency = Math.max(1, Math.min(4, Number(restored.imageConcurrency) || aiImageConcurrency));
        aiPetGenerationStage = restored.stage || "base_ready";
        aiPetBaseAsset = restored.base;
        aiPetBasePreviewAttemptedKey = "";
        aiPetBasePreviewUrl = petGeneratedPreviewUrl(restored.base);
        aiPetActionAssets = restored.actions || [];
        aiPetActionPreviewsLoading = false;
        petCreatorProgress = { state:"completed", message:restored.stage === "actions_ready" ? "已恢复待确认的动作素材" : restored.stage === "actions_partial" ? "已恢复基础形象和部分动作" : "已恢复已保存的基础形象", items:[] };
      } else {
        aiPetGenerationStage = "idle";
        aiPetBaseAsset = null;
        aiPetBasePreviewUrl = "";
        aiPetActionAssets = [];
        petCreatorProgress = { state:"idle", items:[] };
      }
    } catch (error) {
      if (restoreEpoch === petCreatorRestoreEpoch && petCreatorOpen) setStatus(error?.message || "读取已保存基础形象失败", "error");
    } finally {
      if (restoreEpoch === petCreatorRestoreEpoch) {
        petCreatorRestoring = false;
        if (petCreatorOpen) renderPanel();
      }
    }
  }

  function petCreatorProgressHtml() {
    const items = petCreatorProgress.items || [];
    const completed = items.filter(item => item.status === "completed").length;
    const failed = items.filter(item => item.status === "failed").length;
    const active = items.filter(item => item.status === "generating");
    const total = items.length;
    const percent = total ? Math.round(completed / total * 100) : busy ? 8 : ["base_ready", "actions_ready"].includes(aiPetGenerationStage) ? 100 : 0;
    if (petCreatorRestoring) {
      return `<div class="ti-pet-creator-progress" data-pet-creator-progress data-state="planning"><div class="ti-generation-phase"><strong>正在读取基础形象</strong><span>检查是否有可以继续使用的已保存形象和动作素材。</span></div></div>`;
    }
    if (!busy && aiPetGenerationStage === "idle" && !total) {
      const idleTitle = petCreatorMode === "actions" ? "选择动作生成来源" : "独立生成基础形象";
      const idleCopy = petCreatorMode === "actions" ? "使用之前保存的基础形象，或上传一张基础图，再单独生成完整动作。" : "这里只生成和保存角色基础形象，不会自动开始生成动作。";
      return `<div class="ti-pet-creator-progress" data-pet-creator-progress data-state="idle"><div class="ti-generation-phase"><strong>${idleTitle}</strong><span>${idleCopy}</span></div></div>`;
    }
    const title = busy
      ? active.length ? `正在生成：${active.map(item => item.label).join("、")}` : petCreatorProgress.message || "正在准备宠物生成"
      : aiPetGenerationStage === "base_ready" ? "基础形象等待确认"
      : aiPetGenerationStage === "actions_partial" ? "部分动作已保存"
      : aiPetGenerationStage === "actions_ready" ? "动作素材等待确认"
      : aiPetGenerationStage === "completed" ? "宠物已加入宠物库"
      : "准备创建宠物";
    return `<div class="ti-generation-overview ti-pet-creator-progress" data-pet-creator-progress data-state="${escapeHtml(petCreatorProgress.state || "idle")}"><div class="ti-generation-phase"><strong>${escapeHtml(title)}</strong><span>${escapeHtml(petCreatorProgress.message || (busy ? "生成会在后台继续，可以等待当前阶段完成。" : petCreatorMode === "actions" ? "基础形象保持不变，动作可以分批生成并继续。" : "基础形象会独立保存，稍后可用于动作生成。"))}</span></div><div class="ti-progress-track"><span style="width:${percent}%"></span></div><div class="ti-generation-counts"><span>${completed}/${total || "—"} 已完成</span>${failed ? `<span>${failed} 失败</span>` : ""}</div></div>`;
  }

  function syncPetCreatorProgressPanel() {
    if (!petCreatorOpen || activeTab !== "pets") return;
    const panel = document.getElementById(PANEL_ID);
    if (!panel || panel.dataset.open !== "true") return;
    const current = panel.querySelector("[data-pet-creator-progress]");
    if (!current) return;
    const container = document.createElement("div");
    container.innerHTML = petCreatorProgressHtml();
    const next = container.firstElementChild;
    if (next) localizeElement(next);
    if (next && current.outerHTML !== next.outerHTML) current.replaceWith(next);
  }

  function petCreatorHtml() {
    const generating = busy && activeGenerationKind === "pet";
    const locked = generating || petCreatorRestoring || Boolean(petActionPromptSuggestSlot);
    const isActions = petCreatorMode === "actions";
    const clear = isActions ? "" : `<button class="ti-button" type="button" data-pet-creator-clear title="放弃已保存的基础形象并重新开始" ${locked ? "disabled" : ""}>重新开始</button>`;
    const tabs = `<div class="ti-studio-mode-tabs ti-pet-creator-tabs" role="tablist" aria-label="宠物生成步骤"><button class="ti-studio-tab" type="button" role="tab" aria-selected="${!isActions}" data-pet-creator-tab="base" data-active="${!isActions}" ${locked ? "disabled" : ""}>基础形象</button><button class="ti-studio-tab" type="button" role="tab" aria-selected="${isActions}" data-pet-creator-tab="actions" data-active="${isActions}" ${locked ? "disabled" : ""}>动作生成</button></div>`;
    return `<div class="ti-pet-creator" data-pet-creator-mode="${petCreatorMode}"><div class="ti-pet-creator-header"><div><strong>新建宠物</strong><span>先确定基础形象，再单独生成并确认动作</span></div><div class="ti-inline ti-pet-creator-header-actions">${clear}<button class="ti-button" type="button" data-pet-creator-back ${generating ? "data-busy-allow title=\"返回宠物库，生成任务继续在后台运行\"" : "title=\"返回宠物库\""}>${generating ? "后台运行" : "返回"}</button></div></div>${tabs}${petCreatorProgressHtml()}${isActions ? petActionsCreatorHtml(locked) : petBaseCreatorHtml(locked)}${generating ? `<div class="ti-inline ti-pet-generation-actions ti-pet-creator-primary-actions"><button class="ti-button" data-danger="true" type="button" data-pet-creator-interrupt>中断生成</button></div>` : ""}</div>`;
  }

  function petCreatorMetadataHtml(generating, includeBasePrompt) {
    if (includeBasePrompt) return `<div class="ti-pet-creator-section" data-pet-base-settings><div class="ti-section-title">基础形象描述</div><textarea class="ti-control ti-small-textarea" data-pet-creator-prompt maxlength="6000" placeholder="描述角色种类、造型、配色、材质和性格" ${generating ? "disabled" : ""}>${escapeHtml(aiPetPrompt)}</textarea></div>`;
    const concurrency = row("统一图片并发", `<select class="ti-control" data-pet-creator-concurrency ${generating ? "disabled" : ""}>${[1,2,3,4].map(value => `<option value="${value}" ${aiImageConcurrency === value ? "selected" : ""}>${value}</option>`).join("")}</select>`);
    return `<div class="ti-pet-creator-section" data-pet-action-settings><div class="ti-section-title">宠物信息</div>${row("宠物名称", `<input class="ti-control" data-pet-creator-meta="name" maxlength="80" value="${escapeHtml(aiPetName)}" ${generating ? "disabled" : ""}>`)}${row("宠物介绍", `<input class="ti-control" data-pet-creator-meta="description" maxlength="240" value="${escapeHtml(aiPetDescription)}" ${generating ? "disabled" : ""}>`)}${concurrency}</div>`;
  }

  function petActionPromptSettingsHtml(generating) {
    const suggesting = Boolean(petActionPromptSuggestSlot);
    const rows = petActions.map(([id, label, , frames, , trigger]) => {
      const slot = `pet.${id}`;
      return `<div class="ti-pet-action-prompt-row"><div class="ti-pet-action-prompt-heading"><div><strong>${escapeHtml(label)}</strong><span>${frames} 帧 · 触发：${escapeHtml(trigger)}</span></div><button class="ti-button" type="button" data-pet-action-prompt-suggest="${escapeHtml(slot)}" ${generating || suggesting ? "disabled" : ""}>${petActionPromptSuggestSlot === slot ? "生成中…" : "AI 生成"}</button></div><textarea class="ti-control ti-small-textarea" data-pet-action-prompt-slot="${escapeHtml(slot)}" maxlength="6000" placeholder="可选：描述这一个动作的姿态、节奏和表情；留空则使用内置完整循环" ${generating || suggesting ? "disabled" : ""}>${escapeHtml(aiPetActionPrompts[slot] || "")}</textarea></div>`;
    }).join("");
    return `<div class="ti-pet-creator-section ti-pet-action-prompts"><div class="ti-pet-action-prompts-header"><div><div class="ti-section-title">逐项动作要求</div><span class="ti-description">帧数和触发条件由 Codex 固定。提示词只补充动作表现，基础形象、尺寸和中心点会保持锁定。</span></div><button class="ti-button" type="button" data-pet-action-prompts-suggest-all ${generating || suggesting ? "disabled" : ""}>${petActionPromptSuggestSlot === "*" ? "生成中…" : "AI 生成全部"}</button></div><div class="ti-pet-action-prompt-list">${rows}</div></div>`;
  }

  function petBaseCreatorHtml(generating) {
    const start = aiPetBaseAsset ? "" : `<div class="ti-inline ti-pet-generation-actions ti-pet-creator-primary-actions"><button class="ti-button" data-primary="true" type="button" data-pet-creator-generate-base ${generating ? "disabled" : ""}>生成新的基础形象</button></div>`;
    return `${petCreatorMetadataHtml(generating, true)}${petCreatorReferenceHtml("base")}${petBaseReviewHtml()}${start}`;
  }

  function petActionsCreatorHtml(generating) {
    const hasSavedBase = Boolean(aiPetBaseAsset && ["base_ready", "actions_partial", "actions_ready"].includes(aiPetGenerationStage));
    const usingSavedBase = hasSavedBase && !petActionUseUploadedBase;
    const canStart = usingSavedBase || (petActionUseUploadedBase && aiPetReferences.length > 0);
    const sourcePreview = usingSavedBase ? petSavedBaseHtml() : "";
    const uploadedPreview = petActionUseUploadedBase ? `<div data-pet-creator-reference-list>${petReferenceListHtml("当前只使用这一张图片作为动作生成的基础形象。")}</div>` : "";
    const sourceCopy = usingSavedBase ? "已保存基础形象会作为动作生成的唯一身份锚点。" : "上传一张基础形象后，直接生成完整动作。";
    const sourceControls = `<div class="ti-inline ti-pet-generation-actions"><button class="ti-button" type="button" data-pet-creator-base-select ${generating ? "disabled" : ""}>${usingSavedBase ? "改用上传基础图" : "上传基础形象"}</button>${hasSavedBase && petActionUseUploadedBase ? `<button class="ti-button" type="button" data-pet-use-saved-base ${generating ? "disabled" : ""}>使用已保存基础形象</button>` : ""}</div>`;
    const sources = `<div class="ti-pet-creator-section"><div class="ti-section-title">动作基础形象</div>${sourcePreview}${uploadedPreview}<span class="ti-description">${sourceCopy}</span>${sourceControls}</div>`;
    const startLabel = aiPetGenerationStage === "actions_partial" ? "继续生成未完成动作" : "开始生成动作";
    const start = aiPetGenerationStage === "actions_ready" ? "" : `<div class="ti-inline ti-pet-generation-actions ti-pet-creator-primary-actions"><button class="ti-button" data-primary="true" type="button" data-pet-creator-start-actions ${!canStart || generating ? "disabled" : ""}>${startLabel}</button></div>`;
    return `${petCreatorMetadataHtml(generating, false)}${sources}${petActionPromptSettingsHtml(generating)}${petActionsReviewHtml()}${start}`;
  }

  function petCreatorReferenceHtml(kind) {
    const copy = kind === "base" ? "这些图片只用于生成和校准基础形象。没有参考图时会复用主题工作台参考图，再没有则只使用文字描述。" : "动作参考图不会替换当前基础形象，只用于动作姿态和连续性。";
    return `<div class="ti-pet-creator-section"><div class="ti-section-title">${kind === "base" ? "基础形象参考图" : "动作参考图"}</div><div data-pet-creator-reference-list>${petReferenceListHtml()}</div><span class="ti-description">${copy}</span><div class="ti-inline ti-pet-generation-actions"><button class="ti-button" type="button" data-pet-creator-reference-select ${busy ? "disabled" : ""}>添加参考图</button></div></div>`;
  }

  function currentPetGenerationRequest(theme = draft, resume = false, useUploadedAsBase = false) {
    const references = petCreatorMode === "actions"
      ? useUploadedAsBase ? aiPetReferences.slice(0, 1) : []
      : aiPetReferences.length ? aiPetReferences : aiReferences;
    return {
      theme,
      name: aiPetName,
      description: aiPetDescription,
      prompt: aiPetPrompt.trim() || aiPrompt.trim() || theme?.description || theme?.name || "Codex 宠物",
      actionPrompt: aiPetActionPrompt.trim(),
      actionPrompts: Object.fromEntries(Object.entries(aiPetActionPrompts).filter(([, value]) => value.trim()).map(([slot, value]) => [slot, value.trim()])),
      references: references.map(({ session, path }) => ({ session, path })),
      imageConcurrency: aiImageConcurrency,
      resume,
      phase: "complete",
      lockFirstReferenceIdentity: petCreatorMode === "base" && aiPetReferences.length > 0,
      useFirstReferenceAsBase: useUploadedAsBase,
    };
  }

  function petGeneratedPreviewUrl(asset) {
    if (isPreviewDataUrl(asset?.previewUrl)) return asset.previewUrl;
    return thumbnailUrls.get(previewCacheKey(asset)) || thumbnailUrls.get(asset?.path) || "";
  }

  function petGeneratedPreviewAsset(asset) {
    return asset?.previewPath ? { ...asset, path:asset.previewPath, previewUrl:"" } : asset;
  }

  function petActionsReviewHtml() {
    if (petCreatorMode !== "actions") return "";
    if (busy && activeGenerationKind === "pet") return "";
    if (aiPetGenerationStage !== "actions_ready") return "";
    if (!aiPetActionPreviewsLoading && aiPetActionAssets.some(asset => !petGeneratedPreviewUrl(asset))) void ensurePetCreatorActionPreviews();
    const labels = Object.fromEntries(petActions.map(([id, label]) => [`pet.${id}`, label]));
    const assets = (aiPetActionAssets || []).map(asset => { const previewUrl = petGeneratedPreviewUrl(asset); return `<div class="ti-pet-action-review-item">${isPreviewImageUrl(previewUrl) ? `<img src="${escapeHtml(previewUrl)}" alt="${escapeHtml(labels[asset.slot] || asset.slot)}动作候选" loading="lazy" decoding="async">` : `<div class="ti-pet-review-loading" aria-label="正在加载动作预览"></div>`}<span>${escapeHtml(labels[asset.slot] || asset.slot)}</span></div>`; }).join("");
    return `<div class="ti-pet-base-review ti-pet-actions-review"><strong>确认宠物动作</strong><span class="ti-description">基础形象保持锁定。确认后会装配图集、加入宠物库并绑定当前主题；入库后仍可单独调优任一动作。</span><div class="ti-pet-action-review-grid">${assets || `<div class="ti-studio-empty">动作预览正在加载…</div>`}</div><div class="ti-inline ti-pet-generation-actions"><button class="ti-button" type="button" data-pet-creator-actions-regenerate ${busy ? "disabled" : ""}>补齐未完成动作</button><button class="ti-button" data-primary="true" type="button" data-pet-creator-publish ${busy || aiPetActionPreviewsLoading ? "disabled" : ""}>确认动作并加入宠物库</button></div></div>`;
  }

  async function ensurePetCreatorActionPreviews() {
    if (aiPetGenerationStage !== "actions_ready" || aiPetActionPreviewsLoading || !aiPetActionAssets.length) return;
    aiPetActionPreviewsLoading = true;
    const assets = aiPetActionAssets;
    try {
      await mapConcurrent(assets, 1, async asset => {
        if (petGeneratedPreviewUrl(asset)) return;
        const url = await previewDataUrl(petGeneratedPreviewAsset(asset), 2, true);
        if (url && assets.includes(asset)) asset.previewUrl = url;
      });
    } finally {
      if (assets === aiPetActionAssets) aiPetActionPreviewsLoading = false;
      if (petCreatorOpen && aiPetGenerationStage === "actions_ready") renderPanel();
    }
  }

  async function ensurePetCreatorBasePreview() {
    const key = previewCacheKey(aiPetBaseAsset);
    if (!aiPetBaseAsset || isPreviewImageUrl(aiPetBasePreviewUrl) || aiPetBasePreviewLoading || aiPetBasePreviewAttemptedKey === key) return;
    aiPetBasePreviewLoading = true;
    aiPetBasePreviewAttemptedKey = key;
    const asset = aiPetBaseAsset;
    try {
      const url = petGeneratedPreviewUrl(asset) || await previewDataUrl(petGeneratedPreviewAsset(asset), 4, true);
      if (asset === aiPetBaseAsset && url) aiPetBasePreviewUrl = url;
    } finally {
      aiPetBasePreviewLoading = false;
      if (petCreatorOpen && asset === aiPetBaseAsset) renderPanel();
    }
  }

  function petBaseReviewHtml() {
    if (petCreatorMode !== "base") return "";
    if (!aiPetBaseAsset) return "";
    if (!isPreviewImageUrl(aiPetBasePreviewUrl)) void ensurePetCreatorBasePreview();
    return `<div class="ti-pet-base-review"><strong>已保存的基础形象</strong>${isPreviewImageUrl(aiPetBasePreviewUrl) ? `<img src="${escapeHtml(aiPetBasePreviewUrl)}" alt="已保存的宠物基础形象" decoding="async">` : `<div class="ti-pet-review-loading" aria-label="正在加载基础形象"></div>`}<span class="ti-description">该形象已独立保存。可以切换到“动作生成”，也可以返回宠物库后再继续。</span><div class="ti-inline ti-pet-generation-actions"><button class="ti-button" type="button" data-pet-creator-regenerate-base ${busy ? "disabled" : ""}>按描述重新生成</button><button class="ti-button" data-primary="true" type="button" data-pet-creator-open-actions ${busy ? "disabled" : ""}>进入动作生成</button></div></div>`;
  }

  function petSavedBaseHtml() {
    if (!aiPetBaseAsset) return "";
    if (!isPreviewImageUrl(aiPetBasePreviewUrl)) void ensurePetCreatorBasePreview();
    return `<div class="ti-pet-base-review ti-pet-saved-base"><strong>当前已保存基础形象</strong>${isPreviewImageUrl(aiPetBasePreviewUrl) ? `<img src="${escapeHtml(aiPetBasePreviewUrl)}" alt="动作生成使用的基础形象" decoding="async">` : `<div class="ti-pet-review-loading" aria-label="正在加载基础形象"></div>`}<span class="ti-description">动作生成会锁定该形象的脸、轮廓、比例、配色、材质、标记与已有道具。</span></div>`;
  }

  async function choosePetCreatorReference() {
    if (busy) return;
    try {
      const result = await call("ai.reference.import");
      await addPetReferences(result.references || [result]);
      renderPanel();
    } catch (error) {
      setStatus(error?.message || "宠物参考图加载失败", "error");
      renderPanel();
    }
  }

  async function choosePetCreatorBaseReference() {
    if (busy) return;
    try {
      const result = await call("ai.reference.import");
      const selected = (result.references || [result]).filter(Boolean);
      if (!selected.length) return;
      aiPetReferences = [];
      await addPetReferences([selected[0]]);
      if (petCreatorMode === "actions") petActionUseUploadedBase = true;
      setStatus("基础形象已上传，确认后再开始生成动作", "success");
      renderPanel();
    } catch (error) {
      setStatus(error?.message || "宠物基础形象加载失败", "error");
      renderPanel();
    }
  }

  async function suggestPetActionPrompts(slot = "") {
    if (busy || petActionPromptSuggestSlot) return;
    petActionPromptSuggestSlot = slot || "*";
    renderPanel();
    try {
      await saveAiSettings();
      if (!state.ai?.hasApiKey) throw new Error("请先在 AI 生成设置中填写并保存基础 API Key");
      const result = await call("ai.pet.action.prompts.suggest", {
        prompt: aiPetPrompt.trim() || aiPrompt.trim() || draft?.description || draft?.name || "Codex 宠物",
        actionPrompts: aiPetActionPrompts,
        slot,
      });
      aiPetActionPrompts = { ...aiPetActionPrompts, ...(result.actionPrompts || {}) };
      setStatus(slot ? "已生成当前动作提示词，可继续手动调整" : "已生成全部动作提示词，可逐项调整", "success");
    } catch (error) {
      setStatus(error?.message || "智能生成动作提示词失败", "error");
    } finally {
      petActionPromptSuggestSlot = "";
      renderPanelIfVisible();
    }
  }

  function petCreatorGenerationIsActive(epoch) {
    return !destroyed && epoch === generationEpoch && busy && activeGenerationKind === "pet";
  }

  async function pollPetCreatorProgress(epoch) {
    clearTimeout(petCreatorTimer);
    if (!petCreatorGenerationIsActive(epoch)) return;
    const [progress, logs] = await Promise.all([
      call("ai.progress.get").catch(() => petCreatorProgress),
      call("ai.log.get").catch(() => petCreatorRequestLog),
    ]);
    if (!petCreatorGenerationIsActive(epoch)) return;
    petCreatorProgress = progress;
    petCreatorRequestLog = logs;
    syncGenerationIsland();
    syncPetCreatorProgressPanel();
    if (petCreatorGenerationIsActive(epoch)) petCreatorTimer = setTimeout(() => void pollPetCreatorProgress(epoch), 1000);
  }

  async function generatePetCreatorBase() {
    if (busy) return;
    const epoch = ++generationEpoch;
    busy = true;
    activeGenerationKind = "pet";
    petCreatorProgress = { state:"planning", message:"正在生成宠物基础形象", items:[] };
    renderPanel();
    void pollPetCreatorProgress(epoch);
    try {
      await saveAiSettings();
      if (!state.ai?.hasImageApiKey) throw new Error("请先在 AI 生成设置中填写并保存生图 Key");
      const request = currentPetGenerationRequest(draft, false, false);
      const result = await call("ai.pet.base.generate", request);
      if (!petCreatorGenerationIsActive(epoch)) return;
      aiPetGenerationStage = result.stage;
      aiPetBaseAsset = result.base || null;
      aiPetBasePreviewAttemptedKey = "";
      aiPetBasePreviewUrl = result.base ? petGeneratedPreviewUrl(result.base) || await previewDataUrl(result.base, 4, true) : "";
      aiPetActionAssets = result.actions || [];
      aiPetActionPreviewsLoading = false;
      setStatus("基础形象已生成并保存，可稍后单独生成动作", "success");
    } catch (error) {
      petCreatorProgress = await call("ai.progress.get").catch(() => ({ state:"failed", message:error?.message || "宠物基础形象生成失败", items:[] }));
      petCreatorProgress.state = "failed";
      setStatus(error?.message || "宠物基础形象生成失败", "error");
    } finally {
      if (epoch === generationEpoch) {
        busy = false;
        activeGenerationKind = "";
        clearTimeout(petCreatorTimer);
        renderPanelIfVisible();
      }
    }
  }

  async function generatePetCreatorActions(resume = false, useUploadedAsBase = false) {
    if (busy) return;
    if (useUploadedAsBase && !aiPetReferences.length) {
      setStatus("请先上传一张宠物基础形象", "error");
      renderPanel();
      return;
    }
    const epoch = ++generationEpoch;
    busy = true;
    activeGenerationKind = "pet";
    petCreatorProgress = { state:"planning", message:useUploadedAsBase ? "正在读取上传的基础形象并生成动作" : resume ? "正在读取宠物检查点并补齐未完成动作" : "正在生成完整宠物动作", items:[] };
    renderPanel();
    void pollPetCreatorProgress(epoch);
    try {
      await saveAiSettings();
      if (!state.ai?.hasImageApiKey) throw new Error("请先在 AI 生成设置中填写并保存生图 Key");
      const request = currentPetGenerationRequest(draft, resume, useUploadedAsBase);
      const result = await call("ai.pet.actions.generate", request);
      if (!petCreatorGenerationIsActive(epoch)) return;
      aiPetGenerationStage = result.stage;
      aiPetBaseAsset = result.base || aiPetBaseAsset;
      aiPetBasePreviewAttemptedKey = "";
      aiPetBasePreviewUrl = result.base ? petGeneratedPreviewUrl(result.base) || await previewDataUrl(result.base, 2, true) : aiPetBasePreviewUrl;
      aiPetActionAssets = result.actions || [];
      aiPetActionPreviewsLoading = false;
      petActionUseUploadedBase = false;
      setStatus("动作已生成，请确认后加入宠物库", "warning");
    } catch (error) {
      petCreatorProgress = await call("ai.progress.get").catch(() => ({ state:"failed", message:error?.message || "宠物动作生成失败", items:[] }));
      petCreatorProgress.state = "failed";
      setStatus(error?.message || "宠物动作生成失败", "error");
    } finally {
      if (epoch === generationEpoch) {
        busy = false;
        activeGenerationKind = "";
        clearTimeout(petCreatorTimer);
        renderPanelIfVisible();
      }
    }
  }

  async function publishPetCreator() {
    if (busy) return;
    const epoch = ++generationEpoch;
    busy = true;
    activeGenerationKind = "pet";
    petCreatorProgress = { state:"finalizing", message:"正在装配动作图集并加入宠物库", items:petCreatorProgress.items || [] };
    renderPanel();
    void pollPetCreatorProgress(epoch);
    try {
      await saveAiSettings();
      const request = currentPetGenerationRequest(draft, true);
      const result = await call("ai.pet.publish", request);
      if (!petCreatorGenerationIsActive(epoch)) return;
      if (!result.pet) throw new Error("宠物图集尚未装配完成，请继续生成缺失动作");
      invalidatePetAsset(result.pet.id);
      await reloadPets();
      let bindingError = "";
      try {
        state = await call("theme.pet.bind", { themeId:draft.id, petId:result.pet.id });
        draft = clone(state.activeTheme);
        snapshot = clone(state.activeTheme);
        draftCustomCss = state.customCss || "";
      } catch (error) {
        bindingError = error?.message || "绑定当前主题失败";
      }
      await reloadPets();
      await hydratePetAssets([result.pet.id]);
      await clearPetCreatorState(true);
      petCreatorOpen = false;
      setStatus(bindingError ? `宠物已加入宠物库，但${bindingError}` : "宠物已加入宠物库并绑定当前主题", bindingError ? "warning" : "success");
    } catch (error) {
      petCreatorProgress = await call("ai.progress.get").catch(() => ({ state:"failed", message:error?.message || "宠物入库失败", items:[] }));
      petCreatorProgress.state = "failed";
      setStatus(error?.message || "宠物入库失败", "error");
    } finally {
      if (epoch === generationEpoch) {
        busy = false;
        activeGenerationKind = "";
        clearTimeout(petCreatorTimer);
        renderPanelIfVisible();
      }
    }
  }

  async function interruptPetCreatorGeneration() {
    if (!busy || activeGenerationKind !== "pet") return;
    const interruptedEpoch = generationEpoch;
    clearTimeout(petCreatorTimer);
    setStatus("正在中断宠物生成…", "warning");
    try {
      await call("ai.generation.cancel");
      generationEpoch += 1;
      busy = false;
      activeGenerationKind = "";
      petCreatorProgress = await call("ai.progress.get").catch(() => ({ state:"cancelled", message:"生成已中断，已完成素材可继续使用", items:petCreatorProgress.items || [] }));
      setStatus("宠物生成已中断，可继续上次生成", "warning");
    } catch (error) {
      if (interruptedEpoch === generationEpoch) setStatus(error?.message || "中断宠物生成失败", "error");
    }
    renderPanel();
  }

  async function clearPetCreatorState(cancelReferences = false) {
    const references = aiPetReferences;
    aiPetReferences = [];
    if (cancelReferences) await cancelPreviewSessions(references);
    references.forEach(item => { thumbnailUrls.delete(previewCacheKey(item)); thumbnailUrls.delete(item.path); });
    aiPetName = "";
    aiPetDescription = "";
    aiPetPrompt = "";
    aiPetActionPrompt = "";
    aiPetActionPrompts = {};
    petActionPromptSuggestSlot = "";
    aiPetGenerationStage = "idle";
    aiPetBaseAsset = null;
    aiPetBasePreviewUrl = "";
    aiPetBasePreviewLoading = false;
    aiPetBasePreviewAttemptedKey = "";
    aiPetActionAssets = [];
    aiPetActionPreviewsLoading = false;
    petCreatorProgress = { state:"idle", items:[] };
    petCreatorRequestLog = [];
    petCreatorMode = "";
    petActionUseUploadedBase = false;
  }

  async function restartPetBaseCreator() {
    if (busy || !draft?.id) return;
    try {
      await call("ai.pet.discard", { themeId:draft.id });
      await clearPetCreatorState(true);
      openPetCreator();
      setStatus("已放弃原基础形象，可以重新生成", "success");
    } catch (error) {
      setStatus(error?.message || "重新开始基础形象失败", "error");
      renderPanel();
    }
  }

  function bindPetLibraryEvents(panel) {
    panel.querySelector("[data-pet-create]")?.addEventListener("click", () => openPetCreator());
    panel.querySelectorAll("[data-pet-creator-tab]").forEach(button => button.addEventListener("click", () => {
      if (busy || petCreatorRestoring) return;
      petCreatorMode = button.dataset.petCreatorTab === "actions" ? "actions" : "base";
      renderPanel();
    }));
    panel.querySelector("[data-pet-creator-back]")?.addEventListener("click", () => { petCreatorOpen = false; renderPanel(); });
    panel.querySelector("[data-pet-creator-clear]")?.addEventListener("click", () => void restartPetBaseCreator());
    panel.querySelector("[data-pet-creator-open-actions]")?.addEventListener("click", () => { petCreatorMode = "actions"; setStatus("基础形象已保存，可以开始生成动作", "success"); renderPanel(); });
    panel.querySelectorAll("[data-pet-creator-meta]").forEach(input => input.addEventListener("input", () => { if (input.dataset.petCreatorMeta === "name") aiPetName = input.value; else aiPetDescription = input.value; }));
    panel.querySelectorAll("[data-pet-creator-prompt]").forEach(input => input.addEventListener("input", () => { aiPetPrompt = input.value; panel.querySelectorAll("[data-pet-creator-prompt]").forEach(peer => { if (peer !== input) peer.value = input.value; }); }));
    panel.querySelectorAll("[data-pet-action-prompt-slot]").forEach(input => input.addEventListener("input", event => { aiPetActionPrompts[event.target.dataset.petActionPromptSlot] = event.target.value; }));
    panel.querySelectorAll("[data-pet-action-prompt-suggest]").forEach(button => button.addEventListener("click", () => void suggestPetActionPrompts(button.dataset.petActionPromptSuggest)));
    panel.querySelector("[data-pet-action-prompts-suggest-all]")?.addEventListener("click", () => void suggestPetActionPrompts());
    panel.querySelector("[data-pet-creator-concurrency]")?.addEventListener("change", event => { aiImageConcurrency = Math.max(1, Math.min(4, Number(event.target.value) || 4)); });
    panel.querySelectorAll("[data-pet-creator-reference-select]").forEach(button => button.addEventListener("click", () => void choosePetCreatorReference()));
    panel.querySelectorAll("[data-pet-creator-base-select]").forEach(button => button.addEventListener("click", () => void choosePetCreatorBaseReference()));
    panel.querySelectorAll("[data-pet-creator-reference-remove]").forEach(button => button.addEventListener("click", () => void removePetReference(Number(button.dataset.petCreatorReferenceRemove)).then(() => renderPanel())));
    panel.querySelector("[data-pet-creator-generate-base]")?.addEventListener("click", () => void generatePetCreatorBase());
    panel.querySelector("[data-pet-creator-regenerate-base]")?.addEventListener("click", () => void generatePetCreatorBase());
    panel.querySelector("[data-pet-use-saved-base]")?.addEventListener("click", () => { petActionUseUploadedBase = false; setStatus("已切换为保存的基础形象", "success"); renderPanel(); });
    panel.querySelector("[data-pet-creator-start-actions]")?.addEventListener("click", () => void generatePetCreatorActions(!petActionUseUploadedBase, petActionUseUploadedBase));
    panel.querySelector("[data-pet-creator-actions-regenerate]")?.addEventListener("click", () => void generatePetCreatorActions(true));
    panel.querySelector("[data-pet-creator-publish]")?.addEventListener("click", () => void publishPetCreator());
    panel.querySelectorAll("[data-pet-creator-interrupt]").forEach(button => button.addEventListener("click", () => void interruptPetCreatorGeneration()));
    panel.querySelector("[data-pet-import]")?.addEventListener("click", () => void runAction("等待选择宠物包…", async () => { await call("pet.library.import"); await reloadPets(); await hydratePetAssets(); }, "宠物已加入宠物库"));
    panel.querySelectorAll("[data-pet-preview]").forEach(button => button.addEventListener("click", () => { const pet = findPet(button.dataset.petPreview); if (!pet) return; petPreviewAction = "idle"; modal = { kind:"pet-preview", pet }; renderPanel(); if (!pet.assetUrl) void hydratePetAssets([pet.id]); }));
    panel.querySelectorAll("[data-pet-install]").forEach(button => button.addEventListener("click", () => void runAction("正在验证并安装宠物…", async () => { await call("pet.library.install", { id:button.dataset.petInstall }); await reloadPets(); if (modal?.pet?.id === button.dataset.petInstall) modal.pet = findPet(button.dataset.petInstall); }, "宠物已安装，请在 Codex 的宠物设置中选择")));
    panel.querySelectorAll("[data-pet-bind]").forEach(button => button.addEventListener("click", () => void runAction(button.dataset.bound === "true" ? "正在取消主题绑定…" : "正在绑定配套宠物…", async () => { state = await call("theme.pet.bind", { themeId:draft.id, petId:button.dataset.bound === "true" ? null : button.dataset.petBind }); draft = clone(state.activeTheme); snapshot = clone(state.activeTheme); }, button.dataset.bound === "true" ? "已取消主题宠物绑定" : "宠物已绑定，导出主题时会一并携带")));
    panel.querySelectorAll("[data-pet-export]").forEach(button => button.addEventListener("click", () => { closePanelMenus(panel); void runAction("等待选择导出位置…", () => call("pet.library.export", { id:button.dataset.petExport }), "宠物已导出"); }));
    panel.querySelectorAll("[data-pet-edit]").forEach(button => button.addEventListener("click", () => { closePanelMenus(panel); const pet = findPet(button.dataset.petEdit); if (pet) { modal = { kind:"pet-edit", pet }; renderPanel(); } }));
    panel.querySelectorAll("[data-pet-delete]").forEach(button => button.addEventListener("click", () => { closePanelMenus(panel); const pet = findPet(button.dataset.petDelete); if (pet) { modal = { kind:"pet-delete", pet }; renderPanel(); } }));
    panel.querySelector('[data-modal-form="pet-edit"]')?.addEventListener("submit", event => { event.preventDefault(); const pet = modal?.pet; const name = panel.querySelector("[data-modal-pet-name]")?.value.trim(); const description = panel.querySelector("[data-modal-pet-description]")?.value.trim() || ""; if (!pet || !name) { setStatus("请输入宠物名称", "error"); return; } void runAction("正在保存宠物信息…", async () => { const assetUrl = findPet(pet.id)?.assetUrl || pet.assetUrl || ""; const updated = await call("pet.library.metadata", { id:pet.id, name, description }); await reloadPets(); const current = findPet(pet.id); if (current) current.assetUrl = assetUrl; modal = null; }, pet.installed ? "宠物信息已保存；重新安装后同步到 Codex" : "宠物信息已保存"); });
    panel.querySelector("[data-pet-delete-confirm]")?.addEventListener("click", button => void runAction("正在删除宠物…", async () => { const id = button.currentTarget.dataset.petDeleteConfirm; await call("pet.library.delete", { id }); invalidatePetAsset(id); modal = null; await reloadPets(); }, "宠物已从宠物库删除"));
    panel.querySelectorAll("[data-pet-action]").forEach(button => button.addEventListener("click", () => { petPreviewAction = button.dataset.petAction; renderPanel(); }));
    panel.querySelector("[data-pet-action-prompt]")?.addEventListener("input", event => {
      petActionEditor ||= { petId:modal?.pet?.id || "", slot:petPreviewAction, prompt:"", references:[], candidate:null, candidatePreviewUrl:"" };
      petActionEditor.prompt = event.target.value;
    });
    panel.querySelector("[data-pet-action-reference]")?.addEventListener("click", () => void choosePetActionReferences());
    panel.querySelector("[data-pet-action-generate]")?.addEventListener("click", button => void generatePetActionCandidate(modal.pet, button.currentTarget.dataset.petActionGenerate));
    panel.querySelector("[data-pet-action-apply]")?.addEventListener("click", () => void applyPetActionCandidate(modal.pet));
    panel.querySelector("[data-pet-stage-toggle]")?.addEventListener("click", () => { const stage = panel.querySelector("[data-pet-stage-theme]"); if (stage) stage.dataset.petStageTheme = stage.dataset.petStageTheme === "dark" ? "light" : "dark"; });
  }

  async function choosePetActionReferences() {
    const result = await call("ai.reference.import");
    petActionEditor ||= { petId:modal?.pet?.id || "", slot:petPreviewAction, prompt:"", references:[], candidate:null, candidatePreviewUrl:"" };
    petActionEditor.references = result.references || [result];
    setStatus(`已添加 ${petActionEditor.references.length} 张动作参考图`, "success");
  }

  async function generatePetActionCandidate(pet, slot) {
    if (busy) return;
    petActionEditor ||= { petId:pet.id, slot, prompt:"", references:[], candidate:null, candidatePreviewUrl:"" };
    petActionEditor.petId = pet.id;
    petActionEditor.slot = slot;
    const label = availablePetActions(pet).find(([id]) => id === slot)?.[1] || slot;
    let succeeded = false;
    activeGenerationKind = "pet";
    petCreatorProgress = { state:"generating", message:`正在生成${label}动作候选`, items:[{ slot:`pet.${slot}`, label, status:"generating" }] };
    syncGenerationIsland();
    try {
      await runAction("正在生成动作候选…", async () => {
        const candidate = await call("ai.pet.action.generate", { petId:pet.id, slot:`pet.${slot}`, prompt:petActionEditor.prompt || `保持原宠物形象，仅优化${label}动作`, references:petActionEditor.references || [] });
        petActionEditor.candidate = candidate;
        petActionEditor.candidatePreviewUrl = await previewDataUrl(candidate, 2, true) || candidate.previewUrl || "";
        succeeded = true;
        renderPanelIfVisible();
      }, "动作候选已生成，请确认后采用");
    } finally {
      petCreatorProgress = { state:succeeded ? "completed" : "failed", message:succeeded ? `${label}动作候选已生成` : `${label}动作候选生成失败`, items:[{ slot:`pet.${slot}`, label, status:succeeded ? "completed" : "failed" }] };
      activeGenerationKind = "";
      syncGenerationIsland();
    }
  }

  async function applyPetActionCandidate(pet) {
    if (!petActionEditor?.candidate) return;
    await runAction("正在替换动作并备份原宠物…", async () => {
      await call("ai.pet.action.apply", { petId:pet.id, slot:petActionEditor.candidate.slot, session:petActionEditor.candidate.session, path:petActionEditor.candidate.path });
      invalidatePetAsset(pet.id);
      await reloadPets();
      const refreshed = findPet(pet.id);
      if (refreshed) modal.pet = refreshed;
      await hydratePetAssets([pet.id]);
      petActionEditor = null;
    }, "动作已替换，原宠物已备份");
  }
