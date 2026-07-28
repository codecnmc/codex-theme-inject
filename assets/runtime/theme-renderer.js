  // Runtime module: theme-renderer
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
    const rightSidebarBase = normalizeColor(chrome.rightSidebarBackground || tokens.inputBackground);
    const rightSidebarForeground = safeForeground(rightSidebarBase, chrome.rightSidebarForeground || tokens.foreground);
    const rightSidebarMuted = safeForeground(rightSidebarBase, chrome.rightSidebarMutedForeground || tokens.mutedForeground, 3.2);
    const rightSidebarIcon = safeForeground(rightSidebarBase, chrome.rightSidebarIcon || rightSidebarForeground, 3.2);
    const rightSidebarBorder = chrome.rightSidebarBorder || mixColor(rightSidebarBase, rightSidebarForeground, .18);
    const rightSidebarHover = chrome.rightSidebarHover || mixColor(rightSidebarBase, rightSidebarForeground, theme.baseMode === "dark" ? .13 : .08);
    const rightSidebarActive = chrome.rightSidebarActive || mixColor(rightSidebarBase, tokens.accent, .24);
    const rightSidebarActiveForeground = safeForeground(rightSidebarActive, chrome.rightSidebarActiveForeground || rightSidebarForeground);
    const rightSidebarShortcutBase = normalizeColor(chrome.rightSidebarShortcutBackground || tokens.elevatedBackground);
    const rightSidebarShortcutForeground = safeForeground(rightSidebarShortcutBase, chrome.rightSidebarShortcutForeground || rightSidebarMuted, 3.2);
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
    const composerMuted = safeForeground(composerBase, tokens.subtleForeground, 4.5);
    const accentForeground = safeForeground(normalizeColor(tokens.accent), tokens.accentForeground, 4.5);
    const popoverBase = normalizeColor((skin.enabled && surfaces.popover?.fill) || tokens.elevatedBackground);
    const popoverForeground = safeForeground(popoverBase, tokens.foreground);
    const popoverMuted = safeForeground(popoverBase, tokens.mutedForeground, 4.5);
    const codeBase = normalizeColor(theme.terminal?.background || tokens.codeBackground);
    const codeForeground = safeForeground(codeBase, theme.terminal?.foreground || tokens.codeForeground, 7);
    const codeMuted = safeForeground(codeBase, tokens.mutedForeground, 4.5);
    const codeAccent = safeTint(codeBase, tokens.accent, 4.5);
    const codeString = safeTint(codeBase, tokens.success, 4.5);
    const codeLiteral = safeTint(codeBase, tokens.warning, 4.5);
    const codeFunction = safeTint(codeBase, tokens.link, 4.5);
    const inlineCodeBase = normalizeColor(mixColor(contentBase, codeBase, .72));
    const inlineCodeForeground = safeForeground(inlineCodeBase, tokens.codeForeground, 4.5);
    const codeBorder = mixColor(codeBase, codeForeground, .2);
    const terminalCursor = safeForeground(codeBase, theme.terminal?.cursor || tokens.accent, 3);
    const diffSuccess = safeTint(codeBase, tokens.success, 4.5);
    const diffDanger = safeTint(codeBase, tokens.danger, 4.5);
    const diffInserted = rgbaFromHex(mixColor(codeBase, diffSuccess, .24), .84);
    const diffRemoved = rgbaFromHex(mixColor(codeBase, diffDanger, .24), .84);
    const diffInsertedText = rgbaFromHex(mixColor(codeBase, diffSuccess, .34), .9);
    const diffRemovedText = rgbaFromHex(mixColor(codeBase, diffDanger, .34), .9);
    const lineNumber = safeForeground(codeBase, tokens.mutedForeground, 3.2);
    const density = layout.density || 1, baseFontSize = 14 * (type.scale || 1);
    const sidebarWidth = Math.max(180, Math.min(360, Number(layout.sidebarWidth) || 300));
    const contentMaxWidth = Math.max(480, Math.min(2400, Number(layout.contentMaxWidth) || 960));
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
  --ti-subtle:${tokens.subtleForeground};--ti-border:${effectiveBorder};--ti-accent:${tokens.accent};--ti-accent-foreground:${accentForeground};
  --ti-selection:${tokens.selection};--ti-link:${tokens.link};--ti-success:${tokens.success};--ti-warning:${tokens.warning};--ti-danger:${tokens.danger};
  --ti-code-bg:${codeBase};--ti-code-fg:${codeForeground};--ti-code-muted:${codeMuted};--ti-code-accent:${codeAccent};--ti-code-string:${codeString};--ti-code-literal:${codeLiteral};--ti-code-function:${codeFunction};--ti-code-inline-bg:${inlineCodeBase};--ti-code-inline-fg:${inlineCodeForeground};--ti-code-border:${codeBorder};--ti-ui-font:${type.uiFont};--ti-mono-font:${type.monoFont};
  --ti-radius:${shape.radius}px;--ti-border-width:${shape.borderWidth}px;--ti-shadow:0 12px 34px rgba(0,0,0,${shadowAlpha});--ti-app-header-bottom:${headerBottom};--ti-content-header-bottom:${contentHeaderBottom};
  --ti-titlebar-bg:${titlebarBase};--ti-titlebar-fg:${titlebarForeground};--ti-titlebar-muted:${titlebarMuted};--ti-titlebar-border:${titlebarBorder};--ti-titlebar-hover:${titlebarHover};
  --ti-sidebar-fg:${sidebarForeground};--ti-sidebar-muted:${sidebarMuted};--ti-sidebar-icon:${sidebarIcon};--ti-sidebar-border:${sidebarBorder};--ti-sidebar-hover:${sidebarHover};--ti-sidebar-selected:${sidebarSelected};--ti-sidebar-selected-fg:${sidebarSelectedForeground};--ti-skin-sidebar-fill:${surfaces.sidebar?.fill || tokens.sidebarBackground};
  --ti-right-sidebar-bg:${rightSidebarBase};--ti-right-sidebar-fg:${rightSidebarForeground};--ti-right-sidebar-muted:${rightSidebarMuted};--ti-right-sidebar-icon:${rightSidebarIcon};--ti-right-sidebar-border:${rightSidebarBorder};--ti-right-sidebar-hover:${rightSidebarHover};--ti-right-sidebar-active:${rightSidebarActive};--ti-right-sidebar-active-fg:${rightSidebarActiveForeground};--ti-right-sidebar-shortcut-bg:${rightSidebarShortcutBase};--ti-right-sidebar-shortcut-fg:${rightSidebarShortcutForeground};
  --ti-content-fg:${contentForeground};--ti-content-muted:${contentMuted};--ti-hero-fg:${heroForeground};--ti-hero-muted:${heroMuted};--ti-hero-protection:rgba(${heroProtection},.88);--ti-hero-protection-soft:rgba(${heroProtection},.58);--ti-card-fg:${cardForeground};--ti-card-muted:${cardMuted};--ti-composer-fg:${composerForeground};--ti-composer-muted:${composerMuted};--ti-popover-fg:${popoverForeground};--ti-popover-muted:${popoverMuted};
  --ti-skin-icon:${skin.icons?.color || sidebarIcon};--ti-skin-icon-active:${skin.icons?.activeColor || tokens.accent};
  --ti-decoration-sidebar-width:${sidebarWidth}px;--thread-content-max-width:${contentMaxWidth}px;--conversation-item-gap:${Math.round(16 * density)}px;
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
.app-header-tint button,.app-header-tint [role="button"],.app-header-tint [role="menuitem"],.app-header-tint [role="menubar"]>*,[data-theme-inject-titlebar-text="true"],[data-theme-inject-titlebar-text="true"] *{color:var(--ti-titlebar-muted)!important;opacity:1!important;--token-foreground:var(--ti-titlebar-muted);--color-token-foreground:var(--ti-titlebar-muted);--color-token-icon-foreground:var(--ti-titlebar-muted);}
.app-header-tint button{border-radius:6px!important;box-shadow:none!important;filter:none!important;}
.app-header-tint button:hover,.app-header-tint [role="button"]:hover{background:var(--ti-titlebar-hover)!important;color:var(--ti-titlebar-fg)!important;}
[data-theme-inject-project-actions="true"]{color:var(--ti-titlebar-fg)!important;}
[data-theme-inject-project-action="true"]{background:var(--ti-titlebar-bg)!important;color:var(--ti-titlebar-fg)!important;border-color:var(--ti-titlebar-border)!important;--token-foreground:var(--ti-titlebar-fg);--color-token-foreground:var(--ti-titlebar-fg);--color-token-icon-foreground:var(--ti-titlebar-fg);}
[data-theme-inject-project-action="true"] :is(span,svg){color:inherit!important;}
[data-theme-inject-project-action="true"]:is(:hover,:focus-visible){background:var(--ti-titlebar-hover)!important;color:var(--ti-titlebar-fg)!important;}
[data-theme-inject-project-action="true"][data-state="open"]{background:var(--ti-titlebar-hover)!important;color:var(--ti-titlebar-fg)!important;}
.app-header-tint svg,[data-theme-inject-titlebar-text="true"] svg{color:currentColor!important;stroke:currentColor!important;opacity:1!important;}
.app-header-tint svg [fill]:not([fill="none"]),[data-theme-inject-titlebar-text="true"] svg [fill]:not([fill="none"]){fill:currentColor!important;}
.app-header-tint svg [stroke]:not([stroke="none"]),[data-theme-inject-titlebar-text="true"] svg [stroke]:not([stroke="none"]){stroke:currentColor!important;}
.app-header-tint [data-testid="app-shell-header-context-menu-surface"],.app-header-tint [data-testid="app-shell-header-context-menu-surface"] *{color:var(--ti-titlebar-fg)!important;}
[data-theme-inject-task-header="true"]{left:var(--ti-main-left,0px)!important;right:0!important;width:auto!important;}
.app-shell-left-panel{position:relative!important;isolation:isolate;color:var(--ti-sidebar-fg)!important;background-color:${sidebarActive ? "transparent" : "var(--ti-sidebar)"}!important;border-color:var(--ti-sidebar-border)!important;box-sizing:border-box!important;overflow-x:hidden!important;--height-token-row:${Math.round(30*density)}px;--height-token-nav-row:${Math.round(30*density)}px;--padding-row-x:${Math.max(5,Math.round(8*density))}px;--token-foreground:var(--ti-sidebar-fg);--token-text-primary:var(--ti-sidebar-fg);--token-text-secondary:var(--ti-sidebar-muted);--token-text-tertiary:var(--ti-sidebar-muted);--token-muted-foreground:var(--ti-sidebar-muted);--token-list-hover-background:var(--ti-sidebar-hover);--color-token-side-bar-foreground:var(--ti-sidebar-fg);${skinEnabled ? surfaceCss(surfaces.sidebar,tokens.sidebarBackground,sidebarBorder,0,{radius:false,maxOpacity:sidebarOpacity}) : ""}}
.app-shell-left-panel>*{position:relative;z-index:1;}
.app-shell-left-panel{box-sizing:border-box;}.app-shell-left-panel *{box-sizing:border-box;max-width:100%;}
.app-shell-left-panel nav,.app-shell-left-panel [data-app-action-sidebar-section]{min-width:0;max-width:100%;overflow-x:hidden;}
.app-shell-left-panel :is([data-app-action-sidebar-thread-id],[data-app-action-sidebar-project-row]){min-width:0;max-width:100%;overflow:hidden;}
.app-shell-left-panel :is([data-app-action-sidebar-thread-id],[data-app-action-sidebar-project-row]) :is(span,div){min-width:0;max-width:100%;}
.app-shell-left-panel :is([data-app-action-sidebar-thread-id],[data-app-action-sidebar-project-row]) span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}
.app-shell-left-panel nav,.app-shell-left-panel [data-app-action-sidebar-section]{background-color:transparent!important;}
.app-shell-left-panel svg{color:var(--ti-sidebar-icon)!important;stroke:currentColor!important;}
.app-shell-left-panel [data-app-action-sidebar-section-heading],.app-shell-left-panel .text-token-text-tertiary,.app-shell-left-panel .text-token-muted-foreground{color:var(--ti-sidebar-muted)!important;}
.app-shell-left-panel [data-app-action-sidebar-thread-id],.app-shell-left-panel [data-app-action-sidebar-project-row]{color:var(--ti-sidebar-fg)!important;}
.app-shell-left-panel :is(button,[role="button"],[data-app-action-sidebar-thread-id],[data-app-action-sidebar-project-row]){color:var(--ti-sidebar-fg)!important;--token-foreground:var(--ti-sidebar-fg);--token-text-primary:var(--ti-sidebar-fg);--token-text-secondary:var(--ti-sidebar-muted);--color-token-foreground:var(--ti-sidebar-fg);--color-token-icon-foreground:var(--ti-sidebar-icon);}
.app-shell-left-panel :is(button,[role="button"]) :is(span,p,div){color:inherit!important;}
.app-shell-left-panel [data-app-action-sidebar-thread-id] :is(span,div),.app-shell-left-panel [data-app-action-sidebar-project-row] :is(span,div){color:inherit!important;}
.app-shell-left-panel [data-app-action-sidebar-thread-id]:hover,.app-shell-left-panel [data-app-action-sidebar-project-row]:hover{background-color:var(--ti-sidebar-hover)!important;}
.app-shell-left-panel [data-app-action-sidebar-thread-active="true"]{background-color:var(--ti-sidebar-selected)!important;color:var(--ti-sidebar-selected-fg)!important;}
[data-theme-inject-right-sidebar="true"]{color:var(--ti-right-sidebar-fg)!important;--token-foreground:var(--ti-right-sidebar-fg);--token-text-primary:var(--ti-right-sidebar-fg);--token-text-secondary:var(--ti-right-sidebar-muted);--token-text-tertiary:var(--ti-right-sidebar-muted);--color-token-foreground:var(--ti-right-sidebar-fg);--color-token-text-primary:var(--ti-right-sidebar-fg);--color-token-text-secondary:var(--ti-right-sidebar-muted);--color-token-icon-foreground:var(--ti-right-sidebar-icon);}
[data-theme-inject-right-sidebar-item="true"]{background:var(--ti-right-sidebar-bg)!important;color:var(--ti-right-sidebar-fg)!important;border-color:var(--ti-right-sidebar-border)!important;--token-foreground:var(--ti-right-sidebar-fg);--token-text-primary:var(--ti-right-sidebar-fg);--token-text-secondary:var(--ti-right-sidebar-muted);--token-text-tertiary:var(--ti-right-sidebar-muted);--color-token-icon-foreground:var(--ti-right-sidebar-icon);}
[data-theme-inject-right-sidebar-item="true"]>:first-child{flex:0 0 16px!important;width:16px!important;height:16px!important;margin:0!important;display:flex!important;align-items:center!important;justify-content:center!important;}
[data-theme-inject-right-sidebar-item="true"][data-theme-inject-custom-icon]>:first-child{display:none!important;}
[data-theme-inject-right-sidebar-item="true"] :is(span,p,strong,small){color:inherit!important;}[data-theme-inject-right-sidebar-item="true"] svg{color:var(--ti-right-sidebar-icon)!important;stroke:currentColor!important;}[data-theme-inject-right-sidebar-item="true"] svg [fill]:not([fill="none"]){fill:currentColor!important;}
[data-theme-inject-right-sidebar-item="true"]:hover,[data-theme-inject-right-sidebar-item="true"]:focus-visible{background:var(--ti-right-sidebar-hover)!important;color:var(--ti-right-sidebar-fg)!important;}
[data-theme-inject-right-sidebar-item="true"][aria-pressed="true"],[data-theme-inject-right-sidebar-item="true"][aria-selected="true"],[data-theme-inject-right-sidebar-item="true"][aria-current="true"],[data-theme-inject-right-sidebar-item="true"][data-state="open"],[data-theme-inject-right-sidebar-item="true"][data-state="active"],[data-theme-inject-right-sidebar-item="true"][data-active="true"],[data-theme-inject-right-sidebar-item="true"][data-selected="true"]{background:var(--ti-right-sidebar-active)!important;color:var(--ti-right-sidebar-active-fg)!important;}
[data-theme-inject-right-sidebar-shortcut="true"]{background:var(--ti-right-sidebar-shortcut-bg)!important;color:var(--ti-right-sidebar-shortcut-fg)!important;border-color:var(--ti-right-sidebar-border)!important;}
.app-shell-left-panel [data-app-action-sidebar-thread-active="true"] *,.app-shell-left-panel [data-app-action-sidebar-thread-active="true"] svg{color:var(--ti-sidebar-selected-fg)!important;}
.main-surface{position:relative!important;isolation:isolate;background-color:${contentActive ? "transparent" : "var(--ti-content)"}!important;color:var(--ti-content-fg)!important;--token-foreground:var(--ti-content-fg);--token-text-primary:var(--ti-content-fg);--token-text-secondary:var(--ti-content-muted);--color-token-foreground:var(--ti-content-fg);--color-token-text-primary:var(--ti-content-fg);--color-token-text-secondary:var(--ti-content-muted);${skinEnabled ? surfaceCss(surfaces.content,tokens.contentBackground,effectiveBorder,shape.radius,{maxOpacity:effectiveContentOpacity,blur:false}) : ""}}
.app-shell-main-content-viewport,.thread-scroll-container{background:transparent!important;}
.main-surface,.main-surface *{--thread-content-max-width:${contentMaxWidth}px!important;--composer-adjacent-max-width:calc(${contentMaxWidth}px + 22px)!important;}
#${BACKDROP_IDS.fullscreen},#${BACKDROP_IDS.content},#${BACKDROP_IDS.sidebar}{pointer-events:none!important;overflow:hidden!important;}
#${BACKDROP_IDS.content},#${BACKDROP_IDS.sidebar}{z-index:-1!important;}
[data-theme-inject-custom-icon]{position:relative;}
[data-theme-inject-custom-icon] :is(svg,[data-slot="icon"],[class*="icon"]){display:none!important;}
[data-theme-inject-custom-icon="project-app"] img[src^="apps/"],[data-theme-inject-custom-icon="project-app"] img[src*="/apps/"]{display:none!important;}
[data-theme-inject-custom-icon]::before{content:"";flex:0 0 auto;width:16px;height:16px;background-image:var(--ti-custom-icon);background-size:contain;background-position:center;background-repeat:no-repeat;}
[data-theme-inject-custom-icon="settings"]::before,[data-theme-inject-custom-icon="account"]::before{flex-basis:22px;width:22px;height:22px;}
[data-theme-inject-account-avatar-target="true"]:not(img){flex:0 0 auto;width:20px!important;height:20px!important;border-radius:50%;background:var(--ti-account-avatar) center/cover no-repeat!important;color:transparent!important;font-size:0!important;}
[data-theme-inject-account-avatar-target="true"]:is(svg) *{opacity:0!important;}
${skinEnabled && skin.icons?.mode !== "native" ? `.app-shell-left-panel button svg,.app-shell-left-panel [role="button"] svg{color:var(--ti-skin-icon)!important;stroke:currentColor!important;}.app-header-tint button svg,.app-header-tint [role="button"] svg{color:var(--ti-titlebar-muted)!important;stroke:currentColor!important;}.app-header-tint button:hover svg,.app-header-tint [role="button"]:hover svg{color:var(--ti-titlebar-fg)!important;}.app-shell-left-panel [data-app-action-sidebar-thread-active="true"] svg,.app-shell-left-panel [aria-pressed="true"] svg{color:var(--ti-skin-icon-active)!important;}` : ""}
:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"]){background:var(--ti-accent)!important;color:var(--ti-accent-foreground)!important;border-color:var(--ti-accent)!important;}
:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"]) svg{display:block!important;color:var(--ti-accent-foreground)!important;fill:currentColor!important;stroke:currentColor!important;opacity:1!important;}
[data-theme-inject-button-contrast="true"]{color:var(--ti-native-button-fg)!important;}
[data-theme-inject-button-contrast="true"] svg{color:var(--ti-native-button-fg)!important;fill:currentColor!important;stroke:currentColor!important;opacity:1!important;}
[data-turn-key],[data-codex-composer-root],.text-size-chat{font-size:${baseFontSize}px;line-height:${type.lineHeight};}
[data-thread-scroll-footer]{--thread-content-max-width:${contentMaxWidth}px;}
.composer-surface-chrome,[data-codex-composer-root] .composer-surface-chrome{background-color:var(--ti-input)!important;color:var(--ti-composer-fg)!important;border:var(--ti-border-width) solid var(--ti-border)!important;border-radius:var(--ti-radius)!important;box-shadow:var(--ti-shadow)!important;--token-foreground:var(--ti-composer-fg);--token-text-primary:var(--ti-composer-fg);--token-text-tertiary:var(--ti-composer-muted);${skinEnabled ? surfaceCss(surfaces.composer,tokens.inputBackground,effectiveBorder,shape.radius) : ""}}
[data-thread-find-composer="true"]{color:var(--ti-composer-muted)!important;--token-foreground:var(--ti-composer-muted);--token-text-primary:var(--ti-composer-muted);--token-text-secondary:var(--ti-composer-muted);--token-text-tertiary:var(--ti-composer-muted);--color-token-foreground:var(--ti-composer-muted);--color-token-text-primary:var(--ti-composer-muted);--color-token-text-secondary:var(--ti-composer-muted);--color-token-icon-foreground:var(--ti-composer-muted);--color-token-button-tertiary-foreground:var(--ti-composer-muted);}
[data-thread-find-composer="true"] :is(button,[role="button"]):not(:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"])){color:var(--ti-composer-muted)!important;}
[data-thread-find-composer="true"] :is(button,[role="button"]):not(:is([data-codex-composer-submit],[data-app-action-submit],[data-composer-navigation-target="submit"])):hover{color:var(--ti-composer-fg)!important;}
[data-thread-find-composer="true"] :is(svg,[data-slot="icon"]){color:currentColor!important;stroke:currentColor!important;opacity:1!important;}
[data-thread-find-composer="true"] [data-theme-inject-button-contrast="true"]{color:var(--ti-native-button-fg)!important;}
[data-thread-find-composer="true"] [data-theme-inject-button-contrast="true"] svg{display:block!important;color:var(--ti-native-button-fg)!important;fill:currentColor!important;stroke:currentColor!important;opacity:1!important;}
[data-thread-find-composer="true"] [data-theme-inject-custom-icon="send"]:not(:is([aria-label="停止"],[aria-label="停止回應"],[aria-label="Stop"],[aria-label="停止する"],[aria-label="중지"])) svg{display:none!important;}
[data-thread-find-composer="true"] [data-theme-inject-custom-icon="send"]:is([aria-label="停止"],[aria-label="停止回應"],[aria-label="Stop"],[aria-label="停止する"],[aria-label="중지"])::before{display:none!important;}
:is([data-thread-find-composer="true"] button[aria-label="停止"],[data-thread-find-composer="true"] button[aria-label="停止回應"],[data-thread-find-composer="true"] button[aria-label="Stop"],[data-thread-find-composer="true"] button[aria-label="停止する"],[data-thread-find-composer="true"] button[aria-label="중지"]){background:var(--ti-accent)!important;color:var(--ti-accent-foreground)!important;border-color:var(--ti-accent)!important;}
:is([data-thread-find-composer="true"] button[aria-label="停止"],[data-thread-find-composer="true"] button[aria-label="停止回應"],[data-thread-find-composer="true"] button[aria-label="Stop"],[data-thread-find-composer="true"] button[aria-label="停止する"],[data-thread-find-composer="true"] button[aria-label="중지"]) svg{display:block!important;color:var(--ti-accent-foreground)!important;fill:currentColor!important;stroke:currentColor!important;opacity:1!important;}
[data-composer-navigation-target="workspace-project"]{color:var(--ti-composer-fg)!important;--token-foreground:var(--ti-composer-fg);--token-text-primary:var(--ti-composer-fg);--token-text-secondary:var(--ti-composer-muted);--color-token-foreground:var(--ti-composer-fg);--color-token-icon-foreground:var(--ti-composer-fg);}
[data-composer-navigation-target="workspace-project"] :is(span,svg,[data-slot="icon"]){color:inherit!important;stroke:currentColor!important;opacity:1!important;}
:is([data-composer-navigation-target="run-location"],[data-composer-navigation-target="branch"]){color:var(--ti-composer-muted)!important;--token-foreground:var(--ti-composer-muted);--token-text-primary:var(--ti-composer-muted);--token-text-secondary:var(--ti-composer-muted);--color-token-foreground:var(--ti-composer-muted);--color-token-icon-foreground:var(--ti-composer-muted);}
:is([data-composer-navigation-target="run-location"],[data-composer-navigation-target="branch"]) :is(span,svg,[data-slot="icon"]){color:inherit!important;stroke:currentColor!important;opacity:1!important;}
.composer-surface-chrome :is([contenteditable="true"],textarea,input),.composer-surface-chrome :is([contenteditable="true"],textarea,input) *{color:var(--ti-composer-fg)!important;caret-color:var(--ti-composer-fg)!important;--token-foreground:var(--ti-composer-fg);--token-text-primary:var(--ti-composer-fg);--color-token-foreground:var(--ti-composer-fg);--color-token-text-primary:var(--ti-composer-fg);}
[data-codex-composer="true"]{color:var(--ti-composer-fg)!important;caret-color:var(--ti-composer-fg);}[data-codex-composer="true"] p,[data-codex-composer="true"] span{color:inherit!important;}[data-codex-composer="true"][data-placeholder]:empty::before,.composer-surface-chrome [contenteditable="true"][data-placeholder]:empty::before{color:var(--ti-composer-muted)!important;}
[role="dialog"],[role="menu"],[role="listbox"],[data-radix-popper-content-wrapper]>*{background-color:var(--ti-elevated)!important;color:var(--ti-popover-fg)!important;border-color:var(--ti-border)!important;--token-foreground:var(--ti-popover-fg);--token-text-primary:var(--ti-popover-fg);${skinEnabled ? surfaceCss(surfaces.popover,tokens.elevatedBackground,effectiveBorder,shape.radius) : ""}}
.rounded-3xl.bg-token-dropdown-background{background-color:var(--ti-elevated)!important;color:var(--ti-popover-fg)!important;border-color:var(--ti-border)!important;--token-foreground:var(--ti-popover-fg);--token-text-primary:var(--ti-popover-fg);--token-text-secondary:var(--ti-popover-muted);--token-text-tertiary:var(--ti-popover-muted);--color-token-bg-fog:var(--ti-elevated);--color-token-dropdown-background:var(--ti-elevated);}
[class~="group/turn-diff-header"] .bg-token-bg-fog,[role="tabpanel"][aria-label="审阅"] .bg-token-bg-fog{background-color:var(--ti-elevated)!important;color:var(--ti-popover-fg)!important;--token-foreground:var(--ti-popover-fg);--token-text-primary:var(--ti-popover-fg);--color-token-button-tertiary-foreground:var(--ti-popover-fg);}
a{color:var(--ti-link);}::selection{background:var(--ti-selection);color:var(--ti-foreground);}.monaco-editor,.monaco-editor-background,.xterm,.xterm-viewport,.xterm-screen{font-family:var(--ti-mono-font)!important;}pre,code{font-family:var(--ti-mono-font)!important;}
/* Markdown renderers keep their own translucent code surface and inline syntax colors. Keep code readable over image backgrounds even for themes created before these tokens existed. */
pre:not([data-diff]),.shiki,[data-code-block]{background:var(--ti-code-bg)!important;background-blend-mode:normal!important;color:var(--ti-code-fg)!important;border-color:var(--ti-code-border)!important;opacity:1!important;backdrop-filter:none!important;box-shadow:none!important;--shiki-foreground:var(--ti-code-fg);--shiki-background:var(--ti-code-bg);}
pre:not([data-diff]) code,.shiki code,[data-code-block] code{background:transparent!important;color:var(--ti-code-fg)!important;text-shadow:none!important;}
pre:not([data-diff]) code :is(span,mark),.shiki code :is(span,mark),[data-code-block] code :is(span,mark){background-color:transparent!important;text-shadow:none!important;}
pre:not([data-diff]) :is(.token.comment,.token.prolog,.token.doctype,.token.cdata,.hljs-comment,.hljs-quote),.shiki :is(.token.comment,.token.prolog,.token.doctype,.token.cdata,.hljs-comment,.hljs-quote),[data-code-block] :is(.token.comment,.token.prolog,.token.doctype,.token.cdata,.hljs-comment,.hljs-quote){color:var(--ti-code-muted)!important;}
pre:not([data-diff]) :is(.token.keyword,.token.operator,.token.atrule,.hljs-keyword,.hljs-meta,.hljs-selector-tag),.shiki :is(.token.keyword,.token.operator,.token.atrule,.hljs-keyword,.hljs-meta,.hljs-selector-tag),[data-code-block] :is(.token.keyword,.token.operator,.token.atrule,.hljs-keyword,.hljs-meta,.hljs-selector-tag){color:var(--ti-code-accent)!important;}
pre:not([data-diff]) :is(.token.string,.token.char,.token.attr-value,.hljs-string,.hljs-regexp),.shiki :is(.token.string,.token.char,.token.attr-value,.hljs-string,.hljs-regexp),[data-code-block] :is(.token.string,.token.char,.token.attr-value,.hljs-string,.hljs-regexp){color:var(--ti-code-string)!important;}
pre:not([data-diff]) :is(.token.number,.token.boolean,.token.constant,.token.symbol,.hljs-number,.hljs-literal),.shiki :is(.token.number,.token.boolean,.token.constant,.token.symbol,.hljs-number,.hljs-literal),[data-code-block] :is(.token.number,.token.boolean,.token.constant,.token.symbol,.hljs-number,.hljs-literal){color:var(--ti-code-literal)!important;}
pre:not([data-diff]) :is(.token.function,.token.class-name,.token.property,.token.variable,.hljs-title,.hljs-attr),.shiki :is(.token.function,.token.class-name,.token.property,.token.variable,.hljs-title,.hljs-attr),[data-code-block] :is(.token.function,.token.class-name,.token.property,.token.variable,.hljs-title,.hljs-attr){color:var(--ti-code-function)!important;}
:not(pre)>code{background-color:var(--ti-code-inline-bg)!important;background-image:none!important;color:var(--ti-code-inline-fg)!important;border-color:var(--ti-code-border)!important;text-shadow:none!important;}
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
    syncRightSidebar();
    syncSkin(theme);
    syncIcons(theme);
    syncNativeButtonContrast();
    syncAmbientEffect(theme);
    syncWindowChrome(theme);
    patchShadowRoots(style.textContent);
    patchDiffRoots(theme);
    patchTerminal(theme);
    queueMicrotask(reportThemeState);
  }

  function syncWindowChrome(theme) {
    clearTimeout(windowChromeTimer);
    const epoch = ++windowChromeEpoch;
    const background = normalizeColor(theme.chrome?.titlebarBackground || theme.tokens.appBackground);
    const chrome = {
      background,
      foreground: safeForeground(background, theme.chrome?.titlebarForeground || theme.tokens.foreground),
    };
    const appearance = colorLuminance(background) < .5 ? "dark" : "light";
    const apply = async () => {
      try {
        if (window.electronBridge?.getSystemThemeVariant?.() !== appearance) {
          const setAppearance = await loadCodexAppearanceSetter();
          if (epoch !== windowChromeEpoch) return;
          await setAppearance({ key: "appearanceTheme", default: "system" }, appearance);
        }
      } catch {}
      if (epoch === windowChromeEpoch) await call("theme.window.chrome", chrome).catch(() => {});
    };
    apply();
    windowChromeTimer = setTimeout(apply, 400);
  }

  function loadCodexAppearanceSetter() {
    if (codexAppearanceSetterPromise) return codexAppearanceSetterPromise;
    codexAppearanceSetterPromise = (async () => {
      const entryUrl = [...document.scripts].map(script => script.src).find(source => /\/assets\/index-[^/]+\.js$/.test(source));
      if (!entryUrl) throw new Error("Codex entry script not found");
      const entrySource = await fetch(entryUrl).then(response => {
        if (!response.ok) throw new Error(`Codex entry script returned ${response.status}`);
        return response.text();
      });
      const moduleName = entrySource.match(/["']\.\/(setting-storage-[^"']+\.js)["']/)?.[1];
      if (!moduleName) throw new Error("Codex appearance module not found");
      const settingsModule = await import(new URL(moduleName, entryUrl).href);
      const setter = Object.values(settingsModule).find(value => typeof value === "function" && value.toString().includes("set-setting"));
      if (!setter) throw new Error("Codex appearance setter not found");
      return setter;
    })().catch(error => {
      codexAppearanceSetterPromise = null;
      throw error;
    });
    return codexAppearanceSetterPromise;
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
    document.querySelectorAll("[data-theme-inject-titlebar-surface],[data-theme-inject-titlebar-text],[data-theme-inject-task-header],[data-theme-inject-project-actions],[data-theme-inject-project-action]").forEach(node => {
      delete node.dataset.themeInjectTitlebarSurface;
      delete node.dataset.themeInjectTitlebarText;
      delete node.dataset.themeInjectTaskHeader;
      delete node.dataset.themeInjectProjectActions;
      delete node.dataset.themeInjectProjectAction;
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
    if (taskTitlebar) {
      [...taskTitlebar.querySelectorAll("div")].forEach(group => {
        const controls = [...group.children].filter(node => node.matches("button,[role=button]"));
        if (controls.length !== 2) return;
        const menu = controls.find(node => node.matches('[aria-haspopup="menu"]'));
        const primary = controls.find(node => node !== menu);
        const bounds = group.getBoundingClientRect();
        const hasAppIcon = Boolean(primary?.querySelector('img[src^="apps/"],img[src*="/apps/"]'));
        if (!menu || !primary || !primary.textContent?.trim() || !hasAppIcon || bounds.height < 20 || bounds.height > 44) return;
        group.dataset.themeInjectProjectActions = "true";
        controls.forEach(node => node.dataset.themeInjectProjectAction = "true");
      });
    }
    const rootStyle = getComputedStyle(document.documentElement);
    const titlebarForeground = rootStyle.getPropertyValue("--ti-titlebar-fg").trim();
    const roots = [document, ...[...document.querySelectorAll("*")].map(node => node.shadowRoot).filter(Boolean)];
    const topControls = roots.flatMap(root => [...root.querySelectorAll('button,[role="button"]')]).filter(node => {
      const bounds = node.getBoundingClientRect();
      return bounds.top >= 0 && bounds.top < 82 && bounds.right > window.innerWidth - 220 && bounds.width > 8 && bounds.height > 8;
    });
    const titlebarControls = [...document.querySelectorAll('.app-header-tint button,.app-header-tint [role="button"],[data-theme-inject-titlebar-text="true"]'), ...topControls];
    titlebarControls.forEach(node => {
      node.style.setProperty("color", titlebarForeground, "important");
      node.style.setProperty("--token-foreground", titlebarForeground);
      node.style.setProperty("--color-token-icon-foreground", titlebarForeground);
      node.querySelectorAll("svg,path,line,polyline,rect").forEach(icon => {
        icon.style.setProperty("color", titlebarForeground, "important");
        icon.style.setProperty("stroke", titlebarForeground, "important");
        if (icon.getAttribute("fill") && icon.getAttribute("fill") !== "none") icon.style.setProperty("fill", titlebarForeground, "important");
        icon.style.setProperty("opacity", "1", "important");
      });
    });
    const sidebar = document.querySelector(".app-shell-left-panel");
    if (sidebar) {
      const foreground = rootStyle.getPropertyValue("--ti-sidebar-fg").trim();
      const icon = rootStyle.getPropertyValue("--ti-sidebar-icon").trim();
      sidebar.querySelectorAll('button,[role="button"],[data-app-action-sidebar-thread-id],[data-app-action-sidebar-project-row]').forEach(node => node.style.setProperty("color", foreground, "important"));
      sidebar.querySelectorAll("svg").forEach(node => node.style.setProperty("color", icon, "important"));
    }
  }

  function syncSkin(theme) {
    if (!theme.skin?.enabled) {
      document.querySelector(".app-shell-left-panel")?.style.removeProperty("--ti-sidebar-watermark");
      document.querySelectorAll('[data-theme-inject-native-home="hidden"]').forEach(node => node.removeAttribute("data-theme-inject-native-home"));
      document.getElementById(BRAND_ID)?.remove();
      document.getElementById(HOME_ID)?.remove();
      document.getElementById(DECORATIONS_ID)?.remove();
      restoreAccountName();
      restoreAccountAvatar();
      return;
    }
    syncBrand(theme);
    syncDecorations(theme);
    syncHome(theme);
  }

  function syncIcons(theme, roots = null) {
    const accountButton = findAccountButton();
    [accountButton, ...nodesWithinRoots(roots, '[data-theme-inject-custom-icon="account"]')].filter(Boolean).forEach(node => {
      delete node.dataset.themeInjectCustomIcon;
      node.style.removeProperty("--ti-custom-icon");
    });
    nodesWithinRoots(roots, '[data-theme-inject-custom-icon="send"]').filter(isComposerStopButton).forEach(node => {
      delete node.dataset.themeInjectCustomIcon;
      node.style.removeProperty("--ti-custom-icon");
    });
    if (!roots) {
      document.querySelectorAll("[data-theme-inject-custom-icon]").forEach(node => {
        delete node.dataset.themeInjectCustomIcon;
        node.style.removeProperty("--ti-custom-icon");
      });
    }
    if (!theme.skin?.enabled || theme.skin.icons?.mode !== "custom") return;
    const mappings = theme.skin.icons.mappings || {};
    const actions = [...skinIconFields.map(([action]) => action), ...Object.keys(mappings).filter(action => !skinIconFields.some(([known]) => known === action))];
    for (const action of actions) {
      const path = mappings[action];
      if (!path) continue;
      if (action === "account") continue;
      const url = resolvedAsset(theme, path);
      if (!url) continue;
      const nodes = resolveIconNodes(action, roots).map(node => customIconTarget(action, node)).filter(node => node && (action !== "send" || !isComposerStopButton(node)));
      if (action === "send" && !iconAssetReady(url)) {
        nodes.forEach(node => {
          if (node.dataset.themeInjectCustomIcon === action) delete node.dataset.themeInjectCustomIcon;
          node.style.removeProperty("--ti-custom-icon");
        });
        continue;
      }
      nodes.forEach(node => {
        node.dataset.themeInjectCustomIcon = action;
        node.style.setProperty("--ti-custom-icon", `url("${url.replace(/"/g, "%22")}")`);
      });
    }
  }

  function nodesWithinRoots(roots, selector) {
    if (!roots) return [...document.querySelectorAll(selector)];
    const matches = new Set();
    roots.forEach(root => {
      if (!(root instanceof Element) || !root.isConnected) return;
      if (root.matches(selector)) matches.add(root);
      root.querySelectorAll(selector).forEach(node => matches.add(node));
      const ancestor = root.closest(selector);
      if (ancestor) matches.add(ancestor);
    });
    return [...matches];
  }

  function resolveIconNodes(action, roots = null) {
    const selector = iconSelectors[action];
    const stable = selector ? nodesWithinRoots(roots, selector) : [];
    if (stable.length) return stable;
    const sidebar = document.querySelector(".app-shell-left-panel");
    const composer = document.querySelector('[data-codex-composer-root],[data-codex-composer="true"]');
    const scope = action === "send" ? composer : action === "terminal" ? document : sidebar;
    const candidates = roots
      ? nodesWithinRoots(roots, "button,[role=button]").filter(node => !scope || scope === document || scope.contains(node))
      : [...(scope || document).querySelectorAll("button,[role=button]")];
    const label = node => [node.getAttribute("aria-label"), node.getAttribute("title"), node.innerText]
      .filter(Boolean).join(" ").trim().replace(/\s+/g, " ");
    const isLabel = (node, values) => values.some(value => label(node) === value || label(node).startsWith(`${value} `));
    const matchers = {
      "new-task": node => isLabel(node, ["新建任务", "新增任務", "New task", "新しいタスク", "새 작업"]),
      search: node => isLabel(node, ["搜索", "搜尋", "Search", "検索", "검색"]),
      scheduled: node => isLabel(node, ["已安排", "已排程", "Scheduled", "スケジュール済み", "예약됨"]),
      plugins: node => isLabel(node, ["插件", "Plugins", "プラグイン", "플러그인"]),
      "pull-requests": node => isLabel(node, ["拉取请求", "提取要求", "Pull requests", "プルリクエスト", "풀 리퀘스트"]),
      settings: node => isLabel(node, ["设置", "設定", "Settings"]),
      send: node => isLabel(node, ["发送", "提交", "傳送", "送出", "Send", "Submit", "送信", "보내기", "제출"]),
      terminal: node => isLabel(node, ["终端", "終端機", "Terminal", "ターミナル", "터미널"]),
      files: node => isLabel(node, ["文件", "檔案", "Files", "ファイル", "파일"]),
      browser: node => isLabel(node, ["浏览器", "瀏覽器", "Browser", "ブラウザー", "브라우저"]),
      environments: node => isLabel(node, ["环境", "環境", "Environments", "環境", "환경"]),
      git: node => isLabel(node, ["Git"]),
      connections: node => isLabel(node, ["连接", "連線", "Connections", "接続", "연결"]),
      worktrees: node => isLabel(node, ["工作树", "工作樹", "Worktrees", "ワークツリー", "워크트리"]),
      hooks: node => isLabel(node, ["钩子", "掛鉤", "Hooks", "フック", "후크"]),
      account: node => isLabel(node, ["账户", "帳戶", "Account", "アカウント", "계정", "プロフィール メニューを開く", "프로필 메뉴 열기", "打开个人资料菜单", "開啟個人資料選單", "Open profile menu"]),
      general: node => isLabel(node, ["常规", "一般", "General", "一般設定", "일반"]),
      appearance: node => isLabel(node, ["外观", "外觀", "Appearance", "外観", "모양"]),
      voice: node => isLabel(node, ["语音", "語音", "Voice", "音声", "음성"]),
      configuration: node => isLabel(node, ["配置", "Configuration", "構成", "구성"]),
      personalization: node => isLabel(node, ["个性化", "個人化", "Personalization", "パーソナライズ", "개인 설정"]),
      pets: node => isLabel(node, ["宠物", "寵物", "Pets", "ペット", "반려동물"]),
      "keyboard-shortcuts": node => isLabel(node, ["键盘快捷键", "鍵盤快速鍵", "Keyboard shortcuts", "キーボード ショートカット", "키보드 단축키"]),
      "computer-control": node => isLabel(node, ["电脑操控", "電腦操控", "Computer control", "コンピューター操作", "컴퓨터 제어"]),
    };
    const matched = matchers[action] ? candidates.filter(matchers[action]) : [];
    if (matched.length || action !== "send") return matched;
    return candidates.filter(node => {
      if (isComposerStopButton(node) || node.matches('[aria-haspopup],[data-composer-navigation-target]') || !node.querySelector("svg")) return false;
      const bounds = node.getBoundingClientRect();
      if (bounds.width < 22 || bounds.width > 44 || bounds.height < 22 || bounds.height > 44) return false;
      const peers = [...(node.parentElement?.querySelectorAll(":scope > button") || [])].filter(peer => {
        const peerBounds = peer.getBoundingClientRect();
        return peerBounds.width > 0 && peerBounds.height > 0;
      });
      return peers.length === 0 || peers.every(peer => peer === node || peer.getBoundingClientRect().right <= bounds.right);
    });
  }

  function isComposerStopButton(node) {
    return ["停止", "停止回應", "Stop", "停止する", "중지"].includes(node?.getAttribute?.("aria-label") || "");
  }

  function iconAssetReady(url) {
    const preload = iconPreloads.get(url)?.image;
    return Boolean(preload?.complete && preload.naturalWidth > 0 && preload.naturalHeight > 0);
  }

  async function preloadIconAssets(theme) {
    if (iconPreloadThemeId !== theme.id) {
      iconPreloads.clear();
      iconPreloadThemeId = theme.id;
    }
    if (!theme.skin?.enabled || theme.skin.icons?.mode !== "custom") return;
    const urls = [...new Set(Object.values(theme.skin.icons.mappings || {}).map(path => resolvedAsset(theme, path)).filter(Boolean))];
    await Promise.all(urls.map(url => {
      const cached = iconPreloads.get(url);
      if (cached) return cached.promise;
      const image = new Image();
      image.decoding = "async";
      image.src = url;
      const promise = typeof image.decode === "function"
        ? image.decode().catch(() => {})
        : new Promise(resolve => { image.onload = image.onerror = resolve; });
      iconPreloads.set(url, { image, promise });
      return promise;
    }));
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
    const accountAvatar = resolvedAsset(theme, theme.skin.resources?.accountAvatar);
    const watermark = resolvedAsset(theme, theme.skin.resources?.sidebarWatermark);
    const signature = JSON.stringify([theme.skin.brand, logo, avatar, accountAvatar]);
    sidebar.style.setProperty("--ti-sidebar-watermark", watermark ? `url("${watermark.replace(/"/g, "%22")}")` : "none");
    if (brandSignature === signature && brand.childElementCount) {
      syncAccountName(theme.skin.brand?.accountName || "");
      syncAccountAvatar(accountAvatar);
      return;
    }
    brandSignature = signature;
    brand.style.removeProperty("--ti-brand-watermark");
    brand.innerHTML = `${logo ? `<img class="ti-brand-logo" src="${escapeHtml(logo)}" alt="">` : ""}<div class="ti-brand-copy"><strong>${escapeHtml(theme.skin.brand?.title || theme.name)}</strong>${theme.skin.brand?.subtitle ? `<span>${escapeHtml(theme.skin.brand.subtitle)}</span>` : ""}</div>${avatar ? `<img class="ti-brand-avatar" src="${escapeHtml(avatar)}" alt="">` : ""}`;
    syncAccountName(theme.skin.brand?.accountName || "");
    syncAccountAvatar(accountAvatar);
  }

  function customIconTarget(action, node) {
    if (!["project-folder", "project-open"].includes(action)) return node;
    const icon = node.querySelector("svg,[data-slot=icon],[class*=icon]");
    return icon?.parentElement || node;
  }

  function findAccountButton() {
    const labels = ["打开个人资料菜单", "開啟個人資料選單", "Open profile menu", "プロフィール メニューを開く", "프로필 메뉴 열기"];
    return [...document.querySelectorAll('button,[role="button"]')].find(node => labels.includes(node.getAttribute("aria-label") || "")) || null;
  }

  function findAccountNameNode() {
    const button = findAccountButton();
    if (!button) return null;
    return [...button.querySelectorAll("span")].reverse().find(node => !node.matches('[data-theme-inject-account-avatar-target="true"]') && node.childElementCount === 0 && node.textContent?.trim()) || null;
  }

  function restoreAccountAvatar() {
    document.querySelectorAll('[data-theme-inject-account-avatar="true"]').forEach(button => {
      delete button.dataset.themeInjectAccountAvatar;
      button.style.removeProperty("--ti-account-avatar");
    });
    document.querySelectorAll('[data-theme-inject-account-avatar-target="true"]').forEach(target => {
      if (target instanceof HTMLImageElement && target.dataset.themeInjectOriginalAvatarSrc != null) {
        if (target.dataset.themeInjectOriginalAvatarHadSrc === "true") target.setAttribute("src", target.dataset.themeInjectOriginalAvatarSrc);
        else target.removeAttribute("src");
      }
      delete target.dataset.themeInjectAccountAvatarTarget;
      delete target.dataset.themeInjectOriginalAvatarSrc;
      delete target.dataset.themeInjectOriginalAvatarHadSrc;
      target.style.removeProperty("--ti-account-avatar");
    });
    document.querySelectorAll('[data-theme-inject-account-avatar-node="true"]').forEach(node => node.remove());
  }

  function syncAccountAvatar(url) {
    restoreAccountAvatar();
    const button = findAccountButton();
    if (!url || !button) return;
    const target = button.firstElementChild;
    if (!target?.matches("img,span,svg")) return;
    delete button.dataset.themeInjectCustomIcon;
    button.style.removeProperty("--ti-custom-icon");
    button.dataset.themeInjectAccountAvatar = "true";
    button.style.setProperty("--ti-account-avatar", `url("${url.replace(/"/g, "%22")}")`);
    target.dataset.themeInjectAccountAvatarTarget = "true";
    if (target instanceof HTMLImageElement) {
      target.dataset.themeInjectOriginalAvatarHadSrc = String(target.hasAttribute("src"));
      target.dataset.themeInjectOriginalAvatarSrc = target.getAttribute("src") || "";
      target.setAttribute("src", url);
    } else {
      target.style.setProperty("--ti-account-avatar", `url("${url.replace(/"/g, "%22")}")`);
    }
  }

  function restoreAccountName() {
    if (accountNameNode?.isConnected && originalAccountName) accountNameNode.textContent = originalAccountName;
    accountNameNode = null;
    originalAccountName = "";
  }

  function syncAccountName(name) {
    const node = findAccountNameNode();
    if (!node) return;
    if (node !== accountNameNode) {
      restoreAccountName();
      accountNameNode = node;
      originalAccountName = node.textContent || "";
    }
    node.textContent = name.trim() || originalAccountName;
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
      document.getElementById(HOME_ID)?.remove();
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
    await preloadIconAssets(theme);
    if (destroyed || lifecycle !== lifecycleEpoch || epoch !== assetHydrationEpoch) return null;
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
