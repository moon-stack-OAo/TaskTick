# IPC 契约（领域模型）

数据文件：`%APPDATA%\com.moon.tasktick\store.json`（Tauri `app_data_dir`）。

## Commands

| Command                           | 参数                                                                                                         | 返回                                                                            |
|-----------------------------------|------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| `get_store_info`                  | —                                                                                                          | `StoreInfo`                                                                   |
| `list_tasks`                      | —                                                                                                          | `Task[]`（含计算字段 `nextRunAt`）                                                   |
| `get_task`                        | `{ id }`                                                                                                   | `Task \| null`（含 `nextRunAt`）                                                 |
| `save_task`                       | `{ task }`                                                                                                 | `Task`（空 `id` 则新建；非法 cron 返回中文错误）                                             |
| `delete_task`                     | `{ id }`                                                                                                   | `boolean`                                                                     |
| `toggle_task`                     | `{ id, enabled }`                                                                                          | `Task`                                                                        |
| `list_logs`                       | `{ limit? }`                                                                                               | `ExecutionLog[]`                                                              |
| `append_log`                      | `{ log }`                                                                                                  | `ExecutionLog`                                                                |
| `clear_logs`                      | —                                                                                                          | `number`（清除条数）                                                                |
| `run_task_now`                    | `{ id, force? }`                                                                                           | `ExecutionLog`（立即执行并写入日志；含 wecom 且空闲发送开启时默认入队返回 skipped；`force=true` 跳过空闲立即跑） |
| `list_pending_wecom`              | —                                                                                                          | `PendingWecomItem[]`                                                          |
| `cancel_pending_wecom`            | `{ args?: { id? } }`                                                                                       | `number`（取消条数；`id` 空则清空队列）                                                    |
| `get_idle_send_status`            | —                                                                                                          | `IdleSendStatus`                                                              |
| `get_scheduler_status`            | —                                                                                                          | `SchedulerStatus`                                                             |
| `reload_scheduler`                | —                                                                                                          | `SchedulerStatus`（刷新启用计数 / 清理已删任务调度态）                                         |
| `get_settings`                    | —                                                                                                          | `SettingsView`（含 OS 实际开机自启状态、通知权限、暂停态、补跑策略）                                   |
| `update_settings`                 | `{ patch: { autostart?, minimizeToTrayOnClose?, notifyOnTaskFail?, schedulerPaused?, missedJobPolicy? } }` | `SettingsView`                                                                |
| `set_scheduler_paused`            | `{ paused }`                                                                                               | `SchedulerStatus`（持久化 + 托盘菜单同步；从暂停恢复时可能触发补跑）                                  |
| `show_main_window`                | —                                                                                                          | `void`                                                                        |
| `wecom_probe_window`              | `{ args: { launchWecom?, timeoutSec? } }`                                                                  | `WecomProbeResult`（预检：定位窗口，不发消息，不写正式日志；启用 Agent 时优先 FlaUI）                    |
| `wecom_try_send`                  | `{ args: { contact, message, launchWecom?, timeoutSec?, retryCount? } }`                                   | `WecomTrySendResult`（预检：完整发送，不写正式日志）                                          |
| `wecom_agent_ping`                | —                                                                                                          | `{ ok, note, steps, usedAgent }`（探测 C# FlaUI Agent）                           |
| `request_notification_permission` | —                                                                                                          | `string`（权限状态文案，如 `granted`）                                                  |
| `open_data_dir`                   | —                                                                                                          | `void`（系统资源管理器打开 `dataDir`）                                                   |
| `reset_settings`                  | —                                                                                                          | `SettingsView`（settings 恢复默认；同步关闭 OS 自启；不改任务/日志）                              |
| `export_tasks`                    | `{ args?: { includeLogs? } }`                                                                              | `ExportPayload`（任务；可选 logs；不含计算字段 nextRunAt）                                  |
| `import_tasks`                    | `{ args: { json, mode: "merge"\|"replace" } }`                                                             | `ImportResult`（非法项跳过；成功后已刷新调度计数）                                              |
| `filter_logs`                     | `{ filter: LogFilter }`                                                                                    | `ExecutionLog[]`                                                              |
| `export_filtered_logs`            | `{ args: { filter, format?: "json"\|"txt" } }`                                                             | `string`（文件内容；由前端选路径后 `write_text_file`）                                      |
| `write_text_file`                 | `{ path, content }`                                                                                        | `void`                                                                        |
| `read_text_file`                  | `{ path }`                                                                                                 | `string`                                                                      |
| `prepare_for_exit`                | —                                                                                                          | `void`（更新安装 / 退出前置 `ExitFlag`，避免关窗进托盘）                                        |

