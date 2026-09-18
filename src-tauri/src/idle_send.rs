//! 企微空闲发送队列：含 `wecom_ui_dm` 的整任务入队，空闲达标后倒计时再执行。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Local;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::models::{Action, Task};
use crate::notify;
use crate::scheduler::{self, RunKind, SchedulerHandle};
use crate::settings::WecomIdleSendSettings;
use crate::store::AppState;
use crate::tray;

const IDLE_POLL_MS: u64 = 500;
pub const EVENT_IDLE_SEND_CHANGED: &str = "idle-send-changed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingWecomItem {
    pub id: String,
    pub task_id: String,
    pub task_name: String,
    /// schedule | manual | catchup
    pub source: String,
    pub enqueued_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdleSendPhase {
    Idle,
    WaitingIdle,
    Countdown,
    Running,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdleSendStatus {
    pub enabled: bool,
    pub phase: IdleSendPhase,
    pub pending_count: usize,
    pub idle_seconds_config: u32,
    pub countdown_seconds_config: u32,
    pub current_idle_seconds: u64,
    pub countdown_remaining_secs: u32,
    pub active_pending_id: Option<String>,
    pub active_task_name: Option<String>,
    pub scheduler_paused: bool,
}

#[derive(Debug, Clone)]
struct QueueEntry {
    id: String,
    task: Task,
    source: String,
    kind: RunKind,
    enqueued_at: String,
}

struct CountdownState {
    pending_id: String,
    started_at: std::time::Instant,
    input_baseline: u32,
    total_secs: u32,
}

struct IdleSendInner {
    queue: VecDeque<QueueEntry>,
    phase: IdleSendPhase,
    countdown: Option<CountdownState>,
    running_id: Option<String>,
    running_task_id: Option<String>,
    running_task_name: Option<String>,
}

pub struct IdleSendHandle {
    inner: Mutex<IdleSendInner>,
    started: AtomicBool,
}

impl IdleSendHandle {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(IdleSendInner {
                queue: VecDeque::new(),
                phase: IdleSendPhase::Idle,
                countdown: None,
                running_id: None,
                running_task_id: None,
                running_task_name: None,
            }),
            started: AtomicBool::new(false),
        }
    }

    pub fn is_task_queued(&self, task_id: &str) -> bool {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.queue.iter().any(|e| e.task.id == task_id)
            || g.running_task_id.as_deref() == Some(task_id)
    }

    pub fn enqueue(
        &self,
        app: &AppHandle,
        task: Task,
        kind: RunKind,
    ) -> Result<PendingWecomItem, String> {
        if self.is_task_queued(&task.id) {
            return Err("该任务已在企微空闲发送队列中".into());
        }

        let source = match kind {
            RunKind::Scheduled => "schedule",
            RunKind::Manual => "manual",
            RunKind::Catchup => "catchup",
        };
        let item = PendingWecomItem {
            id: Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            source: source.into(),
            enqueued_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };

        {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            g.queue.push_back(QueueEntry {
                id: item.id.clone(),
                task,
                source: item.source.clone(),
                kind,
                enqueued_at: item.enqueued_at.clone(),
            });
            if matches!(g.phase, IdleSendPhase::Idle) {
                g.phase = IdleSendPhase::WaitingIdle;
            }
        }

        let _ = notify::send_info_notification(
            app,
            "企微任务已排队",
            &format!(
                "「{}」将在空闲后发送（来源：{}）",
                item.task_name, item.source
            ),
        );
        emit_changed(app);
        let _ = tray::refresh_tray_tooltip(app);
        Ok(item)
    }

    pub fn list(&self) -> Vec<PendingWecomItem> {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.queue
            .iter()
            .map(|e| PendingWecomItem {
                id: e.id.clone(),
                task_id: e.task.id.clone(),
                task_name: e.task.name.clone(),
                source: e.source.clone(),
                enqueued_at: e.enqueued_at.clone(),
            })
            .collect()
    }

    /// 取消指定 id；`id` 为空则清空全部待发（不影响正在执行中的项）。
    pub fn cancel(&self, app: &AppHandle, id: Option<&str>) -> usize {
        let cancelled = {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            let n = match id {
                Some(target) if !target.trim().is_empty() => {
                    let before = g.queue.len();
                    g.queue.retain(|e| e.id != target);
                    let removed = before.saturating_sub(g.queue.len());
                    if g.countdown.as_ref().is_some_and(|c| c.pending_id == target) {
                        g.countdown = None;
                        g.phase = if g.running_id.is_some() {
                            IdleSendPhase::Running
                        } else if g.queue.is_empty() {
                            IdleSendPhase::Idle
                        } else {
                            IdleSendPhase::WaitingIdle
                        };
                    }
                    removed
                }
                _ => {
                    let n = g.queue.len();
                    g.queue.clear();
                    g.countdown = None;
                    g.phase = if g.running_id.is_some() {
                        IdleSendPhase::Running
                    } else {
                        IdleSendPhase::Idle
                    };
                    n
                }
            };
            n
        };
        if cancelled > 0 {
            emit_changed(app);
            let _ = tray::refresh_tray_tooltip(app);
        }
        cancelled
    }

    pub fn status(&self, app: &AppHandle) -> IdleSendStatus {
        let settings = app
            .try_state::<AppState>()
            .and_then(|s| s.get_settings().ok())
            .map(|s| s.wecom_idle_send)
            .unwrap_or_default();
        let paused = app
            .try_state::<Arc<SchedulerHandle>>()
            .map(|h| h.is_paused())
            .unwrap_or(false);
        let idle_now = system_idle_seconds();

        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (countdown_remaining, active_id, active_name) = match (&g.phase, &g.countdown) {
            (IdleSendPhase::Countdown, Some(c)) => {
                let elapsed = c.started_at.elapsed().as_secs() as u32;
                let remain = c.total_secs.saturating_sub(elapsed);
                let name = g
                    .queue
                    .front()
                    .filter(|e| e.id == c.pending_id)
                    .map(|e| e.task.name.clone());
                (remain, Some(c.pending_id.clone()), name)
            }
            (IdleSendPhase::Running, _) => (
                0,
                g.running_id.clone(),
                g.running_task_name.clone(),
            ),
            _ => (0, None, g.queue.front().map(|e| e.task.name.clone())),
        };

        IdleSendStatus {
            enabled: settings.enabled,
            phase: g.phase.clone(),
            pending_count: g.queue.len(),
            idle_seconds_config: settings.idle_seconds,
            countdown_seconds_config: settings.countdown_seconds,
            current_idle_seconds: idle_now,
            countdown_remaining_secs: countdown_remaining,
            active_pending_id: active_id,
            active_task_name: active_name,
            scheduler_paused: paused,
        }
    }

    pub fn pending_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .queue
            .len()
    }
}

