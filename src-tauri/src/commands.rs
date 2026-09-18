use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::idle_send::{self, IdleSendHandle, IdleSendStatus, PendingWecomItem};
use crate::models::{
    ExecutionLog, SchedulerStatus, StoreInfo, Task, WecomProbeResult, WecomTrySendResult,
};
use crate::notify;
use crate::scheduler::{self, SchedulerHandle};
use crate::settings::{AppSettings, SettingsPatch, SettingsView};
use crate::store::{
    self, AppState, ExportPayload, ImportMode, ImportResult, LogFilter,
};
use crate::tray;

use crate::wecom::{self, WecomParams};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn get_store_info(state: State<'_, AppState>) -> Result<StoreInfo, String> {
    state.info()
}

#[tauri::command]
pub fn list_tasks(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Task>, String> {
    let interval_last = app
        .try_state::<Arc<SchedulerHandle>>()
        .map(|h| h.interval_last_snapshot())
        .unwrap_or_default();
    state.list_tasks_with_schedule(&interval_last)
}

#[tauri::command]
pub fn get_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<Task>, String> {
    let mut task = state.get_task(&id)?;
    if let Some(ref mut t) = task {
        let interval_last = app
            .try_state::<Arc<SchedulerHandle>>()
            .map(|h| h.interval_last_snapshot())
            .unwrap_or_default();
        t.next_run_at =
            scheduler::compute_next_run_at(t, chrono::Local::now(), &interval_last);
    }
    Ok(task)
}

#[tauri::command]
pub fn save_task(state: State<'_, AppState>, mut task: Task) -> Result<Task, String> {
    // 计算字段不入库
    task.next_run_at = None;
    state.save_task(task)
}

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state.delete_task(&id)
}

