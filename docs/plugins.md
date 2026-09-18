# Action Registry 扩展指南

本文说明如何在「时序 · TaskTick」中新增动作类型，以及日志 steps、失败通知的约定。

相关代码：

| 模块   | 路径                                            | 职责                     |
|------|-----------------------------------------------|------------------------|
| 动作模型 | `src-tauri/src/models.rs` → `Action`          | 序列化契约（`type` 标签）       |
| 执行入口 | `src-tauri/src/actions.rs` → `execute_action` | Action Registry 分发     |
| 调度编排 | `src-tauri/src/scheduler.rs`                  | 动作链、`onActionFail`、写日志 |
| 失败通知 | `src-tauri/src/notify.rs`                     | 总开关 + 权限 + 发送          |
| 前端类型 | `src/types/task.ts`                           | 与 Rust 对齐              |
| 编辑器  | `src/components/TaskEditor.tsx`               | 表单字段                   |

---

## 1. 扩展新动作的步骤

### 1.1 在 `Action` 枚举增加变体

使用 `#[serde(tag = "type", rename_all = "snake_case")]`，对外 JSON 的 `type` 用 **snake_case**（如 `open_app`、`wecom_ui_dm`）。

字段命名约定：

- Rust 侧可用 `snake_case`；需与前端 camelCase 对齐的字段加 `#[serde(rename = "camelCase")]`。
- 每个动作必须有稳定的 `id: String`（前端生成，用于编辑/排序）。
- 布尔默认值优先 `#[serde(default = "default_true")]` 或 `#[serde(default)]`，保证旧 store 可反序列化。
- 超时类字段统一 `timeoutSec`（秒）；`0` 表示不限时（若该动作支持）。

示例骨架：

```rust
#[serde(rename = "my_action")]
MyAction {
    id: String,
    // 业务字段…
    #[serde(rename = "timeoutSec", default = "default_timeout_sec")]
    timeout_sec: u32,
    #[serde(rename = "notifyOnFail", default = "default_true")]
    notify_on_fail: bool,
},
```

### 1.2 在 `execute_action` 增加分支

返回 `ActionOutcome`：

```rust
pub struct ActionOutcome {
    pub title: String,      // 日志展示标题，如「打开网址」
    pub status: RunStatus,  // Success / Failed
    pub note: String,       // 单行摘要（可含耗时）
    pub steps: Vec<LogStep>, // 分步明细；可为空
}
```

约定：

1. **阻塞执行**：`execute_action` 在 `spawn_blocking` 中调用，勿在此处再开无界异步任务而不等待。
2. **失败信息可读**：`Err` / `note` 使用中文，便于托盘通知与日志页展示。
3. **分步日志**：复杂动作（如企微）填充 `steps`；简单动作可留空，由 `outcome_to_steps` 合成一条汇总 step。
4. **副作用可控**：预检类 IPC（如 `wecom_probe_window`）不要写入正式 `ExecutionLog`。

### 1.3 同步前端

1. `src/types/task.ts`：为 `Action` 联合类型增加成员。
2. `TaskEditor`：增加表单项与校验。
3. `utils/labels.ts`（若有）：动作类型中文名。
4. 更新 `docs/ipc-contract.md` 中的 `Action` 说明。

### 1.4 不需要改调度器的情况

调度器只认 `Action` 枚举与 `ActionOutcome`；新增变体后，动作链、`onActionFail`、补跑逻辑自动适用。

---

## 2. 模型字段约定

### 任务级

| 字段             | 说明                             |
|----------------|--------------------------------|
| `onActionFail` | `stop`（默认）中止后续动作；`continue` 继续 |
| `enabled`      | 调度是否纳入；`once` 调度/补跑成功后可自动禁用    |
| `nextRunAt`    | **计算字段**，不持久化；导入导出时应清除         |

### 动作级（推荐）

| 字段             | 说明                      |
|----------------|-------------------------|
| `id`           | 动作实例 ID                 |
| `timeoutSec`   | 超时秒数                    |
| `notifyOnFail` | 本动作失败时是否希望通知（仍受设置总开关约束） |
| `retryCount`   | 可选；由动作自行实现重试            |

