use crate::buy::TicketInfo;
use anyhow::{anyhow, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

static SHARE_SUBMIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn share_submit_lock() -> &'static Mutex<()> {
    SHARE_SUBMIT_LOCK.get_or_init(|| Mutex::new(()))
}

pub fn current_unix_secs() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(_) => 0,
    }
}

pub fn generate_share_token() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_share_token(token: &str) -> String {
    format!("{:x}", md5::compute(token.as_bytes()))
}

pub fn share_token_matches_hash(token: &str, expected_hash: &str) -> bool {
    hash_share_token(token) == expected_hash
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SharePresetStatus {
    Active,
    Completed,
    Expired,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockedTaskConfig {
    pub project_id: String,
    pub project_name: String,
    pub screen_id: String,
    pub screen_name: String,
    pub sku_id: String,
    pub sku_name: String,
    pub count: u32,
    pub pay_money: u32,
    #[serde(default)]
    pub is_hot_project: bool,
    pub time_start: Option<String>,
    pub interval: u64,
    pub mode: u32,
    pub total_attempts: u32,
    pub proxy: Option<String>,
    pub ntp_server: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShareDisplaySnapshot {
    pub venue_name: Option<String>,
    pub sale_start_text: Option<String>,
    pub ticket_desc: String,
    pub price_text: String,
    #[serde(default)]
    pub locked_fields_text: Vec<String>,
    #[serde(default)]
    pub tips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShareSubmissionSummary {
    pub submitted_at: i64,
    pub submitter_uid: String,
    pub submitter_name: String,
    pub task_id: String,
    pub task_status: String,
    pub buyer_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SharePresetRecord {
    pub id: String,
    pub token_hash: String,
    pub status: SharePresetStatus,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub creator_uid: Option<String>,
    pub creator_name: Option<String>,
    pub title: Option<String>,
    pub max_success_submissions: u32,
    #[serde(default)]
    pub success_submission_count: u32,
    pub locked_task: LockedTaskConfig,
    pub display_snapshot: ShareDisplaySnapshot,
    pub last_submission: Option<ShareSubmissionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShareSubmissionInput {
    pub cookies: Vec<String>,
    pub buyers: Vec<Value>,
    pub deliver_info: Option<Value>,
    pub contact_name: Option<String>,
    pub contact_tel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockedTaskPublicView {
    pub project_name: String,
    pub screen_name: String,
    pub sku_name: String,
    pub count: u32,
    pub pay_money: u32,
    pub time_start: Option<String>,
    pub mode: u32,
    pub interval: u64,
    pub total_attempts: u32,
}

impl LockedTaskConfig {
    pub fn public_view(&self) -> LockedTaskPublicView {
        LockedTaskPublicView {
            project_name: self.project_name.clone(),
            screen_name: self.screen_name.clone(),
            sku_name: self.sku_name.clone(),
            count: self.count,
            pay_money: self.pay_money,
            time_start: self.time_start.clone(),
            mode: self.mode,
            interval: self.interval,
            total_attempts: self.total_attempts,
        }
    }
}

pub fn effective_share_status(preset: &SharePresetRecord, now: i64) -> SharePresetStatus {
    if preset.status == SharePresetStatus::Active
        && preset.expires_at.is_some_and(|expires_at| now > expires_at)
    {
        return SharePresetStatus::Expired;
    }

    preset.status.clone()
}

pub fn normalize_share_preset_status(preset: &mut SharePresetRecord, now: i64) -> bool {
    let effective = effective_share_status(preset, now);
    if effective != preset.status {
        preset.status = effective;
        return true;
    }
    false
}

fn first_non_empty_string(candidates: &[Option<String>]) -> Option<String> {
    candidates
        .iter()
        .filter_map(|value| value.as_ref())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

fn buyer_string_field(buyer: &Value, key: &str) -> Option<String> {
    buyer.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_contact_name(submission: &ShareSubmissionInput) -> Option<String> {
    if let Some(name) = first_non_empty_string(&[submission.contact_name.clone()]) {
        return Some(name);
    }

    submission.buyers.first().and_then(|buyer| {
        first_non_empty_string(&[
            buyer_string_field(buyer, "contact_name"),
            buyer_string_field(buyer, "name"),
        ])
    })
}

fn resolve_contact_tel(submission: &ShareSubmissionInput) -> Option<String> {
    if let Some(tel) = first_non_empty_string(&[submission.contact_tel.clone()]) {
        return Some(tel);
    }

    submission.buyers.first().and_then(|buyer| {
        first_non_empty_string(&[
            buyer_string_field(buyer, "contact_tel"),
            buyer_string_field(buyer, "tel"),
            buyer_string_field(buyer, "mobile"),
            buyer_string_field(buyer, "phone"),
        ])
    })
}

pub fn build_ticket_info_from_submission(
    locked_task: &LockedTaskConfig,
    submission: &ShareSubmissionInput,
) -> Result<TicketInfo> {
    if locked_task.count == 0 {
        return Err(anyhow!("分享任务未配置有效购票张数"));
    }

    if submission.buyers.len() != locked_task.count as usize {
        return Err(anyhow!("需要选择 {} 位购票人", locked_task.count));
    }

    if submission.cookies.is_empty() {
        return Err(anyhow!("缺少登录凭证，请重新扫码登录"));
    }

    let contact_name = resolve_contact_name(submission)
        .ok_or_else(|| anyhow!("请填写联系人姓名"))?;
    let contact_tel = resolve_contact_tel(submission)
        .ok_or_else(|| anyhow!("请填写可用手机号"))?;

    if contact_tel.contains('*') {
        return Err(anyhow!("请填写可用手机号"));
    }

    let deliver_info = submission
        .deliver_info
        .clone()
        .or_else(|| {
            submission
                .buyers
                .first()
                .and_then(|buyer| buyer.get("deliver_info").cloned())
        })
        .unwrap_or_else(|| serde_json::json!({}));

    Ok(TicketInfo {
        project_id: locked_task.project_id.clone(),
        project_name: Some(locked_task.project_name.clone()),
        screen_id: locked_task.screen_id.clone(),
        sku_id: locked_task.sku_id.clone(),
        count: locked_task.count,
        buyer_info: Value::Array(submission.buyers.clone()),
        deliver_info,
        cookies: submission.cookies.clone(),
        is_hot_project: Some(locked_task.is_hot_project),
        pay_money: Some(locked_task.pay_money),
        contact_name: Some(contact_name),
        contact_tel: Some(contact_tel),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn generated_token_matches_hash() {
        let token = generate_share_token();
        let hashed = hash_share_token(&token);
        assert!(share_token_matches_hash(&token, &hashed));
        assert!(!share_token_matches_hash("wrong-token", &hashed));
    }

    #[test]
    fn past_due_preset_becomes_expired() {
        let preset = SharePresetRecord {
            id: "preset-1".to_string(),
            token_hash: "hash".to_string(),
            status: SharePresetStatus::Active,
            created_at: 100,
            expires_at: Some(120),
            creator_uid: Some("1".to_string()),
            creator_name: Some("tester".to_string()),
            title: Some("demo".to_string()),
            max_success_submissions: 1,
            success_submission_count: 0,
            locked_task: LockedTaskConfig {
                project_id: "1".to_string(),
                project_name: "project".to_string(),
                screen_id: "2".to_string(),
                screen_name: "screen".to_string(),
                sku_id: "3".to_string(),
                sku_name: "sku".to_string(),
                count: 1,
                pay_money: 6800,
                is_hot_project: false,
                time_start: Some("2026-03-12 20:00:00".to_string()),
                interval: 1000,
                mode: 0,
                total_attempts: 10,
                proxy: None,
                ntp_server: Some("https://api.bilibili.com/x/report/click/now".to_string()),
            },
            display_snapshot: ShareDisplaySnapshot {
                venue_name: Some("venue".to_string()),
                sale_start_text: Some("2026-03-12 20:00:00".to_string()),
                ticket_desc: "demo".to_string(),
                price_text: "68.00".to_string(),
                locked_fields_text: vec!["场次已锁定".to_string()],
                tips: vec!["只需填写个人信息".to_string()],
            },
            last_submission: None,
        };

        assert_eq!(effective_share_status(&preset, 121), SharePresetStatus::Expired);
        assert_eq!(effective_share_status(&preset, 120), SharePresetStatus::Active);
    }

    #[test]
    fn locked_task_fields_are_rebuilt_server_side() {
        let locked_task = LockedTaskConfig {
            project_id: "project-1".to_string(),
            project_name: "项目 A".to_string(),
            screen_id: "screen-1".to_string(),
            screen_name: "第一场".to_string(),
            sku_id: "sku-1".to_string(),
            sku_name: "看台".to_string(),
            count: 2,
            pay_money: 6800,
            is_hot_project: true,
            time_start: Some("2026-03-12 20:00:00".to_string()),
            interval: 800,
            mode: 1,
            total_attempts: 5,
            proxy: Some("http://127.0.0.1:7890".to_string()),
            ntp_server: Some("ntp.aliyun.com".to_string()),
        };

        let submission = ShareSubmissionInput {
            cookies: vec!["SESSDATA=abc".to_string()],
            buyers: vec![
                json!({"id":"buyer-1","name":"张三","tel":"13800000001","project_id":"evil-project"}),
                json!({"id":"buyer-2","name":"李四","tel":"13800000002","sku_id":"evil-sku"}),
            ],
            deliver_info: Some(json!({"addr_id":"addr-1","addr":"上海市"})),
            contact_name: Some("联系人".to_string()),
            contact_tel: Some("13800000009".to_string()),
        };

        let ticket_info = build_ticket_info_from_submission(&locked_task, &submission).unwrap();

        assert_eq!(ticket_info.project_id, "project-1");
        assert_eq!(ticket_info.screen_id, "screen-1");
        assert_eq!(ticket_info.sku_id, "sku-1");
        assert_eq!(ticket_info.count, 2);
        assert_eq!(ticket_info.pay_money, Some(6800));
        assert_eq!(ticket_info.contact_tel.as_deref(), Some("13800000009"));
    }
}
