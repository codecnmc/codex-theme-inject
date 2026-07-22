# Theme Inject

English | [简体中文](README.md)

A standalone launcher that injects custom themes into Codex for Windows.

Theme Inject uses the Chromium DevTools Protocol (CDP) to inject a theme runtime into the Codex renderer. It adds a theme panel, theme library, background controls, advanced skins, and an AI theme workspace without modifying the Codex installation.

> This project is under active development. Features, UI, and compatibility may change.

![AI theme workspace](images/AI主题工作台.png)

## Features

- Starts and connects to Codex for Windows automatically.
- Adds an in-app theme launcher and a live editing panel.
- Styles the sidebar, conversations, composer, dialogs, code blocks, terminal, and other workspace surfaces.
- Controls colors, fonts, corner radius, opacity, blur, saturation, shadows, and terminal colors.
- Supports unified fullscreen backgrounds or separate content and sidebar backgrounds.
- Imports, exports, duplicates, deletes, and switches local themes immediately.
- Supports advanced skin assets including logos, watermarks, hero images, avatars, action icons, home cards, decorations, and component surfaces.
- Generates theme drafts and image assets through OpenAI-compatible text, vision, and image APIs.
- Keeps generation checkpoints, resumes failed assets, exposes request logs, and regenerates individual resources.
- Detects the system language and loads the Simplified Chinese or English UI from JSON locale files.

## Theme Gallery

Every ZIP below can be downloaded directly and imported from the theme library.

| Theme | Preview | Style |
| --- | --- | --- |
| [Gundam](themes/高达主题.zip)<br>Dark | <img src="images/最新高达主题.png" alt="Gundam theme preview" width="360"> | A deep-blue space hangar and metallic mecha theme with high-contrast glass panels. |
| [CLANNAD Dango Daikazoku](themes/团子大家族主题.zip)<br>Light | <img src="images/clannad主题.png" alt="CLANNAD theme preview" width="360"> | A soft cherry-blossom campus theme with bright, readable surfaces and dango decorations. |
| [Rain](themes/雨姐主题.zip)<br>Light | <img src="images/雨姐主题.png" alt="Rain theme preview" width="360"> | A sweet pink theme built around strawberries, ribbons, lace, character art, and custom decorations. |
| [ikun](themes/ikun.zip)<br>Dark | <img src="images/ikun主题.png" alt="ikun theme preview" width="360"> | A dark stage with blue and orange basketball accents, custom imagery, icons, and corner decorations. |
| [Lu Benwei](themes/卢本伟.zip)<br>Dark | <img src="images/卢本伟主题.png" alt="Lu Benwei theme preview" width="360"> | A purple esports stage with bright-blue accents, playing-card details, coffee motifs, and a complete custom icon set. |
| [Family Guy](themes/恶搞之家.zip)<br>Dark | <img src="images/恶搞之家.png" alt="Family Guy theme preview" width="360"> | An American cartoon theme combining a suburban night scene with a low-distraction dark workspace. |
| [KFC](themes/肯德基.zip)<br>Light | <img src="images/肯德基.png" alt="KFC theme preview" width="360"> | A bright red-and-white theme with warm restaurant lighting, food details, and custom feature icons. |
| [McDonald's](themes/麦当劳.zip)<br>Dark | <img src="images/麦当劳.png" alt="McDonald's theme preview" width="360"> | Deep red and gold surfaces with food imagery and translucent dark work panels. |
| [Orange Cat](themes/耄耋.zip)<br>Dark | <img src="images/耄耋.png" alt="Orange cat theme preview" width="360"> | A calm, warm theme with a close-up orange cat, wooden tones, and muted dark panels. |
| [Round Head and Orange Cat](themes/圆头与耄耋.zip)<br>Dark | <img src="images/圆头与耄耋.png" alt="Round Head and Orange Cat theme preview" width="360"> | A desert science-fiction cartoon theme with cyan skies, yellow suits, and vivid red details. |
| [GG Bond](themes/猪猪侠.zip)<br>Dark | <img src="images/猪猪侠.png" alt="GG Bond theme preview" width="360"> | A vivid red-and-gold hero theme with motion lighting, character icons, and translucent cards. |

## Configuration UI

| Theme library | Advanced skin configuration |
| --- | --- |
| <img src="images/主题库.png" alt="Theme library" width="420"> | <img src="images/配置皮肤.png" alt="Advanced skin configuration" width="420"> |

The Effects tab controls radius, borders, opacity, blur, saturation, and shadows. It also includes animated meteors, rain, snow, starlight, fireflies, and petals.

<img src="images/效果设置.png" alt="Effects and animation settings" width="840">

## Usage

1. Close every running Codex window.
2. Run `theme-inject.exe`.
3. Theme Inject starts Codex with a CDP debugging port.
4. Click the theme button in the bottom-right corner of Codex.
5. Select a theme or edit its colors, background, effects, and skin, then click Apply.

Starting Theme Inject again while an instance is already running opens the existing theme panel.

Optional command-line arguments:

```text
theme-inject --app-path <path-to-Codex>
theme-inject --debug-port <port>
theme-inject --open-panel
```

Theme data is stored in:

```text
%LOCALAPPDATA%\ThemeInject
```

Diagnostic logs are stored in:

```text
%LOCALAPPDATA%\ThemeInject\logs\theme-inject.log
```

## AI Theme Workspace

The AI workspace combines the theme request, reference images, resource plans, progress, request logs, and a static preview.

- Reference images are analyzed before resources are assigned to backgrounds, hero images, logos, avatars, decorations, and card icons.
- Individual assets can be generated or replaced without regenerating the whole theme. Palette-only regeneration preserves existing images.
- Up to four image requests can run concurrently.
- Completed assets remain available when generation is interrupted. Submitting the same request resumes missing resources from its checkpoint.
- Preview assets use a controlled local server and thumbnail fallbacks to limit renderer memory use.

Configure the base model, image model, API URL, and keys under **Advanced > AI Generation**. Third-party API charges may apply.

## Build

Requirements:

- Windows
- Rust 1.85 or later

Build the release binary from the project root:

```powershell
cargo build --release
```

The executable is written to:

```text
target\release\theme-inject.exe
```

For development rebuilds and reinjection:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\restart-dev.ps1
```

Runtime source is organized by responsibility under `assets/runtime/`. Rust concatenates those modules into one closure at compile time. JSON locale files live under `assets/locales/`.

Pushing the `clean-version` branch runs the GitHub Actions tests, builds a Windows release, and uploads a ZIP containing the executable, README, and sample themes.

## Safety Notes

Theme Inject does not modify `app.asar`, the Codex installation directory, or `~/.codex`. It injects the theme runtime through CDP.

The project is still experimental:

- It launches Codex with a debugging port and may require updates for future Codex versions.
- A broken style can affect the current renderer. Closing and reopening Codex restores the original UI.
- Custom CSS is disabled by default and only runs after explicit trust is granted.
- AI generation can incur third-party API charges.
- Theme ZIP files are validated for size, paths, and extensions, but you should still import themes only from trusted sources.

## Theme Package Format

A theme package is a ZIP with at least this file at its root:

```text
theme.json
```

Optional files:

```text
custom.css
preview.png
assets/
```

Imported themes appear in the theme library and can be switched immediately. Exported ZIP files include the manifest and every referenced local image asset.

## Development Checks

Run these checks before committing or publishing:

```powershell
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
node scripts/check-runtime.mjs
```

## More Documentation

- [Original full documentation](docs/original-readme.md)
- [Theme refactor and compatibility notes](docs/theme-refactor-todo.md)
