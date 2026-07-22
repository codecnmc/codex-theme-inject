import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const root = path.resolve(import.meta.dirname, "..");
const sources = [
  "assets/theme-runtime.js",
  "assets/runtime/i18n.js",
  "assets/runtime/theme-renderer.js",
  "assets/runtime/panel-ui.js",
  "assets/runtime/ai-generation.js",
  "assets/runtime/ambient-effects.js",
  "assets/runtime/studio.js",
  "assets/runtime/lifecycle.js",
];

const bundle = sources
  .map(relative => fs.readFileSync(path.join(root, relative), "utf8"))
  .join("\n");

for (const locale of ["zh-CN", "en-US"]) {
  JSON.parse(fs.readFileSync(path.join(root, "assets", "locales", `${locale}.json`), "utf8"));
}

new vm.Script(bundle, { filename: "theme-runtime.bundle.js" });

const en = JSON.parse(fs.readFileSync(path.join(root, "assets", "locales", "en-US.json"), "utf8"));
const i18nSource = fs.readFileSync(path.join(root, "assets", "runtime", "i18n.js"), "utf8");
const context = {
  navigator: { languages: ["en-GB"], language: "en-GB" },
  LOCALES: { "en-US": en },
  NodeFilter: { SHOW_TEXT: 4 },
  document: { createTreeWalker: () => ({ nextNode: () => false }) },
  Element: class Element {},
};
vm.createContext(context);
const probe = new vm.Script(`${i18nSource}\nthis.__probe = [uiLocale, t("主题库"), t("卡片 3")];`);
probe.runInContext(context);
const [locale, library, card] = context.__probe;
if (locale !== "en-US" || library !== "Library" || card !== "Card 3") {
  throw new Error(`i18n probe failed: ${JSON.stringify(context.__probe)}`);
}

console.log(`Runtime bundle and i18n checks OK (${sources.length} modules).`);
