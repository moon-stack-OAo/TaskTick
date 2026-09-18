use serde::{Deserialize, Serialize};

/// 错过触发后的补跑策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissedJobPolicy {
    /// 不补跑（默认，保持现状）。
    Skip,
    /// 启动或从暂停恢复后，对 lookback 窗口内最近一次应触发点补跑一次。
    RunOnce,
}

impl Default for MissedJobPolicy {
    fn default() -> Self {
        Self::Skip
    }
}

/// 企微空闲发送偏好。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WecomIdleSendSettings {
    /// 是否启用空闲队列；关闭则到点立刻抢前台发送（旧行为）。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 键盘鼠标空闲达到该秒数后才开始倒计时。
    #[serde(default = "default_idle_seconds")]
    pub idle_seconds: u32,
    /// 倒计时秒数；期间有输入则取消本次尝试。
    #[serde(default = "default_countdown_seconds")]
    pub countdown_seconds: u32,
}

/// C# FlaUI Agent 偏好。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WecomAgentSettings {
    /// 是否优先走 FlaUI Agent。
    #[serde(default)]
    pub enabled: bool,
    /// wecom-agent.exe 绝对路径；空则按文档自动查找。
    #[serde(default)]
    pub path: String,
    /// Agent 失败时是否回退内置键鼠方案。
    #[serde(default = "default_true")]
    pub fallback_to_input: bool,
}

impl Default for WecomAgentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            path: String::new(),
            fallback_to_input: true,
        }
    }
}

fn default_idle_seconds() -> u32 {
    30
}

fn default_countdown_seconds() -> u32 {
    5
}

impl Default for WecomIdleSendSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_seconds: 30,
            countdown_seconds: 5,
        }
    }
}

/// 应用偏好设置（持久化于 store.json 的 settings 段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// 是否期望开机自启（与 OS 注册表/启动项同步，以插件实际状态为准时可再读）。
    #[serde(default)]
    pub autostart: bool,
    /// 关闭主窗口时隐藏到托盘，而非退出进程。
    #[serde(default = "default_true")]
    pub minimize_to_tray_on_close: bool,
    /// 任务失败时是否允许弹出系统通知（总开关，优先于动作级 notifyOnFail）。
    #[serde(default = "default_true")]
    pub notify_on_task_fail: bool,
    /// 调度是否暂停（持久化；启动时恢复）。
    #[serde(default)]
    pub scheduler_paused: bool,
    /// 错过触发补跑策略。
    #[serde(default)]
    pub missed_job_policy: MissedJobPolicy,
    /// 企微空闲发送队列。
    #[serde(default)]
    pub wecom_idle_send: WecomIdleSendSettings,
    /// FlaUI Agent。
    #[serde(default)]
    pub wecom_agent: WecomAgentSettings,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            autostart: false,
            minimize_to_tray_on_close: true,
            notify_on_task_fail: true,
            scheduler_paused: false,
            missed_job_policy: MissedJobPolicy::Skip,
            wecom_idle_send: WecomIdleSendSettings::default(),
            wecom_agent: WecomAgentSettings::default(),
        }
    }
}

/// 前端读取的设置快照（含调度暂停态与通知权限提示）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub autostart: bool,
    pub minimize_to_tray_on_close: bool,
    pub notify_on_task_fail: bool,
    pub scheduler_paused: bool,
    pub missed_job_policy: MissedJobPolicy,
    pub wecom_idle_send: WecomIdleSendSettings,
    pub wecom_agent: WecomAgentSettings,
    /// 通知权限状态：granted / denied / prompt / unknown
    #[serde(default)]
    pub notification_permission: String,
    /// Agent 可执行文件是否已解析到（只读探测）。
    #[serde(default)]
    pub wecom_agent_resolved_path: Option<String>,
}

/// 部分更新请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub autostart: Option<bool>,
    pub minimize_to_tray_on_close: Option<bool>,
    pub notify_on_task_fail: Option<bool>,
    pub scheduler_paused: Option<bool>,
    pub missed_job_policy: Option<MissedJobPolicy>,
    pub wecom_idle_send: Option<WecomIdleSendSettingsPatch>,
    pub wecom_agent: Option<WecomAgentSettingsPatch>,
}

/// 企微空闲发送局部更新。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WecomIdleSendSettingsPatch {
    pub enabled: Option<bool>,
    pub idle_seconds: Option<u32>,
    pub countdown_seconds: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WecomAgentSettingsPatch {
    pub enabled: Option<bool>,
    pub path: Option<String>,
    pub fallback_to_input: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_safe() {
        let s = AppSettings::default();
        assert!(!s.autostart);
        assert!(s.minimize_to_tray_on_close);
        assert!(s.notify_on_task_fail);
        assert!(!s.scheduler_paused);
        assert_eq!(s.missed_job_policy, MissedJobPolicy::Skip);
        assert!(s.wecom_idle_send.enabled);
        assert_eq!(s.wecom_idle_send.idle_seconds, 30);
        assert_eq!(s.wecom_idle_send.countdown_seconds, 5);
        assert!(!s.wecom_agent.enabled);
        assert!(s.wecom_agent.fallback_to_input);
    }

    #[test]
    fn missed_job_policy_serde_snake_case() {
        let skip: MissedJobPolicy = serde_json::from_str("\"skip\"").unwrap();
        let once: MissedJobPolicy = serde_json::from_str("\"run_once\"").unwrap();
        assert_eq!(skip, MissedJobPolicy::Skip);
        assert_eq!(once, MissedJobPolicy::RunOnce);
        assert_eq!(serde_json::to_string(&skip).unwrap(), "\"skip\"");
    }

    #[test]
    fn settings_camel_case_roundtrip() {
        let raw = r#"{
            "autostart": true,
            "minimizeToTrayOnClose": false,
            "notifyOnTaskFail": false,
            "schedulerPaused": true,
            "missedJobPolicy": "run_once",
            "wecomIdleSend": { "enabled": false, "idleSeconds": 60, "countdownSeconds": 10 }
        }"#;
        let s: AppSettings = serde_json::from_str(raw).unwrap();
        assert!(s.autostart);
        assert!(!s.minimize_to_tray_on_close);
        assert!(!s.notify_on_task_fail);
        assert!(s.scheduler_paused);
        assert_eq!(s.missed_job_policy, MissedJobPolicy::RunOnce);
        assert!(!s.wecom_idle_send.enabled);
        assert_eq!(s.wecom_idle_send.idle_seconds, 60);
        let out = serde_json::to_value(&s).unwrap();
        assert_eq!(out["minimizeToTrayOnClose"], false);
        assert_eq!(out["missedJobPolicy"], "run_once");
        assert_eq!(out["wecomIdleSend"]["idleSeconds"], 60);
    }

    #[test]
    fn wecom_idle_send_defaults_when_missing() {
        let raw = r#"{
            "autostart": false,
            "minimizeToTrayOnClose": true,
            "notifyOnTaskFail": true,
            "schedulerPaused": false,
            "missedJobPolicy": "skip"
        }"#;
        let s: AppSettings = serde_json::from_str(raw).unwrap();
        assert!(s.wecom_idle_send.enabled);
        assert_eq!(s.wecom_idle_send.idle_seconds, 30);
    }
}
