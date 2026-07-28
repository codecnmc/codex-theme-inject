# Theme Inject GitHub Release 自动更新计划

## 目标

为 Theme Inject 增加可控的版本检查、下载、校验、安装和回滚能力。更新只替换程序文件，不修改 `%LOCALAPPDATA%\ThemeInject` 下的主题、宠物、AI 设置、检查点和日志。

## 当前基础

- 程序版本来自 `Cargo.toml` 的 `package.version`，当前使用 SemVer。
- GitHub Actions 在 `clean-version` 分支推送后构建 Windows x64 ZIP，并创建对应的 GitHub Release。
- 发布包包含 `theme-inject.exe` 和中英文 README。
- 项目已依赖 `reqwest`、`semver`、`serde_json` 和 `sha2`，可以复用现有网络与校验能力。
- 当前是便携式单 EXE 启动器，没有安装器，也没有独立更新进程。

## 更新源与版本规则

更新源固定为：

```text
https://github.com/codecnmc/codex-theme-inject
```

使用 GitHub Releases API 获取最新正式版本，不直接比较普通分支 commit。Release 标签必须为 `v{SemVer}`，例如 `v0.0.4`。

发布要求：

1. 每次正式发布必须先提升 `Cargo.toml` 版本。
2. Release 必须包含 `theme-inject-windows-x64-v{version}.zip`。
3. Release 必须同时包含同名 `.sha256` 文件。
4. Release 同时提供 `theme-inject-themes-v{version}.zip` 主题合集及其 `.sha256`，但程序更新器只下载 Windows 程序包。
5. 预发布版本默认不推送给稳定版用户，后续可增加更新通道设置。

## 用户流程

1. 程序启动后读取上次检查时间，超过 24 小时才静默检查一次。
2. “高级”页显示当前版本、最新版本和上次检查结果，并提供“检查更新”。
3. 有新版本时展示版本号、发布时间和 Release 更新说明。
4. 用户点击“下载更新”后显示下载进度；下载不阻塞主题面板和 Codex。
5. 下载完成并通过 SHA-256 校验后，用户点击“安装并重启”。
6. Theme Inject 启动临时更新器并退出；更新器替换 EXE 后重新启动新版本。
7. 更新失败时恢复旧 EXE，并保留可读错误日志。

网络失败、GitHub 限流、无新版本或 Release 结构不完整时，不影响 Theme Inject 正常启动和注入。

## 后端模块

新增 `src/updater.rs`，职责分为四部分：

### 版本检查

- 请求 `repos/codecnmc/codex-theme-inject/releases/latest`。
- 设置明确的 `User-Agent`、GitHub API `Accept` 和请求超时。
- 校验仓库地址、标签格式、目标资产名称、HTTPS 下载地址和最大文件大小。
- 使用 `semver::Version` 比较当前版本与最新版本。

### 下载与校验

- 流式下载 ZIP 和 `.sha256`，不将整个安装包一次性读入内存。
- 临时文件写入 `%LOCALAPPDATA%\ThemeInject\updates\v{version}`。
- 限制响应大小，并拒绝非 GitHub HTTPS 重定向目标。
- 下载完成后计算 SHA-256，必须与 Release 中的校验文件一致。
- 解压时复用安全路径校验，拒绝绝对路径、父目录跳转、符号链接和额外可执行文件。

### 安装辅助进程

- 将新版本 EXE 解压到更新临时目录。
- 从临时目录启动更新辅助进程，并传入父进程 PID、当前 EXE 路径、新 EXE 路径和重启参数。
- 辅助进程等待旧 Theme Inject 退出后执行替换。
- 旧 EXE 先改名为 `.backup`，新 EXE 使用同目录原子移动写入。
- 新版本启动成功后清理备份；启动失败则恢复旧 EXE。
- 开发构建和 `target` 目录内的 EXE 禁用自动安装，只允许检查 Release。

### 状态持久化

