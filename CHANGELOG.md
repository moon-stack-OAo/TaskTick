# Changelog

本文件遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

发版约定：推送 annotated tag `vX.Y.Z` 触发 `.github/workflows/release.yml`；`package.json`、`src-tauri/tauri.conf.json`、`Cargo.toml` 与本节标题 `## [X.Y.Z]` 须一致。

## [Unreleased]

### 计划

- 安装包代码签名（Authenticode / SmartScreen）
- 企微 Agent 常驻进程与更稳的控件树适配

## [0.1.0] - 2026-09-20

首个可用版本：Windows 托盘常驻定时任务桌面端。

### 新增

- 触发器：`once` / `daily` / `cron` / `interval`
- 动作：打开应用、打开网址、运行脚本（PowerShell 等）
- 插件动作 `wecom_ui_dm`：企业微信私聊 UI 自动化（键鼠方案）
- 可选 C# FlaUI Agent（`tools/wecom-agent`，.NET Windows）：设置启用后 `probe`/`send` 优先走 Agent，失败可回退键鼠
- 安装包随附自包含 `wecom-agent.exe`（`bundle.externalBin`，与 `TaskTick.exe` 同目录；目标机无需另装 .NET）
- 企微空闲发送队列：键鼠空闲后倒计时再发，降低抢前台干扰；`force` 可立即执行
- 发送成功后可关闭企微主窗口（`closeAfterSend`，默认开启；通常进托盘）
- 托盘常驻、单实例、关窗进托盘、开机自启、失败系统通知
- 任务导入/导出、日志筛选与导出
- 调度暂停、补跑策略、下次运行时间展示
- 企微预检：测试定位窗口 / 试发送（不写正式任务日志）
- 应用内自动更新：`tauri-plugin-updater` + GitHub Releases `latest.json`；设置页「检查更新」；启动静默检查提示
- 发版流水线产出 updater 签名产物（`.sig`）与 `latest.json`（需 Secret `TAURI_SIGNING_PRIVATE_KEY`）

### 修复

- 全局禁用 WebView 默认右键菜单，避免桌面端弹出浏览器上下文菜单
- 安装版「检测 Agent」找不到可执行文件：此前 Agent 仅作可选 zip，未打入 NSIS/MSI

### 文档与工程

- `docs/`：架构、IPC、插件扩展、企微 Agent、发版说明
- `npm run agent:stage` / `scripts/stage-wecom-agent.ps1`：发布自包含 sidecar；`tauri:build` 与 CI/release 同步
- GitHub Actions：`ci.yml`（校验）、`build.yml`（手动预览包）、`release.yml`（`v*` tag 草稿 Release）
- NSIS 安装器支持简体中文与英文可选

### 说明

- 企微自动化依赖前台窗口与联系人显示名一致；改版或锁屏可能导致失败
- 关闭企微窗口一般不会结束进程（托盘常驻）
- 应用内更新依赖已 Publish 的 GitHub Release 上的 `latest.json`；draft 阶段客户端不可见