pub fn task_has_wecom(task: &Task) -> bool {
    task.actions
        .iter()
        .any(|a| matches!(a, Action::WecomUiDm { .. }))
}

pub fn should_queue(app: &AppHandle, task: &Task, force: bool) -> bool {
    if force || !task_has_wecom(task) {
        return false;
    }
    app.try_state::<AppState>()
        .and_then(|s| s.get_settings().ok())
        .map(|s| s.wecom_idle_send.enabled)
        .unwrap_or(true)
}

pub fn start(app: AppHandle) {
    let handle = Arc::new(IdleSendHandle::new());
    if handle.started.swap(true, Ordering::SeqCst) {
        return;
    }
    app.manage(handle.clone());

    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(IDLE_POLL_MS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(e) = tick_idle_queue(&app, &handle) {
                eprintln!("[idle_send] tick error: {e}");
            }
        }
    });
}

fn tick_idle_queue(app: &AppHandle, handle: &Arc<IdleSendHandle>) -> Result<(), String> {
    let settings = app
        .try_state::<AppState>()
        .and_then(|s| s.get_settings().ok())
        .map(|s| s.wecom_idle_send)
        .unwrap_or_default();

    let paused = app
        .try_state::<Arc<SchedulerHandle>>()
        .map(|h| h.is_paused())
        .unwrap_or(false);

    if !settings.enabled || paused {
        abort_countdown_if_any(app, handle);
        return Ok(());
    }

    let idle_now = system_idle_seconds();
    let input_tick = last_input_tick();

    enum TickAction {
        None,
        StartedCountdown { task_name: String, secs: u32 },
        AbortedCountdown,
        Fire {
            entry: QueueEntry,
        },
        EmitProgress,
    }

    let action = {
        let mut g = handle.inner.lock().unwrap_or_else(|e| e.into_inner());

        if g.running_id.is_some() {
            return Ok(());
        }

        if g.queue.is_empty() {
            if !matches!(g.phase, IdleSendPhase::Idle) || g.countdown.is_some() {
                g.phase = IdleSendPhase::Idle;
                g.countdown = None;
                drop(g);
                emit_changed(app);
                let _ = tray::refresh_tray_tooltip(app);
            }
            return Ok(());
        }

        match g.phase.clone() {
            IdleSendPhase::Idle | IdleSendPhase::WaitingIdle => {
                g.phase = IdleSendPhase::WaitingIdle;
                g.countdown = None;
                if idle_now >= settings.idle_seconds as u64 {
                    let front = g.queue.front().expect("queue non-empty");
                    let secs = settings.countdown_seconds.max(1);
                    let task_name = front.task.name.clone();
                    g.countdown = Some(CountdownState {
                        pending_id: front.id.clone(),
                        started_at: std::time::Instant::now(),
                        input_baseline: input_tick,
                        total_secs: secs,
                    });
                    g.phase = IdleSendPhase::Countdown;
                    TickAction::StartedCountdown { task_name, secs }
                } else {
                    TickAction::None
                }
            }
            IdleSendPhase::Countdown => {
                let Some(c) = g.countdown.as_ref() else {
                    g.phase = IdleSendPhase::WaitingIdle;
                    return Ok(());
                };
                if input_tick != c.input_baseline {
                    g.countdown = None;
                    g.phase = IdleSendPhase::WaitingIdle;
                    TickAction::AbortedCountdown
                } else {
                    let elapsed = c.started_at.elapsed().as_secs() as u32;
                    if elapsed >= c.total_secs {
                        let entry = g
                            .queue
                            .pop_front()
                            .ok_or_else(|| "队列为空".to_string())?;
                        g.countdown = None;
                        g.running_id = Some(entry.id.clone());
                        g.running_task_id = Some(entry.task.id.clone());
                        g.running_task_name = Some(entry.task.name.clone());
                        g.phase = IdleSendPhase::Running;
                        TickAction::Fire { entry }
                    } else {
                        TickAction::EmitProgress
                    }
                }
            }
            IdleSendPhase::Running => TickAction::None,
        }
    };

    match action {
        TickAction::None => Ok(()),
        TickAction::EmitProgress => {
            emit_changed(app);
            Ok(())
        }
        TickAction::StartedCountdown { task_name, secs } => {
            let _ = notify::send_info_notification(
                app,
                "即将发送企微消息",
                &format!("「{task_name}」将在 {secs} 秒后发送，移动鼠标可取消本次尝试"),
            );
            emit_changed(app);
            let _ = tray::refresh_tray_tooltip(app);
            Ok(())
        }
        TickAction::AbortedCountdown => {
            emit_changed(app);
            let _ = tray::refresh_tray_tooltip(app);
            Ok(())
        }
        TickAction::Fire { entry } => {
            emit_changed(app);
            let _ = tray::refresh_tray_tooltip(app);

            let idle_handle = handle.clone();
            let app2 = app.clone();
            let task_id = entry.task.id.clone();
            let pending_id = entry.id.clone();
            let kind = entry.kind;
            let task = entry.task;

            if let Some(sched) = app.try_state::<Arc<SchedulerHandle>>() {
                // force=true：已从空闲队列弹出，避免再次入队
                scheduler::spawn_run_with_options(
                    app.clone(),
                    (*sched).clone(),
                    task,
                    kind,
                    true,
                );
            }

            tauri::async_runtime::spawn(async move {
                wait_until_task_done(&app2, &task_id).await;
                {
                    let mut g = idle_handle.inner.lock().unwrap_or_else(|e| e.into_inner());
                    if g.running_id.as_deref() == Some(pending_id.as_str()) {
                        g.running_id = None;
                        g.running_task_id = None;
                        g.running_task_name = None;
                    }
                    g.phase = if g.queue.is_empty() {
                        IdleSendPhase::Idle
                    } else {
                        IdleSendPhase::WaitingIdle
                    };
                }
                emit_changed(&app2);
                let _ = tray::refresh_tray_tooltip(&app2);
            });
            Ok(())
        }
    }
}

