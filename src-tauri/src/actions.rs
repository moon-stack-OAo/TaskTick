use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::models::{Action, LogStep, RunStatus, WaitMode};
use crate::wecom::{self, WecomParams};

/// stdout/stderr 写入日志时的截断上限（字节近似按字符计）。
const OUTPUT_TRUNCATE_CHARS: usize = 4096;

pub struct ActionOutcome {
    pub title: String,
    pub status: RunStatus,
    pub note: String,
    /// 动作级分步日志（如企微自动化）；为空时调度器仅写一条汇总 step。
    pub steps: Vec<LogStep>,
}

pub fn execute_action(app: &AppHandle, action: &Action) -> ActionOutcome {
    let started = Instant::now();
    let title = action_title(action);

    match action {
        Action::OpenApp {
            path,
            args,
            wait_mode,
            timeout_sec,
            ..
        } => {
            let (result, steps) = open_app(path, args, wait_mode, *timeout_sec);
            finish_with_steps(title, started, result, steps)
        }
        Action::OpenUrl { url, .. } => {
            let result = open_url(app, url);
            finish_simple(title, started, result)
        }
        Action::RunScript {
            runtime,
            path,
            working_dir,
            timeout_sec,
            ..
        } => {
            let (result, steps) = run_script(runtime, path, working_dir, *timeout_sec);
            finish_with_steps(title, started, result, steps)
        }
        Action::WecomUiDm {
            contact,
            message,
            launch_wecom,
            timeout_sec,
            retry_count,
            notify_on_fail,
            close_after_send,
            ..
        } => {
            let result = wecom::execute_wecom_ui_dm(
                app,
                WecomParams {
                    contact,
                    message,
                    launch_wecom: *launch_wecom,
                    timeout_sec: *timeout_sec,
                    retry_count: *retry_count,
                    notify_on_fail: *notify_on_fail,
                    close_after_send: *close_after_send,
                },
            );
            let elapsed_ms = started.elapsed().as_millis();
            let notify_suffix = if result.ok {
                ""
            } else if result.notified {
                " · 已通知"
            } else if *notify_on_fail {
                " · 通知未送达"
            } else {
                " · 未通知"
            };
            ActionOutcome {
                title,
                status: if result.ok {
                    RunStatus::Success
                } else {
                    RunStatus::Failed
                },
                note: format!("{}{} · {elapsed_ms}ms", result.note, notify_suffix),
                steps: result.steps,
            }
        }
    }
}

fn finish_simple(
    title: String,
    started: Instant,
    result: Result<String, String>,
) -> ActionOutcome {
    finish_with_steps(title, started, result, vec![])
}

fn finish_with_steps(
    title: String,
    started: Instant,
    result: Result<String, String>,
    steps: Vec<LogStep>,
) -> ActionOutcome {
    let elapsed_ms = started.elapsed().as_millis();
    match result {
        Ok(note) => ActionOutcome {
            title,
            status: RunStatus::Success,
            note: if note.is_empty() {
                format!("完成 · {elapsed_ms}ms")
            } else {
                format!("{note} · {elapsed_ms}ms")
            },
            steps,
        },
        Err(err) => ActionOutcome {
            title,
            status: RunStatus::Failed,
            note: format!("{err} · {elapsed_ms}ms"),
            steps,
        },
    }
}

pub fn outcome_to_step(outcome: &ActionOutcome) -> LogStep {
    LogStep {
        title: outcome.title.clone(),
        status: match outcome.status {
            RunStatus::Success => "ok".into(),
            RunStatus::Failed => "fail".into(),
            RunStatus::Running => "running".into(),
            RunStatus::Skipped => "skip".into(),
        },
        time: Local::now().format("%H:%M:%S").to_string(),
        note: outcome.note.clone(),
    }
}

/// 将动作结果展开为日志 steps：优先使用动作内部分步，否则单条汇总。
pub fn outcome_to_steps(outcome: &ActionOutcome) -> Vec<LogStep> {
    if outcome.steps.is_empty() {
        vec![outcome_to_step(outcome)]
    } else {
        let mut steps = outcome.steps.clone();
        steps.insert(0, outcome_to_step(outcome));
        steps
    }
}

