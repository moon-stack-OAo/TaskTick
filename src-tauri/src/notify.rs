//! 系统通知封装：尊重设置总开关，并返回可读的权限/发送反馈。

use tauri::AppHandle;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;

use crate::store::AppState;

#[derive(Debug, Clone)]
pub struct NotifyOutcome {
    pub sent: bool,
    pub note: String,
}

/// 是否允许发送失败通知（设置总开关）。
pub fn notify_on_task_fail_enabled(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .and_then(|s| s.get_settings().ok())
        .map(|s| s.notify_on_task_fail)
        .unwrap_or(true)
}

pub fn permission_label(app: &AppHandle) -> String {
    match app.notification().permission_state() {
        Ok(state) => format!("{state:?}").to_ascii_lowercase(),
        Err(_) => "unknown".into(),
    }
}

/// 请求通知权限，返回可读状态文案。
pub fn request_permission(app: &AppHandle) -> Result<String, String> {
    let state = app
        .notification()
        .request_permission()
        .map_err(|e| format!("申请通知权限失败: {e}"))?;
    Ok(format!("{state:?}").to_ascii_lowercase())
}

/// 发送信息类系统通知（入队 / 倒计时等），不检查失败通知总开关。
pub fn send_info_notification(app: &AppHandle, title: &str, body: &str) -> NotifyOutcome {
    let perm = app.notification().permission_state();
    match &perm {
        Ok(state) => {
            let label = format!("{state:?}").to_ascii_lowercase();
            if label.contains("denied") {
                return NotifyOutcome {
                    sent: false,
                    note: "通知权限被拒绝".into(),
                };
            }
        }
        Err(e) => {
            return NotifyOutcome {
                sent: false,
                note: format!("无法读取通知权限: {e}"),
            };
        }
    }
    let _ = app.notification().request_permission();
    match app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
    {
        Ok(()) => NotifyOutcome {
            sent: true,
            note: "已发送系统通知".into(),
        },
        Err(e) => NotifyOutcome {
            sent: false,
            note: format!("通知发送失败: {e}"),
        },
    }
}

/// 发送失败类系统通知。
///
/// 优先级：`settings.notifyOnTaskFail`（总开关）> 动作级 `notifyOnFail`。
/// 调用方应先判断动作级开关；本函数再校验总开关与权限。
pub fn send_fail_notification(
    app: &AppHandle,
    title: &str,
    body: &str,
    action_wants_notify: bool,
) -> NotifyOutcome {
    if !action_wants_notify {
        return NotifyOutcome {
            sent: false,
            note: "动作未要求通知（notifyOnFail=false）".into(),
        };
    }
    if !notify_on_task_fail_enabled(app) {
        return NotifyOutcome {
            sent: false,
            note: "设置总开关「任务失败时通知」已关闭，未发送".into(),
        };
    }

    let perm = app.notification().permission_state();
    match &perm {
        Ok(state) => {
            let label = format!("{state:?}").to_ascii_lowercase();
            if label.contains("denied") {
                return NotifyOutcome {
                    sent: false,
                    note: "通知权限被拒绝，请在系统设置中允许后重试".into(),
                };
            }
        }
        Err(e) => {
            return NotifyOutcome {
                sent: false,
                note: format!("无法读取通知权限: {e}"),
            };
        }
    }

    // Desktop 上 request 通常直接 Granted；仍尝试一次以确保状态刷新。
    let _ = app.notification().request_permission();

    match app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
    {
        Ok(()) => NotifyOutcome {
            sent: true,
            note: "已发送系统通知".into(),
        },
        Err(e) => NotifyOutcome {
            sent: false,
            note: format!("通知发送失败: {e}"),
        },
    }
}
