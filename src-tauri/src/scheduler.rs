use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDateTime, Timelike, Weekday};
use croner::Cron;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::actions::{execute_action, outcome_to_steps};
use crate::models::{
    ExecutionLog, OnActionFail, RunStatus, SchedulerStatus, Task, Trigger,
};
use crate::settings::MissedJobPolicy;
use crate::store::AppState;

const TICK_SECS: u64 = 1;
const STATUS_REFRESH_SECS: u64 = 5;

/// once 任务执行后自动禁用，避免重复触发。
const DISABLE_ONCE_AFTER_RUN: bool = true;

/// 补跑 lookback 窗口（小时）：只考虑窗口内最近一次应触发点。
pub const MISSED_LOOKBACK_HOURS: i64 = 24;

pub struct SchedulerHandle {
    pub running: AtomicBool,
    /// 全局暂停：为 true 时 tick 不触发新任务（手动执行仍可用）。
    pub paused: AtomicBool,
    pub enabled_task_count: Mutex<usize>,
    pub last_tick_at: Mutex<Option<String>>,
    pub next_check_at: Mutex<Option<String>>,
    /// task_id -> 上次已触发的分钟槽（YYYY-MM-DD HH:MM），防止同分钟重复。
    fired_slots: Mutex<HashMap<String, String>>,
    /// 正在执行中的任务，避免重入。
    running_tasks: Mutex<HashSet<String>>,
    /// interval 任务上次成功触发时间（unix 秒）。
    interval_last: Mutex<HashMap<String, i64>>,
    /// 是否已完成启动补跑扫描。
    startup_catchup_done: AtomicBool,
}

impl SchedulerHandle {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            enabled_task_count: Mutex::new(0),
            last_tick_at: Mutex::new(None),
            next_check_at: Mutex::new(None),
            fired_slots: Mutex::new(HashMap::new()),
            running_tasks: Mutex::new(HashSet::new()),
            interval_last: Mutex::new(HashMap::new()),
            startup_catchup_done: AtomicBool::new(false),
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// 切换暂停态，返回切换后是否处于暂停。
    pub fn toggle_pause(&self) -> bool {
        let next = !self.is_paused();
        self.paused.store(next, Ordering::SeqCst);
        next
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    pub fn status(&self) -> SchedulerStatus {
        let enabled = *self
            .enabled_task_count
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let last = self
            .last_tick_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let next = self
            .next_check_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let running_count = self
            .running_tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len();
        let paused = self.is_paused();

        SchedulerStatus {
            running: self.running.load(Ordering::SeqCst) && !paused,
            enabled_task_count: enabled,
            running_task_count: running_count,
            last_tick_at: last,
            next_check_at: next,
            tick_interval_secs: TICK_SECS,
            paused,
        }
    }

    pub fn try_begin_task(&self, task_id: &str) -> bool {
        let mut set = self
            .running_tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if set.contains(task_id) {
            return false;
        }
        set.insert(task_id.to_string());
        true
    }

    pub fn end_task(&self, task_id: &str) {
        if let Ok(mut set) = self.running_tasks.lock() {
            set.remove(task_id);
        }
    }

    pub fn is_task_running(&self, task_id: &str) -> bool {
        self.running_tasks
            .lock()
            .ok()
            .map(|s| s.contains(task_id))
            .unwrap_or(false)
    }

    pub fn mark_fired(&self, task_id: &str, slot: &str) {
        if let Ok(mut map) = self.fired_slots.lock() {
            map.insert(task_id.to_string(), slot.to_string());
        }
    }

    pub fn already_fired(&self, task_id: &str, slot: &str) -> bool {
        self.fired_slots
            .lock()
            .ok()
            .and_then(|m| m.get(task_id).cloned())
            .map(|s| s == slot)
            .unwrap_or(false)
    }

    pub fn set_interval_last(&self, task_id: &str, ts: i64) {
        if let Ok(mut map) = self.interval_last.lock() {
            map.insert(task_id.to_string(), ts);
        }
    }

    pub fn get_interval_last(&self, task_id: &str) -> Option<i64> {
        self.interval_last
            .lock()
            .ok()
            .and_then(|m| m.get(task_id).copied())
    }

    pub fn interval_last_snapshot(&self) -> HashMap<String, i64> {
        self.interval_last
            .lock()
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    /// 任务变更后清理过期调度状态（删除的任务）。
    pub fn prune_to_tasks(&self, task_ids: &HashSet<String>) {
        if let Ok(mut map) = self.fired_slots.lock() {
            map.retain(|id, _| task_ids.contains(id));
        }
        if let Ok(mut map) = self.interval_last.lock() {
            map.retain(|id, _| task_ids.contains(id));
        }
    }
}

pub fn start(app: AppHandle) {
    let handle = Arc::new(SchedulerHandle::new());
    handle.running.store(true, Ordering::SeqCst);

    // 启动时恢复持久化的暂停态
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(settings) = state.get_settings() {
            handle.set_paused(settings.scheduler_paused);
        }
    }

    app.manage(handle.clone());

    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(TICK_SECS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut status_age: u64 = STATUS_REFRESH_SECS;

        loop {
            ticker.tick().await;
            status_age += TICK_SECS;
            if let Err(err) = tick_once(&app, &handle, status_age >= STATUS_REFRESH_SECS) {
                eprintln!("[scheduler] tick error: {err}");
            }
            if status_age >= STATUS_REFRESH_SECS {
                status_age = 0;
            }
        }
    });
}