fn action_title(action: &Action) -> String {
    match action {
        Action::OpenApp { path, .. } => format!("打开应用 · {}", file_name(path)),
        Action::OpenUrl { url, .. } => format!("打开网址 · {url}"),
        Action::RunScript { path, runtime, .. } => {
            format!("运行脚本 · {} ({runtime})", file_name(path))
        }
        Action::WecomUiDm { contact, .. } => format!("企微私聊 · {contact}"),
    }
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn log_step(title: &str, ok: bool, note: impl Into<String>) -> LogStep {
    LogStep {
        title: title.into(),
        status: if ok { "ok".into() } else { "fail".into() },
        time: Local::now().format("%H:%M:%S").to_string(),
        note: note.into(),
    }
}

fn truncate_output(raw: &str) -> (String, bool) {
    let trimmed = raw.trim();
    if trimmed.chars().count() <= OUTPUT_TRUNCATE_CHARS {
        return (trimmed.to_string(), false);
    }
    let short: String = trimmed.chars().take(OUTPUT_TRUNCATE_CHARS).collect();
    (format!("{short}\n…(已截断，原文超过 {OUTPUT_TRUNCATE_CHARS} 字符)"), true)
}

fn open_url(app: &AppHandle, url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("网址为空".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("打开网址失败: {e}"))?;
    Ok(format!("已用默认浏览器打开 {url}"))
}

fn open_app(
    path: &str,
    args: &str,
    wait_mode: &WaitMode,
    timeout_sec: u32,
) -> (Result<String, String>, Vec<LogStep>) {
    let mut steps = Vec::new();
    let path = path.trim();
    if path.is_empty() {
        let err = "应用路径为空".to_string();
        steps.push(log_step("参数校验", false, &err));
        return (Err(err), steps);
    }
    if !Path::new(path).exists() {
        let err = format!("应用不存在: {path}");
        steps.push(log_step("参数校验", false, &err));
        return (Err(err), steps);
    }
    steps.push(log_step("参数校验", true, path));

    let mut cmd = Command::new(path);
    let parsed = split_command_args(args);
    if !parsed.is_empty() {
        cmd.args(&parsed);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    match wait_mode {
        WaitMode::FireAndForget => match cmd.spawn() {
            Ok(child) => {
                let note = if parsed.is_empty() {
                    format!("已启动 {path}（fire_and_forget · pid={})", child.id())
                } else {
                    format!(
                        "已启动 {path}（{} 个参数 · fire_and_forget · pid={}）",
                        parsed.len(),
                        child.id()
                    )
                };
                steps.push(log_step("启动进程", true, &note));
                (Ok(note), steps)
            }
            Err(e) => {
                let err = format!("启动应用失败: {e}");
                steps.push(log_step("启动进程", false, &err));
                (Err(err), steps)
            }
        },
        WaitMode::WaitExit => {
            steps.push(log_step(
                "等待策略",
                true,
                if timeout_sec == 0 {
                    "wait_exit · 不限时".to_string()
                } else {
                    format!("wait_exit · 超时 {timeout_sec}s")
                },
            ));
            match cmd.spawn() {
                Ok(mut child) => {
                    let pid = child.id();
                    steps.push(log_step("启动进程", true, format!("pid={pid}")));
                    match wait_child_with_timeout(&mut child, timeout_sec) {
                        Ok(code) => {
                            if code == 0 {
                                let note = format!("进程已退出 · 退出码 0 · pid={pid}");
                                steps.push(log_step("等待退出", true, &note));
                                (Ok(note), steps)
                            } else {
                                let err = format!("进程退出码 {code} · pid={pid}");
                                steps.push(log_step("等待退出", false, &err));
                                (Err(err), steps)
                            }
                        }
                        Err(err) => {
                            let _ = child.kill();
                            let _ = child.wait();
                            steps.push(log_step("等待退出", false, &err));
                            (Err(err), steps)
                        }
                    }
                }
                Err(e) => {
                    let err = format!("启动应用失败: {e}");
                    steps.push(log_step("启动进程", false, &err));
                    (Err(err), steps)
                }
            }
        }
    }
}

fn wait_child_with_timeout(
    child: &mut std::process::Child,
    timeout_sec: u32,
) -> Result<i32, String> {
    if timeout_sec == 0 {
        let status = child
            .wait()
            .map_err(|e| format!("等待进程退出失败: {e}"))?;
        return Ok(status.code().unwrap_or(-1));
    }

    let deadline = Instant::now() + Duration::from_secs(timeout_sec as u64);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code().unwrap_or(-1)),
            Ok(None) => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "等待进程退出超时（{timeout_sec}s），已尝试终止"
                    ));
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(format!("探测进程状态失败: {e}")),
        }
    }
}

