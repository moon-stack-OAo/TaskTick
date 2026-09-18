# 企业微信 FlaUI Agent 架构

## 为什么拆 Agent

Tauri/Rust 侧不适合深度 UI Automation；用独立 **C# + FlaUI** 进程：

- 控件级查找（比纯 `SendInput` 稳）
- 与调度器解耦，可单独升级 Agent
- 失败时仍可回退到现有键鼠方案（`wecom.rs`）

**现实限制**：FlaUI 仍通常需要企微窗口存在；多数场景仍会激活窗口。目标是「少抢全局鼠标、更准地点控件」，不是保证完全后台静默。

**锁屏与修饰键**：执行前用 `OpenInputDesktop` 检测锁屏，已锁屏则跳过注入（不抢前台）；快捷键序列结束时强制抬起 Ctrl/Shift/Alt，避免粘键。

## 目录

```text
tools/wecom-agent/          # .NET 8 控制台
  WecomAgent.csproj
  Program.cs                # JSON Lines stdin/stdout
  WecomAutomation.cs        # FlaUI 定位/发送
src-tauri/src/wecom_agent.rs  # Rust 拉起与协议客户端
```

## 进程模型

```text
时序 · TaskTick (Rust)
    │  stdin/stdout JSON Lines（一行一请求/响应）
    ▼
wecom-agent.exe (FlaUI)
    │
    ▼
企业微信窗口 / UIA 树
```

- 按需拉起，命令结束可退出（首版 **one-shot**：每个请求启停一次，实现简单）
- 后续可改为常驻进程 + 心跳，降低冷启动

## 协议（JSON Lines）

### 请求

```json
[
  {
    "id": "1",
    "cmd": "ping"
  },
  {
    "id": "2",
    "cmd": "probe",
    "launchWecom": true,
    "timeoutSec": 30
  },
  {
    "id": "3",
    "cmd": "send",
    "contact": "张三",
    "message": "你好",
    "launchWecom": true,
    "timeoutSec": 30,
    "retryCount": 1
  }
]
```

### 响应

```json
{
  "id": "2",
  "ok": true,
  "note": "已定位企微主窗口",
  "steps": [
    {
      "title": "定位窗口",
      "status": "ok",
      "time": "12:00:01",
      "note": "Name=企业微信"
    }
  ]
}
```

| cmd     | 含义         |
|---------|------------|
| `ping`  | 探活         |
| `probe` | 仅定位/前置窗口   |
| `send`  | 搜索联系人并发送私聊 |

## 构建

需安装 [.NET SDK](https://dotnet.microsoft.com/download)（当前工程目标 `net10.0-windows`；若只有 .NET 8，把 csproj 的 `TargetFramework` 改回 `net8.0-windows` 即可）。

若终端提示找不到 `dotnet`，先用完整路径，或把 `C:\Program Files\dotnet` 加入用户 PATH 后**重开终端**：

```powershell
& "C:\Program Files\dotnet\dotnet.exe" --version
```

```powershell
cd D:\Moon\tools\TaskTick\tools\wecom-agent
& "C:\Program Files\dotnet\dotnet.exe" restore
& "C:\Program Files\dotnet\dotnet.exe" build -c Release
# 产物：bin\Release\net10.0-windows\wecom-agent.exe
```

也可发布单文件：

```powershell
& "C:\Program Files\dotnet\dotnet.exe" publish -c Release -r win-x64 --self-contained false -p:PublishSingleFile=true
```

## 时序 · TaskTick 如何找到 Agent

查找顺序：

1. 设置 `wecomAgent.path`（绝对路径）
2. 环境变量 `AUTO_TASK_WECOM_AGENT`
3. 相对仓库：`tools/wecom-agent/bin/Release/net8.0-windows/wecom-agent.exe`
4. 与 `TaskTick.exe` 同目录的 `wecom-agent.exe`（打包分发时）

设置 `wecomAgent.enabled=true` 且能 `ping` 成功时：

- `probe` / `send` **优先走 Agent**
- Agent 失败再 **fallback** 到内置键鼠（可关）

## 与空闲队列的关系

空闲/预约队列仍在 Rust：`idle_send` 决定「何时发」；真正点 UI 时调用 Agent 或键鼠。两者正交。

## 后续

1. 用 FlaUI Inspect / Accessibility Insights 抓企微控件树，固化 AutomationId/Name
2. 常驻 Agent + 命名管道（比反复启进程更快）
3. 打包时把 `wecom-agent.exe` 打进 NSIS 旁路目录
