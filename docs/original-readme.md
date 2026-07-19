# Theme Inject

Theme Inject 是一个独立的 Windows Codex 主题启动器。它通过 Chromium DevTools Protocol（CDP）向 Codex renderer 注入主题运行时，并用 CDP Binding 在页面面板与本地 Rust 进程之间传递主题数据。

它不会修改 Codex 的 `app.asar`、安装目录或 `~/.codex` 配置。

## 当前能力

- 自动发现并启动 Windows MSIX/独立版 Codex。
- 注入侧边栏、对话、composer、弹窗、代码块和终端主题。
- 在 Codex 内提供实时主题面板。
- 使用 schema v2 自定义颜色、背景、字体、圆角、透明度、模糊与终端配色；布局由运行时自动约束。
- 背景支持“全屏统一”和“分区独立”；内容区与侧边栏可分别设置无背景、纯色、线性/径向渐变或本地图片。
- 每个背景区域可独立调整图片焦点、适应方式、透明度、表面遮罩、模糊和饱和度。
- 标题栏、侧边栏和输入区使用语义前景色与自动对比度保护，背景层不会覆盖原生按钮和图标。
- 内置 Neutral Dark、Neutral Light 和 Midnight Glass。
- 新建、即时切换、导入、导出、复制和删除本地 ZIP 主题包。
- 可选“高级皮肤”支持品牌区、7 类组件表面、图片资源槽、图标重着色/替换、最多 16 个装饰层和空白首页。
- 空白首页支持 Hero 和最多 4 张快捷卡片；卡片只执行受限的 composer Prompt 填入操作。
- 主题包完整包含高级皮肤配置和所有引用的本地图片；缺少引用资源的 ZIP 会拒绝导入。
- “AI 生成”支持兼容 OpenAI 协议的服务，可从文字或参考图生成主题蓝图，并可选调用生图模型生成本地皮肤资源。
- 自定义 CSS 默认禁用，显式启用后按内容 hash 记录信任。
- watchdog 在页面刷新或 target 变化后恢复注入。

## 构建

需要 Windows 和 Rust 1.85+：

```powershell
cargo build --release
```

输出：

```text
target\release\theme-inject.exe
```

## 使用

1. 关闭所有正在运行的 Codex 窗口。
2. 运行 `theme-inject.exe`。
3. Theme Inject 会以 CDP 参数启动 Codex。
4. 点击 Codex 右下角的“主题”按钮打开编辑面板。

主题库中的卡片会在点击后立即切换并持久化。“新建主题”会在面板内打开命名窗口；导入、导出和背景图片选择使用以 Codex 窗口为 owner 的原生 Windows 文件对话框。文件对话框等待期间不会按普通 RPC 的 15 秒超时中断。

颜色编辑器同时提供色块和 `#RRGGBB` 输入。背景页会显示图片缩略图、文件名、数据加载状态以及“仅预览/已保存”状态；主题库中的图片主题带有“背景图”标记。

背景页的“配置模式”决定编辑方式：全屏统一只显示一套配置并覆盖整个 Codex 内容；分区独立会分别显示内容区域和侧边栏区域。实时编辑只存在于当前预览会话，点击“放弃”恢复打开面板前的快照，点击“应用”后才原子保存主题和本地资源。

高级皮肤在“皮肤”页显式开启。可为 Logo、侧栏水印、Hero、Hero 徽章、头像、角落贴纸和 composer 装饰选择本地图片，并配置标题栏、侧栏、内容、Hero、卡片、composer 与浮层表面。未提供映射或宿主不兼容时会保留 Codex 原生结构和图标。

## AI 生成主题

“AI 生成”页可以配置：

- OpenAI 协议兼容的基础 API 地址，以及可选的独立生图 API 地址；生图地址留空时回退到基础地址。
- 基础 Key 与生图 Key；两者彼此独立，Windows 下分别使用当前用户 DPAPI 加密保存到 `%LOCALAPPDATA%\ThemeInject\ai-settings.json`。
- 自定义基础模型、视觉模型、生图模型和图片比例（如 `3:2` 或 `16:9`）。
- 主题文字描述、可选参考图片，以及是否调用生图模型。
- 侧栏水印默认不生成；只有勾选“生成侧栏水印”后才加入资源计划。
- 同主题系统图标默认不生成；勾选后生成固定 3×3 图集并自动切割为 9 个独立图标。

基础模型和视觉模型使用基础 Key，通过 Responses API 返回受限主题蓝图，不能写入任意 CSS 或 JavaScript；参考图会作为视觉输入交给视觉模型，蓝图会为每个图片资源分配对应的参考图编号。启用生图后，没有参考图时使用 Images 接口；有参考图时使用标准 Images Edits 多图请求，把计划分配的原图直接交给生图模型。两种请求都使用独立的生图 Key 和可选的独立地址，并按背景、Hero、品牌和装饰用途发送合法的像素尺寸。`gpt-image-2` 不支持透明背景，因此 Logo、徽章、装饰、卡片图标和可选系统图标会强制使用纯绿背景生成，再在本地完成色键抠图、去绿边、透明边裁切和画布标准化。图片任务最多并发 4 个，所有 AI HTTP 请求全局最多并发 5 个；遇到 HTTP 429 或 502 时遵循 `Retry-After` 或递增退避，只续跑尚未完成的资源。生成资源先进入预览 staging，可从工作台恢复检查点；点击“保存为新主题”后才复制到新的本地主题目录并启用。

