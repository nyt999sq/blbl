use crate::api;
use crate::auth;
use crate::buy::{self, TicketInfo};
use crate::headless::auth as headless_auth;
use crate::headless::ws::WsEventSink;
use crate::headless::HeadlessState;
use crate::storage::{self, Account, ProjectConfig};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Deserialize)]
pub struct StartTaskRequest {
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

    if let Some(b) = req.buyers.clone() {
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

    let task_id = Uuid::new_v4().to_string();
    let stop_flag = Arc::new(AtomicBool::new(false));
    state
        .tasks
        .lock()
        .unwrap()
        .insert(task_id.clone(), stop_flag.clone());

    let task_id_clone = task_id.clone();
    let sink = WsEventSink::new(state.ws_hub.clone());
    tokio::spawn(async move {
        let time_start = req.time_start.filter(|s| !s.trim().is_empty());
        if let Err(e) = buy::start_buy_task(
            sink,
            task_id_clone,
            stop_flag,
            info,
            req.interval,
            req.mode,
            req.total_attempts,
            time_start,
            req.proxy,
            req.time_offset,
            req.ntp_server,
        )
        .await
        {
            eprintln!("headless task error: {}", e);
        }
    });

    (StatusCode::OK, Json(StartTaskResponse { task_id })).into_response()
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
