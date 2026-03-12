use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum TaskEvent {
    #[serde(rename = "log")]
    Log {
        task_id: String,
        message: String,
        ts: i64,
    },
    #[serde(rename = "payment_qrcode")]
    PaymentQrcode {
        task_id: String,
        url: String,
        ts: i64,
    },
    #[serde(rename = "task_result")]
    TaskResult {
        task_id: String,
        success: bool,
        message: String,
        ts: i64,
    },
}

pub fn now_ts_millis() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(_) => 0,
    }
}

pub trait TaskEventSink: Send + Sync {
    fn emit_log(&self, task_id: &str, message: &str);
    fn emit_payment_qrcode(&self, task_id: &str, url: &str);
    fn emit_task_result(&self, task_id: &str, success: bool, message: &str);
}
