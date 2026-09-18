use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

use crate::scheduler::SchedulerHandle;
use crate::settings::AppSettings;
use crate::store::AppState;

pub const TRAY_ID: &str = "main-tray";

/// 用户主动退出标志：为 true 时允许关闭窗口并退出进程。
pub struct ExitFlag(pub AtomicBool);

impl ExitFlag {
    pub fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    pub fn request_exit(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_exiting(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn pause_label(paused: bool) -> &'static str {
    if paused {
        "恢复调度"
    } else {
        "暂停调度"
    }
}

/// 根据当前暂停态刷新托盘菜单文案。
pub fn refresh_tray_menu(app: &AppHandle) -> Result<(), String> {
    let paused = app
        .try_state::<Arc<SchedulerHandle>>()
        .map(|h| h.is_paused())
        .unwrap_or(false);

    let show_i = MenuItem::with_id(app, "tray_show", "打开主窗口", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let pause_i = MenuItem::with_id(app, "tray_pause", pause_label(paused), true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let sep = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let quit_i = MenuItem::with_id(app, "tray_quit", "退出", true, None::<&str>)
        .map_err(|e| e.to_string())?;

    let menu = Menu::with_items(app, &[&show_i, &pause_i, &sep, &quit_i])
        .map_err(|e| e.to_string())?;

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 刷新托盘悬停文案（含企微待发送数量）。
pub fn refresh_tray_tooltip(app: &AppHandle) -> Result<(), String> {
    let suffix = crate::idle_send::tooltip_pending_suffix(app);
    let tip = format!("时序 · TaskTick{suffix}");
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_tooltip(Some(tip)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_i = MenuItem::with_id(app, "tray_show", "打开主窗口", true, None::<&str>)?;
    let pause_i = MenuItem::with_id(app, "tray_pause", "暂停调度", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "tray_quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &pause_i, &sep, &quit_i])?;

    // 编译时嵌入专用托盘图标，小尺寸下保持清晰。
    let icon = tauri::include_image!("icons/tray/32x32.png");

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("时序 · TaskTick")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => show_main_window(app),
            "tray_pause" => {
                if let Some(handle) = app.try_state::<Arc<SchedulerHandle>>() {
                    let was_paused = handle.is_paused();
                    let now_paused = handle.toggle_pause();
                    if let Some(state) = app.try_state::<AppState>() {
                        let _ = state.update_settings(|s| s.scheduler_paused = now_paused);
                    }
                    let _ = refresh_tray_menu(app);
                    if was_paused && !now_paused {
                        crate::scheduler::on_scheduler_resumed(app, &handle);
                    }
                    eprintln!(
                        "[tray] scheduler {}",
                        if now_paused { "paused" } else { "resumed" }
                    );
                }
            }
            "tray_quit" => {
                if let Some(flag) = app.try_state::<ExitFlag>() {
                    flag.request_exit();
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

pub fn should_minimize_on_close(app: &AppHandle) -> bool {
    if let Some(flag) = app.try_state::<ExitFlag>() {
        if flag.is_exiting() {
            return false;
        }
    }
    app.try_state::<AppState>()
        .and_then(|s| s.get_settings().ok())
        .map(|s: AppSettings| s.minimize_to_tray_on_close)
        .unwrap_or(true)
}
