  // Runtime module: i18n
  const systemLocale = (() => {
    const requested = [...(navigator.languages || []), navigator.language]
      .filter(Boolean)
      .map(value => String(value).toLowerCase());
    return requested.some(value => value === "zh" || value.startsWith("zh-")) ? "zh-CN" : "en-US";
  })();
  let uiLanguage = "system";
  let uiLocale = systemLocale;
  let translations = {};
  let translationPatterns = [];

  function setUiLanguage(language) {
    uiLanguage = ["system", "zh-CN", "en-US"].includes(language) ? language : "system";
    uiLocale = uiLanguage === "system" ? systemLocale : uiLanguage;
    const locale = LOCALES[uiLocale] || LOCALES["en-US"];
    translations = locale?.translations || {};
    translationPatterns = (locale?.patterns || []).map(pattern => {
      const names = [];
      const source = pattern.source.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/\\\{([a-zA-Z][\w]*)\\\}/g, (_, name) => {
        names.push(name);
        return "(.+?)";
      });
      return { regex: new RegExp(`^${source}$`), names, target: pattern.target };
    });
  }
  setUiLanguage("system");

  function t(value, variables = {}) {
    const source = String(value ?? "");
    let translated = translations[source];
    if (translated == null) {
      for (const pattern of translationPatterns) {
        const match = source.match(pattern.regex);
        if (!match) continue;
        translated = pattern.target;
        pattern.names.forEach((name, index) => { variables[name] ??= match[index + 1]; });
        break;
      }
    }
    translated ??= source;
    return translated.replace(/\{([a-zA-Z][\w]*)\}/g, (_, name) => variables[name] ?? `{${name}}`);
  }

  function translateTextNode(node) {
    const match = node.nodeValue?.match(/^(\s*)(.*?)(\s*)$/s);
    if (!match || !match[2]) return;
    const translated = t(match[2]);
    if (translated !== match[2]) node.nodeValue = `${match[1]}${translated}${match[3]}`;
  }

  function localizeElement(root) {
    if (!root || uiLocale === "zh-CN") return root;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    nodes.forEach(translateTextNode);
    const elements = root instanceof Element ? [root, ...root.querySelectorAll("*")] : [...root.querySelectorAll?.("*") || []];
    for (const element of elements) {
      for (const attribute of ["title", "aria-label", "placeholder"]) {
        const value = element.getAttribute(attribute);
        if (value) element.setAttribute(attribute, t(value));
      }
    }
    root.setAttribute?.("lang", uiLocale);
    return root;
  }
