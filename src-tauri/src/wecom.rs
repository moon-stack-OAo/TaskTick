//! 企业微信私聊 UI 自动化（Windows 首版）。
//!
//! 实现策略：Win32 找窗 + 前台激活 + 剪贴板粘贴 + 键鼠快捷键。
//! 依赖前台窗口与企微默认快捷键（Ctrl+F 搜索），改版/锁屏/未前台时可能失败。

use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use tauri::AppHandle;

use crate::models::{LogStep, WecomProbeResult, WecomTrySendResult};
use crate::notify;
use crate::store::AppState;
use tauri::Manager;

#[cfg(windows)]
fn load_agent_settings(app: &AppHandle) -> Option<crate::settings::WecomAgentSettings> {
    app.try_state::<AppState>()
        .and_then(|s| s.get_settings().ok())
        .map(|s| s.wecom_agent)
}

#[cfg(windows)]
fn try_agent_probe(
    app: &AppHandle,
    launch_wecom: bool,
    timeout_sec: u32,
) -> Option<WecomProbeResult> {
    let settings = load_agent_settings(app)?;
    if !settings.enabled {
        return None;
    }
    match crate::wecom_agent::probe(&settings, launch_wecom, timeout_sec) {
        Ok(out) => Some(WecomProbeResult {
            ok: out.ok,
            note: out.note,
            steps: out.steps,
        }),
        Err(err) => {
            if settings.fallback_to_input {
                // 让调用方继续走键鼠
                let _ = err;
                None
            } else {
                Some(WecomProbeResult {
                    ok: false,
                    note: format!("FlaUI Agent 预检失败: {err}"),
                    steps: vec![step("FlaUI Agent", false, err)],
                })
            }
        }
    }
}

#[cfg(windows)]
fn try_agent_send(app: &AppHandle, params: &WecomParams<'_>) -> Option<WecomResult> {
    let settings = load_agent_settings(app)?;
    if !settings.enabled {
        return None;
    }

    match crate::wecom_agent::send(
        &settings,
        params.contact,
        params.message,
        params.launch_wecom,
        params.timeout_sec,
        params.retry_count,
        params.close_after_send,
    ) {
        Ok(out) if out.ok => {
            return Some(WecomResult {
                ok: true,
                note: out.note,
                steps: out.steps,
                notified: false,
            });
        }
        Ok(out) => {
            if settings.fallback_to_input {
                // Agent 业务失败也允许回退键鼠再试一次
                return None;
            }
            let outcome = notify::send_fail_notification(
                app,
                "时序 · TaskTick · 企微私聊失败",
                &out.note,
                params.notify_on_fail,
            );
            let mut steps = out.steps;
            steps.push(step("失败通知", outcome.sent, outcome.note));
            return Some(WecomResult {
                ok: false,
                note: out.note,
                steps,
                notified: outcome.sent,
            });
        }
        Err(err) => {
            if settings.fallback_to_input {
                return None;
            }
            let outcome = notify::send_fail_notification(
                app,
                "时序 · TaskTick · 企微私聊失败",
                &err,
                params.notify_on_fail,
            );
            return Some(WecomResult {
                ok: false,
                note: err.clone(),
                steps: vec![
                    step("FlaUI Agent", false, &err),
                    step("失败通知", outcome.sent, outcome.note),
                ],
                notified: outcome.sent,
            });
        }
    }
}

pub struct WecomParams<'a> {
    pub contact: &'a str,
    pub message: &'a str,
    pub launch_wecom: bool,
    pub timeout_sec: u32,
    pub retry_count: u32,
    pub notify_on_fail: bool,
    pub close_after_send: bool,
}

pub struct WecomResult {
    pub ok: bool,
    pub note: String,
    pub steps: Vec<LogStep>,
    pub notified: bool,
}

fn step(title: &str, ok: bool, note: impl Into<String>) -> LogStep {
    LogStep {
        title: title.into(),
        status: if ok { "ok".into() } else { "fail".into() },
        time: Local::now().format("%H:%M:%S").to_string(),
        note: note.into(),
    }
}

#[cfg(windows)]
const SESSION_LOCKED_MSG: &str = "系统已锁屏，已跳过企微自动化";

/// 锁屏时返回失败结果；未锁屏返回 None。
#[cfg(windows)]
fn session_locked_result() -> Option<WecomResult> {
    if !windows_impl::is_session_locked() {
        return None;
    }
    let note = SESSION_LOCKED_MSG.to_string();
    Some(WecomResult {
        ok: false,
        note: note.clone(),
        steps: vec![step("锁屏检查", false, note)],
        notified: false,
    })
}

