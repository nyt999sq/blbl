use crate::headless::HeadlessState;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

pub type SessionStore = Arc<RwLock<HashMap<String, i64>>>;

const DEFAULT_SESSION_TTL_SECONDS: i64 = 12 * 60 * 60;

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub session: String,
    pub expires_at: i64,
}

pub fn new_session_store() -> SessionStore {
    Arc::new(RwLock::new(HashMap::new()))
}

fn now_unix_secs() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => 0,
    }
}

pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ").map(|v| v.trim().to_string())
}

pub async fn issue_session(store: &SessionStore) -> SessionResponse {
    let token = Uuid::new_v4().to_string();
    let expires_at = now_unix_secs() + DEFAULT_SESSION_TTL_SECONDS;
    store.write().await.insert(token.clone(), expires_at);
    SessionResponse {
        session: token,
        expires_at,
    }
}

pub async fn validate_server_token(
    headers: &HeaderMap,
    server_token: &Option<String>,
) -> Result<(), (StatusCode, &'static str)> {
    match server_token {
        None => Ok(()),
        Some(expected) => match extract_bearer_token(headers) {
            Some(actual) if actual == *expected => Ok(()),
            _ => Err((StatusCode::UNAUTHORIZED, "invalid server token")),
        },
    }
}

pub async fn is_session_valid(store: &SessionStore, session: &str) -> bool {
    let now = now_unix_secs();
    let guard = store.read().await;
    guard.get(session).is_some_and(|expire| *expire > now)
}

fn extract_session_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get("x-session-token") {
        if let Ok(s) = v.to_str() {
            let token = s.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }

    extract_bearer_token(headers)
}

pub async fn require_session(
    State(state): State<HeadlessState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let headers = req.headers();
    let session = extract_session_from_headers(headers);

    if let Some(session) = session {
        if is_session_valid(&state.sessions, &session).await {
            return next.run(req).await;
        }
    }

    (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}
