use anyhow::Result;
use crate::share::SharePresetRecord;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
#[cfg(feature = "desktop")]
use tauri::api::path::app_config_dir;
#[cfg(feature = "desktop")]
use tauri::Config;

static DATA_DIR_OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static SHARE_PRESETS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn data_dir_override() -> &'static Mutex<Option<PathBuf>> {
    DATA_DIR_OVERRIDE.get_or_init(|| Mutex::new(None))
}

fn share_presets_lock() -> &'static Mutex<()> {
    SHARE_PRESETS_LOCK.get_or_init(|| Mutex::new(()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::{
        hash_share_token, LockedTaskConfig, ShareDisplaySnapshot, SharePresetRecord,
        SharePresetStatus,
    };
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
        }];

        save_share_presets(&presets).expect("save share presets");
        let loaded = get_share_presets().expect("load share presets");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "preset-1");
        assert!(test_dir.join("share_presets.json").exists());

        set_data_dir(None);
        let _ = fs::remove_dir_all(test_dir);
    }
}