#[tauri::command]
pub fn toggle_task(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<Task, String> {
    state.toggle_task(&id, enabled)
}

#[tauri::command]
pub fn list_logs(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<ExecutionLog>, String> {
    state.list_logs(limit)
}

#[tauri::command]
pub fn append_log(state: State<'_, AppState>, log: ExecutionLog) -> Result<ExecutionLog, String> {
    state.append_log(log)
}

#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>) -> Result<usize, String> {
    state.clear_logs()
}

#[tauri::command]
pub async fn run_task_now(
    app: AppHandle,
    id: String,
    force: Option<bool>,
) -> Result<ExecutionLog, String> {
    scheduler::run_task_now(app, id, force.unwrap_or(false)).await
}

#[tauri::command]
pub fn get_scheduler_status(
    handle: State<'_, Arc<SchedulerHandle>>,
) -> Result<SchedulerStatus, String> {
    Ok(handle.status())
}

#[tauri::command]
pub fn reload_scheduler(
    app: AppHandle,
    handle: State<'_, Arc<SchedulerHandle>>,
) -> Result<SchedulerStatus, String> {
    // 调度循环每秒读 store；此处刷新计数与清理已删任务状态。
    let state = app.state::<AppState>();
    let tasks = state.list_tasks()?;
    let enabled: usize = tasks.iter().filter(|t| t.enabled).count();
    if let Ok(mut g) = handle.enabled_task_count.lock() {
        *g = enabled;
    }
    let ids = tasks.into_iter().map(|t| t.id).collect();
    handle.prune_to_tasks(&ids);
    Ok(handle.status())
}

fn settings_view(
    app: &AppHandle,
    settings: &crate::settings::AppSettings,
    paused: bool,
) -> SettingsView {
    let resolved = crate::wecom_agent::resolve_agent_path(&settings.wecom_agent)
        .map(|p| p.display().to_string());
    SettingsView {
        autostart: settings.autostart,
        minimize_to_tray_on_close: settings.minimize_to_tray_on_close,
        notify_on_task_fail: settings.notify_on_task_fail,
        scheduler_paused: paused,
        missed_job_policy: settings.missed_job_policy.clone(),
        wecom_idle_send: settings.wecom_idle_send.clone(),
        wecom_agent: settings.wecom_agent.clone(),
        notification_permission: notify::permission_label(app),
        wecom_agent_resolved_path: resolved,
    }
}

fn clamp_idle_secs(v: u32) -> u32 {
    v.clamp(5, 600)
}

fn clamp_countdown_secs(v: u32) -> u32 {
    v.clamp(1, 60)
}

#[tauri::command]
pub fn get_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    handle: State<'_, Arc<SchedulerHandle>>,
) -> Result<SettingsView, String> {
    let settings = state.get_settings()?;
    // 开机自启以插件实际状态为准，并回写 store 以免漂移
    let autostart = app
        .autolaunch()
        .is_enabled()
        .unwrap_or(settings.autostart);
    if autostart != settings.autostart {
        let _ = state.update_settings(|s| s.autostart = autostart);
    }
    let settings = state.get_settings()?;
    Ok(settings_view(&app, &settings, handle.is_paused()))
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    handle: State<'_, Arc<SchedulerHandle>>,
    patch: SettingsPatch,
) -> Result<SettingsView, String> {
    if let Some(want) = patch.autostart {
        let mgr = app.autolaunch();
        let current = mgr.is_enabled().unwrap_or(false);
        if want && !current {
            mgr.enable()
                .map_err(|e| format!("启用开机自启失败: {e}"))?;
        } else if !want && current {
            mgr.disable()
                .map_err(|e| format!("关闭开机自启失败: {e}"))?;
        }
    }

    let was_paused = handle.is_paused();
    let resume_requested = patch.scheduler_paused == Some(false) && was_paused;

    if let Some(paused) = patch.scheduler_paused {
        handle.set_paused(paused);
        let _ = tray::refresh_tray_menu(&app);
    }

    let settings = state.update_settings(|s| {
        if let Some(v) = patch.autostart {
            s.autostart = v;
        }
        if let Some(v) = patch.minimize_to_tray_on_close {
            s.minimize_to_tray_on_close = v;
        }
        if let Some(v) = patch.notify_on_task_fail {
            s.notify_on_task_fail = v;
        }
        if let Some(v) = patch.scheduler_paused {
            s.scheduler_paused = v;
        }
        if let Some(v) = patch.missed_job_policy {
            s.missed_job_policy = v;
        }
        if let Some(p) = patch.wecom_idle_send {
            if let Some(v) = p.enabled {
                s.wecom_idle_send.enabled = v;
            }
            if let Some(v) = p.idle_seconds {
                s.wecom_idle_send.idle_seconds = clamp_idle_secs(v);
            }
            if let Some(v) = p.countdown_seconds {
                s.wecom_idle_send.countdown_seconds = clamp_countdown_secs(v);
            }
        }
        if let Some(p) = patch.wecom_agent {
            if let Some(v) = p.enabled {
                s.wecom_agent.enabled = v;
            }
            if let Some(v) = p.path {
                s.wecom_agent.path = v;
            }
            if let Some(v) = p.fallback_to_input {
                s.wecom_agent.fallback_to_input = v;
            }
        }
    })?;

    idle_send::on_settings_changed(&app, &settings.wecom_idle_send);

    if resume_requested {
        scheduler::on_scheduler_resumed(&app, &handle);
    }

    Ok(settings_view(&app, &settings, handle.is_paused()))
}

