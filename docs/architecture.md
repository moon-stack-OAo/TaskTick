# 时序 · TaskTick 架构说明

## 分层

```text
React UI（任务 / 日志 / 设置）
        ↓ invoke
Tauri Commands（Rust）
        ↓
Scheduler + Action Registry + Tray + Notify
        ├── open_app（fire_and_forget | wait_exit）
        ├── open_url
        ├── run_script（stdout/stderr · workingDir · timeout）
        └── wecom_ui_dm（键鼠 或 C# FlaUI Agent · 见 docs/wecom-agent.md）
```

插件扩展（新增动作、steps、失败通知）见 **[docs/plugins.md](./plugins.md)**。

## 托盘与窗口

- Tauri 2 内置 `tray-icon`：`TrayIconBuilder` + 菜单。
- `CloseRequested`：按 `settings.minimizeToTrayOnClose` 决定隐藏或退出（保留系统标题栏，不重写整套自定义装饰）。
- `ExitFlag`：托盘「退出」时允许真正结束进程。
- **单实例**：`tauri-plugin-single-instance`（须最先注册）；二次启动时 `show` + `unminimize` + `set_focus` 已有主窗口，新进程退出。该插件无 JS API，桌面端无需额外 capability。
- 开机自启：`tauri-plugin-autostart`（Windows 启动项）；偏好写入 `store.json` → `settings`。

## 调度要点

- 启动时 `scheduler::start`：tokio interval **1s** tick，读取 store 中 `enabled` 任务。
- **时区**：全部使用本机本地时区（chrono `Local`）。
- Cron：`croner`（5 字段 POSIX）；保存时校验，非法拒绝。
- once：调度/补跑触发后自动 `enabled=false`；手动执行不自动禁用。
- 并发：`running_tasks` 防重入；动作在 `spawn_blocking` 中执行。
- 全局暂停：`SchedulerHandle.paused` + 持久化 `settings.schedulerPaused`；设置页 / 托盘 / 侧栏状态一致；暂停时不触发调度，手动执行仍可用。
- **错过补跑**：`settings.missedJobPolicy`（`skip` | `run_once`）。`run_once` 在启动首次非暂停 tick 或从暂停恢复时，对 24h 内最近应触发点每任务最多补一次；日志前缀「补跑」。
- **nextRunAt**：`list_tasks` 计算字段；暂停时仍为「若恢复后」的下次时间。
- **动作链**：串行执行；`task.onActionFail` 为 `stop`（默认）或 `continue`。

## 任务模型

详见 `docs/ipc-contract.md`。示例：

```json
{
  "id": "t1",
  "name": "给张三发送晨间提醒",
  "enabled": true,
  "onActionFail": "stop",
  "trigger": {
    "type": "cron",
    "expr": "5 9 * * 1-5",
    "note": "工作日 09:05"
  },
  "actions": [
    {
      "type": "wecom_ui_dm",
      "id": "a1",
      "contact": "张三",
      "message": "早上好",
      "launchWecom": true,
      "timeoutSec": 30,
      "retryCount": 1,
      "notifyOnFail": true
    }
  ]
}
```

本地存储：`%APPDATA%\\com.moon.tasktick\\store.json`（含 `settings`）。旧版 `com.moon.autotask` 目录会在首次启动时自动迁移并保留。

## 存储：JSON 现状与升级路径（尚未迁移）

### 当前方案适用规模

| 维度    | 建议上限（经验值）            | 说明                           |
|-------|----------------------|------------------------------|
| 任务数   | 约 **200** 以内         | 全量读写 `store.json`，启动与保存成本可接受 |
| 日志条数  | 硬上限 **500**（FIFO 截断） | `list_logs` / 筛选均在内存过滤       |
| 单文件体积 | 约 **数 MB** 内         | 过大时启动解析与原子写（tmp+rename）变慢    |
| 并发写入  | 单进程 Mutex            | 无多实例共享；不适合多端同时写同一文件          |

**结论**：个人桌面、少量定时任务 + 近期日志场景，JSON 足够简单可靠。P3 已提供任务导入/导出与日志筛选导出，便于备份迁移。

### 何时考虑 SQLite

出现以下任一信号时再评估：

1. 任务数百级、日志需长期保留（万级以上）且要按索引查询。
2. 需要事务、部分更新、或避免「整文件重写」带来的 IO / 损坏窗口放大。
3. 需要多窗口/多进程安全读写，或后续做同步/云备份增量。
4. 复杂筛选（全文、动作类型、时间范围分页）成为主路径且内存过滤不够。

### 迁移思路（文档级，本阶段不实现）

1. **双写/旁路**：新增 `store.db`，启动时若仅有 JSON 则一次性导入；写路径先 DB 后可选同步导出 JSON 备份。
2. **表设计草图**：`tasks(id PK, payload JSON, enabled, updated_at)`、`logs(id PK, task_id, time, status, detail, steps JSON)`、`settings(key, value)`；索引 `(logs.task_id, time)`、`(logs.status, time)`。
3. **版本**：`AppStore.version` / DB `user_version` 升级脚本；导入导出 JSON 格式保持兼容作迁移工具。
4. **回滚**：保留最后一份 `store.json.bak`；失败时可读 JSON 降级。

**本阶段明确不做**：不引入 SQLite 依赖、不改运行时存储引擎。

## 失败通知

1. 设置总开关 `settings.notifyOnTaskFail`（默认开）优先。
2. 动作级 `notifyOnFail` 次之。
3. 实现见 `notify.rs`：统一申请/发送与可读反馈；日志 step 记录是否送达。

## FlaUI Agent

可选 C# 进程 `tools/wecom-agent`（FlaUI.UIA3）。设置 `wecomAgent.enabled=true` 后，`probe`/`send` 优先走 Agent，失败可回退键鼠。协议与构建见 `docs/wecom-agent.md`。

## wecom_ui_dm 实现要点

1. 可选启动 `WXWork.exe`（常见安装路径探测）。
2. `EnumWindows` 按标题匹配「企业微信」。
3. `SetForegroundWindow` + `AttachThreadInput` 前置。
4. 剪贴板写入联系人/消息，模拟 `Ctrl+F` → 粘贴 → `Enter` → 粘贴消息 → `Enter`。
5. 失败可按 `retryCount` 重试；通知走总开关 + `notifyOnFail`。
6. 预检：`wecom_probe_window` / `wecom_try_send`（不写正式任务日志）。
7. **锁屏跳过**：`OpenInputDesktop` 失败则返回「系统已锁屏，已跳过企微自动化」，不注入键鼠；空闲队列到点执行时同理。
8. **修饰键复位**：快捷键序列与 `run_once` 结束（含失败）强制 KEYUP Ctrl/Shift/Alt，避免粘键。

## 设计同步约定

1. 在 OpenDesign 项目 `auto-task-5f73`（显示名「时序 · TaskTick 桌面应用」）改原型
2. 确认后复制到本仓库 `design/tasktick-desktop-prototype.html`
3. 前端按 `design/` 实现，不直接把整页 HTML 当生产代码嵌入
