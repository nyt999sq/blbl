use anyhow::{anyhow, Result};
use crate::buy::TicketInfo;
use crate::share::{current_unix_secs, SharePresetRecord, SharePresetStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
#[cfg(feature = "desktop")]
use tauri::api::path::app_config_dir;
#[cfg(feature = "desktop")]
use tauri::Config;

static DATA_DIR_OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static SHARE_PRESETS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static TASKS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn data_dir_override() -> &'static Mutex<Option<PathBuf>> {
    DATA_DIR_OVERRIDE.get_or_init(|| Mutex::new(None))
}

fn share_presets_lock() -> &'static Mutex<()> {
    SHARE_PRESETS_LOCK.get_or_init(|| Mutex::new(()))
}

fn tasks_lock() -> &'static Mutex<()> {
    TASKS_LOCK.get_or_init(|| Mutex::new(()))
}

pub fn set_data_dir(path: Option<PathBuf>) {
    if let Ok(mut slot) = data_dir_override().lock() {
        *slot = path;
    }
}

fn get_storage_root() -> PathBuf {
    if let Ok(slot) = data_dir_override().lock() {
        if let Some(path) = slot.clone() {
            if !path.exists() {
                let _ = fs::create_dir_all(&path);
            }
            return path;
        }
    }

    #[cfg(feature = "desktop")]
    let path = app_config_dir(&Config::default()).unwrap_or_else(|| PathBuf::from("."));

    #[cfg(not(feature = "desktop"))]
    let path = PathBuf::from("./data");

    if !path.exists() {
        let _ = fs::create_dir_all(&path);
    }
    path
}

