use crate::core::events::{now_ts_millis, TaskEvent, TaskEventSink};
use crate::headless::auth;
use crate::headless::HeadlessState;
use crate::storage::{self, TaskStatus};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct WsHub {
    tx: broadcast::Sender<TaskEvent>,
}

impl WsHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    pub fn publish(&self, event: TaskEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.tx.subscribe()
    }
}

pub struct WsEventSink {
    hub: WsHub,
}

impl WsEventSink {
    pub fn new(hub: WsHub) -> Arc<Self> {
        Arc::new(Self { hub })
    }
}

impl TaskEventSink for WsEventSink {
    fn emit_log(&self, task_id: &str, message: &str) {
        let _ = storage::append_task_log(task_id, message);
        self.hub.publish(TaskEvent::Log {
            task_id: task_id.to_string(),
            message: message.to_string(),
            ts: now_ts_millis(),
        });
    }

    fn emit_payment_qrcode(&self, task_id: &str, url: &str) {
        let _ = storage::update_task_payment_url(task_id, url);
        self.hub.publish(TaskEvent::PaymentQrcode {
            task_id: task_id.to_string(),
            url: url.to_string(),
            ts: now_ts_millis(),
        });
    }

    fn emit_task_result(&self, task_id: &str, success: bool, message: &str) {
        let status = if success {
            TaskStatus::Success
        } else if message.contains("Task stopped")
            || message.contains("stopped by user")
            || message.contains("任务停止")
        {
            TaskStatus::Stopped
        } else {
            TaskStatus::Failed
        };
        let _ = storage::update_task_status(task_id, status, Some(message));
        self.hub.publish(TaskEvent::TaskResult {
            task_id: task_id.to_string(),
            success,
            message: message.to_string(),
            ts: now_ts_millis(),
        });
    }
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub session: String,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<HeadlessState>,
    Query(query): Query<WsQuery>,
) -> Response {
    if !auth::is_session_valid(&state.sessions, &query.session).await {
        return (StatusCode::UNAUTHORIZED, "invalid session").into_response();
    }

    let mut rx = state.ws_hub.subscribe();
    ws.on_upgrade(move |socket| async move {
        handle_socket(socket, &mut rx).await;
    })
}

async fn handle_socket(mut socket: WebSocket, rx: &mut broadcast::Receiver<TaskEvent>) {
    loop {
        tokio::select! {
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            outbound = rx.recv() => {
                match outbound {
                    Ok(event) => {
                        if let Ok(text) = serde_json::to_string(&event) {
                            if socket.send(Message::Text(text)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}
