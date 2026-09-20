# 时序 · TaskTick

Windows 托盘常驻的通用定时任务桌面应用（Tauri 2 + React + TypeScript）。

| 项     | 值                                           |
|-------|---------------------------------------------|
| 产品名   | 时序 · TaskTick                               |
| 可执行文件 | `TaskTick.exe`                              |
| 标识    | `com.moon.tasktick`                         |
| 当前版本  | **0.1.0**（见 [CHANGELOG.md](./CHANGELOG.md)） |
| 平台    | Windows 10/11（WebView2）                     |

## 功能概览

- **触发器**：一次性 / 每天（可按星期）/ Cron / 间隔
- **动作**：打开应用、打开网址、运行脚本
- **企微私聊**（`wecom_ui_dm`）：键鼠自动化；可选 FlaUI Agent；空闲队列延后发送；发送后可关窗
- **运行形态**：托盘常驻、单实例、关窗进托盘、开机自启、失败通知
- **数据**：任务/日志 JSON 持久化；导入导出

## 快速开始

```bash
npm install
npm run tauri dev
```

类型检查与后端测试：

```bash
npx tsc --noEmit
cargo test --manifest-path src-tauri/Cargo.toml
```

本地打包：

```bash
npm run tauri build
```

产物路径与安装注意见 [`docs/release.md`](./docs/release.md)。

### 企微 FlaUI Agent（可选）

安装包**不含** Agent。发版时 CI 产出 `wecom-agent-windows-x64.zip`（自包含）；设置页可下载、选路径并检测。

```bash
npm run agent:stage   # 本地：自包含 → src-tauri/binaries/
npm run agent:build   # 开发调试（framework-dependent）
```

协议见 [`docs/wecom-agent.md`](./docs/wecom-agent.md)。

## CI / 发版

| Workflow                                       | 触发                            | 作用                                              |
|------------------------------------------------|-------------------------------|-------------------------------------------------|
| [ci.yml](./.github/workflows/ci.yml)           | `push`/`PR` → `main`/`master` | `tsc` + `cargo check/test` + Agent 构建           |
| [build.yml](./.github/workflows/build.yml)     | **仅**手动 `workflow_dispatch`   | 预览安装包 → Actions artifact                        |
| [release.yml](./.github/workflows/release.yml) | 推送 tag `v*`                   | 校验版本与 CHANGELOG → 构建 → **draft** GitHub Release |

原则：

1. **release 不消费 build 的 artifact**，各自按 tag/commit 独立构建。
2. 发版前对齐：`package.json`、`src-tauri/tauri.conf.json`、`Cargo.toml`、`CHANGELOG.md` 的 `## [x.y.z]`。
3. **未明确授权前不改版本号、不打 tag、不 push**。

发版示例（需已 `git init` / 配置 remote，并经你确认）：

```bash
# 工作区干净且版本已改为 0.1.0 后
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
# 在 GitHub Releases 检查 draft，确认安装包后 Publish
```

## 目录结构

```text
TaskTick/
├── .github/workflows/      # ci / build / release
├── design/                 # OpenDesign 原型快照
├── docs/                   # 架构 · IPC · 插件 · Agent · 发版
├── tools/wecom-agent/      # .NET (net10.0-windows) FlaUI 独立进程
├── src/                    # React 前端
│   ├── api/
│   ├── components/
│   ├── pages/
│   ├── types/
│   └── utils/
├── src-tauri/              # Rust / Tauri（crate: tasktick）
│   └── src/
│       ├── actions.rs
│       ├── commands.rs
│       ├── idle_send.rs    # 企微空闲发送队列
│       ├── models.rs
│       ├── notify.rs
│       ├── scheduler.rs
│       ├── settings.rs
│       ├── store.rs
│       ├── tray.rs
│       ├── wecom.rs        # 键鼠方案
│       └── wecom_agent.rs  # Agent 客户端
├── CHANGELOG.md
└── README.md
```

## 文档

| 文档                                             | 内容                |
|------------------------------------------------|-------------------|
| [CHANGELOG.md](./CHANGELOG.md)                 | 版本变更              |
| [docs/architecture.md](./docs/architecture.md) | 分层、托盘、调度、存储       |
| [docs/ipc-contract.md](./docs/ipc-contract.md) | Commands 与领域模型    |
| [docs/plugins.md](./docs/plugins.md)           | 扩展新动作             |
| [docs/wecom-agent.md](./docs/wecom-agent.md)   | FlaUI Agent 协议与构建 |
| [docs/release.md](./docs/release.md)           | 安装包、CI、应用内更新      |
| [docs/design-sync.md](./docs/design-sync.md)   | 与 OpenDesign 同步约定 |

## 设计稿（OpenDesign）

- 本地：`design/tasktick-desktop-prototype.html`、`design/brand-spec.md`
- OD 项目：`auto-task-5f73`（显示名「时序 · TaskTick 桌面应用」）

```bash
start design/tasktick-desktop-prototype.html
```

`design/` 仅为设计快照；实现以本仓库代码为准。

## 注意事项

- 企微动作为 **UI 自动化**：需已登录、勿锁屏；联系人名称须与会话显示名一致；首版仅私聊。
- 关闭企微窗口通常进托盘，不会强制结束进程。
- 未签名安装包可能触发 SmartScreen；企业环境可能需自行签名。
- 数据目录：`%APPDATA%\com.moon.tasktick\store.json`（旧版 `com.moon.autotask` 会自动迁移副本）。
