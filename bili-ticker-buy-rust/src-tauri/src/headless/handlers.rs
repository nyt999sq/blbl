use crate::api;
use crate::auth;
use crate::auth_phone;
use crate::buy::{self, TicketInfo};
use crate::headless::auth as headless_auth;
use crate::headless::ws::WsEventSink;
use crate::headless::HeadlessState;
use crate::share::{
    build_share_submission_export_config, build_ticket_info_from_submission, current_unix_secs, generate_share_token,
    hash_share_token, normalize_share_preset_status, share_submit_lock, share_token_matches_hash,
    validate_share_preset_batch_delete, LockedTaskConfig, ShareDisplaySnapshot,
    SharePresetRecord, SharePresetStatus, ShareSubmissionInput, ShareSubmissionSummary,
};
use crate::storage::{
    self, Account, ProjectConfig, TaskArgs, TaskCreateInput, TaskRecord, TaskSource, TaskStatus,
    TaskSummary,
};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct ApiOk {
    pub ok: bool,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ApiError {
            error: message.into(),
        }),
    )
        .into_response()
}

fn share_state_error_response(
    status_code: StatusCode,
    status: SharePresetStatus,
    message: impl Into<String>,
) -> Response {
    (
        status_code,
        Json(json!({
            "error": message.into(),
            "status": status,
        })),
    )
        .into_response()
}

fn normalize_share_presets(presets: &mut Vec<SharePresetRecord>) -> bool {
    let now = current_unix_secs();
    let mut changed = false;
    for preset in presets.iter_mut() {
        changed |= normalize_share_preset_status(preset, now);
    }
    changed
}

fn find_share_preset_index_by_token(presets: &[SharePresetRecord], token: &str) -> Option<usize> {
    presets
        .iter()
        .position(|preset| share_token_matches_hash(token, &preset.token_hash))
}

fn active_share_preset_or_response(
    preset: &SharePresetRecord,
    now: i64,
) -> std::result::Result<(), Response> {
    match crate::share::effective_share_status(preset, now) {
        SharePresetStatus::Active => Ok(()),
        SharePresetStatus::Expired => Err(share_state_error_response(
            StatusCode::GONE,
            SharePresetStatus::Expired,
            "分享链接已过期",
        )),
        SharePresetStatus::Closed => Err(share_state_error_response(
            StatusCode::CONFLICT,
            SharePresetStatus::Closed,
            "分享链接已关闭",
        )),
        SharePresetStatus::Completed => Err(share_state_error_response(
            StatusCode::CONFLICT,
            SharePresetStatus::Completed,
            "该分享链接已被使用",
        )),
    }
}

