# 安装包与更新说明

## 构建命令

在仓库根目录：

```bash
npm install
npm run tauri build
```

等价于：先 `npm run build`（`tsc && vite build`），再打包 Rust / 安装器。

因已开启 `bundle.createUpdaterArtifacts`，构建时必须提供 Updater 签名私钥（环境变量，**.env 无效**）：

```powershell
# Windows PowerShell（本地）
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\tasktick.key" -Raw
# 若生成密钥时设置了密码：
# $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "你的密码"
npm run tauri build
```

仅检查后端：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

前端类型检查：

```bash
npx tsc --noEmit
```

企微 Agent（可选）：

```bash
cd tools/wecom-agent
dotnet restore
dotnet build -c Release
```

## Windows 产物路径

默认输出目录（NSIS / MSI 等依 `bundle.targets`）：

```text
src-tauri/target/release/bundle/
├── nsis/          # *.exe 安装器 + *.exe.sig（updater 签名）
├── msi/           # 若启用 MSI + *.msi.sig
└── …

src-tauri/target/release/
└── TaskTick.exe  # 未打包的可执行文件（调试/便携试跑；由 mainBinaryName 指定）
```

具体文件名以构建日志为准；`productName` / `mainBinaryName` 均为 `TaskTick`（安装包与开始菜单名用英文，避免 WiX MSI 对非 ASCII 的限制；窗口标题等 UI 仍可为「时序 · TaskTick」），`identifier` 为 `com.moon.tasktick`。

数据目录（运行时，非安装目录）：

```text
%APPDATA%\com.moon.tasktick\store.json
```

首次启动若新目录尚无数据，会从旧目录 `%APPDATA%\com.moon.autotask\` 自动复制并**保留旧目录**作为备份。

## Windows 安装器注意点

1. **权限**：当前为用户级桌面应用；若企业环境拦截未签名安装包，需自行代码签名（见 Tauri 文档 [Windows 签名](https://v2.tauri.app/distribute/sign/windows/)）。这与 **Updater 更新签名**（minisign）是两套机制。
2. **WebView2**：目标机器需已安装 Microsoft Edge WebView2 Runtime（Win10/11 通常已有）。
3. **杀软 / SmartScreen**：未代码签名时首次运行可能提示；属预期。
4. **托盘常驻**：安装后从开始菜单启动；关闭主窗口默认进托盘（可用设置关闭该行为）。真正退出请用托盘菜单「退出」。
5. **单实例**：重复打开安装的快捷方式会激活已有窗口，不会起第二进程。
6. **开机自启**：由设置页开关控制，写入系统启动项（`tauri-plugin-autostart`）。
7. **应用内更新**：安装前会设置退出标志，避免关窗进托盘导致安装器卡住。

## GitHub Actions

| Workflow      | 文件                              | 触发                              | 产物                                                                 |
|---------------|---------------------------------|---------------------------------|--------------------------------------------------------------------|
| CI            | `.github/workflows/ci.yml`      | `push` / `PR` → `main`/`master` | 无安装包；校验 TS / Cargo / Agent                                         |
| Build preview | `.github/workflows/build.yml`   | **仅** `workflow_dispatch`       | Actions **artifact**（预览包 + 可选 Agent zip）；需签名 Secret                |
| Release       | `.github/workflows/release.yml` | 推送 tag `v*`                     | **draft** GitHub Release（安装包 + `.sig` + `latest.json` + Agent zip） |

约定：

1. **release 不消费 build 的 artifact**；发版流水线按 tag 指向的 commit **独立再构建**。
2. 发版前版本必须对齐：`package.json`、`src-tauri/tauri.conf.json`、`Cargo.toml`、`CHANGELOG.md` 中 `## [x.y.z]`。
3. Release 默认为 **draft**，人工确认安装包与说明后再 **Publish**。
4. Runner：`windows-latest`（含 WebView2 / MSVC 常规依赖）。
5. Updater 签名：仓库 Secrets 须配置 `TAURI_SIGNING_PRIVATE_KEY`（私钥**文件内容**，非路径）；可选 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。

本地等价于 CI 校验：

```bash
npm ci
npx tsc --noEmit
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
dotnet build -c Release --project tools/wecom-agent
```

## 应用内自动更新（Updater）

方案：`tauri-plugin-updater` + GitHub Releases 静态清单 `latest.json`。

### 配置要点

| 项      | 位置 / 值                                                                            |
|--------|-----------------------------------------------------------------------------------|
| 公钥     | `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`（可公开）                       |
| 清单地址   | `https://github.com/moon-stack-OAo/TaskTick/releases/latest/download/latest.json` |
| 产物签名   | CI 用 `TAURI_SIGNING_PRIVATE_KEY`；`createUpdaterArtifacts: true`                   |
| 客户端 UI | 设置 → 关于 →「检查更新」；启动约 8s 后静默检查并 toast                                               |
| 安装前退出  | 命令 `prepare_for_exit` 置 `ExitFlag`，再下载安装                                          |

### 密钥（切勿提交私钥）

本地密钥对（本机生成，勿入库）：

```text
%USERPROFILE%\.tauri\tasktick.key      # 私钥
%USERPROFILE%\.tauri\tasktick.key.pub  # 公钥（内容已写入 tauri.conf.json）
```

配置 GitHub Secret（仓库 Settings → Secrets → Actions）：

```powershell
# 将私钥内容写入 Secret（需已登录 gh）
Get-Content "$env:USERPROFILE\.tauri\tasktick.key" -Raw | gh secret set TAURI_SIGNING_PRIVATE_KEY
```

丢失私钥或密码后，**无法**为已安装客户端签发后续更新；须重新生成密钥并强制用户重装（公钥变更）。

### 发版与更新可见性

1. 推送 `vX.Y.Z` → `release.yml` 构建并创建 **draft** Release（含安装包、`.sig`、`latest.json`）。
2. 人工核对产物后 **Publish** Release。
3. 仅 **已发布**（非 draft）的 latest Release 上的 `latest.json` 可被客户端拉取；draft 阶段检查更新会失败或显示无更新。

### 用户侧行为

- 「检查更新」：有新版本则确认后下载安装（Windows `installMode: passive`）。
- 启动静默检查：仅 toast 提示，不自动下载。
- 更新通道依赖 GitHub；企业代理/断网会导致检查失败。

## 版本号

- 前端 `package.json` 与 `src-tauri/tauri.conf.json` / `Cargo.toml` 的 `version` 应保持一致。
- 未打 TAG 前不随意改版本号（见项目 Git 约定）。