fn tick_once(
    app: &AppHandle,
    handle: &Arc<SchedulerHandle>,
    refresh_status: bool,
) -> Result<(), String> {
    let now = Local::now();
    let now_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let next_str = (now + ChronoDuration::seconds(TICK_SECS as i64))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    if let Ok(mut g) = handle.last_tick_at.lock() {
        *g = Some(now_str);
    }
    if let Ok(mut g) = handle.next_check_at.lock() {
        *g = Some(next_str);
    }

    let state = app.state::<AppState>();
    let tasks = state.list_tasks()?;
    let enabled: Vec<Task> = tasks.into_iter().filter(|t| t.enabled).collect();

    if refresh_status {
        if let Ok(mut g) = handle.enabled_task_count.lock() {
            *g = enabled.len();
        }
        let ids: HashSet<String> = enabled.iter().map(|t| t.id.clone()).collect();
        handle.prune_to_tasks(&ids);
    }

    // 全局暂停时仍更新状态，但不触发调度 / 不补跑。
    if handle.is_paused() {
        return Ok(());
    }

    // 启动后首次非暂停 tick：按策略做一次补跑扫描。
    if !handle.startup_catchup_done.swap(true, Ordering::SeqCst) {
        maybe_catchup(app, handle, &enabled, now)?;
    }

    let minute_slot = now.format("%Y-%m-%d %H:%M").to_string();
    let now_ts = now.timestamp();

    for task in enabled {
        if should_fire(handle, &task, now, &minute_slot, now_ts) {
            spawn_run(app.clone(), handle.clone(), task, RunKind::Scheduled);
        }
    }

    Ok(())
}

/// 从暂停恢复时由 IPC / 托盘调用：按策略补跑一次。
pub fn on_scheduler_resumed(app: &AppHandle, handle: &Arc<SchedulerHandle>) {
    if handle.is_paused() {
        return;
    }
    // 避免「启动时处于暂停 → 恢复补跑 → 随后首个 tick 再补跑」重复。
    handle
        .startup_catchup_done
        .store(true, Ordering::SeqCst);

    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(tasks) = state.list_tasks() else {
        return;
    };
    let enabled: Vec<Task> = tasks.into_iter().filter(|t| t.enabled).collect();
    let now = Local::now();
    if let Err(e) = maybe_catchup(app, handle, &enabled, now) {
        eprintln!("[scheduler] catchup on resume error: {e}");
    }
}

