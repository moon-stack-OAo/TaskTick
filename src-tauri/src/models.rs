use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Success,
    Failed,
    Running,
    Skipped,
}

/// 动作失败时的任务级策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnActionFail {
    Stop,
    Continue,
}

impl Default for OnActionFail {
    fn default() -> Self {
        Self::Stop
    }
}

/// 打开应用的等待策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WaitMode {
    FireAndForget,
    WaitExit,
}

impl Default for WaitMode {
    fn default() -> Self {
        Self::FireAndForget
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    Once {
        #[serde(default)]
        datetime: String,
    },
    Daily {
        time: String,
        #[serde(default = "default_weekdays")]
        weekdays: Vec<u8>,
    },
    Cron {
        expr: String,
        #[serde(default)]
        note: String,
    },
    Interval {
        every: u32,
        #[serde(default = "default_interval_unit")]
        unit: String,
    },
}

fn default_weekdays() -> Vec<u8> {
    vec![1, 2, 3, 4, 5]
}

fn default_interval_unit() -> String {
    "分钟".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    #[serde(rename = "open_app")]
    OpenApp {
        id: String,
        path: String,
        #[serde(default)]
        args: String,
        #[serde(rename = "waitMode", default)]
        wait_mode: WaitMode,
        /// wait_exit 时的超时秒数；超时记失败。默认 60；0 表示不限时。
        #[serde(rename = "timeoutSec", default = "default_open_app_timeout_sec")]
        timeout_sec: u32,
    },
    #[serde(rename = "open_url")]
    OpenUrl {
        id: String,
        url: String,
    },
    #[serde(rename = "run_script")]
    RunScript {
        id: String,
        #[serde(default = "default_runtime")]
        runtime: String,
        path: String,
        /// 工作目录；空则使用脚本所在目录。
        #[serde(rename = "workingDir", default)]
        working_dir: String,
        /// 超时秒数；默认 60；0 表示不限时。
        #[serde(rename = "timeoutSec", default = "default_script_timeout_sec")]
        timeout_sec: u32,
    },
    #[serde(rename = "wecom_ui_dm")]
    WecomUiDm {
        id: String,
        contact: String,
        message: String,
        #[serde(rename = "launchWecom", default = "default_true")]
        launch_wecom: bool,
        #[serde(rename = "timeoutSec", default = "default_timeout_sec")]
        timeout_sec: u32,
        #[serde(rename = "retryCount", default)]
        retry_count: u32,
        #[serde(rename = "notifyOnFail", default = "default_true")]
        notify_on_fail: bool,
        /// 发送成功后关闭/最小化企微主窗口（归还桌面）。
        #[serde(rename = "closeAfterSend", default = "default_true")]
        close_after_send: bool,
    },
}

fn default_runtime() -> String {
    "ps1".to_string()
}

fn default_timeout_sec() -> u32 {
    30
}

fn default_script_timeout_sec() -> u32 {
    60
}

fn default_open_app_timeout_sec() -> u32 {
    60
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub actions: Vec<Action>,
    /// 某一动作失败后：stop=中止后续；continue=继续执行。默认 stop。
    #[serde(default)]
    pub on_action_fail: OnActionFail,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub last_status: Option<RunStatus>,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// 下次预计触发时间（本地时区 `YYYY-MM-DD HH:MM:SS`）；计算字段，不持久化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStep {
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionLog {
    pub id: String,
    pub task_id: String,
    pub task_name: String,
    pub time: String,
    pub status: RunStatus,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub steps: Vec<LogStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStore {
    pub version: u32,
    pub tasks: Vec<Task>,
    pub logs: Vec<ExecutionLog>,
    #[serde(default)]
    pub settings: crate::settings::AppSettings,
}

impl Default for AppStore {
    fn default() -> Self {
        Self {
            version: 1,
            tasks: Vec::new(),
            logs: Vec::new(),
            settings: crate::settings::AppSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreInfo {
    pub data_dir: String,
    pub store_file: String,
    pub task_count: usize,
    pub log_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerStatus {
    pub running: bool,
    pub enabled_task_count: usize,
    pub running_task_count: usize,
    pub last_tick_at: Option<String>,
    pub next_check_at: Option<String>,
    pub tick_interval_secs: u64,
    #[serde(default)]
    pub paused: bool,
}

/// 企微预检：定位窗口结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WecomProbeResult {
    pub ok: bool,
    pub note: String,
    pub steps: Vec<LogStep>,
}

/// 企微预检：试发送结果（不写正式任务日志）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WecomTrySendResult {
    pub ok: bool,
    pub note: String,
    pub steps: Vec<LogStep>,
    pub notified: bool,
}