pub fn execute_wecom_ui_dm(app: &AppHandle, params: WecomParams<'_>) -> WecomResult {
    #[cfg(not(windows))]
    {
        let _ = app;
        let note = "企业微信 UI 自动化仅支持 Windows".to_string();
        return WecomResult {
            ok: false,
            note: note.clone(),
            steps: vec![step("平台检查", false, note)],
            notified: false,
        };
    }

    #[cfg(windows)]
    {
        // 锁屏时不抢前台/注入键鼠，也不拉起 Agent
        if let Some(locked) = session_locked_result() {
            return locked;
        }
        if let Some(agent_result) = try_agent_send(app, &params) {
            return agent_result;
        }
        execute_wecom_windows(app, params)
    }
}

/// 预检：仅启动/定位/前置企微窗口，不发消息。
pub fn probe_window(
    app: &AppHandle,
    launch_wecom: bool,
    timeout_sec: u32,
) -> WecomProbeResult {
    #[cfg(not(windows))]
    {
        let _ = app;
        let note = "企业微信 UI 自动化仅支持 Windows".to_string();
        return WecomProbeResult {
            ok: false,
            note: note.clone(),
            steps: vec![step("平台检查", false, note)],
        };
    }

    #[cfg(windows)]
    {
        if windows_impl::is_session_locked() {
            let note = SESSION_LOCKED_MSG.to_string();
            return WecomProbeResult {
                ok: false,
                note: note.clone(),
                steps: vec![step("锁屏检查", false, note)],
            };
        }
        if let Some(r) = try_agent_probe(app, launch_wecom, timeout_sec) {
            return r;
        }
        let timeout = Duration::from_secs(timeout_sec.max(5) as u64);
        match probe_window_once(launch_wecom, timeout) {
            Ok(steps) => WecomProbeResult {
                ok: true,
                note: "预检：企微窗口已定位并前置（键鼠）".into(),
                steps,
            },
            Err((steps, err)) => WecomProbeResult {
                ok: false,
                note: format!("预检失败: {err}"),
                steps,
            },
        }
    }
}

/// 预检：对编辑中的 contact/message 走完整发送流程（不写正式任务日志）。
pub fn try_send(app: &AppHandle, params: WecomParams<'_>) -> WecomTrySendResult {
    let result = execute_wecom_ui_dm(app, params);
    WecomTrySendResult {
        ok: result.ok,
        note: if result.ok {
            format!("预检试发送成功 · {}", result.note)
        } else {
            format!("预检试发送失败 · {}", result.note)
        },
        steps: {
            let mut steps = vec![step(
                "预检",
                true,
                "试发送（不写入正式任务日志）",
            )];
            steps.extend(result.steps);
            steps
        },
        notified: result.notified,
    }
}

#[cfg(windows)]
fn probe_window_once(
    launch_wecom: bool,
    timeout: Duration,
) -> Result<Vec<LogStep>, (Vec<LogStep>, String)> {
    use windows_impl::*;

    let mut steps = Vec::new();
    if is_session_locked() {
        let err = SESSION_LOCKED_MSG.to_string();
        steps.push(step("锁屏检查", false, &err));
        return Err((steps, err));
    }
    steps.push(step("预检", true, "仅定位窗口，不发消息"));

    if launch_wecom {
        match ensure_wecom_running() {
            Ok(note) => steps.push(step("启动企微", true, note)),
            Err(err) => {
                steps.push(step("启动企微", false, &err));
                return Err((steps, err));
            }
        }
    } else {
        steps.push(step(
            "启动企微",
            true,
            "launchWecom=false，跳过启动",
        ));
    }

    let hwnd = match wait_for_wecom_window(timeout) {
        Ok(h) => {
            steps.push(step("定位主窗口", true, format!("HWND={h:?}")));
            h
        }
        Err(err) => {
            steps.push(step("定位主窗口", false, &err));
            return Err((steps, err));
        }
    };

    if let Err(err) = foreground_window(hwnd) {
        steps.push(step("窗口前置", false, &err));
        return Err((steps, err));
    }
    steps.push(step(
        "窗口前置",
        true,
        "已激活企微前台（预检完成，未发送消息）",
    ));
    Ok(steps)
}