## Task

```ts
type Trigger =
    | { type: "once"; datetime: string }
    | { type: "daily"; time: string; weekdays: number[] }
    | { type: "cron"; expr: string; note?: string }
    | { type: "interval"; every: number; unit: string };

type OnActionFail = "stop" | "continue"; // 默认 stop；旧 store 缺省兼容
type WaitMode = "fire_and_forget" | "wait_exit"; // 默认 fire_and_forget
type MissedJobPolicy = "skip" | "run_once"; // 默认 skip

type Action =
    | {
    type: "open_app";
    id: string;
    path: string;
    args?: string;
    waitMode?: WaitMode;
    /** wait_exit 时超时秒数；0=不限时；默认 60；超时记失败并尝试终止进程 */
    timeoutSec?: number;
}
    | { type: "open_url"; id: string; url: string }
    | {
    type: "run_script";
    id: string;
    runtime: string;
    path: string;
    /** 工作目录；空则脚本所在目录 */
    workingDir?: string;
    /** 超时秒数；0=不限时；默认 60 */
    timeoutSec?: number;
}
    | {
    type: "wecom_ui_dm";
    id: string;
    contact: string;
    message: string;
    launchWecom?: boolean;
    timeoutSec?: number;
    retryCount?: number;
    notifyOnFail?: boolean;
};

interface Task {
    id: string;
    name: string;
    enabled: boolean;
    trigger: Trigger;
    actions: Action[];
    /** 动作失败策略，默认 stop */
    onActionFail?: OnActionFail;
    lastRunAt?: string | null;
    lastStatus?: "success" | "failed" | "running" | "skipped" | null;
    updatedAt?: string | null;
    /** 下次预计触发（本地 `YYYY-MM-DD HH:MM:SS`）；计算字段，不入库；禁用/不再执行时为 null */
    nextRunAt?: string | null;
}

interface SchedulerStatus {
    running: boolean;
    enabledTaskCount: number;
    runningTaskCount: number;
    lastTickAt?: string | null;
    nextCheckAt?: string | null;
    tickIntervalSecs: number;
    paused?: boolean;
}

interface WecomIdleSendSettings {
    enabled: boolean;          // 默认 true
    idleSeconds: number;       // 默认 30；写入时钳制 5–600
    countdownSeconds: number;  // 默认 5；写入时钳制 1–60
}

interface SettingsView {
    autostart: boolean;
    minimizeToTrayOnClose: boolean;
    /** 失败通知总开关；默认 true */
    notifyOnTaskFail: boolean;
    schedulerPaused: boolean;
    /** 错过触发补跑策略；默认 skip */
    missedJobPolicy: MissedJobPolicy;
    wecomIdleSend: WecomIdleSendSettings;
    /** 通知权限：granted / denied / prompt / unknown */
    notificationPermission?: string;
}

type IdleSendPhase = "idle" | "waiting_idle" | "countdown" | "running";

interface PendingWecomItem {
    id: string;
    taskId: string;
    taskName: string;
    /** schedule | manual | catchup */
    source: string;
    enqueuedAt: string;
}

interface IdleSendStatus {
    enabled: boolean;
    phase: IdleSendPhase;
    pendingCount: number;
    idleSecondsConfig: number;
    countdownSecondsConfig: number;
    currentIdleSeconds: number;
    countdownRemainingSecs: number;
    activePendingId?: string | null;
    activeTaskName?: string | null;
    schedulerPaused: boolean;
}

interface WecomProbeResult {
    ok: boolean;
    note: string;
    steps: LogStep[];
}

interface WecomTrySendResult {
    ok: boolean;
    note: string;
    steps: LogStep[];
    notified: boolean;
}
```

## store.json 结构

```json
{
  "version": 1,
  "tasks": [],
  "logs": [],
  "settings": {
    "autostart": false,
    "minimizeToTrayOnClose": true,
    "notifyOnTaskFail": true,
    "schedulerPaused": false,
    "missedJobPolicy": "skip",
    "wecomIdleSend": {
      "enabled": true,
      "idleSeconds": 30,
      "countdownSeconds": 5
    }
  }
}
```

