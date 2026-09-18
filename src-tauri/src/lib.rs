mod actions;
mod commands;
mod idle_send;
mod models;
mod notify;
mod scheduler;
mod settings;
mod store;
mod tray;
mod wecom;
mod wecom_agent;

use store::AppState;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tray::ExitFlag;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // single-instance 必须最先注册，确保二次启动在其它插件初始化前被拦截。
    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(ExitFlag::new())
        .setup(|app| {
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            let state = AppState::load(app.handle())?;
            // 同步开机自启：以 store 偏好为准写回系统
            if let Ok(settings) = state.get_settings() {
                let mgr = app.handle().autolaunch();
                let enabled = mgr.is_enabled().unwrap_or(false);
                if settings.autostart && !enabled {
                    let _ = mgr.enable();
                } else if !settings.autostart && enabled {
                    // 不在启动时强制 disable，避免用户手动加了启动项被清掉；
                    // 仅当用户在设置里关闭时才会 disable。
                    let _ = settings.autostart;
                }
            }
            app.manage(state);
            scheduler::start(app.handle().clone());
            idle_send::start(app.handle().clone());
            tray::setup_tray(app.handle())?;
            // 启动时若从 settings 恢复了暂停态，同步托盘菜单文案
            let _ = tray::refresh_tray_menu(app.handle());
            let _ = tray::refresh_tray_tooltip(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if tray::should_minimize_on_close(app) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_store_info,
            commands::list_tasks,
            commands::get_task,
            commands::save_task,
            commands::delete_task,
            commands::toggle_task,
            commands::list_logs,
            commands::append_log,
            commands::clear_logs,
            commands::run_task_now,
            commands::get_scheduler_status,
            commands::reload_scheduler,
            commands::get_settings,
            commands::update_settings,
            commands::set_scheduler_paused,
            commands::show_main_window,
            commands::wecom_probe_window,
            commands::wecom_try_send,
            commands::request_notification_permission,
            commands::open_data_dir,
            commands::reset_settings,
            commands::export_tasks,
            commands::import_tasks,
            commands::filter_logs,
            commands::export_filtered_logs,
            commands::write_text_file,
            commands::read_text_file,
            commands::list_pending_wecom,
            commands::cancel_pending_wecom,
            commands::get_idle_send_status,
            commands::wecom_agent_ping,
            commands::prepare_for_exit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