#[cfg(windows)]
fn execute_wecom_windows(app: &AppHandle, params: WecomParams<'_>) -> WecomResult {
    // 试发送时编辑弹窗仍在前台，键鼠会被本应用吃掉；先隐藏主窗口再操作企微。
    let hidden = hide_tasktick_windows(app);
    if hidden {
        thread::sleep(Duration::from_millis(120));
    }

    let timeout = Duration::from_secs(params.timeout_sec.max(5) as u64);
    let attempts = params.retry_count.saturating_add(1);
    let mut all_steps: Vec<LogStep> = Vec::new();
    let mut last_err = String::from("未知错误");

    if hidden {
        all_steps.push(step(
            "让出前台",
            true,
            "已暂时隐藏「时序 · TaskTick」主窗口，避免抢走企微焦点",
        ));
    }

    let result = (|| {
        for attempt in 1..=attempts {
            if attempts > 1 {
                all_steps.push(step(
                    "重试",
                    true,
                    format!("第 {attempt}/{attempts} 次尝试"),
                ));
            }

            match run_once(
                params.contact,
                params.message,
                params.launch_wecom,
                params.close_after_send,
                timeout,
            ) {
                Ok(mut steps) => {
                    all_steps.append(&mut steps);
                    let note = format!(
                        "已向「{}」发送私聊（依赖前台窗口 · Ctrl+F / 粘贴 / 点击输入区 / Enter）",
                        params.contact
                    );
                    return WecomResult {
                        ok: true,
                        note,
                        steps: std::mem::take(&mut all_steps),
                        notified: false,
                    };
                }
                Err((mut steps, err)) => {
                    all_steps.append(&mut steps);
                    last_err = err;
                    if attempt < attempts {
                        thread::sleep(Duration::from_millis(800));
                    }
                }
            }
        }

        let outcome = notify::send_fail_notification(
            app,
            "时序 · TaskTick · 企微私聊失败",
            &last_err,
            params.notify_on_fail,
        );
        all_steps.push(step("失败通知", outcome.sent, outcome.note));

        WecomResult {
            ok: false,
            note: last_err,
            steps: std::mem::take(&mut all_steps),
            notified: outcome.sent,
        }
    })();

    if hidden {
        show_tasktick_windows(app);
    }
    result
}

#[cfg(windows)]
fn hide_tasktick_windows(app: &AppHandle) -> bool {
    use tauri::Manager;
    let mut any = false;
    for (_, window) in app.webview_windows() {
        if window.hide().is_ok() {
            any = true;
        }
    }
    any
}