fn merge_buyer_overrides(info: &mut TicketInfo, buyers: Option<Vec<Value>>) {
    if let Some(b) = buyers {
        if !b.is_empty() {
            info.buyer_info = Value::Array(b.clone());
            let contact_name_missing = info
                .contact_name
                .as_ref()
                .map(|s| s.is_empty())
                .unwrap_or(true);
            let contact_tel_missing = info
                .contact_tel
                .as_ref()
                .map(|s| s.is_empty())
                .unwrap_or(true);
            if contact_name_missing || contact_tel_missing {
                if let Some(first) = b.first() {
                    if contact_name_missing {
                        if let Some(name) = first["name"].as_str() {
                            if !name.is_empty() {
                                info.contact_name = Some(name.to_string());
                            }
                        }
                    }
                    if contact_tel_missing {
                        let tel = first["tel"]
                            .as_str()
                            .or(first["mobile"].as_str())
                            .or(first["phone"].as_str());
                        if let Some(t) = tel {
                            if !t.is_empty() && !t.contains('*') {
                                info.contact_tel = Some(t.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct SpawnTaskOptions {
    interval: u64,
    mode: u32,
    total_attempts: u32,
    time_start: Option<String>,
    proxy: Option<String>,
    time_offset: Option<f64>,
    ntp_server: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct SpawnTaskResult {
    task_id: String,
    task_status: String,
}

fn spawn_buy_task(
    state: &HeadlessState,
    info: TicketInfo,
    options: SpawnTaskOptions,
) -> SpawnTaskResult {
    spawn_buy_task_with_id(
        state,
        Uuid::new_v4().to_string(),
        info,
        options,
    )
}

fn spawn_buy_task_with_id(
    state: &HeadlessState,
    task_id: String,
    info: TicketInfo,
    options: SpawnTaskOptions,
) -> SpawnTaskResult {
    let stop_flag = Arc::new(AtomicBool::new(false));
    state
        .tasks
        .lock()
        .unwrap()
        .insert(task_id.clone(), stop_flag.clone());

    let task_id_clone = task_id.clone();
    let task_id_for_cleanup = task_id.clone();
    let runtime_tasks = state.tasks.clone();
    let sink = WsEventSink::new(state.ws_hub.clone());
    let task_status = if options.time_start.is_some() {
        "scheduled".to_string()
    } else {
        "running".to_string()
    };

    tokio::spawn(async move {
        if let Err(e) = buy::start_buy_task(
            sink,
            task_id_clone,
            stop_flag,
            info,
            options.interval,
            options.mode,
            options.total_attempts,
            options.time_start,
            options.proxy,
            options.time_offset,
            options.ntp_server,
        )
        .await
        {
            eprintln!("headless task error: {}", e);
        }
        runtime_tasks.lock().unwrap().remove(&task_id_for_cleanup);
    });

    SpawnTaskResult {
        task_id,
        task_status,
    }
}

pub async fn token_login(State(state): State<HeadlessState>, headers: HeaderMap) -> Response {
    if let Err((status, msg)) =
        headless_auth::validate_server_token(&headers, &state.server_token).await
    {
        return error_response(status, msg);
    }

    let session = headless_auth::issue_session(&state.sessions).await;
    (StatusCode::OK, Json(session)).into_response()
}

#[derive(Debug, Serialize)]
pub struct QrcodeResponse {
    pub url: String,
    pub qrcode_key: String,
}

pub async fn get_login_qrcode() -> Response {
    match auth::generate_qrcode().await {
        Ok((url, qrcode_key)) => {
            (StatusCode::OK, Json(QrcodeResponse { url, qrcode_key })).into_response()
        }
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct PollQuery {
    pub qrcode_key: String,
}

#[derive(Debug, Serialize)]
pub struct PollResponse {
    pub status: String,
    pub cookies: Option<Vec<String>>,
    pub message: Option<String>,
}

pub async fn poll_login_status(Query(query): Query<PollQuery>) -> Response {
    match auth::poll_login(&query.qrcode_key).await {
        Ok(raw) => {
            let cookies = serde_json::from_str::<Vec<String>>(&raw).ok();
            (
                StatusCode::OK,
                Json(PollResponse {
                    status: "success".to_string(),
                    cookies,
                    message: None,
                }),
            )
                .into_response()
        }
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn get_sms_login_countries() -> Response {
    match auth_phone::fetch_phone_countries().await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn get_sms_login_captcha() -> Response {
    match auth_phone::fetch_sms_captcha().await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn send_sms_login_code(
    Json(req): Json<auth_phone::SendSmsCodeRequest>,
) -> Response {
    match auth_phone::send_sms_code(req).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

pub async fn verify_sms_login_code(
    Json(req): Json<auth_phone::VerifySmsCodeRequest>,
) -> Response {
    match auth_phone::verify_sms_code(req).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ImportCookieRequest {
    pub cookies: Vec<String>,
}

pub async fn import_cookie(Json(payload): Json<ImportCookieRequest>) -> Response {
    let res = api::fetch_user_info(payload.cookies.clone()).await;
    let res = match res {
        Ok(v) => v,
        Err(e) => return error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };

    if res["code"].as_i64().unwrap_or(-1) != 0 {
        return error_response(StatusCode::BAD_REQUEST, "invalid cookies").into_response();
    }

    let data = &res["data"];
    let account = Account {
        uid: data["mid"].to_string(),
        name: data["uname"].as_str().unwrap_or("").to_string(),
        face: data["face"].as_str().unwrap_or("").to_string(),
        cookies: payload.cookies,
        level: data["level_info"]["current_level"].as_i64().unwrap_or(0) as i32,
        is_vip: data["vipStatus"].as_i64().unwrap_or(0) == 1,
        coins: data["money"].as_f64().unwrap_or(0.0),
    };

    let mut accounts = match storage::get_accounts() {
        Ok(v) => v,
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };
    accounts.retain(|a| a.uid != account.uid);
    accounts.push(account.clone());
    if let Err(e) = storage::save_accounts(&accounts) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    (StatusCode::OK, Json(account)).into_response()
}

pub async fn get_accounts() -> Response {
    match storage::get_accounts() {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_account(Path(uid): Path<String>) -> Response {
    let mut accounts = match storage::get_accounts() {
        Ok(v) => v,
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };
    accounts.retain(|a| a.uid != uid);
    if let Err(e) = storage::save_accounts(&accounts) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    (StatusCode::OK, Json(ApiOk { ok: true })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct FetchProjectRequest {
    pub id: String,
}

pub async fn fetch_project(Json(req): Json<FetchProjectRequest>) -> Response {
    match api::fetch_project_info(req.id).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct BuyerRequest {
    pub project_id: String,
    pub cookies: Vec<String>,
}

pub async fn fetch_buyers(Json(req): Json<BuyerRequest>) -> Response {
    match api::fetch_buyers(req.project_id, req.cookies).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct AddressRequest {
    pub cookies: Vec<String>,
}

pub async fn fetch_addresses(Json(req): Json<AddressRequest>) -> Response {
    match api::fetch_address_list(req.cookies).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UserInfoRequest {
    pub cookies: Vec<String>,
}

pub async fn get_user_info(Json(req): Json<UserInfoRequest>) -> Response {
    match api::fetch_user_info(req.cookies).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct SyncTimeRequest {
    pub server_url: Option<String>,
}

pub async fn sync_time(Json(req): Json<SyncTimeRequest>) -> Response {
    let url = req
        .server_url
        .unwrap_or_else(|| "https://api.bilibili.com/x/report/click/now".to_string());

    let server_time = if url.starts_with("http") {
        match api::get_server_time(Some(url)).await {
            Ok(t) => t,
            Err(e) => {
                return error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response()
            }
        }
    } else {
        match api::get_ntp_time(&url) {
            Ok(t) => t as i64,
            Err(e) => {
                return error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response()
            }
        }
    };

    let local_time = api::get_local_time();
    let diff = server_time - local_time;
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "diff": diff,
            "server": server_time,
            "local": local_time
        })),
    )
        .into_response()
}

fn normalized_time_start(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_task_args(
    info: TicketInfo,
    interval: u64,
    mode: u32,
    total_attempts: u32,
    time_start: Option<String>,
    proxy: Option<String>,
    time_offset: Option<f64>,
    ntp_server: Option<String>,
) -> TaskArgs {
    TaskArgs {
        ticket_info: info,
        interval,
        mode,
        total_attempts,
        time_start: normalized_time_start(time_start),
        proxy,
        time_offset,
        ntp_server,
    }
}

fn task_status_from_spawn(task_status: &str) -> TaskStatus {
    match task_status {
        "scheduled" => TaskStatus::Scheduled,
        "running" => TaskStatus::Running,
        "success" => TaskStatus::Success,
        "failed" => TaskStatus::Failed,
        "stopped" => TaskStatus::Stopped,
        _ => TaskStatus::Pending,
    }
}

fn spawn_options_from_task_args(args: &TaskArgs) -> SpawnTaskOptions {
    SpawnTaskOptions {
        interval: args.interval,
        mode: args.mode,
        total_attempts: args.total_attempts,
        time_start: args.time_start.clone(),
        proxy: args.proxy.clone(),
        time_offset: args.time_offset,
        ntp_server: args.ntp_server.clone(),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    #[serde(alias = "ticketInfo")]
    pub ticket_info: String,
    pub interval: u64,
    pub mode: u32,
    #[serde(alias = "totalAttempts")]
    pub total_attempts: u32,
    #[serde(alias = "timeStart")]
    pub time_start: Option<String>,
    pub proxy: Option<String>,
    #[serde(alias = "timeOffset")]
    pub time_offset: Option<f64>,
    pub buyers: Option<Vec<Value>>,
    #[serde(alias = "ntpServer")]
    pub ntp_server: Option<String>,
    pub project: Option<String>,
    pub screen: Option<String>,
    pub sku: Option<String>,
    #[serde(alias = "accountName")]
    pub account_name: Option<String>,
}

fn build_manual_task_record(req: CreateTaskRequest) -> anyhow::Result<TaskRecord> {
    let mut info: TicketInfo = serde_json::from_str(&req.ticket_info)?;
    merge_buyer_overrides(&mut info, req.buyers);
    let now = current_unix_secs();
    Ok(storage::build_task_record(TaskCreateInput {
        id: Uuid::new_v4().to_string(),
        source: TaskSource::Manual,
        project: req
            .project
            .filter(|value| !value.trim().is_empty())
            .or_else(|| info.project_name.clone())
            .unwrap_or_else(|| info.project_id.clone()),
        screen: req
            .screen
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| info.screen_id.clone()),
        sku: req
            .sku
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| info.sku_id.clone()),
        account_name: req
            .account_name
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Unknown".to_string()),
        linked_share_preset_id: None,
        args: build_task_args(
            info,
            req.interval,
            req.mode,
            req.total_attempts,
            req.time_start,
            req.proxy,
            req.time_offset,
            req.ntp_server,
        ),
        initial_status: TaskStatus::Pending,
        created_at: now,
        initial_log: Some("Ready to start".to_string()),
    }))
}

pub async fn create_task(Json(req): Json<CreateTaskRequest>) -> Response {
    let task = match build_manual_task_record(req) {
        Ok(task) => task,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    match storage::create_task(task) {
        Ok(task) => (StatusCode::OK, Json(task.summary())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn list_tasks() -> Response {
    match storage::get_tasks() {
        Ok(tasks) => (
            StatusCode::OK,
            Json(tasks.into_iter().map(|task| task.summary()).collect::<Vec<TaskSummary>>()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    #[serde(alias = "timeStart")]
    pub time_start: Option<String>,
}

pub async fn update_task(Path(id): Path<String>, Json(req): Json<UpdateTaskRequest>) -> Response {
    let task = match storage::get_tasks() {
        Ok(tasks) => tasks.into_iter().find(|task| task.id == id),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let Some(task) = task else {
        return error_response(StatusCode::NOT_FOUND, "任务不存在").into_response();
    };

    if task.status != TaskStatus::Pending {
        return error_response(StatusCode::BAD_REQUEST, "只有待启动任务允许修改时间").into_response();
    }

    match storage::update_task_time_start(&id, req.time_start) {
        Ok(task) => (StatusCode::OK, Json(task.summary())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_task(Path(id): Path<String>) -> Response {
    let task = match storage::get_tasks() {
        Ok(tasks) => tasks.into_iter().find(|task| task.id == id),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let Some(task) = task else {
        return error_response(StatusCode::NOT_FOUND, "任务不存在").into_response();
    };

    if matches!(task.status, TaskStatus::Running | TaskStatus::Scheduled) {
        return error_response(StatusCode::BAD_REQUEST, "运行中的任务不能直接删除").into_response();
    }

    match storage::delete_task(&id) {
        Ok(_) => (StatusCode::OK, Json(ApiOk { ok: true })).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn start_task_record(
    State(state): State<HeadlessState>,
    Path(id): Path<String>,
) -> Response {
    let task = match storage::get_tasks() {
        Ok(tasks) => tasks.into_iter().find(|task| task.id == id),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let Some(task) = task else {
        return error_response(StatusCode::NOT_FOUND, "任务不存在").into_response();
    };

    if task.status != TaskStatus::Pending {
        return error_response(StatusCode::BAD_REQUEST, "只有待启动任务才能启动").into_response();
    }

    let spawned = spawn_buy_task_with_id(
        &state,
        task.id.clone(),
        task.args.ticket_info.clone(),
        spawn_options_from_task_args(&task.args),
    );

    match storage::update_task_after_start(&task.id, task_status_from_spawn(&spawned.task_status)) {
        Ok(updated) => (StatusCode::OK, Json(updated.summary())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn stop_task_record(
    State(state): State<HeadlessState>,
    Path(id): Path<String>,
) -> Response {
    let task = match storage::get_tasks() {
        Ok(tasks) => tasks.into_iter().find(|task| task.id == id),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let Some(task) = task else {
        return error_response(StatusCode::NOT_FOUND, "任务不存在").into_response();
    };

    if let Some(flag) = state.tasks.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }

    let message = if matches!(task.status, TaskStatus::Running | TaskStatus::Scheduled) {
        Some("任务停止中，请稍候…")
    } else {
        Some("任务已停止")
    };

    match storage::update_task_status(&id, TaskStatus::Stopped, message) {
        Ok(updated) => (StatusCode::OK, Json(updated.summary())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct StartTaskRequest {
    #[serde(alias = "taskId")]
    pub task_id: Option<String>,
    #[serde(alias = "ticketInfo")]
    pub ticket_info: String,
    pub interval: u64,
    pub mode: u32,
    #[serde(alias = "totalAttempts")]
    pub total_attempts: u32,
    #[serde(alias = "timeStart")]
    pub time_start: Option<String>,
    pub proxy: Option<String>,
    #[serde(alias = "timeOffset")]
    pub time_offset: Option<f64>,
    pub buyers: Option<Vec<Value>>,
    #[serde(alias = "ntpServer")]
    pub ntp_server: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StartTaskResponse {
    pub task_id: String,
}

pub async fn start_task(
    State(state): State<HeadlessState>,
    Json(req): Json<StartTaskRequest>,
) -> Response {
    let mut info: TicketInfo = match serde_json::from_str(&req.ticket_info) {
        Ok(v) => v,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    merge_buyer_overrides(&mut info, req.buyers.clone());
    let options = SpawnTaskOptions {
        interval: req.interval,
        mode: req.mode,
        total_attempts: req.total_attempts,
        time_start: req.time_start.filter(|s| !s.trim().is_empty()),
        proxy: req.proxy,
        time_offset: req.time_offset,
        ntp_server: req.ntp_server,
    };
    let spawned = if let Some(task_id) = req.task_id {
        spawn_buy_task_with_id(&state, task_id, info, options)
    } else {
        spawn_buy_task(&state, info, options)
    };

    (
        StatusCode::OK,
        Json(StartTaskResponse {
            task_id: spawned.task_id,
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct StopTaskRequest {
    #[serde(alias = "taskId")]
    pub task_id: String,
}

pub async fn stop_task(
    State(state): State<HeadlessState>,
    Json(req): Json<StopTaskRequest>,
) -> Response {
    if let Some(flag) = state.tasks.lock().unwrap().get(&req.task_id) {
        flag.store(true, Ordering::Relaxed);
    }
    (StatusCode::OK, Json(ApiOk { ok: true })).into_response()
}

pub async fn get_history() -> Response {
    match storage::get_history() {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn clear_history() -> Response {
    match storage::clear_history() {
        Ok(_) => (StatusCode::OK, Json(ApiOk { ok: true })).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_project_history() -> Response {
    match storage::get_project_history() {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct AddProjectHistoryRequest {
    pub item: ProjectConfig,
}

pub async fn add_project_history(Json(req): Json<AddProjectHistoryRequest>) -> Response {
    match storage::add_project_history(req.item) {
        Ok(_) => (StatusCode::OK, Json(ApiOk { ok: true })).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct DeleteProjectHistoryQuery {
    pub project_id: String,
    pub sku_id: String,
}

pub async fn delete_project_history(Query(req): Query<DeleteProjectHistoryQuery>) -> Response {
    match storage::remove_project_history_item(req.project_id, req.sku_id) {
        Ok(_) => (StatusCode::OK, Json(ApiOk { ok: true })).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateSharePresetRequest {
    pub locked_task: LockedTaskConfig,
    pub display_snapshot: ShareDisplaySnapshot,
    pub expires_at: Option<i64>,
    pub title: Option<String>,
    pub creator_uid: Option<String>,
    pub creator_name: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateSharePresetResponse {
    pub preset_id: String,
    pub share_url: String,
    pub status: SharePresetStatus,
}

#[derive(Debug, Deserialize)]
pub struct BatchDeleteSharePresetsRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BatchDeleteSharePresetsResponse {
    pub deleted_count: usize,
}

#[derive(Debug, Serialize)]
pub struct SharePresetSummaryResponse {
    pub id: String,
    pub status: SharePresetStatus,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub creator_uid: Option<String>,
    pub creator_name: Option<String>,
    pub title: Option<String>,
    pub max_success_submissions: u32,
    pub success_submission_count: u32,
    pub has_export_config: bool,
    pub locked_task: LockedTaskConfig,
    pub display_snapshot: ShareDisplaySnapshot,
    pub last_submission: Option<ShareSubmissionSummary>,
}

fn summarize_share_preset(preset: &SharePresetRecord) -> SharePresetSummaryResponse {
    SharePresetSummaryResponse {
        id: preset.id.clone(),
        status: preset.status.clone(),
        created_at: preset.created_at,
        expires_at: preset.expires_at,
        creator_uid: preset.creator_uid.clone(),
        creator_name: preset.creator_name.clone(),
        title: preset.title.clone(),
        max_success_submissions: preset.max_success_submissions,
        success_submission_count: preset.success_submission_count,
        has_export_config: preset.last_submission_export.is_some(),
        locked_task: preset.locked_task.clone(),
        display_snapshot: preset.display_snapshot.clone(),
        last_submission: preset.last_submission.clone(),
    }
}

pub async fn create_share_preset(Json(req): Json<CreateSharePresetRequest>) -> Response {
    let _share_guard = share_submit_lock().lock().await;
    let now = current_unix_secs();
    if req.locked_task.project_id.trim().is_empty()
        || req.locked_task.screen_id.trim().is_empty()
        || req.locked_task.sku_id.trim().is_empty()
    {
        return error_response(StatusCode::BAD_REQUEST, "请先锁定项目、场次和票档").into_response();
    }
    if req.locked_task.count == 0 {
        return error_response(StatusCode::BAD_REQUEST, "请设置至少 1 张票").into_response();
    }
    if req
        .expires_at
        .is_some_and(|expires_at| expires_at <= now)
    {
        return error_response(StatusCode::BAD_REQUEST, "链接过期时间必须晚于当前时间").into_response();
    }

    let raw_token = generate_share_token();
    let preset = SharePresetRecord {
        id: Uuid::new_v4().to_string(),
        token_hash: hash_share_token(&raw_token),
        status: SharePresetStatus::Active,
        created_at: now,
        expires_at: req.expires_at,
        creator_uid: req.creator_uid,
        creator_name: req.creator_name,
        title: req.title,
        max_success_submissions: 1,
        success_submission_count: 0,
        locked_task: req.locked_task,
        display_snapshot: req.display_snapshot,
        last_submission: None,
        last_submission_export: None,
    };

    let preset_id = preset.id.clone();
    if let Err(e) = storage::with_share_presets_mut(|presets| {
        normalize_share_presets(presets);
        presets.insert(0, preset);
        Ok(())
    }) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    let base_url = req
        .base_url
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_string();
    let share_url = if base_url.is_empty() {
        format!("/?share_token={}", raw_token)
    } else {
        format!("{}/?share_token={}", base_url, raw_token)
    };

    (
        StatusCode::OK,
        Json(CreateSharePresetResponse {
            preset_id,
            share_url,
            status: SharePresetStatus::Active,
        }),
    )
        .into_response()
}

pub async fn list_share_presets() -> Response {
    match storage::with_share_presets_mut(|presets| {
        normalize_share_presets(presets);
        Ok(presets.iter().map(summarize_share_preset).collect::<Vec<_>>())
    }) {
        Ok(presets) => (StatusCode::OK, Json(presets)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn close_share_preset(Path(id): Path<String>) -> Response {
    let _share_guard = share_submit_lock().lock().await;
    match storage::with_share_presets_mut(|presets| {
        normalize_share_presets(presets);
        if let Some(preset) = presets.iter_mut().find(|preset| preset.id == id) {
            if preset.status == SharePresetStatus::Active {
                preset.status = SharePresetStatus::Closed;
            }
            Ok(())
        } else {
            Err(anyhow::anyhow!("分享链接不存在"))
        }
    }) {
        Ok(_) => (StatusCode::OK, Json(ApiOk { ok: true })).into_response(),
        Err(e) if e.to_string().contains("不存在") => {
            error_response(StatusCode::NOT_FOUND, e.to_string()).into_response()
        }
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn batch_delete_share_presets(
    Json(req): Json<BatchDeleteSharePresetsRequest>,
) -> Response {
    let _share_guard = share_submit_lock().lock().await;
    let now = current_unix_secs();

    match storage::with_share_presets_mut(|presets| {
        normalize_share_presets(presets);
        let deletable_ids = validate_share_preset_batch_delete(presets, &req.ids, now)?;
        let delete_set = deletable_ids.into_iter().collect::<std::collections::HashSet<_>>();
        let before = presets.len();
        presets.retain(|preset| !delete_set.contains(&preset.id));
        Ok(before.saturating_sub(presets.len()))
    }) {
        Ok(deleted_count) => (
            StatusCode::OK,
            Json(BatchDeleteSharePresetsResponse { deleted_count }),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

pub async fn export_share_preset_config(Path(id): Path<String>) -> Response {
    match storage::with_share_presets_mut(|presets| {
        normalize_share_presets(presets);
        Ok(presets
            .iter()
            .find(|preset| preset.id == id)
            .and_then(|preset| preset.last_submission_export.clone()))
    }) {
        Ok(Some(config)) => (StatusCode::OK, Json(config)).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "未找到可导出的代抢配置").into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Debug, Serialize)]
pub struct SharePresetPublicResponse {
    pub status: SharePresetStatus,
    pub title: Option<String>,
    pub expires_at: Option<i64>,
    pub display_snapshot: ShareDisplaySnapshot,
    pub locked_task: crate::share::LockedTaskPublicView,
}

pub async fn get_share_preset_public(Path(token): Path<String>) -> Response {
    let preset = match storage::with_share_presets_mut(|presets| {
        normalize_share_presets(presets);
        Ok(find_share_preset_index_by_token(presets, &token).map(|idx| presets[idx].clone()))
    }) {
        Ok(Some(preset)) => preset,
        Ok(None) => {
            return share_state_error_response(
                StatusCode::NOT_FOUND,
                SharePresetStatus::Closed,
                "分享链接不存在",
            )
        }
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };

    let now = current_unix_secs();
    if let Err(response) = active_share_preset_or_response(&preset, now) {
        return response;
    }

    (
        StatusCode::OK,
        Json(SharePresetPublicResponse {
            status: preset.status,
            title: preset.title,
            expires_at: preset.expires_at,
            display_snapshot: preset.display_snapshot,
            locked_task: preset.locked_task.public_view(),
        }),
    )
        .into_response()
}

pub async fn fetch_share_buyers(
    Path(token): Path<String>,
    Json(req): Json<AddressRequest>,
) -> Response {
    let preset = match storage::with_share_presets_mut(|presets| {
        normalize_share_presets(presets);
        Ok(find_share_preset_index_by_token(presets, &token).map(|idx| presets[idx].clone()))
    }) {
        Ok(Some(preset)) => preset,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "分享链接不存在").into_response(),
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };

    if let Err(response) = active_share_preset_or_response(&preset, current_unix_secs()) {
        return response;
    }

    match api::fetch_buyers(preset.locked_task.project_id, req.cookies).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn fetch_share_addresses(
    Path(token): Path<String>,
    Json(req): Json<AddressRequest>,
) -> Response {
    let preset = match storage::with_share_presets_mut(|presets| {
        normalize_share_presets(presets);
        Ok(find_share_preset_index_by_token(presets, &token).map(|idx| presets[idx].clone()))
    }) {
        Ok(Some(preset)) => preset,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "分享链接不存在").into_response(),
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };

    if let Err(response) = active_share_preset_or_response(&preset, current_unix_secs()) {
        return response;
    }

    match api::fetch_address_list(req.cookies).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

pub async fn fetch_share_user_info(
    Path(token): Path<String>,
    Json(req): Json<AddressRequest>,
) -> Response {
    let preset = match storage::with_share_presets_mut(|presets| {
        normalize_share_presets(presets);
        Ok(find_share_preset_index_by_token(presets, &token).map(|idx| presets[idx].clone()))
    }) {
        Ok(Some(preset)) => preset,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "分享链接不存在").into_response(),
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };

    if let Err(response) = active_share_preset_or_response(&preset, current_unix_secs()) {
        return response;
    }

    match api::fetch_user_info(req.cookies).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

#[derive(Debug, Serialize)]
pub struct SubmitSharePresetResponse {
    pub task_id: String,
    pub task_status: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct StartSharePresetTaskRequest {
    #[serde(alias = "taskId")]
    pub task_id: Option<String>,
}

pub async fn submit_share_preset(
    State(_state): State<HeadlessState>,
    Path(token): Path<String>,
    Json(req): Json<ShareSubmissionInput>,
) -> Response {
    let _share_guard = share_submit_lock().lock().await;

    let mut presets = match storage::get_share_presets() {
        Ok(presets) => presets,
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };
    normalize_share_presets(&mut presets);

    let Some(preset_idx) = find_share_preset_index_by_token(&presets, &token) else {
        return error_response(StatusCode::NOT_FOUND, "分享链接不存在").into_response();
    };

    let preset = presets[preset_idx].clone();
    if let Err(response) = active_share_preset_or_response(&preset, current_unix_secs()) {
        return response;
    }

    let user_info = match api::fetch_user_info(req.cookies.clone()).await {
        Ok(value) => value,
        Err(e) => return error_response(StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    };
    if user_info["code"].as_i64().unwrap_or(-1) != 0 {
        return error_response(StatusCode::BAD_REQUEST, "登录已失效，请重新扫码").into_response();
    }

    let info = match build_ticket_info_from_submission(&preset.locked_task, &req) {
        Ok(info) => info,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let export_config = build_share_submission_export_config(info.clone(), &preset.locked_task);

    let submitter_uid = user_info["data"]["mid"]
        .as_str()
        .map(|value| value.to_string())
        .or_else(|| user_info["data"]["mid"].as_i64().map(|value| value.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let submitter_name = user_info["data"]["uname"]
        .as_str()
        .unwrap_or("未知用户")
        .to_string();

    let pending_task_id = Uuid::new_v4().to_string();
    let task = storage::build_task_record(TaskCreateInput {
        id: pending_task_id.clone(),
        source: TaskSource::ShareSubmission,
        project: preset.locked_task.project_name.clone(),
        screen: preset.locked_task.screen_name.clone(),
        sku: preset.locked_task.sku_name.clone(),
        account_name: submitter_name.clone(),
        linked_share_preset_id: Some(preset.id.clone()),
        args: build_task_args(
            info.clone(),
            preset.locked_task.interval,
            preset.locked_task.mode,
            preset.locked_task.total_attempts,
            preset.locked_task.time_start.clone(),
            preset.locked_task.proxy.clone(),
            None,
            preset.locked_task.ntp_server.clone(),
        ),
        initial_status: TaskStatus::Pending,
        created_at: current_unix_secs(),
        initial_log: Some(format!("分享链接提交：{}", submitter_name)),
    });

    if let Err(e) = storage::create_task(task) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    let preset_mut = &mut presets[preset_idx];
    preset_mut.success_submission_count += 1;
    preset_mut.status = SharePresetStatus::Completed;
    preset_mut.last_submission = Some(ShareSubmissionSummary {
        submitted_at: current_unix_secs(),
        submitter_uid,
        submitter_name,
        task_id: pending_task_id.clone(),
        task_status: "pending".to_string(),
        buyer_count: preset_mut.locked_task.count,
    });
    preset_mut.last_submission_export = Some(export_config);

    if let Err(e) = storage::save_share_presets(&presets) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    (
        StatusCode::OK,
        Json(SubmitSharePresetResponse {
            task_id: pending_task_id,
            task_status: "pending".to_string(),
            message: "信息提交成功，等待发起人在后台任务列表中启动抢票".to_string(),
        }),
    )
        .into_response()
}

pub async fn start_share_preset_task(
    State(state): State<HeadlessState>,
    Path(id): Path<String>,
    Json(req): Json<StartSharePresetTaskRequest>,
) -> Response {
    let task = match storage::get_tasks() {
        Ok(tasks) => tasks.into_iter().find(|task| {
            task.linked_share_preset_id.as_deref() == Some(id.as_str())
                && req
                    .task_id
                    .as_ref()
                    .map(|task_id| task.id == *task_id)
                    .unwrap_or(true)
        }),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let Some(task) = task else {
        return error_response(StatusCode::NOT_FOUND, "该分享记录没有可启动的任务").into_response();
    };

    if task.status != TaskStatus::Pending {
        return error_response(
            StatusCode::BAD_REQUEST,
            format!("该分享任务当前状态为 {:?}，不能重复启动", task.status),
        )
        .into_response();
    }

    let spawned = spawn_buy_task_with_id(
        &state,
        task.id.clone(),
        task.args.ticket_info.clone(),
        spawn_options_from_task_args(&task.args),
    );

    match storage::update_task_after_start(&task.id, task_status_from_spawn(&spawned.task_status)) {
        Ok(updated) => (
            StatusCode::OK,
            Json(SubmitSharePresetResponse {
                task_id: updated.id,
                task_status: match updated.status {
                    TaskStatus::Scheduled => "scheduled".to_string(),
                    TaskStatus::Running => "running".to_string(),
                    TaskStatus::Pending => "pending".to_string(),
                    TaskStatus::Success => "success".to_string(),
                    TaskStatus::Stopped => "stopped".to_string(),
                    TaskStatus::Failed => "failed".to_string(),
                },
                message: if updated.status == TaskStatus::Scheduled {
                    "分享任务已启动，等待开抢时间".to_string()
                } else {
                    "分享任务已启动".to_string()
                },
            }),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