在 `AppSettings` 增加向后兼容字段：

```text
lastUpdateCheckAt
skippedUpdateVersion
downloadedUpdateVersion
```

下载进度只保存在内存；已验证安装包的位置可以在重启后恢复。

## RPC 与前端

新增 RPC：

```text
app.update.status
app.update.check
app.update.download
app.update.install
app.update.cancel
```

所有方法加入 Bridge 白名单，并设置独立超时。下载使用后端状态轮询或小型进度事件，不向 renderer 传输 ZIP 或 Base64。

“高级”页新增非嵌套的更新区域：

- 当前版本和更新状态。
- “检查更新”按钮。
- 新版本号与精简 Release 说明。
- 下载按钮、进度条、取消按钮。
- 校验完成后的“安装并重启”按钮。
- 开发构建显示“开发版本不执行自动替换”。

## GitHub Actions 调整

在 `.github/workflows/package.yml` 中增加：

1. 校验 Release 标签与 `Cargo.toml` 版本一致。
2. 为最终 ZIP 生成 SHA-256 文件。
3. 将 ZIP 和 `.sha256` 同时上传为 Artifact 和 Release Asset。
4. 同一版本标签已存在时明确失败，避免覆盖历史 Release。
5. 保留 `cargo test --locked` 和运行时脚本检查作为发布门槛。

## 安全边界

- 更新源和仓库名写死，普通主题或配置不能修改下载地址。
- 只接受 HTTPS 和预期的 GitHub Release 资产。
- 未通过 SHA-256 校验的文件不能进入安装阶段。
- 安装前再次确认目标是当前运行的 Theme Inject EXE。
- 不删除 `%LOCALAPPDATA%\ThemeInject` 用户数据。
- 不在生成任务运行时强制退出；必须等待任务结束或由用户明确确认中断。
- SHA-256 只能验证发布资产一致性，后续正式分发建议增加 Windows Authenticode 代码签名。

## 实施顺序

### 第一阶段：只检查

- 实现 Release API 客户端、SemVer 比较、状态结构和单元测试。
- 在高级页显示版本与手动检查结果。
- 验证离线、限流、无 Release、预发布和版本格式错误场景。

### 第二阶段：安全下载

- 增加 Release 资产选择、流式下载、进度、取消和 SHA-256 校验。
- 更新 GitHub Actions 产出校验文件。
- 验证大文件限制、重定向、截断下载和校验失败场景。

### 第三阶段：安装与回滚

- 实现临时更新辅助进程、EXE 备份、替换、重启和回滚。
- 验证安装路径含中文和空格、目标文件被占用、新版本启动失败等场景。
- 增加开发构建保护和生成任务保护。

### 第四阶段：自动检查

- 增加每日一次静默检查。
- 只提示，不自动下载或安装。
- 增加跳过当前版本和重新检查功能。

## 测试计划

- SemVer：旧版、同版、新版、预发布和非法标签。
- GitHub 响应：正常、404、403 限流、超时、字段缺失和资产重复。
- 下载：正常、断线、取消、超限、哈希不一致和恶意 ZIP 路径。
- 安装：正常替换、文件占用、权限不足、启动失败和回滚成功。
- 数据保护：更新前后主题、宠物、AI Key、检查点和设置保持不变。
- UI：无更新、有更新、下载中、校验失败、等待重启和开发构建状态。
- 回归：`cargo test`、`cargo clippy --all-targets -- -D warnings`、`node scripts/check-runtime.mjs`。

## 验收标准

- 正式版可以手动检查 GitHub 最新 Release，并准确比较版本。
- 下载过程不向 Codex renderer 写入安装包数据，不造成界面卡顿。
- 只有通过 SHA-256 校验的包可以安装。
- 安装成功后版本更新且用户数据完整。
- 任一步骤失败都不会破坏当前可运行版本，并能在日志中定位原因。
- 离线和 GitHub 不可用时不影响主题注入、宠物或 AI 生成功能。