fn maybe_catchup(
    app: &AppHandle,
    handle: &Arc<SchedulerHandle>,
    enabled: &[Task],
    now: chrono::DateTime<Local>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state.get_settings()?;
    if settings.missed_job_policy != MissedJobPolicy::RunOnce {
        return Ok(());
    }

    let lookback_start = now - ChronoDuration::hours(MISSED_LOOKBACK_HOURS);
    // 单次扫描内每个任务最多补一次，避免风暴。
    let mut queued: HashSet<String> = HashSet::new();

    for task in enabled {
        if queued.contains(&task.id) {
            continue;
        }
        let Some(missed_at) = find_missed_trigger(handle, task, now, lookback_start) else {
            continue;
        };

        // 若最近一次执行已覆盖该触发点，则不补。
        if let Some(last) = task.last_run_at.as_deref().and_then(parse_datetime) {
            if last >= missed_at {
                continue;
            }
        }

        queued.insert(task.id.clone());
        let slot = missed_at.format("%Y-%m-%d %H:%M").to_string();
        handle.mark_fired(&task.id, &slot);
        if matches!(task.trigger, Trigger::Interval { .. }) {
            handle.set_interval_last(&task.id, now.timestamp());
        }
        spawn_run(app.clone(), handle.clone(), task.clone(), RunKind::Catchup);
    }

    Ok(())
}

/// 在 lookback 内找最近一次「本应触发」的时刻；若该时刻已过且未执行则返回。
fn find_missed_trigger(
    handle: &SchedulerHandle,
    task: &Task,
    now: chrono::DateTime<Local>,
    lookback_start: chrono::DateTime<Local>,
) -> Option<NaiveDateTime> {
    let now_naive = now.naive_local();
    let lookback_naive = lookback_start.naive_local();

    match &task.trigger {
        Trigger::Once { datetime } => {
            let target = parse_datetime(datetime)?;
            // once：目标时间已过、落在 lookback 内、且尚未执行过（enabled 仍为 true 说明未跑完）
            if target < now_naive && target >= lookback_naive {
                Some(target)
            } else {
                None
            }
        }
        Trigger::Daily { time, weekdays } => {
            let (h, m) = parse_hhmm(time)?;
            // 从今天往回最多 8 天，找最近一个符合 weekday+time 且已过的点
            for day_offset in 0..8 {
                let day = now.date_naive() - ChronoDuration::days(day_offset);
                let wd = weekday_num(day.weekday());
                let day_ok = weekdays.is_empty() || weekdays.contains(&wd);
                if !day_ok {
                    continue;
                }
                let Some(candidate) = day.and_hms_opt(h, m, 0) else {
                    continue;
                };
                if candidate < now_naive && candidate >= lookback_naive {
                    let slot = candidate.format("%Y-%m-%d %H:%M").to_string();
                    if handle.already_fired(&task.id, &slot) {
                        return None;
                    }
                    return Some(candidate);
                }
            }
            None
        }
        Trigger::Cron { expr, .. } => {
            let cron = Cron::from_str(expr.trim()).ok()?;
            let prev = cron.find_previous_occurrence(&now, false).ok()?;
            let prev_naive = prev.naive_local();
            if prev_naive < now_naive && prev_naive >= lookback_naive {
                let slot = prev.format("%Y-%m-%d %H:%M").to_string();
                if handle.already_fired(&task.id, &slot) {
                    return None;
                }
                Some(prev_naive)
            } else {
                None
            }
        }
        Trigger::Interval { every, unit } => {
            let secs = interval_to_secs(*every, unit) as i64;
            if secs <= 0 {
                return None;
            }
            match handle.get_interval_last(&task.id) {
                // 尚无起点：启动补跑不触发 interval（与正常「先记起点」一致）
                None => None,
                Some(last) => {
                    let elapsed = now.timestamp() - last;
                    if elapsed >= secs {
                        let due_at = chrono::DateTime::from_timestamp(last + secs, 0)?
                            .with_timezone(&Local)
                            .naive_local();
                        if due_at >= lookback_naive {
                            Some(due_at)
                        } else {
                            // 过期太久也只补一次：用 lookback 边界附近的应触发点
                            Some(lookback_naive)
                        }
                    } else {
                        None
                    }
                }
            }
        }
    }
}