// 获取通用的存储路径
fn get_storage_path(file_name: &str) -> PathBuf {
    let mut path = get_storage_root();
    path.push(file_name);
    path
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Account {
    pub uid: String,
    pub name: String,
    pub face: String,
    pub cookies: Vec<String>,
    #[serde(default)]
    pub level: i32,
    #[serde(default)]
    pub is_vip: bool,
    #[serde(default)]
    pub coins: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryItem {
    pub order_id: String,
    pub project_name: String,
    pub price: u32,
    pub time: String,
    pub pay_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectConfig {
    pub project_id: String,
    pub project_name: String,
    pub screen_id: String,
    pub screen_name: String,
    pub sku_id: String,
    pub sku_name: String,
    pub price: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    Manual,
    ShareSubmission,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Scheduled,
    Running,
    Success,
    Stopped,
    Failed,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskArgs {
    pub ticket_info: TicketInfo,
    pub interval: u64,
    pub mode: u32,
    pub total_attempts: u32,
    pub time_start: Option<String>,
    pub proxy: Option<String>,
    pub time_offset: Option<f64>,
    pub ntp_server: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub source: TaskSource,
    pub status: TaskStatus,
    pub project: String,
    pub screen: String,
    pub sku: String,
    pub buyer_count: u32,
    #[serde(default)]
    pub buyers: Vec<Value>,
    pub account_name: String,
    #[serde(default)]
    pub linked_share_preset_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub last_log: String,
    #[serde(default)]
    pub logs: Vec<String>,
    #[serde(default)]
    pub payment_url: String,
    pub args: TaskArgs,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub id: String,
    pub source: TaskSource,
    pub status: TaskStatus,
    pub project: String,
    pub screen: String,
    pub sku: String,
    pub buyer_count: u32,
    pub buyers: Vec<Value>,
    pub account_name: String,
    pub linked_share_preset_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub time_start: Option<String>,
    pub last_log: String,
    pub logs: Vec<String>,
    pub payment_url: String,
}

pub struct TaskCreateInput {
    pub id: String,
    pub source: TaskSource,
    pub project: String,
    pub screen: String,
    pub sku: String,
    pub account_name: String,
    pub linked_share_preset_id: Option<String>,
    pub args: TaskArgs,
    pub initial_status: TaskStatus,
    pub created_at: i64,
    pub initial_log: Option<String>,
}

impl TaskRecord {
    pub fn summary(&self) -> TaskSummary {
        TaskSummary {
            id: self.id.clone(),
            source: self.source.clone(),
            status: self.status.clone(),
            project: self.project.clone(),
            screen: self.screen.clone(),
            sku: self.sku.clone(),
            buyer_count: self.buyer_count,
            buyers: self.buyers.clone(),
            account_name: self.account_name.clone(),
            linked_share_preset_id: self.linked_share_preset_id.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            started_at: self.started_at,
            time_start: self.args.time_start.clone(),
            last_log: self.last_log.clone(),
            logs: self.logs.clone(),
            payment_url: self.payment_url.clone(),
        }
    }
}

pub fn build_task_record(input: TaskCreateInput) -> TaskRecord {
    let buyers = input
        .args
        .ticket_info
        .buyer_info
        .as_array()
        .cloned()
        .unwrap_or_default();
    let buyer_count = if input.args.ticket_info.count > 0 {
        input.args.ticket_info.count
    } else {
        buyers.len() as u32
    };
    let initial_log = input.initial_log.unwrap_or_else(|| "Ready to start".to_string());
    TaskRecord {
        id: input.id,
        source: input.source,
        status: input.initial_status.clone(),
        project: input.project,
        screen: input.screen,
        sku: input.sku,
        buyer_count,
        buyers,
        account_name: input.account_name,
        linked_share_preset_id: input.linked_share_preset_id,
        created_at: input.created_at,
        updated_at: input.created_at,
        started_at: matches!(input.initial_status, TaskStatus::Running | TaskStatus::Scheduled)
            .then_some(input.created_at),
        last_log: initial_log.clone(),
        logs: if initial_log.is_empty() {
            vec![]
        } else {
            vec![initial_log]
        },
        payment_url: String::new(),
        args: input.args,
    }
}

fn normalize_task_status_from_legacy(value: &str) -> TaskStatus {
    match value {
        "pending" => TaskStatus::Pending,
        "scheduled" => TaskStatus::Scheduled,
        "running" => TaskStatus::Running,
        "success" => TaskStatus::Success,
        "failed" => TaskStatus::Failed,
        "stopped" => TaskStatus::Stopped,
        _ => TaskStatus::Stopped,
    }
}

fn task_status_to_share_submission_status(status: &TaskStatus) -> String {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Scheduled => "scheduled",
        TaskStatus::Running => "running",
        TaskStatus::Success => "success",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Failed => "failed",
    }
    .to_string()
}

fn maybe_mark_task_stopped_after_restart(task: &mut TaskRecord, now: i64) -> bool {
    if matches!(task.status, TaskStatus::Running | TaskStatus::Scheduled) {
        task.status = TaskStatus::Stopped;
        task.updated_at = now;
        task.last_log = "服务重启后任务未自动恢复，请手动重新启动".to_string();
        task.logs
            .push("服务重启后任务未自动恢复，请手动重新启动".to_string());
        if task.logs.len() > 200 {
            let overflow = task.logs.len() - 200;
            task.logs.drain(0..overflow);
        }
        return true;
    }
    false
}

pub fn get_accounts() -> Result<Vec<Account>> {
    let path = get_storage_path("accounts.json");
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let accounts: Vec<Account> = serde_json::from_str(&content).unwrap_or_default();
        Ok(accounts)
    } else {
        Ok(vec![])
    }
}

pub fn save_accounts(accounts: &Vec<Account>) -> Result<()> {
    let path = get_storage_path("accounts.json");
    let json = serde_json::to_string_pretty(accounts)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn get_history() -> Result<Vec<HistoryItem>> {
    let path = get_storage_path("history.json");
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let history: Vec<HistoryItem> = serde_json::from_str(&content).unwrap_or_default();
        Ok(history)
    } else {
        Ok(vec![])
    }
}

pub fn add_history_item(item: HistoryItem) -> Result<()> {
    let path = get_storage_path("history.json");
    let mut history = get_history()?;
    history.insert(0, item);
    let json = serde_json::to_string_pretty(&history)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn clear_history() -> Result<()> {
    let path = get_storage_path("history.json");
    fs::write(path, "[]")?;
    Ok(())
}

pub fn get_project_history() -> Result<Vec<ProjectConfig>> {
    let path = get_storage_path("project_history.json");
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let history: Vec<ProjectConfig> = serde_json::from_str(&content).unwrap_or_default();
        Ok(history)
    } else {
        Ok(vec![])
    }
}

pub fn add_project_history(item: ProjectConfig) -> Result<()> {
    let path = get_storage_path("project_history.json");
    let mut history = get_project_history()?;

    if item.sku_id.is_empty() {
        history.retain(|p| !(p.project_id == item.project_id && p.sku_id.is_empty()));
        let has_specific = history
            .iter()
            .any(|p| p.project_id == item.project_id && !p.sku_id.is_empty());
        if !has_specific {
            history.insert(0, item);
        }
    } else {
        history.retain(|p| p.sku_id != item.sku_id);
        history.retain(|p| !(p.project_id == item.project_id && p.sku_id.is_empty()));
        history.insert(0, item);
    }

    if history.len() > 100 {
        history.truncate(100);
    }

    let json = serde_json::to_string_pretty(&history)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn remove_project_history_item(project_id: String, sku_id: String) -> Result<()> {
    let path = get_storage_path("project_history.json");
    let mut history = get_project_history()?;
    history.retain(|p| !(p.project_id == project_id && p.sku_id == sku_id));
    let json = serde_json::to_string_pretty(&history)?;
    fs::write(path, json)?;
    Ok(())
}

fn get_share_presets_unlocked() -> Result<Vec<SharePresetRecord>> {
    let path = get_storage_path("share_presets.json");
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let presets: Vec<SharePresetRecord> = serde_json::from_str(&content).unwrap_or_default();
        Ok(presets)
    } else {
        Ok(vec![])
    }
}

fn save_share_presets_unlocked(presets: &Vec<SharePresetRecord>) -> Result<()> {
    let path = get_storage_path("share_presets.json");
    let json = serde_json::to_string_pretty(presets)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn get_share_presets() -> Result<Vec<SharePresetRecord>> {
    let _guard = share_presets_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("share presets lock poisoned"))?;
    get_share_presets_unlocked()
}

pub fn save_share_presets(presets: &Vec<SharePresetRecord>) -> Result<()> {
    let _guard = share_presets_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("share presets lock poisoned"))?;
    save_share_presets_unlocked(presets)
}

pub fn with_share_presets_mut<T, F>(mutator: F) -> Result<T>
where
    F: FnOnce(&mut Vec<SharePresetRecord>) -> Result<T>,
{
    let _guard = share_presets_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("share presets lock poisoned"))?;
    let mut presets = get_share_presets_unlocked()?;
    let result = mutator(&mut presets)?;
    save_share_presets_unlocked(&presets)?;
    Ok(result)
}