#[cfg(windows)]
fn show_tasktick_windows(app: &AppHandle) {
    use tauri::Manager;
    for (_, window) in app.webview_windows() {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(windows)]
fn run_once(
    contact: &str,
    message: &str,
    launch_wecom: bool,
    close_after_send: bool,
    timeout: Duration,
) -> Result<Vec<LogStep>, (Vec<LogStep>, String)> {
    use windows_impl::*;

    // 整段发送结束或提前失败返回前，统一做一次全局修饰键复位
    let result = run_once_inner(contact, message, launch_wecom, close_after_send, timeout);
    force_release_modifiers();
    result
}

fn run_once_inner(
    contact: &str,
    message: &str,
    launch_wecom: bool,
    close_after_send: bool,
    timeout: Duration,
) -> Result<Vec<LogStep>, (Vec<LogStep>, String)> {
    use windows_impl::*;

    let mut steps = Vec::new();

    // 重试间隙若已锁屏，立即跳过，勿注入键鼠
    if is_session_locked() {
        let err = SESSION_LOCKED_MSG.to_string();
        steps.push(step("锁屏检查", false, &err));
        return Err((steps, err));
    }

    let contact = contact.trim();
    let message = message.trim();

    if contact.is_empty() {
        let err = "联系人为空".to_string();
        steps.push(step("参数校验", false, &err));
        return Err((steps, err));
    }
    if message.is_empty() {
        let err = "消息内容为空".to_string();
        steps.push(step("参数校验", false, &err));
        return Err((steps, err));
    }
    steps.push(step("参数校验", true, format!("联系人={contact}")));

    if launch_wecom {
        match ensure_wecom_running() {
            Ok(note) => steps.push(step("启动企微", true, note)),
            Err(err) => {
                steps.push(step("启动企微", false, &err));
                return Err((steps, err));
            }
        }
    } else {
        steps.push(step(
            "启动企微",
            true,
            "launchWecom=false，跳过启动",
        ));
    }

    let hwnd = match wait_for_wecom_window(timeout) {
        Ok(h) => {
            steps.push(step("定位主窗口", true, format!("HWND={h:?}")));
            h
        }
        Err(err) => {
            steps.push(step("定位主窗口", false, &err));
            return Err((steps, err));
        }
    };

    if let Err(err) = foreground_window(hwnd) {
        steps.push(step("窗口前置", false, &err));
        return Err((steps, err));
    }
    // 短轮询等到前台，避免固定长 sleep
    wait_until(|| is_foreground(hwnd), 400, 40);
    steps.push(step(
        "窗口前置",
        true,
        "已激活企微前台（后续键鼠依赖前台窗口）",
    ));

    if let Err(err) = send_ctrl_key('F') {
        steps.push(step("打开搜索", false, &err));
        return Err((steps, err));
    }
    thread::sleep(Duration::from_millis(220));
    steps.push(step("打开搜索", true, "已发送 Ctrl+F"));

    let _ = send_ctrl_key('A');
    thread::sleep(Duration::from_millis(40));

    if let Err(err) = set_clipboard_text(contact) {
        steps.push(step("粘贴联系人", false, &err));
        return Err((steps, err));
    }
    if let Err(err) = send_ctrl_key('V') {
        steps.push(step("粘贴联系人", false, &err));
        return Err((steps, err));
    }
    // 搜索结果：短等即可，过长主要拖慢整体
    thread::sleep(Duration::from_millis(380));
    steps.push(step(
        "粘贴联系人",
        true,
        format!("已粘贴「{contact}」（剪贴板）"),
    ));

    if let Err(err) = send_enter() {
        steps.push(step("打开私聊", false, &err));
        return Err((steps, err));
    }
    thread::sleep(Duration::from_millis(420));
    steps.push(step(
        "打开私聊",
        true,
        "已按 Enter（假定打开首个搜索结果；重名时可能选错）",
    ));

    if let Err(err) = click_message_input(hwnd) {
        steps.push(step("聚焦输入框", false, &err));
        return Err((steps, err));
    }
    thread::sleep(Duration::from_millis(120));
    steps.push(step("聚焦输入框", true, "已点击主窗口下部输入区域"));

    if let Err(err) = set_clipboard_text(message) {
        steps.push(step("粘贴消息", false, &err));
        return Err((steps, err));
    }
    if let Err(err) = send_ctrl_key('V') {
        steps.push(step("粘贴消息", false, &err));
        return Err((steps, err));
    }
    thread::sleep(Duration::from_millis(120));
    steps.push(step(
        "粘贴消息",
        true,
        format!("已粘贴 {} 字消息", message.chars().count()),
    ));

    if let Err(err) = send_enter() {
        steps.push(step("发送消息", false, &err));
        return Err((steps, err));
    }
    thread::sleep(Duration::from_millis(200));
    steps.push(step(
        "发送消息",
        true,
        "已按 Enter 发送（请人工确认会话里是否出现该消息）",
    ));

    if close_after_send {
        // 稍等确保发送完成，再关闭主窗口（企微常见行为是关窗进托盘）
        thread::sleep(Duration::from_millis(350));
        match close_wecom_window(hwnd) {
            Ok(note) => steps.push(step("关闭窗口", true, note)),
            Err(err) => steps.push(step("关闭窗口", false, err)),
        }
    }

    Ok(steps)
}

fn wait_until(mut pred: impl FnMut() -> bool, timeout_ms: u64, step_ms: u64) {
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let step = Duration::from_millis(step_ms.max(10));
    while start.elapsed() < timeout {
        if pred() {
            return;
        }
        thread::sleep(step);
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::System::StationsAndDesktops::{
        CloseDesktop, OpenInputDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS,
        DESKTOP_READOBJECTS, DESKTOP_WRITEOBJECTS,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, VIRTUAL_KEY,
        VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RETURN,
        VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetForegroundWindow, GetSystemMetrics, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible, PostMessageW, SetCursorPos, SetForegroundWindow,
        ShowWindow, SM_CXSCREEN, SM_CYSCREEN, SW_MINIMIZE, SW_RESTORE, WM_CLOSE,
    };

    static FOUND: AtomicBool = AtomicBool::new(false);
    static FOUND_HWND: Mutex<Option<isize>> = Mutex::new(None);

    /// 通过 OpenInputDesktop 判断会话是否锁屏。
    /// 锁屏时通常无法打开输入桌面（失败/无权限），此时不应注入键鼠。
    pub fn is_session_locked() -> bool {
        // DESKTOP_ACCESS_FLAGS 无 BitOr，手动组合读写对象权限
        let access = DESKTOP_ACCESS_FLAGS(DESKTOP_READOBJECTS.0 | DESKTOP_WRITEOBJECTS.0);
        unsafe {
            match OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, access) {
                Ok(hdesk) => {
                    let _ = CloseDesktop(hdesk);
                    false
                }
                Err(_) => true,
            }
        }
    }

    pub fn ensure_wecom_running() -> Result<String, String> {
        if find_wecom_hwnd().is_some() {
            return Ok("企微窗口已存在".into());
        }

        let path = resolve_wecom_path().ok_or_else(|| {
            "未找到企业微信安装路径（常见路径与注册表均未命中），请手动启动后重试".to_string()
        })?;

        Command::new(&path)
            .spawn()
            .map_err(|e| format!("启动企业微信失败: {e}"))?;

        Ok(format!("已启动 {}", path.display()))
    }

    pub fn resolve_wecom_path() -> Option<PathBuf> {
        let candidates = [
            r"C:\Program Files (x86)\WXWork\WXWork.exe",
            r"C:\Program Files\WXWork\WXWork.exe",
            r"D:\Program Files (x86)\WXWork\WXWork.exe",
            r"D:\Program Files\WXWork\WXWork.exe",
        ];
        for c in candidates {
            let p = PathBuf::from(c);
            if p.exists() {
                return Some(p);
            }
        }

        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let p = PathBuf::from(local).join(r"WXWork\WXWork.exe");
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    pub fn wait_for_wecom_window(timeout: Duration) -> Result<HWND, String> {
        let start = Instant::now();
        loop {
            if let Some(hwnd) = find_wecom_hwnd() {
                return Ok(hwnd);
            }
            if start.elapsed() >= timeout {
                return Err(format!(
                    "超时未找到企业微信主窗口（{}s）。请确认已登录且窗口标题含「企业微信」",
                    timeout.as_secs()
                ));
            }
            thread::sleep(Duration::from_millis(120));
        }
    }

    pub fn find_wecom_hwnd() -> Option<HWND> {
        FOUND.store(false, Ordering::SeqCst);
        if let Ok(mut g) = FOUND_HWND.lock() {
            *g = None;
        }

        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(0));
        }

        FOUND_HWND
            .lock()
            .ok()
            .and_then(|g| g.map(|v| HWND(v as *mut _)))
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, _: LPARAM) -> BOOL {
        if FOUND.load(Ordering::SeqCst) {
            return BOOL(0);
        }
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }

        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len <= 0 {
            return BOOL(1);
        }
        let title = String::from_utf16_lossy(&buf[..len as usize]);
        if title.contains("企业微信") || title.contains("WXWork") {
            if let Ok(mut g) = FOUND_HWND.lock() {
                *g = Some(hwnd.0 as isize);
            }
            FOUND.store(true, Ordering::SeqCst);
            return BOOL(0);
        }
        BOOL(1)
    }

    pub fn is_foreground(hwnd: HWND) -> bool {
        unsafe { GetForegroundWindow() == hwnd }
    }

    pub fn foreground_window(hwnd: HWND) -> Result<(), String> {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let fg = GetForegroundWindow();
            let fg_tid = if fg.is_invalid() {
                0u32
            } else {
                GetWindowThreadProcessId(fg, None)
            };
            let target_tid = GetWindowThreadProcessId(hwnd, None);

            use windows::Win32::System::Threading::AttachThreadInput;
            use windows::Win32::System::Threading::GetCurrentThreadId;

            let cur = GetCurrentThreadId();
            if fg_tid != 0 && fg_tid != cur {
                let _ = AttachThreadInput(fg_tid, cur, true);
            }
            if target_tid != 0 && target_tid != cur {
                let _ = AttachThreadInput(target_tid, cur, true);
            }

            let ok = SetForegroundWindow(hwnd).as_bool();

            if fg_tid != 0 && fg_tid != cur {
                let _ = AttachThreadInput(fg_tid, cur, false);
            }
            if target_tid != 0 && target_tid != cur {
                let _ = AttachThreadInput(target_tid, cur, false);
            }

            // 部分环境 SetForegroundWindow 返回 false 但仍切到前台，再验一次
            if !ok && GetForegroundWindow() != hwnd {
                return Err("无法将企微置于前台（可能被系统限制抢焦点）".into());
            }
        }
        Ok(())
    }

    /// 点击主窗口客户区偏下位置，尽量落到消息输入框。
    pub fn click_message_input(hwnd: HWND) -> Result<(), String> {
        unsafe {
            let mut rect = windows::Win32::Foundation::RECT::default();
            GetClientRect(hwnd, &mut rect)
                .map_err(|e| format!("GetClientRect 失败: {e}"))?;

            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width <= 0 || height <= 0 {
                return Err("企微窗口客户区无效".into());
            }

            // 水平居中偏右（避开左侧会话列表），垂直约 92%（输入区）
            let mut pt = POINT {
                x: rect.left + (width * 62) / 100,
                y: rect.top + (height * 92) / 100,
            };
            if !ClientToScreen(hwnd, &mut pt).as_bool() {
                return Err("ClientToScreen 失败".into());
            }

            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);
            if screen_w <= 0 || screen_h <= 0 {
                return Err("无法读取屏幕尺寸".into());
            }

            // 绝对坐标：0..65535
            let abs_x = ((pt.x as i64) * 65535 / (screen_w as i64 - 1)) as i32;
            let abs_y = ((pt.y as i64) * 65535 / (screen_h as i64 - 1)) as i32;

            let _ = SetCursorPos(pt.x, pt.y);
            thread::sleep(Duration::from_millis(40));

            let down = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: abs_x,
                        dy: abs_y,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_LEFTDOWN
                            | windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_ABSOLUTE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let up = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: abs_x,
                        dy: abs_y,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_LEFTUP
                            | windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_ABSOLUTE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };

            let sent = SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
            if sent == 0 {
                return Err("点击输入区失败（SendInput）".into());
            }
        }
        Ok(())
    }

    pub fn set_clipboard_text(text: &str) -> Result<(), String> {
        clipboard_win::set_clipboard_string(text)
            .map_err(|e| format!("写入剪贴板失败: {e}"))
    }

    fn send_key(vk: VIRTUAL_KEY, key_up: bool) -> Result<(), String> {
        let flags = if key_up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS(0)
        };
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            if sent == 0 {
                return Err("SendInput 失败".into());
            }
        }
        Ok(())
    }

    /// 强制抬起修饰键，避免锁屏/中断导致 Ctrl 等 KEYUP 丢失后「粘键」。
    pub fn force_release_modifiers() {
        // 忽略单次失败：尽力复位左右 Ctrl / Shift / Alt，以及 Win
        let keys = [
            VK_CONTROL,
            VK_LCONTROL,
            VK_RCONTROL,
            VK_SHIFT,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_MENU,
            VK_LMENU,
            VK_RMENU,
            VK_LWIN,
            VK_RWIN,
        ];
        for vk in keys {
            let _ = send_key(vk, true);
        }
    }

    pub fn send_ctrl_key(ch: char) -> Result<(), String> {
        let result = (|| {
            let vk = VIRTUAL_KEY((ch.to_ascii_uppercase() as u8) as u16);
            send_key(VK_CONTROL, false)?;
            thread::sleep(Duration::from_millis(12));
            send_key(vk, false)?;
            thread::sleep(Duration::from_millis(12));
            send_key(vk, true)?;
            thread::sleep(Duration::from_millis(12));
            send_key(VK_CONTROL, true)?;
            Ok(())
        })();
        // 成功与失败路径都强制抬起，防止中间步骤失败后 Ctrl 残留
        force_release_modifiers();
        result
    }

    pub fn send_enter() -> Result<(), String> {
        let result = (|| {
            send_key(VK_RETURN, false)?;
            thread::sleep(Duration::from_millis(12));
            send_key(VK_RETURN, true)?;
            Ok(())
        })();
        force_release_modifiers();
        result
    }

    /// 关闭企微主窗口。多数版本会进托盘而非退出进程。
    pub fn close_wecom_window(hwnd: HWND) -> Result<String, String> {
        unsafe {
            let posted = PostMessageW(hwnd, WM_CLOSE, None, None).is_ok();
            if posted {
                thread::sleep(Duration::from_millis(200));
                // 若仍可见，再尝试最小化
                if IsWindowVisible(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                    return Ok("已发送关闭并最小化（企微可能仍在托盘运行）".into());
                }
                return Ok("已关闭企微主窗口（进程可能仍在托盘）".into());
            }
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
            Ok("PostMessage 关闭失败，已最小化窗口".into())
        }
    }
}
