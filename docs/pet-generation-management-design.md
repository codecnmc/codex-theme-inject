# 宠物生成与管理方案

## 结论

该功能可以在不修改 Codex 源码和 `app.asar` 的前提下实现。

Codex 当前版本随包提供的 `hatch-pet` 规范确认，自定义 V2 宠物安装目录为：

```text
${CODEX_HOME:-$HOME/.codex}/pets/<pet-id>/
|- pet.json
`- spritesheet.webp
```

V2 图集固定为 `1536x2288`，由 `8x11` 个 `192x208` 单元格组成。前 9 行是标准动作，后 2 行是 16 个顺时针注视方向。`pet.json` 至少包含：

```json
{
  "id": "pet-id",
  "displayName": "Pet Name",
  "description": "One short sentence.",
  "spriteVersionNumber": 2,
  "spritesheetPath": "spritesheet.webp"
}
```

Theme Inject 可以管理自己的宠物库、生成历史和预览资源，并将验证通过的最终包原子复制到 Codex 宠物目录。不要修改 Codex 的 LevelDB、缓存或安装目录。安装后由用户在 Codex 的“宠物”设置中选择；除非后续发现公开选择接口，否则不自动改写 Codex 的当前宠物偏好。

## 产品边界

### 稳定支持

- 在 AI 主题生成中增加“生成配套宠物”开关。
- 为宠物单独上传 1-6 张参考图。
- 没有宠物参考图时复用主题参考图；仍没有参考图时使用文字描述。
- 生成基础形象、9 行标准动作和 16 个注视方向。
- 在生成结果中预览基础形象、动作循环、完整动作表和注视方向。
- 在资源 Tab 中按动作行重新生成，并允许为本次重生成重新上传参考图。
- 将合格宠物保存到 Theme Inject 宠物库。
- 宠物库导入、导出、复制、改名、删除和安装到 Codex。
- 安装前验证，遇到同名宠物时选择覆盖或另存；覆盖前自动备份。

### 暂不承诺

- 自动选择或启用 Codex 当前宠物。当前公开包协议只确认安装目录，没有确认公开的选择 API。
- 修改 Codex 内置宠物或应用安装目录。
- 将完整宠物生成作为当前 `ai.generate` 的一个普通图片槽同步等待。完整 V2 生成最多包含 13 个视觉任务，必须是独立、可恢复的后台作业。
- 对单个方向格做局部拼补。某个注视方向失败时必须重新生成所属的完整 8 帧方向行，避免身份和画风漂移。

## 用户流程

### 主题生成

1. 用户填写主题描述并可上传主题参考图。
2. 用户勾选“生成配套宠物”。
3. 展开宠物选项后可填写宠物描述、风格，并上传独立宠物参考图。
4. 主题蓝图先生成；宠物作为关联后台任务启动，不阻塞主题草稿进入预览。
5. 宠物基础形象完成后立即展示，标准动作和注视方向继续生成。
6. 全部动作通过校验后显示“保存到宠物库”和“安装到 Codex”。

参考图优先级固定为：

```text
本次动作重生成参考图
  > 宠物专用参考图
  > 主题主蓝图参考图
  > 主题/宠物文字描述
