  // Runtime module: ambient-effects
  function stopAmbientEffect() {
    cancelAnimationFrame(ambientEffectFrame);
    ambientEffectFrame = 0;
    ambientEffectWorker?.terminate();
    ambientEffectWorker = null;
    ambientEffectMetrics = null;
    ambientEffectVisibilityCleanup?.();
    ambientEffectVisibilityCleanup = null;
    ambientEffectSignature = "";
    document.getElementById(AMBIENT_EFFECT_ID)?.remove();
  }
  function syncAmbientEffect(theme) {
    const effects = theme.effects || {};
    if (!effects.ambientEnabled) { stopAmbientEffect(); return; }
    const fallbackColor = effects.ambientEffect === "fireflies" || effects.ambientEffect === "petals" ? theme.tokens.accent : theme.tokens.foreground;
    const ambientColor = normalizeColor(effects.ambientColor || fallbackColor);
    const signature = JSON.stringify([effects.ambientEffect, effects.ambientIntensity, effects.ambientSpeed, effects.ambientOpacity, ambientColor]);
    const existingCanvas = document.getElementById(AMBIENT_EFFECT_ID);
    if (signature === ambientEffectSignature && existingCanvas) {
      const width = innerWidth, height = innerHeight, scale = ambientCanvasScale(width, height);
      existingCanvas.style.width = `${width}px`;
      existingCanvas.style.height = `${height}px`;
      ambientEffectWorker?.postMessage({ type: "resize", width, height, scale });
      return;
    }
    stopAmbientEffect();
    ambientEffectSignature = signature;
    const canvas = document.createElement("canvas");
    canvas.id = AMBIENT_EFFECT_ID;
    canvas.setAttribute("aria-hidden", "true");
    canvas.style.width = `${innerWidth}px`;
    canvas.style.height = `${innerHeight}px`;
    document.body.appendChild(canvas);
    const intensity = Math.max(.1, Math.min(1, effects.ambientIntensity ?? .5));
    const speed = Math.max(.25, Math.min(2.5, effects.ambientSpeed ?? 1));
    const opacity = Math.max(.1, Math.min(1, effects.ambientOpacity ?? .65));
    const effect = effects.ambientEffect || "snow";
    const baseCount = ({ rain: 110, meteor: 34, snow: 72, stars: 54, fireflies: 36, petals: 46 })[effect] || 48;
    const count = Math.round(baseCount * intensity);
    if (startAmbientEffectWorker(canvas, { effect, count, speed, opacity, color: ambientColor })) return;
    let context = null;
    try { context = canvas.getContext("2d", { alpha: true, desynchronized: true }); } catch {}
    if (!context) { stopAmbientEffect(); return; }
    const particles = Array.from({ length: count }, () => createAmbientParticle(effect, ambientColor));
    let last = performance.now();
    const draw = now => {
      if (!canvas.isConnected || ambientEffectSignature !== signature) return;
      ambientEffectFrame = requestAnimationFrame(draw);
      if (document.hidden || now - last < AMBIENT_FRAME_INTERVAL - 1) return;
      const width = innerWidth, height = innerHeight;
      const scale = ambientCanvasScale(width, height);
      if (canvas.width !== Math.round(width * scale) || canvas.height !== Math.round(height * scale)) { canvas.width = Math.round(width * scale); canvas.height = Math.round(height * scale); canvas.style.width = `${width}px`; canvas.style.height = `${height}px`; }
      context.setTransform(scale, 0, 0, scale, 0, 0);
      context.clearRect(0, 0, width, height);
      const delta = Math.min(50, now - last) / 16.67 * speed; last = now;
      for (const particle of particles) drawAmbientParticle(context, particle, effect, delta, width, height, opacity, ambientColor);
    };
    ambientEffectFrame = requestAnimationFrame(draw);
  }

  function ambientCanvasScale(width, height) {
    return Math.min(1, devicePixelRatio || 1, Math.sqrt(AMBIENT_MAX_PIXELS / Math.max(1, width * height)));
  }

  function startAmbientEffectWorker(canvas, options) {
    if (ambientEffectWorkerDisabled || typeof Worker !== "function" || typeof OffscreenCanvas !== "function" || typeof canvas.transferControlToOffscreen !== "function") return false;
    let worker = null;
    let objectUrl = "";
    try {
      objectUrl = URL.createObjectURL(new Blob([`(${ambientEffectWorkerMain.toString()})()`], { type: "text/javascript" }));
      worker = new Worker(objectUrl);
      worker.onmessage = event => {
        if (event.data?.type === "metrics") ambientEffectMetrics = event.data;
      };
      worker.onerror = event => {
        event.preventDefault();
        ambientEffectWorkerDisabled = true;
        worker.terminate();
        if (ambientEffectWorker === worker) ambientEffectWorker = null;
        ambientEffectVisibilityCleanup?.();
        ambientEffectVisibilityCleanup = null;
        ambientEffectSignature = "";
        canvas.remove();
        if (!destroyed && draft) syncAmbientEffect(draft);
      };
      const offscreen = canvas.transferControlToOffscreen();
      ambientEffectWorker = worker;
      canvas.dataset.renderMode = "worker";
      const width = innerWidth, height = innerHeight, scale = ambientCanvasScale(width, height);
      worker.postMessage({ type: "start", canvas: offscreen, width, height, scale, hidden: document.hidden, ...options }, [offscreen]);
      const syncVisibility = () => worker.postMessage({ type: "visibility", hidden: document.hidden });
      document.addEventListener("visibilitychange", syncVisibility);
      ambientEffectVisibilityCleanup = () => document.removeEventListener("visibilitychange", syncVisibility);
      return true;
    } catch (error) {
      worker?.terminate();
      ambientEffectWorker = null;
      ambientEffectWorkerDisabled = true;
      canvas.remove();
      ambientEffectSignature = "";
      queueMicrotask(() => { if (!destroyed && draft) syncAmbientEffect(draft); });
      return true;
    } finally {
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    }
  }

  function createAmbientParticle(effect, color) {
    const width = innerWidth, height = innerHeight;
    const particle = {
      x: Math.random() * width, y: Math.random() * height,
      size: effect === "petals" ? 4 + Math.random() * 7 : effect === "snow" ? 1.5 + Math.random() * 4 : 1 + Math.random() * 3,
      speed: .4 + Math.random() * 1.5, drift: (Math.random() - .5) * .8,
      phase: Math.random() * Math.PI * 2, life: Math.random(), rotation: Math.random() * Math.PI * 2,
      spin: (Math.random() - .5) * .035, twinkle: .018 + Math.random() * .04,
    };
    particle.sprite = createAmbientSprite(effect, particle, color);
    return particle;
  }

  function createAmbientSprite(effect, particle, color) {
    if (!matchesAmbientSpriteEffect(effect)) return null;
    const size = particle.size;
    const extent = effect === "stars" ? size * 6 + 18 : effect === "fireflies" ? size * 5 + 24 : effect === "petals" ? size * 2.2 + 8 : size * 2.5 + 3;
    const sprite = document.createElement("canvas");
    const side = Math.max(8, Math.ceil(extent * 2));
    sprite.width = side;
    sprite.height = side;
    const context = sprite.getContext("2d");
    if (!context) return null;
    context.translate(side / 2, side / 2);
    context.fillStyle = color;
    context.strokeStyle = color;
    if (effect === "snow") {
      context.lineWidth = Math.max(.65, size * .18);
      if (size < 2.4) {
        context.beginPath(); context.arc(0, 0, size, 0, Math.PI * 2); context.fill();
      } else {
        for (let arm = 0; arm < 6; arm += 1) {
          context.rotate(Math.PI / 3); context.beginPath(); context.moveTo(0, 0); context.lineTo(0, size * 2.3);
          context.moveTo(0, size * 1.25); context.lineTo(-size * .52, size * 1.75);
          context.moveTo(0, size * 1.25); context.lineTo(size * .52, size * 1.75); context.stroke();
        }
      }
    } else if (effect === "stars") {
      const radius = size * 2.5;
      context.shadowColor = color; context.shadowBlur = 5 + radius * 1.5;
      context.beginPath(); context.moveTo(0, -radius); context.quadraticCurveTo(radius * .18, -radius * .18, radius, 0); context.quadraticCurveTo(radius * .18, radius * .18, 0, radius); context.quadraticCurveTo(-radius * .18, radius * .18, -radius, 0); context.quadraticCurveTo(-radius * .18, -radius * .18, 0, -radius); context.fill();
    } else if (effect === "fireflies") {
      context.shadowColor = color; context.shadowBlur = 10 + size * 5;
      context.beginPath(); context.arc(0, 0, size * .72, 0, Math.PI * 2); context.fill();
      context.globalAlpha = .3; context.beginPath(); context.arc(0, 0, size * 2.2, 0, Math.PI * 2); context.fill();
    } else if (effect === "petals") {
      context.shadowColor = color; context.shadowBlur = size * .65;
      context.beginPath(); context.moveTo(0, -size);
      context.bezierCurveTo(size * .85, -size * .5, size * .72, size * .72, 0, size * 1.15);
      context.bezierCurveTo(-size * .72, size * .72, -size * .85, -size * .5, 0, -size); context.fill();
      context.globalAlpha = .45; context.lineWidth = .7; context.beginPath(); context.moveTo(0, -size * .65); context.lineTo(0, size * .75); context.stroke();
    }
    return { canvas: sprite, size: side };
  }

  function matchesAmbientSpriteEffect(effect) {
    return effect === "snow" || effect === "stars" || effect === "fireflies" || effect === "petals";
  }

  function drawAmbientSprite(context, particle, scale = 1) {
    if (!particle.sprite) return false;
    const size = particle.sprite.size * scale;
    context.drawImage(particle.sprite.canvas, -size / 2, -size / 2, size, size);
    return true;
  }

  function drawAmbientParticle(context, particle, effect, delta, width, height, opacity, color) {
    context.globalAlpha = opacity * (.4 + particle.life * .6);
    context.fillStyle = color;
    context.strokeStyle = context.fillStyle;
    if (effect === "meteor") {
      context.lineWidth = particle.size; context.beginPath(); context.moveTo(particle.x, particle.y); context.lineTo(particle.x - 32 * particle.size, particle.y - 18 * particle.size); context.stroke();
      particle.x += 5 * particle.speed * delta; particle.y += 3 * particle.speed * delta;
    } else if (effect === "rain") {
      context.lineWidth = Math.max(1, particle.size / 2); context.beginPath(); context.moveTo(particle.x, particle.y); context.lineTo(particle.x - 3, particle.y + 18 * particle.size); context.stroke();
      particle.x -= .35 * delta; particle.y += 9 * particle.speed * delta;
    } else if (effect === "snow") {
      particle.phase += .018 * delta;
      context.save();
      context.translate(particle.x, particle.y); context.rotate(particle.rotation += particle.spin * delta);
      drawAmbientSprite(context, particle);
      context.restore();
      particle.x += (particle.drift + Math.sin(particle.phase) * .35) * delta; particle.y += (.5 + particle.speed * .65) * delta;
    } else if (effect === "stars") {
      particle.phase += particle.twinkle * delta;
      const pulse = .32 + Math.pow((Math.sin(particle.phase) + 1) / 2, 2) * .68;
      context.save();
      context.globalAlpha *= pulse; context.translate(particle.x, particle.y); context.rotate(particle.rotation);
      drawAmbientSprite(context, particle, .62 + pulse * .38);
      context.restore();
      particle.y += .025 * delta;
    } else if (effect === "fireflies") {
      particle.phase += particle.twinkle * delta;
      const wave = Math.sin(particle.phase), glow = .45 + (wave + 1) * .275;
      context.save(); context.globalAlpha *= glow; context.translate(particle.x, particle.y);
      drawAmbientSprite(context, particle, .82 + glow * .18);
      context.restore();
      particle.x += (particle.drift * .55 + wave * .52) * delta; particle.y += (Math.cos(particle.phase * .7) * .38 - .08) * delta;
    } else if (effect === "petals") {
      particle.phase += .022 * delta; particle.rotation += particle.spin * delta;
      context.save();
      context.translate(particle.x, particle.y); context.rotate(particle.rotation + Math.sin(particle.phase) * .6);
      drawAmbientSprite(context, particle);
      context.restore();
      particle.x += (particle.drift + Math.sin(particle.phase) * .7) * delta; particle.y += (.75 + particle.speed * .7) * delta;
    }
    if (particle.x < -80 || particle.x > width + 80 || particle.y > height + 50) {
      particle.x = Math.random() * width;
      particle.y = effect === "stars" ? Math.random() * height : -40;
      particle.phase = Math.random() * Math.PI * 2;
    }
  }

  function ambientEffectWorkerMain() {
    let canvas = null, context = null, width = 0, height = 0, scale = 1, effect = "snow", speed = 1, opacity = .65, color = "#FFFFFF", particles = [], timer = 0, hidden = false, last = performance.now(), measuredAt = last, measuredFrames = 0, measuredMaxGap = 0;
    const interval = 1000 / 60;
    const resize = message => {
      width = message.width;
      height = message.height;
      scale = message.scale;
      const pixelWidth = Math.round(width * scale), pixelHeight = Math.round(height * scale);
      if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) { canvas.width = pixelWidth; canvas.height = pixelHeight; }
    };
    const createSprite = particle => {
      if (!["snow", "stars", "fireflies", "petals"].includes(effect)) return null;
      const size = particle.size;
      const extent = effect === "stars" ? size * 6 + 18 : effect === "fireflies" ? size * 5 + 24 : effect === "petals" ? size * 2.2 + 8 : size * 2.5 + 3;
      const side = Math.max(8, Math.ceil(extent * 2));
      const sprite = new OffscreenCanvas(side, side), spriteContext = sprite.getContext("2d");
      spriteContext.translate(side / 2, side / 2); spriteContext.fillStyle = color; spriteContext.strokeStyle = color;
      if (effect === "snow") {
        spriteContext.lineWidth = Math.max(.65, size * .18);
        if (size < 2.4) { spriteContext.beginPath(); spriteContext.arc(0, 0, size, 0, Math.PI * 2); spriteContext.fill(); }
        else for (let arm = 0; arm < 6; arm += 1) { spriteContext.rotate(Math.PI / 3); spriteContext.beginPath(); spriteContext.moveTo(0, 0); spriteContext.lineTo(0, size * 2.3); spriteContext.moveTo(0, size * 1.25); spriteContext.lineTo(-size * .52, size * 1.75); spriteContext.moveTo(0, size * 1.25); spriteContext.lineTo(size * .52, size * 1.75); spriteContext.stroke(); }
      } else if (effect === "stars") {
        const radius = size * 2.5; spriteContext.shadowColor = color; spriteContext.shadowBlur = 5 + radius * 1.5;
        spriteContext.beginPath(); spriteContext.moveTo(0, -radius); spriteContext.quadraticCurveTo(radius * .18, -radius * .18, radius, 0); spriteContext.quadraticCurveTo(radius * .18, radius * .18, 0, radius); spriteContext.quadraticCurveTo(-radius * .18, radius * .18, -radius, 0); spriteContext.quadraticCurveTo(-radius * .18, -radius * .18, 0, -radius); spriteContext.fill();
      } else if (effect === "fireflies") {
        spriteContext.shadowColor = color; spriteContext.shadowBlur = 10 + size * 5; spriteContext.beginPath(); spriteContext.arc(0, 0, size * .72, 0, Math.PI * 2); spriteContext.fill(); spriteContext.globalAlpha = .3; spriteContext.beginPath(); spriteContext.arc(0, 0, size * 2.2, 0, Math.PI * 2); spriteContext.fill();
      } else {
        spriteContext.shadowColor = color; spriteContext.shadowBlur = size * .65; spriteContext.beginPath(); spriteContext.moveTo(0, -size); spriteContext.bezierCurveTo(size * .85, -size * .5, size * .72, size * .72, 0, size * 1.15); spriteContext.bezierCurveTo(-size * .72, size * .72, -size * .85, -size * .5, 0, -size); spriteContext.fill(); spriteContext.globalAlpha = .45; spriteContext.lineWidth = .7; spriteContext.beginPath(); spriteContext.moveTo(0, -size * .65); spriteContext.lineTo(0, size * .75); spriteContext.stroke();
      }
      return { canvas: sprite, size: side };
    };
    const createParticle = () => {
      const particle = { x: Math.random() * width, y: Math.random() * height, size: effect === "petals" ? 4 + Math.random() * 7 : effect === "snow" ? 1.5 + Math.random() * 4 : 1 + Math.random() * 3, speed: .4 + Math.random() * 1.5, drift: (Math.random() - .5) * .8, phase: Math.random() * Math.PI * 2, life: Math.random(), rotation: Math.random() * Math.PI * 2, spin: (Math.random() - .5) * .035, twinkle: .018 + Math.random() * .04 };
      particle.sprite = createSprite(particle);
      return particle;
    };
    const drawSprite = (particle, spriteScale = 1) => { const size = particle.sprite.size * spriteScale; context.drawImage(particle.sprite.canvas, -size / 2, -size / 2, size, size); };
    const drawParticle = (particle, delta) => {
      context.globalAlpha = opacity * (.4 + particle.life * .6); context.fillStyle = color; context.strokeStyle = color;
      if (effect === "meteor") { context.lineWidth = particle.size; context.beginPath(); context.moveTo(particle.x, particle.y); context.lineTo(particle.x - 32 * particle.size, particle.y - 18 * particle.size); context.stroke(); particle.x += 5 * particle.speed * delta; particle.y += 3 * particle.speed * delta; }
      else if (effect === "rain") { context.lineWidth = Math.max(1, particle.size / 2); context.beginPath(); context.moveTo(particle.x, particle.y); context.lineTo(particle.x - 3, particle.y + 18 * particle.size); context.stroke(); particle.x -= .35 * delta; particle.y += 9 * particle.speed * delta; }
      else if (effect === "snow") { particle.phase += .018 * delta; context.save(); context.translate(particle.x, particle.y); context.rotate(particle.rotation += particle.spin * delta); drawSprite(particle); context.restore(); particle.x += (particle.drift + Math.sin(particle.phase) * .35) * delta; particle.y += (.5 + particle.speed * .65) * delta; }
      else if (effect === "stars") { particle.phase += particle.twinkle * delta; const pulse = .32 + Math.pow((Math.sin(particle.phase) + 1) / 2, 2) * .68; context.save(); context.globalAlpha *= pulse; context.translate(particle.x, particle.y); context.rotate(particle.rotation); drawSprite(particle, .62 + pulse * .38); context.restore(); particle.y += .025 * delta; }
      else if (effect === "fireflies") { particle.phase += particle.twinkle * delta; const wave = Math.sin(particle.phase), glow = .45 + (wave + 1) * .275; context.save(); context.globalAlpha *= glow; context.translate(particle.x, particle.y); drawSprite(particle, .82 + glow * .18); context.restore(); particle.x += (particle.drift * .55 + wave * .52) * delta; particle.y += (Math.cos(particle.phase * .7) * .38 - .08) * delta; }
      else { particle.phase += .022 * delta; particle.rotation += particle.spin * delta; context.save(); context.translate(particle.x, particle.y); context.rotate(particle.rotation + Math.sin(particle.phase) * .6); drawSprite(particle); context.restore(); particle.x += (particle.drift + Math.sin(particle.phase) * .7) * delta; particle.y += (.75 + particle.speed * .7) * delta; }
      if (particle.x < -80 || particle.x > width + 80 || particle.y > height + 50) { particle.x = Math.random() * width; particle.y = effect === "stars" ? Math.random() * height : -40; particle.phase = Math.random() * Math.PI * 2; }
    };
    const draw = () => {
      if (hidden || !context) return;
      const now = performance.now(), elapsed = now - last, delta = Math.min(50, elapsed) / 16.67 * speed; last = now;
      context.setTransform(scale, 0, 0, scale, 0, 0); context.clearRect(0, 0, width, height);
      for (const particle of particles) drawParticle(particle, delta);
      measuredFrames += 1; measuredMaxGap = Math.max(measuredMaxGap, elapsed);
      const measuredDuration = now - measuredAt;
      if (measuredDuration >= 2000) { postMessage({ type: "metrics", effect, fps: measuredFrames * 1000 / measuredDuration, maxGap: measuredMaxGap, width: canvas.width, height: canvas.height }); measuredAt = now; measuredFrames = 0; measuredMaxGap = 0; }
    };
    self.onmessage = event => {
      const message = event.data;
      if (message.type === "start") { canvas = message.canvas; context = canvas.getContext("2d", { alpha: true, desynchronized: true }); effect = message.effect; speed = message.speed; opacity = message.opacity; color = message.color; hidden = message.hidden; resize(message); particles = Array.from({ length: message.count }, createParticle); last = performance.now(); clearInterval(timer); timer = setInterval(draw, interval); }
      else if (message.type === "resize") resize(message);
      else if (message.type === "visibility") { hidden = message.hidden; last = performance.now(); measuredAt = last; measuredFrames = 0; measuredMaxGap = 0; }
    };
  }

  function effectsHtml() {
    const effects = draft.effects;
    const ambientOptions = [["meteor","流星"],["rain","下雨"],["snow","下雪"],["stars","星光"],["fireflies","萤火虫"],["petals","花瓣"]];
    const ambientFallback = effects.ambientEffect === "fireflies" || effects.ambientEffect === "petals" ? draft.tokens.accent : draft.tokens.foreground;
    const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
    return `<div class="ti-card">${row("圆角", rangeInput("shape.radius", draft.shape.radius, 0, 40, 1))}${row("边框宽度", rangeInput("shape.borderWidth", draft.shape.borderWidth, 0, 4, 1))}${row("浮层透明度", rangeInput("effects.panelOpacity", effects.panelOpacity, .25, 1, .01))}${row("全局模糊", rangeInput("effects.backgroundBlur", effects.backgroundBlur, 0, 80, 1))}${row("全局饱和度", rangeInput("effects.saturation", effects.saturation, .5, 2, .05))}${row("阴影强度", rangeInput("effects.shadowStrength", effects.shadowStrength, 0, 1, .05))}</div><div class="ti-section-title ti-section-gap">动态特效</div><div class="ti-card">${row("启用特效", `<label class="ti-switch"><input type="checkbox" data-path="effects.ambientEnabled" ${effects.ambientEnabled ? "checked" : ""}><span>${effects.ambientEnabled ? "已启用" : "已关闭"}</span></label>`)}${row("内置效果", `<select class="ti-control" data-path="effects.ambientEffect" ${effects.ambientEnabled ? "" : "disabled"}>${ambientOptions.map(([value,label]) => `<option value="${value}" ${effects.ambientEffect === value ? "selected" : ""}>${label}</option>`).join("")}</select>`)}${row("特效颜色", colorInput("effects.ambientColor", effects.ambientColor || ambientFallback))}${row("强度", rangeInput("effects.ambientIntensity", effects.ambientIntensity ?? .5, .1, 1, .05))}${row("速度", rangeInput("effects.ambientSpeed", effects.ambientSpeed ?? 1, .25, 2.5, .05))}${row("透明度", rangeInput("effects.ambientOpacity", effects.ambientOpacity ?? .65, .1, 1, .05))}<div class="ti-notice">启用后会立即在整个 Codex 窗口上方显示实时动画；画布不接收鼠标操作。${reducedMotion ? "这是显式启用项，因此会覆盖 Windows 的“减少动态效果”设置。" : ""}</div></div>`;
  }

  function mergeGeneratedAssets(next, assets) {
    for (const asset of assets || []) {
      const slot = asset.slot || "";
      if (slot === "background.fullscreen") draft.background.fullscreen = clone(next.background.fullscreen);
      else if (slot === "background.content") draft.background.content = clone(next.background.content);
      else if (slot === "background.sidebar") draft.background.sidebar = clone(next.background.sidebar);
      else if (slot.startsWith("skin.icons.") && slot !== "skin.icons.atlas") {
        const action = slot.slice("skin.icons.".length);
        const path = next.skin?.icons?.mappings?.[action];
        if (path) {
          draft.skin.enabled = true;
          draft.skin.icons ||= { mode: "custom", mappings: {} };
          draft.skin.icons.mode = "custom";
          draft.skin.icons.mappings ||= {};
          draft.skin.icons.mappings[action] = path;
        }
      } else if (slot.startsWith("skin.home.cards.")) {
        const index = Number(slot.split(".")[3]);
        if (Number.isInteger(index) && next.skin?.home?.cards?.[index]) {
          draft.skin.enabled = true;
          draft.skin.home ||= { cards: [] };
          draft.skin.home.cards ||= [];
          draft.skin.home.cards[index] = clone(next.skin.home.cards[index]);
        }
      } else if (slot.startsWith("skin.decorations.")) {
        const index = Number(slot.split(".")[2]);
        if (Number.isInteger(index)) {
          draft.skin.enabled = true;
          draft.skin.decorations ||= [];
          const existing = draft.skin.decorations[index] || {};
          const generated = next.skin?.decorations?.[index] || {};
          draft.skin.decorations[index] = {
            region: "content", anchor: "bottom-right", offsetX: 24, offsetY: 24, width: 180, opacity: 1, layer: 1,
            ...existing,
            ...clone(generated),
            asset: asset.path || generated.asset || existing.asset || "",
          };
          studioSelectedDecoration = index;
        }
      } else if (slot.startsWith("skin.")) {
        const resourceKey = slot.slice("skin.".length);
        const path = next.skin?.resources?.[resourceKey];
        if (path) {
          draft.skin.enabled = true;
          draft.skin.resources[resourceKey] = path;
        }
      }
    }
  }