AI 图片遇到 HTTP 429 或 502 时会自动等待并仅重试当前未完成资源。每成功一张都会把主题草稿、资源计划、完成状态和 staging 路径原子保存到 `%LOCALAPPDATA%\ThemeInject\ai-generation.json`；即使进程重启或某张图片最终失败，再次使用相同描述和模型配置生成时也只补未完成图片。应用主题成功后自动清除检查点。

“AI 生成主题”会打开独立工作台：主题需求、参考图、生图、水印、同主题系统图标开关、资源计划和请求日志均在工作台内完成。生成概览会区分主题蓝图分析和图片资源生成阶段，并在每个资源旁显示实际使用的参考图编号；实时显示已完成数量、正在处理的具体资源、失败原因及请求日志。资源列表可添加或覆盖背景、Hero、品牌资源和自定义装饰层的提示词。暂存资源通过受限 bridge 转为 data URL，缩略图、Hero 和装饰不依赖页面访问 loopback HTTP；每个资源完成后都会立即应用到当前 DOM，静态 iframe 预览保留当前副本，点击“刷新 DOM 副本”后再一次性更新，无需先另存为主题。装饰层可直接拖动并同步水平/垂直偏移、锚点、宽度和透明度。

AI 结果生成后仍可在工作台以及颜色、背景和皮肤页面继续编辑。点击工作台的“保存为新主题”后，生成主题会进入主题库并可导出 ZIP；只有保存成功后才清理恢复检查点。生图调用可能产生额外 API 费用；Theme Inject 不会在未点击生成时主动请求 AI 服务。

“AI 生成”页会显示最近一次生成的请求日志，包括蓝图或具体图片资源、接口、模型、HTTP 状态和耗时。相同信息会实时追加到 `%LOCALAPPDATA%\ThemeInject\logs\theme-inject.log`；日志不会记录 API Key 或完整提示词。

兼容约定：

- Responses：`POST /responses`。为兼容会将 `text.format=json_object` 错误包装成 HTTP 502 的中转服务，蓝图通过严格 JSON 提示词与本地结构校验生成，不发送 `text.format`。
- Images：`POST /images/generations`，同时接受 `b64_json` 和公开 HTTPS 图片 URL。
- 远程服务必须使用 HTTPS；明文 HTTP 仅允许 `localhost` 或 loopback 本机服务。
- 基础 Key 与生图 Key 均不写入主题 JSON、ZIP 或诊断日志，也不会由状态接口回传给页面。

面板右上角的“重新注入”会重新安装当前内存中的运行时。开发期间修改代码后，运行 `powershell -ExecutionPolicy Bypass -File .\scripts\restart-dev.ps1`，脚本会停止旧注入器、构建 release 并重新附加到 Codex。

可选参数：

```text
theme-inject --app-path <Codex 路径>
theme-inject --debug-port <端口>
theme-inject --open-panel
```

主题数据位于：

```text
%LOCALAPPDATA%\ThemeInject
```

诊断日志位于 `%LOCALAPPDATA%\ThemeInject\logs\theme-inject.log`。

再次运行 Theme Inject 会通知已有实例打开主题面板。

## 主题包

主题包是 ZIP，根目录至少包含 `theme.json`：

```text
theme.json
custom.css        # 可选
preview.png       # 可选
assets/           # 可选，本地背景图片
```

`custom.css` 不会在导入后自动执行。用户必须在“高级”页面明确启用；文件内容变化会撤销已有信任。

当前主题 schema 为 `2`。schema v1 的 `fullscreen`、`content`、`sidebar` 背景会在读取时迁移到新模型；只有应用或导出主题时才写回 schema v2，因此不会在启动时覆盖已有主题文件。

皮肤图片继续位于主题包的 `assets/` 目录。当前允许 PNG、JPEG、WebP、GIF 和 BMP；主题包字体文件与 SVG 尚未开放。导入和导出会验证 `theme.json` 中背景、Logo、Hero、图标、装饰和卡片图标的每个引用资源，避免生成残缺主题包。

## 开发验证

```powershell
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
node --check assets/theme-runtime.js
```

首个实机兼容基线为 Codex `26.707.9981.0`。外部网页、系统原生菜单和不可访问的第三方 canvas 不属于主题覆盖范围。

主题系统的实现状态和后续兼容项见 [`theme-refactor-todo.md`](theme-refactor-todo.md)。

## 安全边界

- CDP、单实例控制和资源服务只监听 loopback。
- Binding 使用每次启动生成的 nonce、方法 allowlist、请求大小限制和超时。
- 本地资源服务要求随机 token，并校验 canonical path。
- ZIP 导入限制文件数、单文件大小、总解压大小和允许扩展名。
- 页面不能向 Rust 提交任意文件路径；导入/导出使用原生文件选择器。
- AI 地址、模型名、提示词和返回 JSON 均有长度与结构限制；模型输出必须通过 schema v2 校验。
- AI 蓝图默认最多接受 6 个资源计划，系统图标图集按一个附加计划生成；图片并发可选 1–4 路，失败资源保留已完成检查点以便续跑。