fn should_fire(
    handle: &SchedulerHandle,
    task: &Task,
    now: chrono::DateTime<Local>,
    minute_slot: &str,
    now_ts: i64,
) -> bool {
    match &task.trigger {
        Trigger::Once { datetime } => {
            if handle.already_fired(&task.id, minute_slot) {
                return false;
            }
            match parse_datetime(datetime) {
                Some(target) => {
                    let due = now.naive_local() >= target
                        && now.naive_local() < target + ChronoDuration::minutes(2);
                    if due {
                        handle.mark_fired(&task.id, minute_slot);
                    }
                    due
                }
                None => false,
            }
        }
        Trigger::Daily { time, weekdays } => {
            if handle.already_fired(&task.id, minute_slot) {
                return false;
            }
            let Some((h, m)) = parse_hhmm(time) else {
                return false;
            };
            let wd = weekday_num(now.weekday());
            let day_ok = weekdays.is_empty() || weekdays.contains(&wd);
            let time_ok = now.hour() == h && now.minute() == m;
            if day_ok && time_ok {
                handle.mark_fired(&task.id, minute_slot);
                true
            } else {
                false
            }
        }
        Trigger::Cron { expr, .. } => {
            if handle.already_fired(&task.id, minute_slot) {
                return false;
            }
            match Cron::from_str(expr.trim()) {
                Ok(cron) => match cron.is_time_matching(&now) {
                    Ok(true) => {
                        handle.mark_fired(&task.id, minute_slot);
                        true
                    }
                    _ => false,
                },
                Err(_) => false,
            }
        }
        Trigger::Interval { every, unit } => {
            let secs = interval_to_secs(*every, unit);
            if secs == 0 {
                return false;
            }
            match handle.get_interval_last(&task.id) {
                None => {
                    // 启用后先记录起点，下一周期再跑，避免刚启用立刻连发。
                    handle.set_interval_last(&task.id, now_ts);
                    false
                }
                Some(last) if now_ts - last >= secs as i64 => {
                    handle.set_interval_last(&task.id, now_ts);
                    true
                }
                _ => false,
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RunKind {
    Scheduled,
    Manual,
    Catchup,
}

pub fn spawn_run(
    app: AppHandle,
    handle: Arc<SchedulerHandle>,
    task: Task,
    kind: RunKind,
) {
    spawn_run_with_options(app, handle, task, kind, false);
}

/// `force`：含 wecom 时跳过空闲队列直接执行。
pub fn spawn_run_with_options(
    app: AppHandle,
    handle: Arc<SchedulerHandle>,
    task: Task,
    kind: RunKind,
    force: bool,
) {
    let task_id = task.id.clone();
    let manual = matches!(kind, RunKind::Manual);

    // 含 wecom 且开启空闲发送：整任务入队，不立刻抢前台。
    if crate::idle_send::should_queue(&app, &task, force) {
        if let Some(idle) = app.try_state::<std::sync::Arc<crate::idle_send::IdleSendHandle>>() {
            if idle.is_task_queued(&task_id) {
                if manual {
                    let _ = append_skip_log(&app, &task, "任务已在企微空闲发送队列中，已跳过");
                }
                return;
            }
            match idle.enqueue(&app, task.clone(), kind) {
                Ok(_) => {
                    // once 调度入队即视为已触发；执行完成后再由队列侧跑完后禁用
                    if DISABLE_ONCE_AFTER_RUN
                        && !manual
                        && matches!(task.trigger, Trigger::Once { .. })
                    {
                        // 延迟到实际执行完成再禁用，避免排队期间被关掉后用户困惑；
                        // 此处仅标记 fired，禁用仍在真正 run 结束后处理（见下方 spawn）。
                    }
                    return;
                }
                Err(e) => {
                    if manual {
                        let _ = append_skip_log(&app, &task, &e);
                        return;
                    }
                    eprintln!("[scheduler] idle enqueue failed: {e}; fallback to immediate");
                }
            }
        }
    }

    if !handle.try_begin_task(&task_id) {
        if manual {
            let _ = append_skip_log(
                &app,
                &task,
                "任务正在执行中，已跳过本次手动触发",
            );
        }
        return;
    }

    tauri::async_runtime::spawn(async move {
        let result = tauri::async_runtime::spawn_blocking({
            let app = app.clone();
            let task = task.clone();
            move || run_task_blocking(&app, &task, kind)
        })
        .await;

        if let Err(e) = result {
            eprintln!("[scheduler] join error for {}: {e}", task.id);
            let _ = append_fail_log(&app, &task, &format!("执行线程异常: {e}"));
        }

        handle.end_task(&task_id);

        if DISABLE_ONCE_AFTER_RUN
            && !manual
            && matches!(task.trigger, Trigger::Once { .. })
        {
            let state = app.state::<AppState>();
            let _ = state.toggle_task(&task.id, false);
        }
    });
}

fn run_task_blocking(app: &AppHandle, task: &Task, kind: RunKind) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut steps = Vec::new();
    let mut all_ok = true;
    let mut details = Vec::new();

    if task.actions.is_empty() {
        let log = ExecutionLog {
            id: Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            time: now_string(),
            status: RunStatus::Failed,
            detail: "任务没有可执行动作".into(),
            steps: vec![],
        };
        state.append_log(log)?;
        return Ok(());
    }

    let action_total = task.actions.len();
    for (idx, action) in task.actions.iter().enumerate() {
        let outcome = execute_action(app, action);
        let failed = !matches!(outcome.status, RunStatus::Success);
        if failed {
            all_ok = false;
        }
        details.push(format!("{}: {}", outcome.title, outcome.note));
        steps.extend(outcome_to_steps(&outcome));

        if failed && matches!(task.on_action_fail, OnActionFail::Stop) {
            let remaining = action_total.saturating_sub(idx + 1);
            if remaining > 0 {
                steps.push(crate::models::LogStep {
                    title: "动作链中止".into(),
                    status: "skip".into(),
                    time: now_string_time(),
                    note: format!(
                        "onActionFail=stop · 跳过后续 {remaining} 个动作"
                    ),
                });
                details.push(format!("已中止，跳过后续 {remaining} 个动作"));
            }
            break;
        }
        if failed && matches!(task.on_action_fail, OnActionFail::Continue) {
            steps.push(crate::models::LogStep {
                title: "动作链继续".into(),
                status: "ok".into(),
                time: now_string_time(),
                note: "onActionFail=continue · 继续执行后续动作".into(),
            });
        }
    }

    let status = if all_ok {
        RunStatus::Success
    } else {
        RunStatus::Failed
    };
    let prefix = match kind {
        RunKind::Manual => "手动执行",
        RunKind::Scheduled => "调度执行",
        RunKind::Catchup => "补跑",
    };
    let detail = format!("{prefix} · {}", details.join("；"));

    let log = ExecutionLog {
        id: Uuid::new_v4().to_string(),
        task_id: task.id.clone(),
        task_name: task.name.clone(),
        time: now_string(),
        status,
        detail,
        steps,
    };
    state.append_log(log)?;
    Ok(())
}

fn append_skip_log(app: &AppHandle, task: &Task, detail: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.append_log(ExecutionLog {
        id: Uuid::new_v4().to_string(),
        task_id: task.id.clone(),
        task_name: task.name.clone(),
        time: now_string(),
        status: RunStatus::Skipped,
        detail: detail.to_string(),
        steps: vec![],
    })?;
    Ok(())
}

fn append_fail_log(app: &AppHandle, task: &Task, detail: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.append_log(ExecutionLog {
        id: Uuid::new_v4().to_string(),
        task_id: task.id.clone(),
        task_name: task.name.clone(),
        time: now_string(),
        status: RunStatus::Failed,
        detail: detail.to_string(),
        steps: vec![],
    })?;
    Ok(())
}

fn now_string() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn now_string_time() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

pub(crate) fn weekday_num(wd: Weekday) -> u8 {
    match wd {
        Weekday::Sun => 0,
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
    }
}

pub(crate) fn parse_hhmm(time: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = time.trim().split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some((h, m))
}

pub(crate) fn parse_datetime(raw: &str) -> Option<NaiveDateTime> {
    let s = raw.trim().replace('T', " ");
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
    ];
    for fmt in formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&s, fmt) {
            return Some(dt);
        }
    }
    // datetime-local 可能只有到分钟
    if s.len() >= 16 {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&s[..16], "%Y-%m-%d %H:%M") {
            return Some(dt);
        }
    }
    None
}

