# TaskTick · AGENTS.md

面向本仓库的 AI / 助手约定。实现以代码与 `docs/` 为准；本文件只写协作约束与入口。

## 项目概览

| 项     | 值                                                                        |
|-------|--------------------------------------------------------------------------|
| 产品名   | 时序 · TaskTick                                                            |
| 可执行文件 | `TaskTick.exe`                                                           |
| 标识    | `com.moon.tasktick`                                                      |
| 形态    | Windows 托盘常驻定时任务（Tauri 2 + React + TypeScript）                           |
| 数据    | `%APPDATA%\com.moon.tasktick\store.json`（旧版 `com.moon.autotask` 自动迁移并保留） |

**平台默认 Windows 10/11 + WebView2。** 企微相关代码依赖 Win32 / FlaUI，非 Windows 应优雅失败，勿假设可跨平台跑通。

## 技术栈（勿擅自大升级）

- **前端**：React 19、TypeScript ~6、Vite 8、`@tauri-apps/api` 2
- **后端**：Rust edition 2021、Tauri 2、crate `tasktick` / lib `tasktick_lib`
- **企微 Agent**（可选）：`tools/wecom-agent`，`net10.0-windows` + FlaUI
- **样式**：`src/styles/tokens.css` + `app.css`（CSS 变量）；**无** Tailwind / CSS-in-JS
- **存储**：单进程 JSON + Mutex；日志硬上限 500；尚未迁数据库

## 目录职责

```text
src/                    React：api / components / pages / types / utils / styles
src-tauri/src/          Rust：commands · models · store · scheduler · actions ·
                        settings · tray · notify · idle_send · wecom · wecom_agent
src-tauri/capabilities/ Tauri 权限
tools/wecom-agent/      独立 C# FlaUI 进程（stdin/stdout JSON Lines）
docs/                   架构 · IPC · 插件 · Agent · 发版 · 设计同步
design/                 OpenDesign 快照（仅参考，实现以 src/ 为准）
.github/workflows/      ci.yml · build.yml · release.yml
```

## 沟通与文档

- 对用户回复使用**简体中文**（除非明确要求其他语言）。
- 代码标识符、API、文件路径保持英文；**注释优先中文**。
- 面向用户的错误信息、托盘文案、失败通知用**中文**。
- 技术文档优先中文 Markdown；改行为时同步更新对应 `docs/`（尤其 IPC / 插件）。
- 不确定需求时先澄清；涉及发版、改版本、Git 写操作先征得确认。

## 常用命令

```bash
npm install
npm run tauri:dev          # 或 npm run tauri -- dev

# 合并前本地校验（与 CI 对齐）
npm run typecheck          # tsc --noEmit
npm run check:rust         # cargo check
npm run test               # cargo test
npm run check              # typecheck + check:rust + test

# 清理构建产物（dist / target 等）
npm run clear              # 别名：npm run clean

# 企微 Agent（可选）
npm run agent:restore
npm run agent:build          # 开发用 framework-dependent
npm run agent:stage          # 自包含单文件 → src-tauri/binaries/（CI 打 zip 用）

# 本地打包（需 Updater 私钥环境变量；.env 无效；安装包不含 Agent）
npm run tauri:build
```

开发端口固定 **1420**（见 `vite.config.ts` / `tauri.conf.json`）。

## 前端约定

- 业务 IPC 一律经 `src/api/*.ts` 封装，页面**不要**直接散落 `invoke` 命令名。
- 类型与 Rust 领域模型对齐：`src/types/task.ts`。
- 字段 **camelCase**；动作 `type` 为 **snake_case**（如 `open_app`、`wecom_ui_dm`）。
- 组件 PascalCase；页面：`TasksPage` / `LogsPage` / `SettingsPage`。
- Design token 对齐 `design/brand-spec.md`；勿把整页 `design/*.html` 嵌进生产 UI。

## 后端约定

| 模块                            | 职责                                                   |
|-------------------------------|------------------------------------------------------|
| `commands.rs`                 | `#[tauri::command]` 入口，集中注册                          |
| `models.rs`                   | 序列化契约；`Action` 用 `tag = "type"` + snake_case         |
| `store.rs`                    | `AppState`、读写校验、导入导出、原子写                             |
| `scheduler.rs`                | 1s tick、触发、补跑、`nextRunAt`（计算字段，不持久化）                 |
| `actions.rs`                  | Action Registry / `execute_action`（`spawn_blocking`） |
| `tray.rs`                     | 托盘、`ExitFlag`、关窗进托盘                                  |
| `wecom.rs` / `wecom_agent.rs` | 键鼠方案 / FlaUI Agent 客户端                               |
| `idle_send.rs`                | 企微空闲发送队列                                             |

要点：