#[tauri::command]
pub fn set_scheduler_paused(
    app: AppHandle,
    state: State<'_, AppState>,
    handle: State<'_, Arc<SchedulerHandle>>,
    paused: bool,
) -> Result<SchedulerStatus, String> {
    let was_paused = handle.is_paused();
    handle.set_paused(paused);
    let _ = state.update_settings(|s| s.scheduler_paused = paused);
    let _ = tray::refresh_tray_menu(&app);
    if was_paused && !paused {
        scheduler::on_scheduler_resumed(&app, &handle);
    }
    Ok(handle.status())
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<(), String> {
    tray::show_main_window(&app);
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WecomProbeArgs {
    #[serde(default = "default_true")]
    pub launch_wecom: bool,
    #[serde(default = "default_probe_timeout")]
    pub timeout_sec: u32,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WecomTrySendArgs {
    pub contact: String,
    pub message: String,
    #[serde(default = "default_true")]
    pub launch_wecom: bool,
    #[serde(default = "default_probe_timeout")]
    pub timeout_sec: u32,
    #[serde(default)]
    pub retry_count: u32,
}

fn default_true() -> bool {
    true
}

fn default_probe_timeout() -> u32 {
    30
}

/// 探测 FlaUI Agent 是否可用（ping）。
#[tauri::command]
pub fn wecom_agent_ping(state: State<'_, AppState>) -> Result<crate::wecom_agent::AgentOutcome, String> {
    let settings = state.get_settings()?.wecom_agent;
    crate::wecom_agent::ping(&settings)
}

/// 企微预检：仅启动/定位/前置窗口，不发消息。不写正式任务日志。
#[tauri::command]
pub async fn wecom_probe_window(
    app: AppHandle,
    args: WecomProbeArgs,
) -> Result<WecomProbeResult, String> {
    let launch = args.launch_wecom;
    let timeout = args.timeout_sec;
    tauri::async_runtime::spawn_blocking(move || {
        wecom::probe_window(&app, launch, timeout)
    })
    .await
    .map_err(|e| format!("预检线程异常: {e}"))
}

/// 企微预检：完整发送流程（对当前编辑中的 contact/message）。不写正式任务日志。
#[tauri::command]
pub async fn wecom_try_send(
    app: AppHandle,
    args: WecomTrySendArgs,
) -> Result<WecomTrySendResult, String> {
    let contact = args.contact;
    let message = args.message;
    let launch = args.launch_wecom;
    let timeout = args.timeout_sec;
    let retry = args.retry_count;
    tauri::async_runtime::spawn_blocking(move || {
        wecom::try_send(
            &app,
            WecomParams {
                contact: &contact,
                message: &message,
                launch_wecom: launch,
                timeout_sec: timeout,
                retry_count: retry,
                // 预检失败不弹系统通知，避免打扰；结果由前端 toast 展示
                notify_on_fail: false,
                // 试发送默认也关闭窗口，便于归还桌面；与正式动作 closeAfterSend 一致
                close_after_send: true,
            },
        )
    })
    .await
    .map_err(|e| format!("试发送线程异常: {e}"))
}

/// 申请系统通知权限，返回可读状态。
#[tauri::command]
pub fn request_notification_permission(app: AppHandle) -> Result<String, String> {
    notify::request_permission(&app)
}

/// 用系统资源管理器打开数据目录。
#[tauri::command]
pub fn open_data_dir(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.data_dir.display().to_string();
    // 目录用 open_path；reveal 更适合「定位文件」
    app.opener()
        .open_path(&dir, None::<&str>)
        .map_err(|e| format!("打开数据目录失败: {e}"))
}

/// 重置 settings 为默认（不影响任务与日志）；开机自启会同步关闭系统启动项。
#[tauri::command]
pub fn reset_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    handle: State<'_, Arc<SchedulerHandle>>,
) -> Result<SettingsView, String> {
    let defaults = AppSettings::default();
    // 同步 OS 自启
    let mgr = app.autolaunch();
    if mgr.is_enabled().unwrap_or(false) {
        let _ = mgr.disable();
    }
    let was_paused = handle.is_paused();
    handle.set_paused(defaults.scheduler_paused);
    let settings = state.reset_settings()?;
    let _ = tray::refresh_tray_menu(&app);
    if was_paused && !defaults.scheduler_paused {
        scheduler::on_scheduler_resumed(&app, &handle);
    }
    idle_send::on_settings_changed(&app, &settings.wecom_idle_send);
    Ok(settings_view(&app, &settings, handle.is_paused()))
}

#[tauri::command]
pub fn list_pending_wecom(
    handle: State<'_, Arc<IdleSendHandle>>,
) -> Result<Vec<PendingWecomItem>, String> {
    Ok(handle.list())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelPendingWecomArgs {
    pub id: Option<String>,
}

#[tauri::command]
pub fn cancel_pending_wecom(
    app: AppHandle,
    handle: State<'_, Arc<IdleSendHandle>>,
    args: Option<CancelPendingWecomArgs>,
) -> Result<usize, String> {
    let id = args.and_then(|a| a.id);
    Ok(handle.cancel(&app, id.as_deref()))
}

#[tauri::command]
pub fn get_idle_send_status(
    app: AppHandle,
    handle: State<'_, Arc<IdleSendHandle>>,
) -> Result<IdleSendStatus, String> {
    Ok(handle.status(&app))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTasksArgs {
    #[serde(default)]
    pub include_logs: bool,
}

/// 导出任务（可选日志）为 JSON 字符串。
#[tauri::command]
pub fn export_tasks(
    state: State<'_, AppState>,
    args: Option<ExportTasksArgs>,
) -> Result<ExportPayload, String> {
    let include_logs = args.map(|a| a.include_logs).unwrap_or(false);
    state.export_payload(include_logs)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportTasksArgs {
    pub json: String,
    /// merge | replace
    pub mode: ImportMode,
}

/// 从 JSON 导入任务；非法项跳过并汇总；成功后需前端调用 reload_scheduler。
#[tauri::command]
pub fn import_tasks(
    app: AppHandle,
    state: State<'_, AppState>,
    handle: State<'_, Arc<SchedulerHandle>>,
    args: ImportTasksArgs,
) -> Result<ImportResult, String> {
    let (tasks, parse_errors) = store::parse_import_tasks(&args.json)?;
    let mut result = state.import_tasks(tasks, args.mode)?;
    if !parse_errors.is_empty() {
        result.skipped += parse_errors.len();
        result.errors.extend(parse_errors);
    }
    // 导入后刷新调度计数
    let tasks = state.list_tasks()?;
    let enabled: usize = tasks.iter().filter(|t| t.enabled).count();
    if let Ok(mut g) = handle.enabled_task_count.lock() {
        *g = enabled;
    }
    let ids = tasks.into_iter().map(|t| t.id).collect();
    handle.prune_to_tasks(&ids);
    let _ = app;
    Ok(result)
}

/// 按条件筛选日志（服务端过滤，最多 MAX_LOGS）。
#[tauri::command]
pub fn filter_logs(
    state: State<'_, AppState>,
    filter: LogFilter,
) -> Result<Vec<ExecutionLog>, String> {
    state.filter_logs(&filter)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportLogsArgs {
    #[serde(default)]
    pub filter: LogFilter,
    /// json | txt
    #[serde(default = "default_log_export_format")]
    pub format: String,
}

fn default_log_export_format() -> String {
    "json".into()
}

/// 导出当前筛选日志为文本内容（由前端写入文件）。
#[tauri::command]
pub fn export_filtered_logs(
    state: State<'_, AppState>,
    args: ExportLogsArgs,
) -> Result<String, String> {
    let logs = state.filter_logs(&args.filter)?;
    let fmt = args.format.trim().to_ascii_lowercase();
    if fmt == "txt" || fmt == "text" {
        Ok(logs_to_txt(&logs))
    } else {
        serde_json::to_string_pretty(&logs).map_err(|e| format!("序列化日志失败: {e}"))
    }
}

/// 写入文本文件（供导出对话框选定路径后调用）。
#[tauri::command]
pub fn write_text_file(path: String, content: String) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("路径为空".into());
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
    }
    std::fs::write(path, content).map_err(|e| format!("写入文件失败: {e}"))
}

/// 读取文本文件（供导入对话框选定路径后调用）。
#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("路径为空".into());
    }
    std::fs::read_to_string(path).map_err(|e| format!("读取文件失败: {e}"))
}

/// 更新安装 / 退出前调用：设置 ExitFlag，避免关窗进托盘卡住安装器。
#[tauri::command]
pub fn prepare_for_exit(app: AppHandle) {
    if let Some(flag) = app.try_state::<tray::ExitFlag>() {
        flag.request_exit();
    }
}

fn logs_to_txt(logs: &[ExecutionLog]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 时序 · TaskTick 日志导出 · {} 条\n\n", logs.len()));
    for (i, log) in logs.iter().enumerate() {
        let status = match log.status {
            crate::models::RunStatus::Success => "success",
            crate::models::RunStatus::Failed => "failed",
            crate::models::RunStatus::Running => "running",
            crate::models::RunStatus::Skipped => "skipped",
        };
        out.push_str(&format!(
            "---- [{}] {} · {} · {}\n",
            i + 1,
            log.time,
            status,
            log.task_name
        ));
        out.push_str(&format!("taskId: {}\n", log.task_id));
        if !log.detail.trim().is_empty() {
            out.push_str(&format!("detail: {}\n", log.detail));
        }
        for step in &log.steps {
            out.push_str(&format!(
                "  - [{}] {} · {} · {}\n",
                step.status,
                step.time,
                step.title,
                step.note
            ));
        }
        out.push('\n');
    }
    out
}
