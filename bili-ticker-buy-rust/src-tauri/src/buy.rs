use crate::api; // Import api module
use crate::core::events::TaskEventSink;
use crate::order_protocol::{OrderProtocol, SubmitOrderResult};
use crate::storage::{self, HistoryItem};
use crate::util::CTokenGenerator;
use anyhow::Result;
use chrono::Local;
use log::info;
use reqwest::cookie::Jar;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TicketInfo {
    pub project_id: String,
    pub project_name: Option<String>,
    pub screen_id: String,
    pub sku_id: String,
    pub count: u32,
    pub buyer_info: serde_json::Value,
    pub deliver_info: serde_json::Value,
    pub cookies: Vec<String>,
    pub is_hot_project: Option<bool>,
    pub pay_money: Option<u32>,
    pub contact_name: Option<String>,
    pub contact_tel: Option<String>,
}

fn emit_log(sink: &dyn TaskEventSink, task_id: &str, message: &str) {
    sink.emit_log(task_id, message);
    info!("[{}] {}", task_id, message);
}

pub async fn start_buy_task(
    sink: Arc<dyn TaskEventSink + Send + Sync>,
    task_id: String,
    stop_flag: Arc<AtomicBool>,
    mut info: TicketInfo,
    interval: u64,
    mode: u32,
    total_attempts: u32,
    time_start: Option<String>,
    proxy: Option<String>,
    time_offset: Option<f64>,
    ntp_server: Option<String>,
) -> Result<()> {
    emit_log(sink.as_ref(), &task_id, "Starting buy task...");

    if let Some(ts) = &time_start {
        emit_log(
            sink.as_ref(),
            &task_id,
            &format!("Scheduled start time: {}", ts),
        );

        // Parse start time
        // Try different formats
        let target_time =
            if let Ok(t) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
                Some(t.and_local_timezone(Local).unwrap())
            } else if let Ok(t) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S") {
                Some(t.and_local_timezone(Local).unwrap())
            } else {
                None
            };

        if let Some(target) = target_time {
            let mut current_offset = time_offset.unwrap_or(0.0) as i64;
            // Correct logic: target_local = target_server - offset
            // If offset > 0 (server > local), we need to start earlier (local time is smaller)
            let mut target_with_offset = target - chrono::Duration::milliseconds(current_offset);

            emit_log(
                sink.as_ref(),
                &task_id,
                &format!(
                    "Waiting until: {} (Offset: {}ms)",
                    target_with_offset.format("%Y-%m-%d %H:%M:%S%.3f"),
                    current_offset
                ),
            );

            let mut last_sync_time = Instant::now();
            let sync_interval = Duration::from_secs(10); // Sync every 10 seconds

            loop {
                if stop_flag.load(Ordering::Relaxed) {
                    emit_log(
                        sink.as_ref(),
                        &task_id,
                        "Task stopped by user while waiting.",
                    );
                    return Ok(());
                }

                let now = Local::now();
                let diff = target_with_offset - now;
                let remaining_ms = diff.num_milliseconds();

                if remaining_ms <= 0 {
                    break;
                }

                // Auto-sync logic: if remaining > 2s and last sync > 10s ago
                if remaining_ms > 2000 && last_sync_time.elapsed() > sync_interval {
                    emit_log(sink.as_ref(), &task_id, "Auto-syncing time offset...");
                    let url = ntp_server.clone().unwrap_or_else(|| {
                        "https://api.bilibili.com/x/report/click/now".to_string()
                    });

                    let sync_result = if url.starts_with("http") {
                        api::get_server_time(Some(url.clone())).await
                    } else {
                        api::get_ntp_time(&url).map(|t| t as i64)
                    };

                    match sync_result {
                        Ok(server_time) => {
                            let local_time = api::get_local_time();
                            let new_offset = server_time - local_time;
                            current_offset = new_offset;
                            // Update target with new offset
                            target_with_offset =
                                target - chrono::Duration::milliseconds(current_offset);
                            emit_log(
                                sink.as_ref(),
                                &task_id,
                                &format!(
                                    "Synced offset: {}ms. New target: {}",
                                    current_offset,
                                    target_with_offset.format("%H:%M:%S%.3f")
                                ),
                            );
                        }
                        Err(e) => {
                            emit_log(sink.as_ref(), &task_id, &format!("Auto-sync failed: {}", e));
                        }
                    }
                    last_sync_time = Instant::now();
                }

                if diff.num_seconds() > 5 {
                    sleep(Duration::from_secs(1)).await;
                } else {
                    sleep(Duration::from_millis(100)).await;
                }
            }
            emit_log(
                sink.as_ref(),
                &task_id,
                "Time reached! Starting execution...",
            );
        } else {
            emit_log(
                sink.as_ref(),
                &task_id,
                "Invalid time format. Starting immediately.",
            );
        }
    }

    if let Some(p) = &proxy {
        emit_log(sink.as_ref(), &task_id, &format!("Using proxy: {}", p));
    }
    if let Some(to) = time_offset {
        emit_log(sink.as_ref(), &task_id, &format!("Time offset: {}ms", to));
    }

    let jar = Arc::new(Jar::default());
    let url = "https://show.bilibili.com".parse::<Url>().unwrap();

    // Parse cookies
    for cookie_str in &info.cookies {
        for part in cookie_str.split(';') {
            jar.add_cookie_str(part.trim(), &url);
        }
    }

    let client = Client::builder()
        .cookie_provider(jar)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 Edg/126.0.0.0")
        .timeout(Duration::from_secs(10))
        .build()?;

    let is_hot = info.is_hot_project.unwrap_or(false);
    let mut ctoken_gen = CTokenGenerator::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
        0,
        rand::random::<u64>() % 8000 + 2000,
    );

    let mut left_time = total_attempts as i32;
    let mut is_running = true;
    let protocol = OrderProtocol::new();

    // Generate static device ID for this task
    let device_id = format!(
        "{:x}",
        md5::compute(format!("{}{}", task_id, rand::random::<u64>()))
    );

    while is_running {
        if stop_flag.load(Ordering::Relaxed) {
            emit_log(sink.as_ref(), &task_id, "Task stopped by user.");
            break;
        }

        emit_log(sink.as_ref(), &task_id, "1) Preparing order...");
        if is_hot {
            emit_log(sink.as_ref(), &task_id, "Hot project enabled, ctoken/ptoken flow active.");
        }
        emit_log(sink.as_ref(), &task_id, "2) Confirming order...");
        emit_log(sink.as_ref(), &task_id, "3) Creating order...");

        let start = Instant::now();
        match protocol
            .submit_order(
                &client,
                &info,
                &mut ctoken_gen,
                &device_id,
                stop_flag.as_ref(),
            )
            .await
        {
            Ok(SubmitOrderResult::Success {
                order_id,
                payment_url,
            }) => {
                emit_log(sink.as_ref(), &task_id, "Order created successfully!");
                emit_log(sink.as_ref(), &task_id, &format!("Order ID: {}", order_id));

                let pay_url = payment_url.unwrap_or_default();
                if pay_url.is_empty() {
                    emit_log(
                        sink.as_ref(),
                        &task_id,
                        "Payment URL missing, order created but QR code unavailable.",
                    );
                } else {
                    sink.emit_payment_qrcode(&task_id, &pay_url);
                }

                let history_item = HistoryItem {
                    order_id: order_id.clone(),
                    project_name: info
                        .project_name
                        .clone()
                        .unwrap_or(info.project_id.clone()),
                    price: info.pay_money.unwrap_or(0),
                    time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    pay_url,
                };
                let _ = storage::add_history_item(history_item);

                sink.emit_task_result(&task_id, true, &format!("抢票成功！订单号: {}", order_id));
                is_running = false;
            }
            Ok(SubmitOrderResult::PriceChanged { pay_money }) => {
                emit_log(
                    sink.as_ref(),
                    &task_id,
                    &format!("Price updated to: {}", pay_money),
                );
                info.pay_money = Some(pay_money);
            }
            Ok(SubmitOrderResult::TokenExpired) => {
                emit_log(
                    sink.as_ref(),
                    &task_id,
                    "Token expired. Restarting prepare flow...",
                );
            }
            Ok(SubmitOrderResult::RetryableFailure { code, message }) => {
                emit_log(
                    sink.as_ref(),
                    &task_id,
                    &format!("Order flow failed: code={} msg={}", code, message),
                );
            }
            Ok(SubmitOrderResult::Stopped) => {
                emit_log(sink.as_ref(), &task_id, "Task stopped by user.");
                is_running = false;
            }
            Err(err) => {
                emit_log(
                    sink.as_ref(),
                    &task_id,
                    &format!("Order flow request error: {}", err),
                );
            }
        }

        if is_running {
            emit_log(
                sink.as_ref(),
                &task_id,
                "Retry attempts exhausted or token expired. Restarting loop...",
            );
            if mode == 1 {
                left_time -= 1;
                if left_time <= 0 {
                    is_running = false;
                    emit_log(sink.as_ref(), &task_id, "Total attempts reached. Stopping.");
                    sink.emit_task_result(&task_id, false, "达到最大尝试次数，任务停止");
                }
            }
        }

        if is_running {
            let elapsed = start.elapsed();
            let interval_duration = Duration::from_millis(interval);
            if elapsed < interval_duration {
                let remaining = interval_duration - elapsed;
                if remaining.as_secs_f64() > 0.02 {
                    sleep(remaining - Duration::from_millis(10)).await;
                }
                while start.elapsed() < interval_duration {
                    std::hint::spin_loop();
                }
            }
        }
    }

    Ok(())
}