### 设置级（影响所有动作）

| 字段                      | 说明                   |
|-------------------------|----------------------|
| `notifyOnTaskFail`      | 失败通知总开关（默认开）         |
| `missedJobPolicy`       | `skip` \| `run_once` |
| `minimizeToTrayOnClose` | 关窗进托盘                |

完整 IPC 见 `docs/ipc-contract.md`。

---

## 3. 日志 `steps` 约定

```ts
type LogStep = {
    title: string;   // 步骤名，如「定位企微窗口」
    status: string;  // 建议：ok | fail | skip（字符串，非枚举强约束）
    time?: string;   // 建议 HH:MM:SS
    note?: string;   // 细节、错误码、截断后的 stdout 等
};
```

约定：

1. **顺序**：按真实执行顺序追加；调度器在 `onActionFail=stop` 时可能追加「动作链中止」。
2. **status**：成功用 `ok`，失败用 `fail`，跳过用 `skip`（与现有企微/脚本一致）。
3. **截断**：stdout/stderr 等大文本在动作内截断（见 `actions.rs` 的 `OUTPUT_TRUNCATE_CHARS`），并在 `note` 标明「已截断」。
4. **通知反馈**：若发送过失败通知，可在汇总 `note` 或 step 中写「已通知」/「通知未送达」。
5. **不要**把敏感密钥完整写入 steps。

任务级日志：

- `detail`：一行摘要，前缀区分 `手动执行` / `调度执行` / `补跑`。
- `status`：全部动作成功 → `success`，否则 `failed`（跳过手动重入等可用 `skipped`）。

---

## 4. 错误与失败通知

### 4.1 错误表达（无独立错误码枚举）

当前以**中文可读字符串**为主，不强制数字错误码。建议在 `note` 中带稳定前缀，便于检索，例如：

| 场景      | 建议前缀 / 文案                |
|---------|--------------------------|
| 路径不存在   | `应用不存在:` / `脚本不存在:`      |
| 超时      | `执行超时`                   |
| 非法 URL  | `网址无效`                   |
| 企微窗口未找到 | 步骤「定位窗口」`fail` + 明确 note |
| Cron 非法 | 保存阶段拒绝：`Cron 表达式无效:`     |

新增动作时：保存校验放在 `store::validate_task`（若需）；运行时错误放在 `execute_action` 返回值。

### 4.2 通知优先级

```text
动作失败
  → 动作级 notifyOnFail == false？ → 不通知
  → settings.notifyOnTaskFail == false？ → 不通知
  → 系统通知权限 denied？ → 不通知，note 说明
  → 调用 notify::send_fail_notification
```

实现：`src-tauri/src/notify.rs`。

### 4.3 与托盘/设置的关系

- 设置页可申请通知权限（`request_notification_permission`）。
- 关窗进托盘、单实例聚焦主窗口不影响通知逻辑。

---

## 5. 内置动作一览

| `type`        | 说明                                                | 主要实现         |
|---------------|---------------------------------------------------|--------------|
| `open_app`    | 启动本地程序；`waitMode`：`fire_and_forget` / `wait_exit` | `actions.rs` |
| `open_url`    | 系统默认浏览器打开                                         | `opener` 插件  |
| `run_script`  | 运行 ps1/bat/cmd 等；捕获输出、工作目录、超时                     | `actions.rs` |
| `wecom_ui_dm` | 企业微信 UI 自动化私聊（Windows）                            | `wecom.rs`   |

---

## 6. 检查清单（合并前）

- [ ] Rust `Action` + `execute_action` +（如需）单元测试
- [ ] 前端类型与 TaskEditor
- [ ] `docs/ipc-contract.md` 已更新
- [ ] 失败路径有中文 `note`；需要通知的动作接好 `notifyOnFail`
- [ ] `cargo test` / `cargo check` / `npx tsc --noEmit` 通过
