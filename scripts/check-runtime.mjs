import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const root = path.resolve(import.meta.dirname, "..");
const sources = [
  "assets/theme-runtime.js",
  "assets/runtime/i18n.js",
  "assets/runtime/theme-renderer.js",
  "assets/runtime/pets.js",
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

const studioSource = fs.readFileSync(path.join(root, "assets", "runtime", "studio.js"), "utf8");
const aiGenerationSource = fs.readFileSync(path.join(root, "assets", "runtime", "ai-generation.js"), "utf8");
const petsSource = fs.readFileSync(path.join(root, "assets", "runtime", "pets.js"), "utf8");
const lifecycleSource = fs.readFileSync(path.join(root, "assets", "runtime", "lifecycle.js"), "utf8");
const panelSource = fs.readFileSync(path.join(root, "assets", "runtime", "panel-ui.js"), "utf8");
if (/data-studio-pet|aiGeneratePet|ai\.pet\.generate/.test(`${studioSource}\n${aiGenerationSource}`)) {
  throw new Error("pet generation must stay out of the AI theme workbench");
}
if (!petsSource.includes("data-pet-create") || /data-pet-create-base|data-pet-create-actions/.test(petsSource)) {
  throw new Error("pet generation must use one library entry");
}
if (!petsSource.includes('data-pet-creator-tab="base"') || !petsSource.includes('data-pet-creator-tab="actions"')) {
  throw new Error("pet base and action generation must be separate tabs in one creator");
}
if (!petsSource.includes("data-pet-base-settings") || !petsSource.includes("data-pet-action-settings")) {
  throw new Error("pet base and action settings must remain in separate tab sections");
}
const petMetadataView = petsSource.match(/function petCreatorMetadataHtml\(generating, includeBasePrompt\) \{([\s\S]*?)\n  \}/)?.[1] || "";
const petBaseSettingsBranch = petMetadataView.split("const concurrency =")[0] || "";
if (!petBaseSettingsBranch.includes("data-pet-base-settings") || /宠物名称|宠物介绍|data-pet-creator-concurrency/.test(petBaseSettingsBranch)) {
  throw new Error("pet base tab must only contain the visual description field");
}
const petBaseReferenceView = petsSource.match(/function petCreatorReferenceHtml\(kind\) \{([\s\S]*?)\n  \}/)?.[1] || "";
if (!petBaseReferenceView.includes("data-pet-creator-reference-select") || petBaseReferenceView.includes("data-pet-creator-base-select")) {
  throw new Error("pet base tab must accept reference images without a direct base-image upload control");
}
if (petsSource.includes("data-pet-creator-use-uploaded-base")) {
  throw new Error("pet base tab must not turn its first reference image into the base asset");
}
const petActionCreatorView = petsSource.match(/function petActionsCreatorHtml\(generating\) \{([\s\S]*?)\n  \}/)?.[1] || "";
if (!petActionCreatorView.includes("data-pet-creator-base-select") || /添加动作参考图|data-pet-creator-reference-select/.test(petActionCreatorView)) {
  throw new Error("pet action tab must accept one base image without separate action references");
}
if (!petsSource.includes("useUploadedAsBase ? aiPetReferences.slice(0, 1) : []")) {
  throw new Error("pet action requests must send at most one uploaded base image");
}
if (!petsSource.includes('lockFirstReferenceIdentity: petCreatorMode === "base" && aiPetReferences.length > 0')) {
  throw new Error("explicit pet base references must be marked as identity evidence");
}
if (!petsSource.includes('call("ai.pet.base.generate"') || !petsSource.includes('call("ai.pet.actions.generate"') || !petsSource.includes('call("ai.pet.restore"')) {
  throw new Error("pet base, action, and checkpoint restore operations must use separate RPC methods");
}
if (/data-pet-creator-confirm-base|data-pet-creator-direct-actions|data-pet-creator-resume/.test(petsSource)) {
  throw new Error("pet base generation must not directly trigger the action generation workflow");
}
if (!lifecycleSource.includes("syncGenerationIsland") || !lifecycleSource.includes("GENERATION_ISLAND_ID")) {
  throw new Error("generation titlebar island is missing");
}
if (!panelSource.includes("data-busy-allow") || !petsSource.includes("后台运行")) {
  throw new Error("background generation close controls are missing");
}
if (!petsSource.includes("function syncPetCreatorProgressPanel()") || !petsSource.includes("data-pet-creator-progress")) {
  throw new Error("pet creator progress must update through its isolated DOM region");
}
const petProgressPoller = petsSource.match(/async function pollPetCreatorProgress\(epoch\) \{([\s\S]*?)\n  \}/)?.[1] || "";
if (!petProgressPoller.includes("syncPetCreatorProgressPanel()") || petProgressPoller.includes("renderPanel()")) {
  throw new Error("pet creator progress polling must not rebuild the full panel");
}
if (!petsSource.includes('if (busy && activeGenerationKind === "pet") return "";')) {
  throw new Error("pet action previews must be unmounted while pet generation is running");
}
if (!petsSource.includes("asset?.previewPath") || !petsSource.includes("petGeneratedPreviewAsset(asset)")) {
  throw new Error("pet action previews must hydrate from cached thumbnail files");
}
const petPreviewLoader = petsSource.match(/async function ensurePetCreatorActionPreviews\(\) \{([\s\S]*?)\n  \}/)?.[1] || "";
const petPreviewLoop = petPreviewLoader.match(/await mapConcurrent\(assets, 1, async asset => \{([\s\S]*?)\n      \}\);/)?.[1] || "";
if (!petPreviewLoop || petPreviewLoop.includes("renderPanel()")) {
  throw new Error("pet action preview loading must not rebuild the panel for each image");
}

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