- `settings.autostart`：偏好开关；真正写入系统启动项由 `tauri-plugin-autostart` 完成。
- `settings.minimizeToTrayOnClose`：关闭主窗口时是否隐藏到托盘（默认 `true`）。
- `settings.notifyOnTaskFail`：任务失败系统通知**总开关**（默认 `true`）。关闭后即使动作 `notifyOnFail=true` 也不弹通知。
- `settings.schedulerPaused`：调度暂停态**持久化**；启动时恢复；设置页开关与托盘「暂停/恢复调度」同步。
- `settings.missedJobPolicy`：`skip`（默认）不补跑；`run_once` 在启动或从暂停恢复时，对 24h lookback 内最近一次应触发点补跑一次（每任务每次扫描最多一次）。
- `settings.wecomIdleSend`：企微空闲发送队列；`enabled=false` 时回退为到点立刻抢前台发送。
- 旧 store 缺少新字段时由 serde `default` 兼容。

## 通知优先级

1. **设置总开关** `settings.notifyOnTaskFail`：关闭 → 永不弹失败通知。
2. **动作级** `notifyOnFail`（目前主要是 `wecom_ui_dm`）：为 false → 该动作失败不通知。
3. 两者皆开 → 调用 `tauri-plugin-notification`；权限拒绝或发送失败时写入日志 step「失败通知」，并给出可读说明。

## 动作链

- 多动作**串行**执行。
- `onActionFail=stop`（默认）：失败后中止后续，日志增加「动作链中止」step。
- `onActionFail=continue`：失败后继续，日志增加「动作链继续」step；任务总状态仍为 `failed`。

## 调度说明

- **时区**：所有触发时间均以**本机本地时区**为准，不做多时区转换。
- 应用启动后自动启动调度循环（默认每 1s tick）。
- `once`：到期执行一次后**自动禁用**任务，避免重复触发；手动「立即执行」不自动禁用。
- `daily`：按 `time`（`HH:MM`）+ `weekdays`（0=周日…6=周六）。
- `cron`：使用 `croner` 解析（标准 5 字段 POSIX，如 `5 9 * * 1-5`）；`save_task` 非法表达式拒绝并返回中文错误。
- `interval`：`every` + `unit`，兼容「秒/分钟/小时」及英文；启用后先记起点，下一周期再执行。
- 全局暂停时 tick 仍跑但不触发新任务；手动 `run_task_now` 仍可用。
- 同一任务禁止重入；错误写入 `ExecutionLog`，不崩进程。

### 错过补跑（missedJobPolicy）

| 策略         | 行为                            |
|------------|-------------------------------|
| `skip`     | 不补跑（默认）                       |
| `run_once` | 启动后首次非暂停 tick，或从暂停恢复时：对启用任务扫描 |

规则：

- **lookback**：过去 **24 小时**内最近一次应触发点。
- **每任务每次扫描最多补一次**，日志 `detail` 前缀为「补跑」。
- 若 `lastRunAt >= 应触发点`，视为已覆盖，不补。
- **once**：目标时间已过、仍 enabled、落在 lookback 内 → 按策略补或不补；补跑后仍会自动禁用。
- **interval**：无内存起点时不补（与「先记起点」一致）；有起点且已逾期则补一次并重置起点。
- 暂停期间不补跑；恢复时再扫描。

### nextRunAt

- `list_tasks` / `get_task` 附带计算字段，格式 `YYYY-MM-DD HH:MM:SS`（本地）。
- **暂停时仍返回「若恢复后」的下次时间**（UI 标注「若恢复 · …」）。
- 禁用 / once 已过期：`null` → UI 显示「不再执行」或「—」。

## wecom_ui_dm（Windows）

首版实现：Win32 找窗 + 前台激活 + 剪贴板粘贴 + 快捷键（`Ctrl+F` / `Ctrl+V` / `Enter`）。

- 成功/失败均写入分步 `steps`（启动、定位窗口、前置、搜索、粘贴联系人、打开私聊、粘贴消息、发送、通知）。
- `notifyOnFail=true` 且总开关开启时通过 `tauri-plugin-notification` 发系统通知。
- **预检 IPC**：`wecom_probe_window` 只定位/前置；`wecom_try_send` 完整发送。结果返回前端 toast 展示；**不写入正式任务日志**（试发送 steps 内标注「预检」）。**试发送始终强制立即**，不走空闲队列。
- **限制**：依赖前台窗口与企微默认快捷键；改版、重名联系人、抢焦点失败可能导致误发或失败。**已锁屏**时跳过注入并失败（不抢前台）；非 Windows 直接失败。快捷键结束后会强制抬起修饰键。

### 企微空闲发送队列（wecomIdleSend）

设计：**动作序列含 `wecom_ui_dm` 时整任务入队**；纯非 wecom 任务仍立即执行。

状态机：