fn get_tasks_unlocked() -> Result<Vec<TaskRecord>> {
    let path = get_storage_path("tasks.json");
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let tasks: Vec<TaskRecord> = serde_json::from_str(&content).unwrap_or_default();
        Ok(tasks)
    } else {
        Ok(vec![])
    }
}

fn save_tasks_unlocked(tasks: &Vec<TaskRecord>) -> Result<()> {
    let path = get_storage_path("tasks.json");
    let json = serde_json::to_string_pretty(tasks)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn get_tasks() -> Result<Vec<TaskRecord>> {
    let _guard = tasks_lock()
        .lock()
        .map_err(|_| anyhow!("tasks lock poisoned"))?;
    let mut tasks = get_tasks_unlocked()?;
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(tasks)
}

pub fn save_tasks(tasks: &Vec<TaskRecord>) -> Result<()> {
    let _guard = tasks_lock()
        .lock()
        .map_err(|_| anyhow!("tasks lock poisoned"))?;
    save_tasks_unlocked(tasks)
}

pub fn with_tasks_mut<T, F>(mutator: F) -> Result<T>
where
    F: FnOnce(&mut Vec<TaskRecord>) -> Result<T>,
{
    let _guard = tasks_lock()
        .lock()
        .map_err(|_| anyhow!("tasks lock poisoned"))?;
    let mut tasks = get_tasks_unlocked()?;
    let result = mutator(&mut tasks)?;
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    save_tasks_unlocked(&tasks)?;
    Ok(result)
}

pub fn create_task(task: TaskRecord) -> Result<TaskRecord> {
    with_tasks_mut(|tasks| {
        tasks.retain(|existing| existing.id != task.id);
        tasks.insert(0, task.clone());
        Ok(task)
    })
}

pub fn append_task_log(task_id: &str, message: &str) -> Result<()> {
    with_tasks_mut(|tasks| {
        let task = tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| anyhow!("task not found"))?;
        task.logs.push(message.to_string());
        if task.logs.len() > 200 {
            let overflow = task.logs.len() - 200;
            task.logs.drain(0..overflow);
        }
        task.last_log = message.to_string();
        task.updated_at = current_unix_secs();
        Ok(())
    })
}

