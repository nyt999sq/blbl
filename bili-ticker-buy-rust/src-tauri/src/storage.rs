use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
#[cfg(feature = "desktop")]
use tauri::api::path::app_config_dir;
#[cfg(feature = "desktop")]
use tauri::Config;

static DATA_DIR_OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

fn data_dir_override() -> &'static Mutex<Option<PathBuf>> {
    DATA_DIR_OVERRIDE.get_or_init(|| Mutex::new(None))
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bili-storage-test-{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn uses_data_dir_override_for_accounts() {
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
}