```

所有动作生成都必须同时带上已确认的宠物基础形象作为身份参考。参考图只能补充角色特征，不能替代基础形象这一身份锚点。

### 资源 Tab

资源类型增加“宠物”，但不要把宠物伪装成 `skin.*` 图片槽。宠物面板包含：

- 基础形象。
- `idle`、`running-right`、`running-left`、`waving`、`jumping`、`failed`、`waiting`、`running`、`review`。
- 注视方向 A、注视方向 B。
- 每项的状态、缩略图、动作播放、重新生成和替换参考图。

点击某个动作打开预览弹窗。弹窗用 spritesheet CSS 裁切逐帧播放，不把 GIF 当主数据；可切换实际尺寸、4 倍放大、浅色背景、深色背景和透明棋盘背景。用户确认候选后才替换当前动作行，旧候选保留到本次生成会话结束，以便撤销。

### 宠物库

主题面板增加一级 Tab“宠物库”，与“主题库”并列。每个宠物条目显示：

- `idle` 动画预览。
- 名称、描述、V1/V2、来源和校验状态。
- 是否已安装到当前 `CODEX_HOME`。
- 预览动作、安装、导出和更多操作。

宠物详情使用一个完整预览区和动作选择器，不使用多层嵌套卡片。动作选择适合使用图标加 tooltip 或紧凑下拉菜单。

## 数据模型

宠物不是主题资源，单独存储：

```text
%LOCALAPPDATA%/ThemeInject/
|- pets/<pet-id>/
|  |- pet.json
|  |- spritesheet.webp
|  |- source.json
|  `- preview.webp
|- pet-runs/<run-id>/
|  |- request.json
|  |- jobs.json
|  |- references/
|  |- candidates/
|  |- rows/
|  |- qa/
|  `- final/
`- pet-backups/<pet-id>/<timestamp>/
```

`pet.json` 保持 Codex 原生兼容，不写 Theme Inject 私有字段。生成来源、主题关联和 QA 信息放在 `source.json`：

```json
{
  "schemaVersion": 1,
  "themeId": "local.example",
  "generationPrompt": "...",
  "style": "auto",
  "createdAt": 0,
  "updatedAt": 0,
  "qa": {
    "atlasValid": true,
    "visualReview": "pass",
    "warnings": []
  }
}
```

主题与宠物是松耦合关系。主题 manifest 可在现有 `extensions` 中保存关联 ID，后续 schema 稳定后再提升为正式字段：

```json
{
  "themeInject": {
    "companionPetId": "pet-id"
  }
}
```

删除主题不能删除宠物；删除宠物时只清除主题关联。主题切换也不自动切换 Codex 宠物，因为 Codex 未提供已确认的公开选择 API。

## 宠物包导入导出

建议使用独立扩展名 `.tipet`，底层为 ZIP：

```text
pet.json
spritesheet.webp
source.json       # 可选
preview.webp      # 可选
```

同时允许导入包含 `pet.json + spritesheet.webp` 的普通 ZIP。为兼容社区宠物市场，运行文件可以位于 ZIP 根目录或同一个单层目录；README、预览图和 macOS 元数据等附加文件会被忽略。缺少 `spriteVersionNumber` 且图集为 `1536x1872` 的旧市场包按 V1 识别，V1 预览 9 个标准动作并保持原清单导入/导出。默认导出普通 `.zip`，只包含 Codex 运行所需的两个文件，不包含 API Key、原始参考图、生成日志和中间产物。

导入时必须校验：

- ZIP 路径穿越和符号链接。
- 文件数量、单文件大小和总解压大小。
- ID 只能使用小写字母、数字、点、下划线和连字符。
- `spritesheetPath` 必须是包内相对路径。
- 图集必须为 WebP；V1 必须为 `1536x1872`，V2 必须为 `1536x2288`。
- 图集必须有 alpha；使用单元格非空、未使用格透明、边界溢出等确定性校验。
- `spriteVersionNumber` 与实际尺寸一致。

导出主题时可增加“包含配套宠物”选项，但底层仍嵌入一个完整 `.tipet` 子包。导入主题时只把宠物加入 Theme Inject 库，不自动覆盖 Codex 中的同名宠物。

## Codex 安装策略

1. 解析 `CODEX_HOME`；未设置时使用 `%USERPROFILE%/.codex`。
2. 将待安装内容写入同盘临时目录。
3. 再次执行 manifest 和 atlas 校验。
4. 目标存在时生成备份，不直接覆盖。
5. 用目录替换完成原子安装；失败时恢复原目录。
6. 返回安装路径，并提示用户在 Codex“设置 -> 宠物”中选择。

删除 Theme Inject 库中的宠物不应删除 Codex 已安装副本。宠物库可以提供单独的“从 Codex 卸载”，该操作需要明确确认且只删除由 Theme Inject 记录为已安装的自定义宠物。

## 生成任务设计

### 请求字段