pub(crate) fn interval_to_secs(every: u32, unit: &str) -> u64 {
    if every == 0 {
        return 0;
    }
    let u = unit.trim().to_ascii_lowercase();
    let factor: u64 = match u.as_str() {
        "秒" | "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "分钟" | "分" | "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "小时" | "时" | "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
        _ => 60, // 默认按分钟
    };
    (every as u64).saturating_mul(factor)
}

/// 计算任务下次执行时间（本地时区字符串）。
/// 禁用 / once 已过期 → None（前端显示「不再执行」）。
/// 暂停时仍返回「若恢复后」的下次时间。
pub fn compute_next_run_at(
    task: &Task,
    now: chrono::DateTime<Local>,
    interval_last: &HashMap<String, i64>,
) -> Option<String> {
    if !task.enabled {
        return None;
    }
    let fmt = |dt: chrono::DateTime<Local>| dt.format("%Y-%m-%d %H:%M:%S").to_string();
    let fmt_naive = |n: NaiveDateTime| n.format("%Y-%m-%d %H:%M:%S").to_string();

    match &task.trigger {
        Trigger::Once { datetime } => {
            let target = parse_datetime(datetime)?;
            if target > now.naive_local() {
                Some(fmt_naive(target))
            } else {
                None
            }
        }
        Trigger::Daily { time, weekdays } => {
            let (h, m) = parse_hhmm(time)?;
            for day_offset in 0..8 {
                let day = now.date_naive() + ChronoDuration::days(day_offset);
                let wd = weekday_num(day.weekday());
                if !(weekdays.is_empty() || weekdays.contains(&wd)) {
                    continue;
                }
                let Some(candidate) = day.and_hms_opt(h, m, 0) else {
                    continue;
                };
                if candidate > now.naive_local() {
                    return Some(fmt_naive(candidate));
                }
            }
            None
        }
        Trigger::Cron { expr, .. } => {
            let cron = Cron::from_str(expr.trim()).ok()?;
            let next = cron.find_next_occurrence(&now, false).ok()?;
            Some(fmt(next))
        }
        Trigger::Interval { every, unit } => {
            let secs = interval_to_secs(*every, unit) as i64;
            if secs <= 0 {
                return None;
            }
            match interval_last.get(&task.id).copied() {
                Some(last) => {
                    let next_ts = last + secs;
                    let next = chrono::DateTime::from_timestamp(next_ts, 0)?
                        .with_timezone(&Local);
                    if next > now {
                        Some(fmt(next))
                    } else {
                        Some(fmt(now + ChronoDuration::seconds(1)))
                    }
                }
                // 尚无起点：与调度一致，启用后先记起点，下一周期才跑
                None => Some(fmt(now + ChronoDuration::seconds(secs))),
            }
        }
    }
}

