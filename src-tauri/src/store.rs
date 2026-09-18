use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Local;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use std::str::FromStr;

use croner::Cron;

use crate::models::{AppStore, ExecutionLog, RunStatus, StoreInfo, Task};
use crate::settings::AppSettings;

const STORE_FILE_NAME: &str = "store.json";
const MAX_LOGS: usize = 500;
/// 更名前的数据目录 identifier（仅用于一次性迁移）。
const LEGACY_APP_IDENTIFIER: &str = "com.moon.autotask";

pub struct AppState {
    pub inner: Mutex<AppStore>,
    pub store_path: PathBuf,
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("无法解析应用数据目录: {e}"))?;
        maybe_migrate_legacy_data_dir(&data_dir);
        Self::load_from_dir(data_dir)
    }

    /// 从指定数据目录加载（测试与工具复用）。
    pub fn load_from_dir(data_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录失败: {e}"))?;

        let store_path = data_dir.join(STORE_FILE_NAME);
        let store = if store_path.exists() {
            read_store(&store_path)?
        } else {
            let empty = AppStore::default();
            write_store(&store_path, &empty)?;
            empty
        };

        Ok(Self {
            inner: Mutex::new(store),
            store_path,
            data_dir,
        })
    }

    fn with_store_mut<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&mut AppStore) -> Result<T, String>,
    {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "内部状态锁损坏".to_string())?;
        let result = f(&mut guard)?;
        write_store(&self.store_path, &guard)?;
        Ok(result)
    }

    fn with_store<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&AppStore) -> Result<T, String>,
    {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "内部状态锁损坏".to_string())?;
        f(&guard)
    }

    pub fn info(&self) -> Result<StoreInfo, String> {
        self.with_store(|store| {
            Ok(StoreInfo {
                data_dir: self.data_dir.display().to_string(),
                store_file: self.store_path.display().to_string(),
                task_count: store.tasks.len(),
                log_count: store.logs.len(),
            })
        })
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>, String> {
        self.with_store(|store| Ok(store.tasks.clone()))
    }

    /// 列出任务并附带 `nextRunAt`（本地时区预览；暂停时仍为「若恢复后」的下次时间）。
    pub fn list_tasks_with_schedule(
        &self,
        interval_last: &std::collections::HashMap<String, i64>,
    ) -> Result<Vec<Task>, String> {
        let mut tasks = self.list_tasks()?;
        let now = Local::now();
        for task in &mut tasks {
            task.next_run_at = crate::scheduler::compute_next_run_at(task, now, interval_last);
        }
        Ok(tasks)
    }

    pub fn get_task(&self, id: &str) -> Result<Option<Task>, String> {
        self.with_store(|store| Ok(store.tasks.iter().find(|t| t.id == id).cloned()))
    }

    pub fn save_task(&self, mut task: Task) -> Result<Task, String> {
        validate_task(&task)?;

        if task.id.trim().is_empty() {
            task.id = Uuid::new_v4().to_string();
        }
        if task.actions.is_empty() {
            return Err("任务至少需要一个动作".to_string());
        }

        task.updated_at = Some(now_string());

        self.with_store_mut(|store| {
            if let Some(existing) = store.tasks.iter_mut().find(|t| t.id == task.id) {
                // 保留运行态字段，避免编辑时被清空
                if task.last_run_at.is_none() {
                    task.last_run_at = existing.last_run_at.clone();
                }
                if task.last_status.is_none() {
                    task.last_status = existing.last_status.clone();
                }
                *existing = task.clone();
            } else {
                store.tasks.insert(0, task.clone());
            }
            Ok(task)
        })
    }

    pub fn delete_task(&self, id: &str) -> Result<bool, String> {
        self.with_store_mut(|store| {
            let before = store.tasks.len();
            store.tasks.retain(|t| t.id != id);
            Ok(store.tasks.len() != before)
        })
    }

    pub fn toggle_task(&self, id: &str, enabled: bool) -> Result<Task, String> {
        self.with_store_mut(|store| {
            let task = store
                .tasks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(|| format!("任务不存在: {id}"))?;
            task.enabled = enabled;
            task.updated_at = Some(now_string());
            Ok(task.clone())
        })
    }

    pub fn list_logs(&self, limit: Option<usize>) -> Result<Vec<ExecutionLog>, String> {
        self.with_store(|store| {
            let take = limit.unwrap_or(100).min(MAX_LOGS);
            Ok(store.logs.iter().take(take).cloned().collect())
        })
    }

    pub fn append_log(&self, mut log: ExecutionLog) -> Result<ExecutionLog, String> {
        if log.id.trim().is_empty() {
            log.id = Uuid::new_v4().to_string();
        }
        if log.time.trim().is_empty() {
            log.time = now_string();
        }

        self.with_store_mut(|store| {
            if let Some(task) = store.tasks.iter_mut().find(|t| t.id == log.task_id) {
                task.last_run_at = Some(log.time.clone());
                task.last_status = Some(log.status.clone());
            }
            store.logs.insert(0, log.clone());
            if store.logs.len() > MAX_LOGS {
                store.logs.truncate(MAX_LOGS);
            }
            Ok(log)
        })
    }

    pub fn clear_logs(&self) -> Result<usize, String> {
        self.with_store_mut(|store| {
            let count = store.logs.len();
            store.logs.clear();
            Ok(count)
        })
    }

    pub fn get_settings(&self) -> Result<AppSettings, String> {
        self.with_store(|store| Ok(store.settings.clone()))
    }

    pub fn update_settings<F>(&self, f: F) -> Result<AppSettings, String>
    where
        F: FnOnce(&mut AppSettings),
    {
        self.with_store_mut(|store| {
            f(&mut store.settings);
            Ok(store.settings.clone())
        })
    }

    pub fn reset_settings(&self) -> Result<AppSettings, String> {
        self.with_store_mut(|store| {
            store.settings = AppSettings::default();
            Ok(store.settings.clone())
        })
    }

    /// 导出任务（可选日志）为可序列化结构。
    pub fn export_payload(&self, include_logs: bool) -> Result<ExportPayload, String> {
        self.with_store(|store| {
            let mut tasks = store.tasks.clone();
            for t in &mut tasks {
                t.next_run_at = None;
            }
            Ok(ExportPayload {
                version: store.version.max(1),
                exported_at: now_string(),
                tasks,
                logs: if include_logs {
                    Some(store.logs.clone())
                } else {
                    None
                },
            })
        })
    }

    /// 导入任务：校验非法项跳过；返回汇总。
    pub fn import_tasks(
        &self,
        tasks: Vec<Task>,
        mode: ImportMode,
    ) -> Result<ImportResult, String> {
        let mut imported = 0usize;
        let mut updated = 0usize;
        let mut skipped = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut accepted: Vec<Task> = Vec::new();

        for (idx, mut task) in tasks.into_iter().enumerate() {
            let label = if task.name.trim().is_empty() {
                format!("第 {} 项", idx + 1)
            } else {
                format!("「{}」", task.name.trim())
            };
            task.next_run_at = None;
            if let Err(e) = validate_task(&task) {
                skipped += 1;
                errors.push(format!("{label}: {e}"));
                continue;
            }
            if task.actions.is_empty() {
                skipped += 1;
                errors.push(format!("{label}: 任务至少需要一个动作"));
                continue;
            }
            if task.id.trim().is_empty() {
                task.id = Uuid::new_v4().to_string();
            }
            if task.updated_at.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                task.updated_at = Some(now_string());
            }
            accepted.push(task);
        }

        // 替换模式下若全部校验失败，拒绝清空现有任务
        if matches!(mode, ImportMode::Replace) && accepted.is_empty() {
            return Ok(ImportResult {
                imported: 0,
                updated: 0,
                skipped,
                errors,
                task_count: self.with_store(|s| Ok(s.tasks.len()))?,
            });
        }

        self.with_store_mut(|store| {
            match mode {
                ImportMode::Replace => {
                    store.tasks = accepted;
                    imported = store.tasks.len();
                }
                ImportMode::Merge => {
                    for task in accepted {
                        if let Some(existing) = store.tasks.iter_mut().find(|t| t.id == task.id) {
                            let mut merged = task;
                            // 若导入未带运行态，保留本地
                            if merged.last_run_at.is_none() {
                                merged.last_run_at = existing.last_run_at.clone();
                            }
                            if merged.last_status.is_none() {
                                merged.last_status = existing.last_status.clone();
                            }
                            *existing = merged;
                            updated += 1;
                        } else {
                            store.tasks.insert(0, task);
                            imported += 1;
                        }
                    }
                }
            }
            Ok(())
        })?;

        Ok(ImportResult {
            imported,
            updated,
            skipped,
            errors,
            task_count: self.with_store(|s| Ok(s.tasks.len()))?,
        })
    }

    pub fn filter_logs(&self, filter: &LogFilter) -> Result<Vec<ExecutionLog>, String> {
        self.with_store(|store| {
            let mut out: Vec<ExecutionLog> = store
                .logs
                .iter()
                .filter(|log| log_matches(log, filter))
                .cloned()
                .collect();
            let take = filter.limit.unwrap_or(MAX_LOGS).min(MAX_LOGS);
            if out.len() > take {
                out.truncate(take);
            }
            Ok(out)
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPayload {
    pub version: u32,
    pub exported_at: String,
    pub tasks: Vec<Task>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs: Option<Vec<ExecutionLog>>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    Merge,
    Replace,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
    pub task_count: usize,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFilter {
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub keyword: Option<String>,
    pub time_from: Option<String>,
    pub time_to: Option<String>,
    pub limit: Option<usize>,
}

fn log_matches(log: &ExecutionLog, filter: &LogFilter) -> bool {
    if let Some(ref id) = filter.task_id {
        let id = id.trim();
        if !id.is_empty() && log.task_id != *id {
            return false;
        }
    }
    if let Some(ref status) = filter.status {
        let status = status.trim().to_ascii_lowercase();
        if !status.is_empty() && status != "all" {
            let ok = match log.status {
                RunStatus::Success => status == "success",
                RunStatus::Failed => status == "failed",
                RunStatus::Running => status == "running",
                RunStatus::Skipped => status == "skipped",
            };
            if !ok {
                return false;
            }
        }
    }
    if let Some(ref kw) = filter.keyword {
        let q = kw.trim().to_lowercase();
        if !q.is_empty() {
            let hay = format!(
                "{} {} {} {}",
                log.task_name,
                log.detail,
                log.time,
                log.task_id
            )
            .to_lowercase();
            if !hay.contains(&q) {
                return false;
            }
        }
    }
    if let Some(ref from) = filter.time_from {
        let from = from.trim();
        if !from.is_empty() && log.time.as_str() < from {
            return false;
        }
    }
    if let Some(ref to) = filter.time_to {
        let to = to.trim();
        if !to.is_empty() && log.time.as_str() > to {
            return false;
        }
    }
    true
}

/// 解析导入 JSON：支持完整导出包、store.json、或任务数组。
/// 返回 (可进一步校验的任务, 结构级跳过错误)。
pub fn parse_import_tasks(raw: &str) -> Result<(Vec<Task>, Vec<String>), String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("JSON 解析失败: {e}"))?;

    if let Some(arr) = value.as_array() {
        return Ok(parse_task_array(arr));
    }
    if let Some(obj) = value.as_object() {
        if let Some(tasks) = obj.get("tasks").and_then(|v| v.as_array()) {
            return Ok(parse_task_array(tasks));
        }
    }
    Err("无法识别导入格式：需要任务数组，或含 tasks 字段的对象".into())
}

fn parse_task_array(arr: &[serde_json::Value]) -> (Vec<Task>, Vec<String>) {
    let mut tasks = Vec::new();
    let mut errors = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        match serde_json::from_value::<Task>(item.clone()) {
            Ok(t) => tasks.push(t),
            Err(e) => errors.push(format!("第 {} 项任务结构无效: {e}", i + 1)),
        }
    }
    (tasks, errors)
}

/// 若新目录尚无 store，且旧 identifier 目录存在数据，则复制到新目录并保留旧目录。
fn maybe_migrate_legacy_data_dir(new_dir: &Path) {
    let new_store = new_dir.join(STORE_FILE_NAME);
    if new_store.exists() {
        return;
    }
    let Some(legacy_dir) = legacy_data_dir_beside(new_dir) else {
        return;
    };
    if !legacy_dir.exists() || legacy_dir == new_dir {
        return;
    }
    let legacy_store = legacy_dir.join(STORE_FILE_NAME);
    if !legacy_store.exists() {
        return;
    }
    if let Err(e) = fs::create_dir_all(new_dir) {
        eprintln!("[migrate] 创建新数据目录失败: {e}");
        return;
    }
    match copy_dir_recursive(&legacy_dir, new_dir) {
        Ok(n) => eprintln!(
            "[migrate] 已从 {} 迁移 {} 个文件到 {}（旧目录保留）",
            legacy_dir.display(),
            n,
            new_dir.display()
        ),
        Err(e) => eprintln!(
            "[migrate] 从 {} 迁移失败: {e}；将使用空数据目录",
            legacy_dir.display()
        ),
    }
}

fn legacy_data_dir_beside(new_dir: &Path) -> Option<PathBuf> {
    let parent = new_dir.parent()?;
    let leaf = new_dir.file_name()?.to_string_lossy();
    if leaf == LEGACY_APP_IDENTIFIER {
        return None;
    }
    Some(parent.join(LEGACY_APP_IDENTIFIER))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<usize, String> {
    let mut copied = 0usize;
    for entry in fs::read_dir(src).map_err(|e| format!("读取旧目录失败: {e}"))? {
        let entry = entry.map_err(|e| format!("遍历旧目录失败: {e}"))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry
            .file_type()
            .map_err(|e| format!("读取文件类型失败: {e}"))?;
        if ft.is_dir() {
            fs::create_dir_all(&to).map_err(|e| format!("创建子目录失败: {e}"))?;
            copied += copy_dir_recursive(&from, &to)?;
        } else if ft.is_file() {
            if to.exists() {
                continue;
            }
            fs::copy(&from, &to).map_err(|e| {
                format!(
                    "复制 {} -> {} 失败: {e}",
                    from.display(),
                    to.display()
                )
            })?;
            copied += 1;
        }
    }
    Ok(copied)
}

fn read_store(path: &Path) -> Result<AppStore, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("读取存储失败: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析存储失败: {e}"))
}