现有 `GenerateRequest` 增加轻量开关和宠物输入，但主题 API 只负责创建关联任务：

```rust
generate_pet: bool,
pet_prompt: String,
pet_style: PetStyle,
pet_references: Vec<ReferenceImage>,
```

建议新增独立 RPC：

```text
pet.generation.start
pet.generation.state
pet.generation.cancel
pet.generation.restore
pet.generation.regenerate
pet.generation.accept-candidate
pet.generation.finalize

pet.library.list
pet.library.read
pet.library.import
pet.library.export
pet.library.clone
pet.library.delete
pet.library.install
pet.library.uninstall
pet.library.installation-state

pet.preview.asset
pet.preview.cancel
```

不要复用主题的单一 `ai_generation.json`。宠物需要 `run-id` 级别的持久化任务和检查点，允许关闭面板、切换主题或重启 Theme Inject 后继续。

### 作业图

完整 V2 宠物最多包含：

1. 基础形象 1 项。
2. 标准动作 9 行。
3. 四向注视锚点 1 行。
4. 注视方向 A、B 共 2 行。

依赖关系：

```text
references/text
  -> base
  -> idle + running-right
  -> remaining standard rows
  -> standard atlas QA
  -> four cardinal anchors
  -> look row A
  -> look row B
  -> final assembly + QA
  -> library candidate
```

`running-left` 只有在角色没有左右不对称文字、徽记或道具时，才可由已确认的 `running-right` 确定性镜像；否则独立生成。

### 状态机

```text
queued -> generating -> processing -> awaiting-review -> accepted
                       |                  |
                       v                  v
                     failed          superseded

run: planning -> generating-base -> generating-standard
     -> reviewing-standard -> generating-look -> finalizing
     -> ready | partial | failed | cancelled
```

动作失败不应使已经完成的主题生成失败。宠物任务进入 `partial`，用户可以从资源 Tab 补生成缺失行。

## 提示词策略

不要要求图片模型直接生成完整 `8x11` 图集。模型负责基础形象和每个动作的连续横向 strip，程序负责切帧、归一化、抠图和组装。

基础形象提示词需要包含：

- 一个居中的全身紧凑角色。
- 在 `192x208` 尺寸仍然可辨认的轮廓和面部特征。
- 固定材质、配色、比例、道具和左右特征。
- 单色纯绿幕或与角色颜色不冲突的纯色键控背景。
- 无文字、Logo、场景、投影、光晕和分离特效。

每个动作 strip 提示词保持短且状态专用，并附加：

- 已确认基础形象，角色身份参考。
- 当前动作布局 guide，仅用于格数、间距和安全边距。
- 本次动作额外参考图（如有）。
- 确切帧数、动作语义和禁止项。

标准动作语义：

| 动作 | 帧数 | 约束 |
| --- | ---: | --- |
| idle | 6 | 呼吸、眨眼或轻微晃动，不能完全静止 |
| running-right | 8 | 向屏幕右侧移动，步态交替 |
| running-left | 8 | 向屏幕左侧移动，步态交替 |
| waving | 4 | 只用肢体表达挥手，不生成动作线 |
| jumping | 5 | 预备、起跳、顶点、下降、落定，无地面阴影 |
| failed | 8 | 明确失败/沮丧反应，不使用漂浮 X 或符号 |
| waiting | 6 | 等待审批或输入，与 idle 明显不同 |
| running | 6 | 表示任务处理中，不是字面奔跑 |
| review | 6 | 专注检查结果，不新增放大镜、纸张或 UI 道具 |

注视方向必须先生成并确认上、右、下、左四个锚点，再生成两行连续的 8 个方向。`000` 表示向上，不是正面；正面/中立由 idle 表示。

## 图像处理与 QA

Theme Inject 当前已有图标图集切割、绿幕处理、缩略图和 staging 基础，可复用其安全模型，但宠物需要独立处理模块。建议将 Codex 随包 `hatch-pet` 的确定性算法作为行为参考；若复用其 Apache-2.0 脚本，必须保留许可证和来源说明。

必需的确定性步骤：