pub async fn run_task_now(
    app: AppHandle,
    id: String,
    force: bool,
) -> Result<ExecutionLog, String> {
    let state = app.state::<AppState>();
    let task = state
        .get_task(&id)?
        .ok_or_else(|| format!("任务不存在: {id}"))?;

    // 默认：含 wecom 且空闲发送开启 → 入队，返回占位 skipped 日志
    if crate::idle_send::should_queue(&app, &task, force) {
        let idle = app
            .try_state::<Arc<crate::idle_send::IdleSendHandle>>()
            .ok_or_else(|| "空闲发送队列未启动".to_string())?;
        if idle.is_task_queued(&task.id) {
            return Err("该任务已在企微空闲发送队列中".into());
        }
        let item = idle.enqueue(&app, task.clone(), RunKind::Manual)?;
        let log = ExecutionLog {
            id: Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            time: now_string(),
            status: RunStatus::Skipped,
            detail: format!(
                "手动执行 · 已加入企微空闲发送队列（pendingId={}，空闲后发送）",
                item.id
            ),
            steps: vec![crate::models::LogStep {
                title: "空闲排队".into(),
                status: "skip".into(),
                time: now_string_time(),
                note: "含 wecom_ui_dm · 等待键鼠空闲后倒计时发送；可用 force 强制立即".into(),
            }],
        };
        state.append_log(log.clone())?;
        return Ok(log);
    }

    // 强制立即：若仍在队列中则先取消对应项
    if force {
        if let Some(idle) = app.try_state::<Arc<crate::idle_send::IdleSendHandle>>() {
            let pending = idle.list();
            for item in pending {
                if item.task_id == task.id {
                    let _ = idle.cancel(&app, Some(&item.id));
                }
            }
        }
    }

    let handle = app.state::<Arc<SchedulerHandle>>();
    if !handle.try_begin_task(&task.id) {
        return Err("任务正在执行中，请稍后再试".into());
    }

    let task_clone = task.clone();
    let app_clone = app.clone();
    let join = tauri::async_runtime::spawn_blocking(move || {
        run_task_blocking(&app_clone, &task_clone, RunKind::Manual)
    })
    .await;

    handle.end_task(&task.id);

    match join {
        Ok(Ok(())) => {
            let logs = state.list_logs(Some(20))?;
            logs.into_iter()
                .find(|l| l.task_id == id)
                .ok_or_else(|| "执行完成但未找到日志".to_string())
        }
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("执行线程异常: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Action, OnActionFail, Task, Trigger};
    use chrono::TimeZone;
    use std::collections::HashMap;

    fn sample_open_url() -> Action {
        Action::OpenUrl {
            id: "a1".into(),
            url: "https://example.com".into(),
        }
    }

    fn task_with(trigger: Trigger) -> Task {
        Task {
            id: "t1".into(),
            name: "测试任务".into(),
            enabled: true,
            trigger,
            actions: vec![sample_open_url()],
            on_action_fail: OnActionFail::Stop,
            last_run_at: None,
            last_status: None,
            updated_at: None,
            next_run_at: None,
        }
    }

    #[test]
    fn parse_hhmm_valid_and_invalid() {
        assert_eq!(parse_hhmm("09:05"), Some((9, 5)));
        assert_eq!(parse_hhmm("23:59"), Some((23, 59)));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("9"), None);
        assert_eq!(parse_hhmm(""), None);
    }

    #[test]
    fn parse_datetime_formats() {
        let a = parse_datetime("2026-09-17 09:05:00").unwrap();
        assert_eq!(a.format("%H:%M").to_string(), "09:05");
        let b = parse_datetime("2026-09-17T09:05").unwrap();
        assert_eq!(b.format("%Y-%m-%d %H:%M").to_string(), "2026-09-17 09:05");
        assert!(parse_datetime("not-a-date").is_none());
    }

    #[test]
    fn interval_to_secs_units() {
        assert_eq!(interval_to_secs(30, "秒"), 30);
        assert_eq!(interval_to_secs(2, "分钟"), 120);
        assert_eq!(interval_to_secs(1, "小时"), 3600);
        assert_eq!(interval_to_secs(3, "min"), 180);
        assert_eq!(interval_to_secs(0, "分钟"), 0);
        assert_eq!(interval_to_secs(5, "未知单位"), 300); // 默认分钟
    }

    #[test]
    fn compute_next_run_once_future_and_past() {
        let now = Local.with_ymd_and_hms(2026, 9, 17, 10, 0, 0).unwrap();
        let future = task_with(Trigger::Once {
            datetime: "2026-09-17 11:00:00".into(),
        });
        assert_eq!(
            compute_next_run_at(&future, now, &HashMap::new()).as_deref(),
            Some("2026-09-17 11:00:00")
        );
        let past = task_with(Trigger::Once {
            datetime: "2026-09-17 09:00:00".into(),
        });
        assert!(compute_next_run_at(&past, now, &HashMap::new()).is_none());
    }

    #[test]
    fn compute_next_run_disabled_is_none() {
        let now = Local.with_ymd_and_hms(2026, 9, 17, 10, 0, 0).unwrap();
        let mut t = task_with(Trigger::Daily {
            time: "12:00".into(),
            weekdays: vec![1, 2, 3, 4, 5],
        });
        t.enabled = false;
        assert!(compute_next_run_at(&t, now, &HashMap::new()).is_none());
    }

    #[test]
    fn compute_next_run_daily_skips_to_weekday() {
        // 2026-09-17 是周四(4)；设仅周一(1) 09:00
        let now = Local.with_ymd_and_hms(2026, 9, 17, 10, 0, 0).unwrap();
        let t = task_with(Trigger::Daily {
            time: "09:00".into(),
            weekdays: vec![1],
        });
        let next = compute_next_run_at(&t, now, &HashMap::new()).unwrap();
        assert_eq!(next, "2026-09-21 09:00:00"); // 下周一
    }

    #[test]
    fn compute_next_run_cron() {
        let now = Local.with_ymd_and_hms(2026, 9, 17, 10, 0, 0).unwrap();
        let t = task_with(Trigger::Cron {
            expr: "0 11 * * *".into(),
            note: String::new(),
        });
        let next = compute_next_run_at(&t, now, &HashMap::new()).unwrap();
        assert_eq!(next, "2026-09-17 11:00:00");
    }

    #[test]
    fn compute_next_run_interval_with_and_without_last() {
        let now = Local.with_ymd_and_hms(2026, 9, 17, 10, 0, 0).unwrap();
        let t = task_with(Trigger::Interval {
            every: 10,
            unit: "分钟".into(),
        });
        // 无起点：下一周期 = now + 10min
        let next = compute_next_run_at(&t, now, &HashMap::new()).unwrap();
        assert_eq!(next, "2026-09-17 10:10:00");

        let mut last = HashMap::new();
        last.insert("t1".into(), now.timestamp() - 60);
        let next2 = compute_next_run_at(&t, now, &last).unwrap();
        assert_eq!(next2, "2026-09-17 10:09:00");
    }

    #[test]
    fn find_missed_once_in_lookback() {
        let handle = SchedulerHandle::new();
        let now = Local.with_ymd_and_hms(2026, 9, 17, 12, 0, 0).unwrap();
        let lookback = now - ChronoDuration::hours(MISSED_LOOKBACK_HOURS);
        let t = task_with(Trigger::Once {
            datetime: "2026-09-17 11:30:00".into(),
        });
        let missed = find_missed_trigger(&handle, &t, now, lookback).unwrap();
        assert_eq!(missed.format("%H:%M").to_string(), "11:30");
    }

    #[test]
    fn find_missed_once_outside_lookback_is_none() {
        let handle = SchedulerHandle::new();
        let now = Local.with_ymd_and_hms(2026, 9, 17, 12, 0, 0).unwrap();
        let lookback = now - ChronoDuration::hours(MISSED_LOOKBACK_HOURS);
        let t = task_with(Trigger::Once {
            datetime: "2026-09-01 11:30:00".into(),
        });
        assert!(find_missed_trigger(&handle, &t, now, lookback).is_none());
    }

    #[test]
    fn find_missed_daily_respects_already_fired() {
        let handle = SchedulerHandle::new();
        let now = Local.with_ymd_and_hms(2026, 9, 17, 12, 0, 0).unwrap();
        let lookback = now - ChronoDuration::hours(MISSED_LOOKBACK_HOURS);
        let t = task_with(Trigger::Daily {
            time: "09:00".into(),
            weekdays: vec![],
        });
        let slot = "2026-09-17 09:00";
        handle.mark_fired(&t.id, slot);
        assert!(find_missed_trigger(&handle, &t, now, lookback).is_none());
    }

    #[test]
    fn weekday_num_sunday_is_zero() {
        assert_eq!(weekday_num(Weekday::Sun), 0);
        assert_eq!(weekday_num(Weekday::Mon), 1);
        assert_eq!(weekday_num(Weekday::Sat), 6);
    }
}