fn write_store(path: &Path, store: &AppStore) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let raw = serde_json::to_string_pretty(store).map_err(|e| format!("序列化存储失败: {e}"))?;
    fs::write(&tmp, raw).map_err(|e| format!("写入临时文件失败: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("提交存储文件失败: {e}"))?;
    Ok(())
}

fn now_string() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub(crate) fn validate_task(task: &Task) -> Result<(), String> {
    if task.name.trim().is_empty() {
        return Err("任务名称不能为空".to_string());
    }
    match &task.trigger {
        crate::models::Trigger::Once { datetime } if datetime.trim().is_empty() => {
            Err("一次性任务必须指定时间".to_string())
        }
        crate::models::Trigger::Daily { time, .. } if time.trim().is_empty() => {
            Err("每日任务必须指定时间".to_string())
        }
        crate::models::Trigger::Cron { expr, .. } => {
            let expr = expr.trim();
            if expr.is_empty() {
                return Err("Cron 表达式不能为空".to_string());
            }
            let fields = expr.split_whitespace().count();
            if fields != 5 {
                return Err(format!(
                    "Cron 须为 5 字段 POSIX（分 时 日 月 周），当前为 {fields} 段"
                ));
            }
            Cron::from_str(expr).map_err(|e| format!("Cron 表达式无效: {e}"))?;
            Ok(())
        }
        crate::models::Trigger::Interval { every, .. } if *every == 0 => {
            Err("间隔必须大于 0".to_string())
        }
        _ => Ok(()),
    }
}

#[allow(dead_code)]
pub fn status_label(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Success => "success",
        RunStatus::Failed => "failed",
        RunStatus::Running => "running",
        RunStatus::Skipped => "skipped",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Action, OnActionFail, Task, Trigger};

    fn open_url_action() -> Action {
        Action::OpenUrl {
            id: "a1".into(),
            url: "https://example.com".into(),
        }
    }

    fn make_task(id: &str, name: &str) -> Task {
        Task {
            id: id.into(),
            name: name.into(),
            enabled: true,
            trigger: Trigger::Daily {
                time: "09:00".into(),
                weekdays: vec![1, 2, 3, 4, 5],
            },
            actions: vec![open_url_action()],
            on_action_fail: OnActionFail::Stop,
            last_run_at: Some("2026-01-01 08:00:00".into()),
            last_status: Some(RunStatus::Success),
            updated_at: None,
            next_run_at: Some("should-be-cleared".into()),
        }
    }

    #[test]
    fn validate_cron_rejects_bad_field_count() {
        let mut t = make_task("t1", "cron");
        t.trigger = Trigger::Cron {
            expr: "* * * *".into(),
            note: String::new(),
        };
        let err = validate_task(&t).unwrap_err();
        assert!(err.contains("5 字段"));
    }

    #[test]
    fn validate_cron_rejects_invalid_expr() {
        let mut t = make_task("t1", "cron");
        t.trigger = Trigger::Cron {
            expr: "99 99 * * *".into(),
            note: String::new(),
        };
        assert!(validate_task(&t).is_err());
    }

    #[test]
    fn validate_cron_accepts_posix5() {
        let mut t = make_task("t1", "cron");
        t.trigger = Trigger::Cron {
            expr: "5 9 * * 1-5".into(),
            note: "工作日".into(),
        };
        assert!(validate_task(&t).is_ok());
    }

    #[test]
    fn validate_rejects_empty_name_and_zero_interval() {
        let mut t = make_task("t1", "  ");
        assert!(validate_task(&t).unwrap_err().contains("名称"));
        t.name = "ok".into();
        t.trigger = Trigger::Interval {
            every: 0,
            unit: "分钟".into(),
        };
        assert!(validate_task(&t).unwrap_err().contains("间隔"));
    }

    #[test]
    fn migrate_copies_legacy_store_and_keeps_old_dir() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join(LEGACY_APP_IDENTIFIER);
        let neu = root.path().join("com.moon.tasktick");
        fs::create_dir_all(&legacy).unwrap();
        let legacy_store = AppStore {
            tasks: vec![make_task("m1", "迁移任务")],
            ..AppStore::default()
        };
        write_store(&legacy.join(STORE_FILE_NAME), &legacy_store).unwrap();

        maybe_migrate_legacy_data_dir(&neu);
        assert!(neu.join(STORE_FILE_NAME).exists());
        assert!(legacy.join(STORE_FILE_NAME).exists());

        let state = AppState::load_from_dir(neu).unwrap();
        let tasks = state.list_tasks().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "迁移任务");
    }

    #[test]
    fn migrate_skips_when_new_store_exists() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join(LEGACY_APP_IDENTIFIER);
        let neu = root.path().join("com.moon.tasktick");
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&neu).unwrap();
        write_store(
            &legacy.join(STORE_FILE_NAME),
            &AppStore {
                tasks: vec![make_task("old", "旧")],
                ..AppStore::default()
            },
        )
        .unwrap();
        write_store(
            &neu.join(STORE_FILE_NAME),
            &AppStore {
                tasks: vec![make_task("new", "新")],
                ..AppStore::default()
            },
        )
        .unwrap();

        maybe_migrate_legacy_data_dir(&neu);
        let state = AppState::load_from_dir(neu).unwrap();
        let tasks = state.list_tasks().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "新");
    }

    #[test]
    fn parse_import_tasks_array_and_wrapped() {
        let arr = r#"[{"id":"x","name":"N","enabled":true,"trigger":{"type":"once","datetime":"2026-09-17 10:00"},"actions":[{"type":"open_url","id":"a","url":"https://a.com"}]}]"#;
        let (tasks, errs) = parse_import_tasks(arr).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(errs.is_empty());

        let wrapped = format!(r#"{{"version":1,"tasks":{arr}}}"#);
        let (tasks2, _) = parse_import_tasks(&wrapped).unwrap();
        assert_eq!(tasks2.len(), 1);

        assert!(parse_import_tasks("{}").is_err());
    }

    #[test]
    fn import_merge_preserves_local_run_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from_dir(dir.path().to_path_buf()).unwrap();
        let local = make_task("same-id", "本地");
        state.save_task(local).unwrap();

        let mut incoming = make_task("same-id", "导入覆盖名");
        incoming.last_run_at = None;
        incoming.last_status = None;
        let result = state
            .import_tasks(vec![incoming], ImportMode::Merge)
            .unwrap();
        assert_eq!(result.updated, 1);
        assert_eq!(result.imported, 0);

        let got = state.get_task("same-id").unwrap().unwrap();
        assert_eq!(got.name, "导入覆盖名");
        assert_eq!(got.last_run_at.as_deref(), Some("2026-01-01 08:00:00"));
        assert_eq!(got.last_status, Some(RunStatus::Success));
        assert!(got.next_run_at.is_none());
    }

    #[test]
    fn import_merge_adds_new_and_skips_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from_dir(dir.path().to_path_buf()).unwrap();
        state.save_task(make_task("keep", "保留")).unwrap();

        let mut bad = make_task("", "坏 cron");
        bad.trigger = Trigger::Cron {
            expr: "* * *".into(),
            note: String::new(),
        };
        let good = make_task("new1", "新任务");
        let result = state
            .import_tasks(vec![bad, good], ImportMode::Merge)
            .unwrap();
        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.task_count, 2);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn import_replace_empty_accepted_does_not_wipe() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from_dir(dir.path().to_path_buf()).unwrap();
        state.save_task(make_task("keep", "保留")).unwrap();

        let mut bad = make_task("x", "");
        bad.name = String::new();
        let result = state
            .import_tasks(vec![bad], ImportMode::Replace)
            .unwrap();
        assert_eq!(result.imported, 0);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.task_count, 1);
        assert!(state.get_task("keep").unwrap().is_some());
    }

    #[test]
    fn import_replace_overwrites_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from_dir(dir.path().to_path_buf()).unwrap();
        state.save_task(make_task("old", "旧")).unwrap();
        let result = state
            .import_tasks(vec![make_task("new", "新")], ImportMode::Replace)
            .unwrap();
        assert_eq!(result.imported, 1);
        assert_eq!(result.task_count, 1);
        assert!(state.get_task("old").unwrap().is_none());
        assert!(state.get_task("new").unwrap().is_some());
    }

    #[test]
    fn log_filter_by_status_and_keyword() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load_from_dir(dir.path().to_path_buf()).unwrap();
        state
            .append_log(ExecutionLog {
                id: "1".into(),
                task_id: "t1".into(),
                task_name: "晨间提醒".into(),
                time: "2026-09-17 09:00:00".into(),
                status: RunStatus::Failed,
                detail: "企微失败".into(),
                steps: vec![],
            })
            .unwrap();
        state
            .append_log(ExecutionLog {
                id: "2".into(),
                task_id: "t2".into(),
                task_name: "打开网页".into(),
                time: "2026-09-17 10:00:00".into(),
                status: RunStatus::Success,
                detail: "ok".into(),
                steps: vec![],
            })
            .unwrap();

        let failed = state
            .filter_logs(&LogFilter {
                status: Some("failed".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, "1");

        let kw = state
            .filter_logs(&LogFilter {
                keyword: Some("晨间".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(kw.len(), 1);
    }
}