| phase          | 含义                                                |
|----------------|---------------------------------------------------|
| `idle`         | 队列空                                               |
| `waiting_idle` | 有待发，等待键鼠空闲 ≥ `idleSeconds`                        |
| `countdown`    | 已空闲，倒计时 `countdownSeconds`；期间输入则回到 `waiting_idle` |
| `running`      | 正在执行队头任务                                          |

规则：

- 调度 / 补跑 / 普通 `run_task_now`：若 `wecomIdleSend.enabled` 且任务含 wecom → 入队（手动入队写一条 `skipped` 日志说明排队）。
- `run_task_now({ force: true })`：取消该任务队列项（若有）并立刻执行。
- 全局 `schedulerPaused` 时不开始倒计时；已开始的倒计时会取消并回队列。
- `enabled=false`：不入队，行为与旧版一致（到点立刻发）。
- 事件：`idle-send-changed` → `IdleSendStatus`（前端/托盘同步「待发送 N」）。

## run_script / open_app（P1）

- `run_script`：捕获 stdout/stderr 写入 steps（各约 4KB，超长截断并提示）；支持 `workingDir`、`timeoutSec`（默认 60，0=不限时；超时终止并记失败）。
- `open_app`：`waitMode=fire_and_forget`（默认）启动即继续；`wait_exit` 等待退出，可配 `timeoutSec`，超时记失败并尝试 kill。

## 托盘

- 左键：显示/聚焦主窗口。
- 右键：打开主窗口 / 暂停或恢复调度 / 退出。
- 关闭窗口：若 `minimizeToTrayOnClose` 为真则隐藏；否则退出。托盘「退出」强制结束进程。
- 暂停态与设置页开关、`settings.schedulerPaused` 三方一致。
- 悬停 tooltip：`时序 · TaskTick`；有企微待发时追加 ` · 待发送 N`。

## 导入 / 导出

### ExportPayload

```ts
interface ExportPayload {
    version: number;
    exportedAt: string; // 本地 YYYY-MM-DD HH:MM:SS
    tasks: Task[];      // 无 nextRunAt
    logs?: ExecutionLog[];
}
```

### ImportMode / ImportResult

| mode      | 行为                                                                      |
|-----------|-------------------------------------------------------------------------|
| `merge`   | 按 `id`：已存在则覆盖（导入缺 lastRunAt/lastStatus 时保留本地）；新 id 插入列表头部；空 id 生成新 UUID |
| `replace` | 用合法任务整体替换 `tasks`；**若全部非法则不改动现有任务**；不改 logs / settings                  |

```ts
interface ImportResult {
    imported: number; // merge=新增数；replace=写入总数
    updated: number;  // 仅 merge
    skipped: number;
    errors: string[];
    taskCount: number;
}
```

支持的 JSON 形态：`ExportPayload`、含 `tasks` 的对象（如 store.json 片段）、或纯 `Task[]`。结构无法反序列化或业务校验失败的项计入 `skipped`/`errors`。

### LogFilter

```ts
interface LogFilter {
    taskId?: string | null;
    status?: string | null; // success|failed|running|skipped|all
    keyword?: string | null;
    timeFrom?: string | null; // 字符串比较，建议 YYYY-MM-DD HH:MM:SS
    timeTo?: string | null;
    limit?: number | null;    // 默认最多 500
}
```

前端日志页可本地筛选；导出走 `export_filtered_logs` 保证与服务端规则一致。文件对话框使用 `@tauri-apps/plugin-dialog`。

## 设置持久化清单

均写入 `store.json` → `settings`，重启保留：

| 字段                             | 默认    | 说明              |
|--------------------------------|-------|-----------------|
| autostart                      | false | 偏好 + OS 启动项     |
| minimizeToTrayOnClose          | true  | 关窗隐藏            |
| notifyOnTaskFail               | true  | 失败通知总开关         |
| schedulerPaused                | false | 与托盘同步           |
| missedJobPolicy                | skip  | skip / run_once |
| wecomIdleSend.enabled          | true  | 企微空闲队列总开关       |
| wecomIdleSend.idleSeconds      | 30    | 空闲阈值（5–600）     |
| wecomIdleSend.countdownSeconds | 5     | 发送前倒计时（1–60）    |

`reset_settings` 恢复上表默认；`notificationPermission` 为只读视图字段。
`update_settings` 的 `patch.wecomIdleSend` 支持局部字段。

## 说明

- 字段风格与设计稿一致：任务层 camelCase，动作类型 snake_case。
- 日志最多保留 500 条；`list_logs` 默认 100。
- 打开数据目录：`open_data_dir` → opener `open_path(dataDir)`（需 `opener:allow-open-path`）。