fn run_script(
    runtime: &str,
    path: &str,
    working_dir: &str,
    timeout_sec: u32,
) -> (Result<String, String>, Vec<LogStep>) {
    let mut steps = Vec::new();
    let path = path.trim();
    if path.is_empty() {
        let err = "脚本路径为空".to_string();
        steps.push(log_step("参数校验", false, &err));
        return (Err(err), steps);
    }
    let script_path = PathBuf::from(path);
    if !script_path.exists() {
        let err = format!("脚本不存在: {path}");
        steps.push(log_step("参数校验", false, &err));
        return (Err(err), steps);
    }

    let runtime = runtime.trim().to_ascii_lowercase();
    let (program, args): (String, Vec<String>) = match runtime.as_str() {
        "ps1" | "powershell" | "pwsh" => (
            "powershell".into(),
            vec![
                "-NoProfile".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                path.to_string(),
            ],
        ),
        "bat" | "cmd" => ("cmd".into(), vec!["/C".into(), path.to_string()]),
        "py" | "python" => ("python".into(), vec![path.to_string()]),
        other => {
            let err = format!("不支持的脚本运行时: {other}（支持 ps1 / bat / cmd / python）");
            steps.push(log_step("参数校验", false, &err));
            return (Err(err), steps);
        }
    };
    steps.push(log_step(
        "参数校验",
        true,
        format!("runtime={runtime} · path={path}"),
    ));

    let cwd = resolve_working_dir(working_dir, &script_path);
    if let Some(ref dir) = cwd {
        if !dir.is_dir() {
            let err = format!("工作目录不存在: {}", dir.display());
            steps.push(log_step("工作目录", false, &err));
            return (Err(err), steps);
        }
        steps.push(log_step(
            "工作目录",
            true,
            dir.display().to_string(),
        ));
    } else {
        steps.push(log_step("工作目录", true, "继承当前进程"));
    }

    steps.push(log_step(
        "超时",
        true,
        if timeout_sec == 0 {
            "不限时".into()
        } else {
            format!("{timeout_sec}s")
        },
    ));

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(ref dir) = cwd {
        cmd.current_dir(dir);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let err = format!("启动脚本失败: {e}");
            steps.push(log_step("启动脚本", false, &err));
            return (Err(err), steps);
        }
    };
    steps.push(log_step(
        "启动脚本",
        true,
        format!("pid={}", child.id()),
    ));

    let output_result = if timeout_sec == 0 {
        child
            .wait_with_output()
            .map_err(|e| format!("等待脚本结束失败: {e}"))
    } else {
        wait_output_with_timeout(&mut child, timeout_sec)
    };

    let output = match output_result {
        Ok(o) => o,
        Err(err) => {
            steps.push(log_step("执行", false, &err));
            return (Err(err), steps);
        }
    };

    let stdout_raw = String::from_utf8_lossy(&output.stdout);
    let stderr_raw = String::from_utf8_lossy(&output.stderr);
    let (stdout, _stdout_cut) = truncate_output(&stdout_raw);
    let (stderr, _stderr_cut) = truncate_output(&stderr_raw);

    if !stdout.is_empty() {
        steps.push(log_step("stdout", true, stdout.clone()));
    } else {
        steps.push(log_step("stdout", true, "(空)"));
    }
    if !stderr.is_empty() {
        // 有 stderr 不一定失败；仅在进程失败时标 fail
        steps.push(log_step(
            "stderr",
            output.status.success(),
            stderr.clone(),
        ));
    } else {
        steps.push(log_step("stderr", true, "(空)"));
    }

    if output.status.success() {
        let preview = if stdout.is_empty() {
            format!("脚本执行成功 ({runtime})")
        } else {
            let short: String = stdout.chars().take(120).collect();
            format!("脚本执行成功: {short}")
        };
        steps.push(log_step("退出码", true, "0"));
        (Ok(preview), steps)
    } else {
        let code = output.status.code();
        let detail = if !stderr.is_empty() {
            stderr.chars().take(200).collect::<String>()
        } else {
            stdout.chars().take(200).collect::<String>()
        };
        let err = format!("脚本退出码 {code:?}：{detail}");
        steps.push(log_step("退出码", false, format!("{code:?}")));
        (Err(err), steps)
    }
}

fn resolve_working_dir(working_dir: &str, script_path: &Path) -> Option<PathBuf> {
    let wd = working_dir.trim();
    if !wd.is_empty() {
        return Some(PathBuf::from(wd));
    }
    script_path.parent().map(|p| p.to_path_buf())
}

fn wait_output_with_timeout(
    child: &mut std::process::Child,
    timeout_sec: u32,
) -> Result<std::process::Output, String> {
    let deadline = Instant::now() + Duration::from_secs(timeout_sec as u64);

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();

    // 在独立线程读管道，避免阻塞；主线程轮询超时。
    let stdout_handle = stdout.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_handle = stderr.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(h) = stdout_handle {
                    if let Ok(b) = h.join() {
                        stdout_buf = b;
                    }
                }
                if let Some(h) = stderr_handle {
                    if let Ok(b) = h.join() {
                        stderr_buf = b;
                    }
                }
                return Ok(std::process::Output {
                    status,
                    stdout: stdout_buf,
                    stderr: stderr_buf,
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    // 丢弃读线程
                    if let Some(h) = stdout_handle {
                        let _ = h.join();
                    }
                    if let Some(h) = stderr_handle {
                        let _ = h.join();
                    }
                    return Err(format!("脚本执行超时（{timeout_sec}s），已终止"));
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("探测脚本状态失败: {e}")),
        }
    }
}

/// 解析带引号的命令行参数（兼容空格路径）。
fn split_command_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            '\\' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::{split_command_args, truncate_output};

    #[test]
    fn splits_quoted_args() {
        let args = split_command_args(r#"--foo "bar baz" -n"#);
        assert_eq!(args, vec!["--foo", "bar baz", "-n"]);
    }

    #[test]
    fn truncates_long_output() {
        let long: String = "a".repeat(5000);
        let (out, cut) = truncate_output(&long);
        assert!(cut);
        assert!(out.contains("已截断"));
    }
}

