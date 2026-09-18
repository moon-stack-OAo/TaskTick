//! C# FlaUI Agent 客户端（JSON Lines one-shot）。
//! 协议见 `docs/wecom-agent.md`。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::models::LogStep;
use crate::settings::WecomAgentSettings;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentRequest<'a> {
    id: String,
    cmd: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    launch_wecom: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_sec: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contact: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    close_after_send: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentResponse {
    #[allow(dead_code)]
    id: String,
    ok: bool,
    #[serde(default)]
    note: String,
    #[serde(default)]
    steps: Vec<LogStep>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentOutcome {
    pub ok: bool,
    pub note: String,
    pub steps: Vec<LogStep>,
    pub used_agent: bool,
}

/// 解析 Agent 可执行文件路径。
pub fn resolve_agent_path(settings: &WecomAgentSettings) -> Option<PathBuf> {
    let configured = settings.path.trim();
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        if p.is_file() {
            return Some(p);
        }
    }

    if let Ok(env) = std::env::var("AUTO_TASK_WECOM_AGENT") {
        let p = PathBuf::from(env.trim());
        if p.is_file() {
            return Some(p);
        }
    }

    // 相对仓库开发路径
    let rel = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tools")
        .join("wecom-agent")
        .join("bin")
        .join("Release")
        .join("net10.0-windows")
        .join("wecom-agent.exe");
    if rel.is_file() {
        return Some(rel);
    }
    // 兼容旧构建目录
    let rel8 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tools")
        .join("wecom-agent")
        .join("bin")
        .join("Release")
        .join("net8.0-windows")
        .join("wecom-agent.exe");
    if rel8.is_file() {
        return Some(rel8);
    }

    // 与当前 exe 同目录（打包分发）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("wecom-agent.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }

    None
}

pub fn ping(settings: &WecomAgentSettings) -> Result<AgentOutcome, String> {
    let path = resolve_agent_path(settings).ok_or_else(|| {
        "未找到 wecom-agent.exe。请先用 .NET 8 构建 tools/wecom-agent，或在设置中填写路径。"
            .to_string()
    })?;
    invoke_agent(
        &path,
        &AgentRequest {
            id: "ping".into(),
            cmd: "ping",
            launch_wecom: None,
            timeout_sec: None,
            retry_count: None,
            contact: None,
            message: None,
            close_after_send: None,
        },
        15,
    )
}

pub fn probe(
    settings: &WecomAgentSettings,
    launch_wecom: bool,
    timeout_sec: u32,
) -> Result<AgentOutcome, String> {
    let path = resolve_agent_path(settings).ok_or_else(|| missing_agent_msg())?;
    invoke_agent(
        &path,
        &AgentRequest {
            id: "probe".into(),
            cmd: "probe",
            launch_wecom: Some(launch_wecom),
            timeout_sec: Some(timeout_sec.max(5)),
            retry_count: None,
            contact: None,
            message: None,
            close_after_send: None,
        },
        timeout_sec.max(10) as u64 + 10,
    )
}

pub fn send(
    settings: &WecomAgentSettings,
    contact: &str,
    message: &str,
    launch_wecom: bool,
    timeout_sec: u32,
    retry_count: u32,
    close_after_send: bool,
) -> Result<AgentOutcome, String> {
    let path = resolve_agent_path(settings).ok_or_else(|| missing_agent_msg())?;
    invoke_agent(
        &path,
        &AgentRequest {
            id: "send".into(),
            cmd: "send",
            launch_wecom: Some(launch_wecom),
            timeout_sec: Some(timeout_sec.max(5)),
            retry_count: Some(retry_count),
            contact: Some(contact),
            message: Some(message),
            close_after_send: Some(close_after_send),
        },
        timeout_sec.max(15) as u64 + 20,
    )
}

fn missing_agent_msg() -> String {
    "未找到 wecom-agent.exe。构建：cd tools/wecom-agent && dotnet build -c Release；或设置 wecomAgent.path / 环境变量 AUTO_TASK_WECOM_AGENT。"
        .into()
}

fn invoke_agent(exe: &Path, req: &AgentRequest<'_>, timeout_secs: u64) -> Result<AgentOutcome, String> {
    let payload = serde_json::to_string(req).map_err(|e| format!("序列化 Agent 请求失败: {e}"))?;

    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 Agent 失败 ({}): {e}", exe.display()))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload.as_bytes())
            .map_err(|e| format!("写入 Agent stdin 失败: {e}"))?;
        stdin
            .write_all(b"\n")
            .map_err(|e| format!("写入 Agent stdin 换行失败: {e}"))?;
    }

    let timeout = Duration::from_secs(timeout_secs.max(5));
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("Agent 超时（{}s）", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(e) => return Err(format!("等待 Agent 失败: {e}")),
        }
    }

    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    let mut stderr = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }

    let line = stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .ok_or_else(|| {
            let tip = if stderr.trim().is_empty() {
                "无输出".to_string()
            } else {
                format!("stderr: {}", stderr.trim())
            };
            format!("Agent 无有效 JSON 响应（{tip}）")
        })?;

    let resp: AgentResponse = serde_json::from_str(line)
        .map_err(|e| format!("解析 Agent 响应失败: {e} · raw={line}"))?;

    let mut steps = resp.steps;
    steps.insert(
        0,
        LogStep {
            title: "FlaUI Agent".into(),
            status: if resp.ok {
                "ok".into()
            } else {
                "fail".into()
            },
            time: chrono::Local::now().format("%H:%M:%S").to_string(),
            note: format!("exe={}", exe.display()),
        },
    );

    Ok(AgentOutcome {
        ok: resp.ok,
        note: resp.note,
        steps,
        used_agent: true,
    })
}