fn abort_countdown_if_any(app: &AppHandle, handle: &Arc<IdleSendHandle>) {
    let mut g = handle.inner.lock().unwrap_or_else(|e| e.into_inner());
    if g.countdown.is_none() {
        return;
    }
    g.countdown = None;
    if g.running_id.is_none() {
        g.phase = if g.queue.is_empty() {
            IdleSendPhase::Idle
        } else {
            IdleSendPhase::WaitingIdle
        };
    }
    drop(g);
    emit_changed(app);
    let _ = tray::refresh_tray_tooltip(app);
}

async fn wait_until_task_done(app: &AppHandle, task_id: &str) {
    // 给 spawn 一点时间把 task 放入 running_tasks
    tokio::time::sleep(Duration::from_millis(50)).await;
    for _ in 0..900 {
        let Some(sched) = app.try_state::<Arc<SchedulerHandle>>() else {
            break;
        };
        if !sched.is_task_running(task_id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn emit_changed(app: &AppHandle) {
    if let Some(handle) = app.try_state::<Arc<IdleSendHandle>>() {
        let status = handle.status(app);
        let _ = app.emit(EVENT_IDLE_SEND_CHANGED, status);
    }
}

pub fn refresh_status_emit(app: &AppHandle) {
    emit_changed(app);
    let _ = tray::refresh_tray_tooltip(app);
}

pub fn tooltip_pending_suffix(app: &AppHandle) -> String {
    let n = app
        .try_state::<Arc<IdleSendHandle>>()
        .map(|h| h.pending_count())
        .unwrap_or(0);
    if n > 0 {
        format!(" · 待发送 {n}")
    } else {
        String::new()
    }
}

#[cfg(windows)]
fn last_input_tick() -> u32 {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut info).as_bool() {
            info.dwTime
        } else {
            0
        }
    }
}

#[cfg(not(windows))]
fn last_input_tick() -> u32 {
    0
}

#[cfg(windows)]
fn system_idle_seconds() -> u64 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if !GetLastInputInfo(&mut info).as_bool() {
            return 0;
        }
        let now = GetTickCount();
        let idle_ms = now.wrapping_sub(info.dwTime);
        (idle_ms / 1000) as u64
    }
}

#[cfg(not(windows))]
fn system_idle_seconds() -> u64 {
    u64::MAX / 4
}

pub fn on_settings_changed(app: &AppHandle, _settings: &WecomIdleSendSettings) {
    refresh_status_emit(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{OnActionFail, Trigger};

    fn task_with_wecom() -> Task {
        Task {
            id: "t1".into(),
            name: "企微".into(),
            enabled: true,
            trigger: Trigger::Once {
                datetime: "2099-01-01 00:00:00".into(),
            },
            actions: vec![Action::WecomUiDm {
                id: "a1".into(),
                contact: "张三".into(),
                message: "hi".into(),
                launch_wecom: true,
                timeout_sec: 30,
                retry_count: 0,
                notify_on_fail: true,
                close_after_send: true,
            }],
            on_action_fail: OnActionFail::Stop,
            last_run_at: None,
            last_status: None,
            updated_at: None,
            next_run_at: None,
        }
    }

    #[test]
    fn detects_wecom_action() {
        assert!(task_has_wecom(&task_with_wecom()));
        let mut t = task_with_wecom();
        t.actions = vec![Action::OpenUrl {
            id: "u".into(),
            url: "https://example.com".into(),
        }];
        assert!(!task_has_wecom(&t));
    }
}