- 按动作行检测并切出 pose group。
- 统一缩放、基线和单元格中心。
- 绿幕转 alpha、边缘去色和隐藏 RGB 清理。
- 组装标准 `8x9` 中间图集。
- 注册两行注视方向并组装 `8x11` 最终图集。
- 检查使用格非空、未使用格全透明、边缘裁切和尺寸。
- 输出 WebP 和 `pet.json`。

预览产物：

- 标准动作 contact sheet。
- V2 完整 contact sheet。
- 每一行的循环预览。
- 中立帧加 16 个注视方向的专用预览。

只有确定性校验通过且用户接受视觉结果后，才能进入宠物库。机器无法可靠判断的身份漂移、动作错误和方向歧义必须保留人工确认门槛。

## 对现有代码的影响

建议新增模块：

```text
src/pet.rs                 # manifest、动作布局和校验
src/pet_store.rs           # Theme Inject 宠物库
src/pet_package.rs         # 导入、导出和 Codex 安装
src/pet_generation.rs      # 可恢复作业编排
src/pet_image.rs           # 切帧、抠图、组装和 QA
assets/runtime/pets.js     # 宠物库和预览交互
assets/runtime/pet-studio.js
```

需要修改：

- `src/storage.rs`：增加 pets、pet-runs、pet-backups 路径。
- `src/launcher.rs` 和 `src/bridge.rs`：增加宠物 RPC 与 allowlist。
- `src/ai.rs`：只增加主题到宠物任务的输入与共享 AI 客户端能力，不在这里承载全部宠物状态机。
- `assets/runtime/ai-generation.js`：生成开关、宠物输入和关联任务启动。
- `assets/runtime/studio.js`：宠物资源列表、候选和逐动作重生成。
- `assets/runtime/panel-ui.js`：宠物库入口。
- `assets/theme-panel.css`：动作预览与弹窗样式。
- `assets/locales/*.json`：中英文文案。

当前 `MAX_GENERATED_ASSETS = 6` 和 `MAX_GENERATED_TOTAL_BYTES = 72 MB` 是主题生成限制，不适用于宠物。宠物作业应按每个 row 独立限额，并限制同时生成数。默认并发建议 2，最大 3；方向行严格串行。

## 分阶段实施

### 阶段 1：协议和宠物库

- 实现 V2 manifest/atlas 校验。
- 宠物库列表、动作预览、导入、导出。
- 安全安装到 `CODEX_HOME/pets`，带覆盖备份。
- 用手工或官方合规宠物包完成端到端验证。

### 阶段 2：生成和重生成

- 增加“生成配套宠物”和独立参考图。
- 实现 run/job/checkpoint。
- 基础形象、标准动作、方向动作生成。
- 逐动作候选、弹窗预览、接受、撤销和重生成。
- 完整 QA 与 finalize。

### 阶段 3：主题关联

- 主题记录 `companionPetId`。
- 主题导出可选携带 `.tipet`。
- 导入主题时展示配套宠物并让用户决定是否加入库/安装。
- 主题应用后仅提示关联宠物，不私自改 Codex 当前宠物。

## 测试与验收

- V1/V2 尺寸和 manifest 兼容测试。
- 11 行各动作使用格数和未使用格透明测试。
- ZIP 路径穿越、超限、非法扩展名和 manifest 路径测试。
- 同名安装、另存、备份、失败回滚测试。
- 任务取消、重启恢复、缺失 row 续跑和候选撤销测试。
- 参考图优先级测试。
- 重生成单行不改变其他已接受动作的测试。
- 主题生成成功但宠物生成失败时，主题仍可保存和应用。
- Codex 升级后安装目录和 V2 contract 探测测试。
- 在真实 Codex 中验证宠物库出现新宠物、9 个状态动作和鼠标注视方向。

## 推荐决策

先实现阶段 1，再接生成。阶段 1 能尽早验证 Codex 实际加载行为、热刷新/重启要求和包兼容性；如果直接从 AI 工作台开工，最终可能得到大量图片生成代码，却还没有可靠的安装和预览闭环。