pub fn update_task_after_start(task_id: &str, status: TaskStatus) -> Result<TaskRecord> {
    let task = with_tasks_mut(|tasks| {
        let task = tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| anyhow!("task not found"))?;
        task.status = status.clone();
        let now = current_unix_secs();
        task.updated_at = now;
        task.started_at = Some(now);
        task.last_log = if status == TaskStatus::Scheduled {
            if let Some(time_start) = task.args.time_start.clone() {
                format!("Waiting for {}", time_start)
            } else {
                "Waiting for scheduled start".to_string()
            }
        } else {
            "Starting...".to_string()
        };
        Ok(task.clone())
    })?;
    sync_share_preset_task_status(&task)?;
    Ok(task)
}

pub fn update_task_status(task_id: &str, status: TaskStatus, message: Option<&str>) -> Result<TaskRecord> {
    let task = with_tasks_mut(|tasks| {
        let task = tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| anyhow!("task not found"))?;
        task.status = status.clone();
        task.updated_at = current_unix_secs();
        if let Some(message) = message {
            task.last_log = message.to_string();
            task.logs.push(message.to_string());
            if task.logs.len() > 200 {
                let overflow = task.logs.len() - 200;
                task.logs.drain(0..overflow);
            }
        }
        Ok(task.clone())
    })?;
    sync_share_preset_task_status(&task)?;
    Ok(task)
}

pub fn update_task_payment_url(task_id: &str, url: &str) -> Result<TaskRecord> {
    let task = with_tasks_mut(|tasks| {
        let task = tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| anyhow!("task not found"))?;
        task.payment_url = url.to_string();
        task.status = TaskStatus::Success;
        task.updated_at = current_unix_secs();
        task.last_log = "抢票成功，请及时支付".to_string();
        Ok(task.clone())
    })?;
    sync_share_preset_task_status(&task)?;
    Ok(task)
}

pub fn delete_task(task_id: &str) -> Result<()> {
    with_tasks_mut(|tasks| {
        let before = tasks.len();
        tasks.retain(|task| task.id != task_id);
        if before == tasks.len() {
            return Err(anyhow!("task not found"));
        }
        Ok(())
    })
}

pub fn update_task_time_start(task_id: &str, time_start: Option<String>) -> Result<TaskRecord> {
    with_tasks_mut(|tasks| {
        let task = tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| anyhow!("task not found"))?;
        task.args.time_start = time_start
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        task.updated_at = current_unix_secs();
        task.last_log = "任务时间已更新".to_string();
        Ok(task.clone())
    })
}