- `tauri-plugin-single-instance` **必须最先注册**。
- Command 返回 `Result<T, String>`；失败信息中文可读。
- 时区一律本机 `Local`；Cron 为 `croner` **5 字段** POSIX，非法表达式保存时拒绝。
- 旧 `store.json` 靠 serde `default` 兼容；勿无故破坏已有字段。
- 日志步骤勿写入密钥、Token、私钥等敏感内容。

扩展新动作时按 `docs/plugins.md` 清单：Rust 变体 + `execute_action` → 前端 types / TaskEditor / labels → 更新 `docs/ipc-contract.md` → 跑校验命令。

## Git 与版本（硬约束）

**未经用户明确授权，禁止：**

- `git add` / `git commit` / `git push`
- `git reset` / `git rebase` / `git checkout --` / 删分支 / 覆盖历史
- 修改版本号、打 / 移动 / 删除 `v*` tag
- 改仓库 Secrets、提交私钥 / keystore / 签名密钥

提交信息（仅在授权后）使用**中文**。未 push 前的多次修改视为同一版本迭代；**未允许打 tag 前不改版本号**。

发版前版本必须对齐：

1. `package.json` → `version`
2. `src-tauri/tauri.conf.json` → `version`
3. `src-tauri/Cargo.toml` → `version`
4. `CHANGELOG.md` → `## [x.y.z]`

（`WecomAgent.csproj` 的 Version **不在** CI 强制对齐范围内。）

## CI / 发版

| Workflow      | 触发                        | 作用                                       |
|---------------|---------------------------|------------------------------------------|
| `ci.yml`      | push/PR → main/master     | `tsc` + `cargo check/test` + Agent 构建    |
| `build.yml`   | **仅** `workflow_dispatch` | 预览安装包 → Actions artifact                 |
| `release.yml` | 推送 tag `v*`               | 校验版本与 CHANGELOG → 构建 → **draft** Release |

原则：

1. **release 不消费 build 的 artifact**，各自按 commit/tag 独立构建。
2. Release 默认为 draft，人工确认后再 Publish。
3. Updater 签名：`TAURI_SIGNING_PRIVATE_KEY`（私钥**文件内容**，非路径）；可选 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。本地密钥常见于 `~\.tauri\tasktick.key`，**禁止入库**。
4. 代码签名（SmartScreen）与 Updater minisign 是两套机制，勿混淆。

详细步骤见 `docs/release.md`。

## 企微与安全注意

- `wecom_ui_dm` 是 **UI 自动化**：需企微已登录、勿锁屏；联系人名称与会话显示名一致；首版仅私聊。
- Agent 非静默后台，常仍需激活窗口；失败可回退键鼠方案。
- 关闭企微窗口通常进托盘，不会强制杀进程。
- Agent 路径：设置 `wecomAgent.path` → 环境变量 `AUTO_TASK_WECOM_AGENT` → 与 `TaskTick.exe` 同目录 → `src-tauri/binaries/`（stage 产物）→ 仓库 `tools/wecom-agent/bin/...`。协议见 `docs/wecom-agent.md`。
- 正式安装包**不**内置 Agent；Release 附件 `wecom-agent-windows-x64.zip`，设置页可下载并选路径。
- 勿在日志、PR、changelog、示例中粘贴签名私钥或密码。

## 设计稿（OpenDesign）

- OD 项目：`auto-task-5f73`（显示名「时序 · TaskTick 桌面应用」）
- 仓库快照：`design/tasktick-desktop-prototype.html`、`design/brand-spec.md`
- 只把**确认过的**原型覆盖进 `design/`（见 `docs/design-sync.md`）
- 启动 OpenDesign 生成时默认用本机 OpenCode；勿默认改走 Cloud / 引导充值（除非用户明确要求）

## 文档索引

| 文档                     | 内容                |
|------------------------|-------------------|
| `docs/architecture.md` | 分层、托盘、调度、存储       |
| `docs/ipc-contract.md` | Commands 与领域模型    |
| `docs/plugins.md`      | 扩展新动作             |
| `docs/wecom-agent.md`  | FlaUI Agent 协议与构建 |
| `docs/release.md`      | 安装包、CI、应用内更新      |
| `docs/design-sync.md`  | 与 OpenDesign 同步   |
| `CHANGELOG.md`         | 版本变更              |

## 助手操作习惯

1. 改代码前先读相关 `docs/` 与现有模块，保持风格一致。
2. 改 IPC / 模型时前后端与 `docs/ipc-contract.md` 一起改。
3. 完成后跑：`npx tsc --noEmit` 与 `cargo test --manifest-path src-tauri/Cargo.toml`（触及 Agent 时再加 `dotnet build`）。
4. 不要主动创建无关 Markdown；用户未要求不写总结性长文到仓库。
5. 不要猜测或提交密钥路径以外的「真实私钥内容」；说明用占位符即可。