pub fn sync_share_preset_task_status(task: &TaskRecord) -> Result<()> {
    let Some(share_preset_id) = task.linked_share_preset_id.clone() else {
        return Ok(());
    };

    let _share_guard = share_presets_lock()
        .lock()
        .map_err(|_| anyhow!("share presets lock poisoned"))?;
    let mut presets = get_share_presets_unlocked()?;
    if let Some(preset) = presets.iter_mut().find(|preset| preset.id == share_preset_id) {
        if let Some(last_submission) = preset.last_submission.as_mut() {
            last_submission.task_id = task.id.clone();
            last_submission.task_status = task_status_to_share_submission_status(&task.status);
        }
        save_share_presets_unlocked(&presets)?;
    }
    Ok(())
}

pub fn prepare_tasks_for_runtime() -> Result<()> {
    let _tasks_guard = tasks_lock()
        .lock()
        .map_err(|_| anyhow!("tasks lock poisoned"))?;
    let _share_guard = share_presets_lock()
        .lock()
        .map_err(|_| anyhow!("share presets lock poisoned"))?;

    let now = current_unix_secs();
    let mut tasks = get_tasks_unlocked()?;
    let mut presets = get_share_presets_unlocked()?;
    let mut changed_tasks = false;
    let mut changed_presets = false;
    let mut existing_ids = tasks.iter().map(|task| task.id.clone()).collect::<std::collections::HashSet<_>>();

    for preset in presets.iter_mut() {
        if preset.status == SharePresetStatus::Active
            && preset.expires_at.is_some_and(|expires_at| now > expires_at)
        {
            preset.status = SharePresetStatus::Expired;
            changed_presets = true;
        }

        let Some(last_submission) = preset.last_submission.clone() else {
            continue;
        };
        let Some(export) = preset.last_submission_export.clone() else {
            continue;
        };

        if !existing_ids.contains(&last_submission.task_id) {
            let task = build_task_record(TaskCreateInput {
                id: last_submission.task_id.clone(),
                source: TaskSource::ShareSubmission,
                project: preset.locked_task.project_name.clone(),
                screen: preset.locked_task.screen_name.clone(),
                sku: preset.locked_task.sku_name.clone(),
                account_name: last_submission.submitter_name.clone(),
                linked_share_preset_id: Some(preset.id.clone()),
                args: TaskArgs {
                    ticket_info: export.ticket_info,
                    interval: export.interval,
                    mode: export.mode,
                    total_attempts: export.total_attempts,
                    time_start: export.time_start.clone().filter(|value| !value.trim().is_empty()),
                    proxy: export.proxy.clone(),
                    time_offset: export.time_offset,
                    ntp_server: export.ntp_server.clone(),
                },
                initial_status: normalize_task_status_from_legacy(&last_submission.task_status),
                created_at: last_submission.submitted_at,
                initial_log: Some(format!("分享链接提交：{}", last_submission.submitter_name)),
            });
            existing_ids.insert(task.id.clone());
            tasks.push(task);
            changed_tasks = true;
        }
    }

    for task in tasks.iter_mut() {
        if maybe_mark_task_stopped_after_restart(task, now) {
            changed_tasks = true;
        }
        if let Some(preset) = task
            .linked_share_preset_id
            .as_ref()
            .and_then(|preset_id| presets.iter_mut().find(|preset| preset.id == *preset_id))
        {
            if let Some(last_submission) = preset.last_submission.as_mut() {
                let mapped_status = task_status_to_share_submission_status(&task.status);
                if last_submission.task_status != mapped_status || last_submission.task_id != task.id {
                    last_submission.task_status = mapped_status;
                    last_submission.task_id = task.id.clone();
                    changed_presets = true;
                }
            }
        }
    }

    if changed_tasks {
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        save_tasks_unlocked(&tasks)?;
    }
    if changed_presets {
        save_share_presets_unlocked(&presets)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::{
        hash_share_token, LockedTaskConfig, ShareDisplaySnapshot, SharePresetRecord,
        SharePresetStatus,
    };
    use crate::buy::TicketInfo;
    use serde_json::json;
    use std::sync::OnceLock;
    use uuid::Uuid;

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn make_test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bili-storage-test-{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn test_lock() -> &'static Mutex<()> {
        TEST_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn uses_data_dir_override_for_accounts() {
        let _guard = test_lock().lock().expect("test lock");
        let test_dir = make_test_dir();
        set_data_dir(Some(test_dir.clone()));

        let accounts = vec![Account {
            uid: "123".to_string(),
            name: "tester".to_string(),
            face: "".to_string(),
            cookies: vec!["a=b".to_string()],
            level: 1,
            is_vip: false,
            coins: 0.0,
        }];

        save_accounts(&accounts).expect("save accounts");
        let loaded = get_accounts().expect("load accounts");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].uid, "123");
        assert!(test_dir.join("accounts.json").exists());

        set_data_dir(None);
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn uses_data_dir_override_for_share_presets() {
        let _guard = test_lock().lock().expect("test lock");
        let test_dir = make_test_dir();
        set_data_dir(Some(test_dir.clone()));

        let presets = vec![SharePresetRecord {
            id: "preset-1".to_string(),
            token_hash: hash_share_token("token-1"),
            status: SharePresetStatus::Active,
            created_at: 1,
            expires_at: Some(2),
            creator_uid: Some("1".to_string()),
            creator_name: Some("tester".to_string()),
            title: Some("demo".to_string()),
            max_success_submissions: 1,
            success_submission_count: 0,
            locked_task: LockedTaskConfig {
                project_id: "project".to_string(),
                project_name: "项目".to_string(),
                screen_id: "screen".to_string(),
                screen_name: "场次".to_string(),
                sku_id: "sku".to_string(),
                sku_name: "票档".to_string(),
                count: 1,
                pay_money: 6800,
                is_hot_project: false,
                time_start: Some("2026-03-12 20:00:00".to_string()),
                interval: 1000,
                mode: 0,
                total_attempts: 10,
                proxy: None,
                ntp_server: None,
            },
            display_snapshot: ShareDisplaySnapshot {
                venue_name: Some("venue".to_string()),
                sale_start_text: Some("2026-03-12 20:00:00".to_string()),
                ticket_desc: "场次 - 票档".to_string(),
                price_text: "68.00".to_string(),
                locked_fields_text: vec!["票档已锁定".to_string()],
                tips: vec!["只需填写个人信息".to_string()],
            },
            last_submission: None,
            last_submission_export: None,
        }];

        save_share_presets(&presets).expect("save share presets");
        let loaded = get_share_presets().expect("load share presets");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "preset-1");
        assert!(test_dir.join("share_presets.json").exists());

        set_data_dir(None);
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn prepare_tasks_for_runtime_migrates_share_submission_and_stops_active_tasks() {
        let _guard = test_lock().lock().expect("test lock");
        let test_dir = make_test_dir();
        set_data_dir(Some(test_dir.clone()));

        let running_task = build_task_record(TaskCreateInput {
            id: "manual-task".to_string(),
            source: TaskSource::Manual,
            project: "项目A".to_string(),
            screen: "场次A".to_string(),
            sku: "票档A".to_string(),
            account_name: "tester".to_string(),
            linked_share_preset_id: None,
            args: TaskArgs {
                ticket_info: TicketInfo {
                    project_id: "p1".to_string(),
                    project_name: Some("项目A".to_string()),
                    screen_id: "s1".to_string(),
                    sku_id: "sku1".to_string(),
                    count: 1,
                    buyer_info: json!([{ "id": "b1", "name": "张三" }]),
                    deliver_info: json!({}),
                    cookies: vec!["SESSDATA=abc".to_string()],
                    is_hot_project: Some(false),
                    pay_money: Some(6800),
                    contact_name: Some("张三".to_string()),
                    contact_tel: Some("13800138000".to_string()),
                },
                interval: 1000,
                mode: 0,
                total_attempts: 10,
                time_start: Some("2026-03-12 20:00:00".to_string()),
                proxy: None,
                time_offset: None,
                ntp_server: None,
            },
            initial_status: TaskStatus::Scheduled,
            created_at: 10,
            initial_log: Some("ready".to_string()),
        });

        save_tasks(&vec![running_task]).expect("save tasks");

        let share_info = TicketInfo {
            project_id: "project".to_string(),
            project_name: Some("项目".to_string()),
            screen_id: "screen".to_string(),
            sku_id: "sku".to_string(),
            count: 1,
            buyer_info: json!([{ "id": "buyer-1", "name": "李四" }]),
            deliver_info: json!({}),
            cookies: vec!["SESSDATA=def".to_string()],
            is_hot_project: Some(false),
            pay_money: Some(8800),
            contact_name: Some("李四".to_string()),
            contact_tel: Some("13900139000".to_string()),
        };

        let presets = vec![SharePresetRecord {
            id: "preset-1".to_string(),
            token_hash: hash_share_token("token-1"),
            status: SharePresetStatus::Completed,
            created_at: 1,
            expires_at: None,
            creator_uid: Some("1".to_string()),
            creator_name: Some("tester".to_string()),
            title: Some("demo".to_string()),
            max_success_submissions: 1,
            success_submission_count: 1,
            locked_task: LockedTaskConfig {
                project_id: "project".to_string(),
                project_name: "项目".to_string(),
                screen_id: "screen".to_string(),
                screen_name: "场次".to_string(),
                sku_id: "sku".to_string(),
                sku_name: "票档".to_string(),
                count: 1,
                pay_money: 8800,
                is_hot_project: false,
                time_start: Some("2026-03-12 20:00:00".to_string()),
                interval: 1000,
                mode: 0,
                total_attempts: 10,
                proxy: None,
                ntp_server: None,
            },
            display_snapshot: ShareDisplaySnapshot {
                venue_name: None,
                sale_start_text: None,
                ticket_desc: "场次 - 票档".to_string(),
                price_text: "88.00".to_string(),
                locked_fields_text: vec![],
                tips: vec![],
            },
            last_submission: Some(crate::share::ShareSubmissionSummary {
                submitted_at: 20,
                submitter_uid: "u1".to_string(),
                submitter_name: "李四".to_string(),
                task_id: "share-task-1".to_string(),
                task_status: "running".to_string(),
                buyer_count: 1,
            }),
            last_submission_export: Some(crate::share::ShareSubmissionExportConfig {
                version: 1,
                source: "share_submission".to_string(),
                ticket_info: share_info,
                interval: 1000,
                mode: 0,
                total_attempts: 10,
                time_start: Some("2026-03-12 20:00:00".to_string()),
                proxy: None,
                time_offset: None,
                ntp_server: None,
            }),
        }];

        save_share_presets(&presets).expect("save share presets");
        prepare_tasks_for_runtime().expect("prepare tasks");

        let tasks = get_tasks().expect("get tasks");
        assert_eq!(tasks.len(), 2);
        let migrated = tasks
            .iter()
            .find(|task| task.id == "share-task-1")
            .expect("migrated share task");
        assert_eq!(migrated.source, TaskSource::ShareSubmission);
        assert_eq!(migrated.status, TaskStatus::Stopped);

        let stopped_manual = tasks
            .iter()
            .find(|task| task.id == "manual-task")
            .expect("manual task");
        assert_eq!(stopped_manual.status, TaskStatus::Stopped);

        let loaded_presets = get_share_presets().expect("get presets");
        assert_eq!(
            loaded_presets[0].last_submission.as_ref().map(|item| item.task_status.as_str()),
            Some("stopped")
        );

        set_data_dir(None);
        let _ = fs::remove_dir_all(test_dir);
    }
}
