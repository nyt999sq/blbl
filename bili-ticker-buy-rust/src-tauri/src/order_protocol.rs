use crate::buy::TicketInfo;
use crate::util::CTokenGenerator;
use reqwest::Client;
use serde_json::{json, Value};
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

const DEFAULT_BASE_URL: &str = "https://show.bilibili.com";
const DEFAULT_REQUEST_SOURCE: &str = "neul-next";
const PENDING_CODES: [i64; 2] = [100048, 100079];
const TOKEN_EXPIRED_CODES: [i64; 1] = [100051];
const PRICE_CHANGED_CODES: [i64; 1] = [100034];

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareResult {
    pub token: String,
    pub ptoken: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmInfoResult {
    pub count: u32,
    pub pay_money: Option<u32>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateOrderState {
    Created,
    Pending,
    PriceChanged,
    TokenExpired,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateOrderResult {
    pub state: CreateOrderState,
    pub code: i64,
    pub message: String,
    pub order_id: Option<String>,
    pub pay_token: Option<String>,
    pub pay_money: Option<u32>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateStatusState {
    Completed,
    Pending,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateStatusResult {
    pub state: CreateStatusState,
    pub code: i64,
    pub message: String,
    pub order_id: Option<String>,
    pub pay_token: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SubmitOrderResult {
    Success {
        order_id: String,
        payment_url: Option<String>,
    },
    PriceChanged {
        pay_money: u32,
    },
    TokenExpired,
    RetryableFailure {
        code: i64,
        message: String,
    },
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolError {
    pub code: Option<i64>,
    pub message: String,
}

impl ProtocolError {
    fn new(code: Option<i64>, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn business(context: &str, raw: &Value) -> Self {
        let code = response_code(raw);
        let message = response_message(raw);
        Self::new(
            Some(code),
            format!("{} failed: code={} msg={}", context, code, message),
        )
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            Some(code) => write!(f, "{} (code={})", self.message, code),
            None => write!(f, "{}", self.message),
        }
    }
}

impl Error for ProtocolError {}

#[derive(Debug, Clone)]
pub struct OrderProtocol {
    base_url: String,
    request_source: String,
    status_poll_interval: Duration,
    status_poll_attempts: usize,
}

impl Default for OrderProtocol {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            request_source: DEFAULT_REQUEST_SOURCE.to_string(),
            status_poll_interval: Duration::from_millis(120),
            status_poll_attempts: 30,
        }
    }
}

impl OrderProtocol {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            ..Self::default()
        }
    }

    pub fn with_status_poll_interval(mut self, interval: Duration) -> Self {
        self.status_poll_interval = interval;
        self
    }

    pub fn with_status_poll_attempts(mut self, attempts: usize) -> Self {
        self.status_poll_attempts = attempts.max(1);
        self
    }

    pub async fn submit_order(
        &self,
        client: &Client,
        info: &TicketInfo,
        ctoken_gen: &mut CTokenGenerator,
        device_id: &str,
        stop_flag: &AtomicBool,
    ) -> Result<SubmitOrderResult, ProtocolError> {
        if stop_flag.load(Ordering::Relaxed) {
            return Ok(SubmitOrderResult::Stopped);
        }

        let is_hot = info.is_hot_project.unwrap_or(false);
        let prepared = self.prepare_order(client, info, is_hot, ctoken_gen).await?;

        if stop_flag.load(Ordering::Relaxed) {
            return Ok(SubmitOrderResult::Stopped);
        }

        let confirmed = match self.confirm_order(client, &info.project_id, &prepared.token).await {
            Ok(result) => result,
            Err(err) => {
                return Ok(SubmitOrderResult::RetryableFailure {
                    code: err.code.unwrap_or(-1),
                    message: err.message,
                });
            }
        };

        if stop_flag.load(Ordering::Relaxed) {
            return Ok(SubmitOrderResult::Stopped);
        }

        let created = self
            .create_order(
                client,
                info,
                &prepared,
                &confirmed,
                is_hot,
                device_id,
                ctoken_gen,
            )
            .await?;

        match created.state {
            CreateOrderState::Created => {
                let Some(order_id) = created.order_id else {
                    return Ok(SubmitOrderResult::RetryableFailure {
                        code: created.code,
                        message: "createV2 returned success without order_id".to_string(),
                    });
                };
                let payment_url = self.get_payment_url(client, &order_id).await?;
                Ok(SubmitOrderResult::Success {
                    order_id,
                    payment_url,
                })
            }
            CreateOrderState::Pending => {
                let Some(pay_token) = created.pay_token.clone() else {
                    return Ok(SubmitOrderResult::RetryableFailure {
                        code: created.code,
                        message: "createV2 returned pending without pay token".to_string(),
                    });
                };
                let mut known_order_id = created.order_id.clone();

                for _ in 0..self.status_poll_attempts {
                    if stop_flag.load(Ordering::Relaxed) {
                        return Ok(SubmitOrderResult::Stopped);
                    }

                    let status = self
                        .fetch_create_status(
                            client,
                            &info.project_id,
                            &pay_token,
                            known_order_id.as_deref(),
                        )
                        .await?;

                    match status.state {
                        CreateStatusState::Completed => {
                            let order_id = status
                                .order_id
                                .clone()
                                .or(known_order_id.clone())
                                .ok_or_else(|| {
                                    ProtocolError::new(
                                        Some(status.code),
                                        "createstatus completed without order_id",
                                    )
                                })?;
                            let payment_url = self.get_payment_url(client, &order_id).await?;
                            return Ok(SubmitOrderResult::Success {
                                order_id,
                                payment_url,
                            });
                        }
                        CreateStatusState::Pending => {
                            known_order_id = status.order_id.clone().or(known_order_id);
                            sleep(self.status_poll_interval).await;
                        }
                        CreateStatusState::Failed => {
                            return Ok(SubmitOrderResult::RetryableFailure {
                                code: status.code,
                                message: status.message,
                            });
                        }
                    }
                }

                Ok(SubmitOrderResult::RetryableFailure {
                    code: created.code,
                    message: "createstatus polling timed out".to_string(),
                })
            }
            CreateOrderState::PriceChanged => Ok(SubmitOrderResult::PriceChanged {
                pay_money: created.pay_money.unwrap_or_default(),
            }),
            CreateOrderState::TokenExpired => Ok(SubmitOrderResult::TokenExpired),
            CreateOrderState::Failed => Ok(SubmitOrderResult::RetryableFailure {
                code: created.code,
                message: created.message,
            }),
        }
    }

    pub async fn prepare_order(
        &self,
        client: &Client,
        info: &TicketInfo,
        is_hot: bool,
        ctoken_gen: &mut CTokenGenerator,
    ) -> Result<PrepareResult, ProtocolError> {
        let mut payload = json!({
            "count": info.count,
            "screen_id": info.screen_id,
            "order_type": 1,
            "project_id": info.project_id,
            "sku_id": info.sku_id,
            "token": "",
            "newRisk": true,
        });

        if is_hot {
            payload["token"] = json!(ctoken_gen.generate_ctoken(false));
        }

        let response = client
            .post(self.url("/api/ticket/order/prepare"))
            .query(&[("project_id", info.project_id.as_str())])
            .json(&payload)
            .send()
            .await
            .map_err(|err| ProtocolError::new(None, format!("prepare request failed: {}", err)))?;
        let raw = parse_json_response(response, "prepare").await?;

        if response_code(&raw) != 0 {
            return Err(ProtocolError::business("prepare", &raw));
        }

        Ok(PrepareResult {
            token: required_string_at(&raw, &["data", "token"], "prepare token")?,
            ptoken: string_at(&raw, &["data", "ptoken"]),
            raw,
        })
    }

    pub async fn confirm_order(
        &self,
        client: &Client,
        project_id: &str,
        token: &str,
    ) -> Result<ConfirmInfoResult, ProtocolError> {
        let response = client
            .get(self.url("/api/ticket/order/confirmInfo"))
            .query(&[
                ("token", token),
                ("voucher", ""),
                ("project_id", project_id),
                ("requestSource", self.request_source.as_str()),
            ])
            .send()
            .await
            .map_err(|err| {
                ProtocolError::new(None, format!("confirmInfo request failed: {}", err))
            })?;
        let raw = parse_json_response(response, "confirmInfo").await?;

        if response_code(&raw) != 0 {
            return Err(ProtocolError::business("confirmInfo", &raw));
        }

        Ok(ConfirmInfoResult {
            count: u32_at(&raw, &["data", "count"]).unwrap_or(0),
            pay_money: u32_at(&raw, &["data", "pay_money"])
                .or_else(|| u32_at(&raw, &["data", "order", "pay_money"])),
            raw,
        })
    }

    pub async fn create_order(
        &self,
        client: &Client,
        info: &TicketInfo,
        prepared: &PrepareResult,
        confirmed: &ConfirmInfoResult,
        is_hot: bool,
        device_id: &str,
        ctoken_gen: &mut CTokenGenerator,
    ) -> Result<CreateOrderResult, ProtocolError> {
        let now_ms = current_millis();
        let click_origin = now_ms.saturating_sub(rand::random::<u64>() % 2000 + 1000);
        let mut payload = json!({
            "project_id": info.project_id,
            "screen_id": info.screen_id,
            "sku_id": info.sku_id,
            "count": confirmed.count.max(info.count),
            "order_type": 1,
            "buyer_info": info.buyer_info.to_string(),
            "deliver_info": info.deliver_info.to_string(),
            "token": prepared.token,
            "again": 1,
            "timestamp": now_ms,
            "deviceId": device_id,
            "requestSource": self.request_source.as_str(),
            "newRisk": true,
            "clickPosition": {
                "x": rand::random::<u64>() % 500 + 100,
                "y": rand::random::<u64>() % 1000 + 500,
                "origin": click_origin,
                "now": now_ms
            }
        });

        if let Some(pay_money) = confirmed.pay_money.or(info.pay_money) {
            payload["pay_money"] = json!(pay_money);
        }

        if let Some(name) = &info.contact_name {
            payload["contact_name"] = json!(name);
            payload["buyer"] = json!(name);
        }
        if let Some(tel) = &info.contact_tel {
            if !tel.contains('*') {
                payload["contact_tel"] = json!(tel);
                payload["tel"] = json!(tel);
            }
        }

        let mut request = client
            .post(self.url("/api/ticket/order/createV2"))
            .query(&[("project_id", info.project_id.as_str())]);

        if is_hot {
            payload["ctoken"] = json!(ctoken_gen.generate_ctoken(true));
            if let Some(ptoken) = &prepared.ptoken {
                payload["ptoken"] = json!(ptoken);
                request = request.query(&[("ptoken", ptoken.as_str())]);
            }
            payload["orderCreateUrl"] =
                json!("https://show.bilibili.com/api/ticket/order/createV2");
        }

        let response = request
            .json(&payload)
            .send()
            .await
            .map_err(|err| ProtocolError::new(None, format!("createV2 request failed: {}", err)))?;
        let raw = parse_json_response(response, "createV2").await?;
        let code = response_code(&raw);
        let message = response_message(&raw);
        let order_id = string_at(&raw, &["data", "orderId"])
            .or_else(|| string_at(&raw, &["data", "order_id"]));
        let pay_token = string_at(&raw, &["data", "token"]);
        let pay_money = u32_at(&raw, &["data", "pay_money"]);

        let state = if code == 0 && order_id.is_some() {
            CreateOrderState::Created
        } else if PRICE_CHANGED_CODES.contains(&code) {
            CreateOrderState::PriceChanged
        } else if TOKEN_EXPIRED_CODES.contains(&code) {
            CreateOrderState::TokenExpired
        } else if PENDING_CODES.contains(&code) || (code == 0 && pay_token.is_some()) {
            CreateOrderState::Pending
        } else {
            CreateOrderState::Failed
        };

        Ok(CreateOrderResult {
            state,
            code,
            message,
            order_id,
            pay_token,
            pay_money,
            raw,
        })
    }

    pub async fn fetch_create_status(
        &self,
        client: &Client,
        project_id: &str,
        pay_token: &str,
        order_id: Option<&str>,
    ) -> Result<CreateStatusResult, ProtocolError> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| ProtocolError::new(None, format!("clock error: {}", err)))?
            .as_secs()
            .to_string();

        let mut query = vec![
            ("project_id".to_string(), project_id.to_string()),
            ("token".to_string(), pay_token.to_string()),
            ("timestamp".to_string(), timestamp),
        ];
        if let Some(order_id) = order_id {
            query.push(("orderId".to_string(), order_id.to_string()));
        }

        let response = client
            .get(self.url("/api/ticket/order/createstatus"))
            .query(&query)
            .send()
            .await
            .map_err(|err| {
                ProtocolError::new(None, format!("createstatus request failed: {}", err))
            })?;
        let raw = parse_json_response(response, "createstatus").await?;
        let code = response_code(&raw);
        let message = response_message(&raw);
        let order_id = string_at(&raw, &["data", "orderId"])
            .or_else(|| string_at(&raw, &["data", "order_id"]));
        let pay_token = string_at(&raw, &["data", "token"]);

        let state = if code == 0 {
            CreateStatusState::Completed
        } else if PENDING_CODES.contains(&code) || is_pending_message(&message) {
            CreateStatusState::Pending
        } else {
            CreateStatusState::Failed
        };

        Ok(CreateStatusResult {
            state,
            code,
            message,
            order_id,
            pay_token,
            raw,
        })
    }

    pub async fn get_payment_url(
        &self,
        client: &Client,
        order_id: &str,
    ) -> Result<Option<String>, ProtocolError> {
        let response = client
            .get(self.url("/api/ticket/order/getPayParam"))
            .query(&[("order_id", order_id)])
            .send()
            .await
            .map_err(|err| {
                ProtocolError::new(None, format!("getPayParam request failed: {}", err))
            })?;
        let raw = parse_json_response(response, "getPayParam").await?;

        if response_code(&raw) != 0 {
            return Ok(None);
        }

        Ok(string_at(&raw, &["data", "code_url"]))
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

fn parse_json_response(
    response: reqwest::Response,
    context: &str,
) -> impl std::future::Future<Output = Result<Value, ProtocolError>> {
    let context = context.to_string();
    async move {
        response
            .json::<Value>()
            .await
            .map_err(|err| ProtocolError::new(None, format!("{} parse failed: {}", context, err)))
    }
}

fn response_code(raw: &Value) -> i64 {
    raw.get("errno")
        .and_then(|value| value.as_i64())
        .or_else(|| raw.get("code").and_then(|value| value.as_i64()))
        .unwrap_or(-1)
}

fn response_message(raw: &Value) -> String {
    raw.get("msg")
        .and_then(|value| value.as_str())
        .or_else(|| raw.get("message").and_then(|value| value.as_str()))
        .unwrap_or("unknown error")
        .to_string()
}

fn string_at(raw: &Value, path: &[&str]) -> Option<String> {
    let value = value_at(raw, path)?;
    value
        .as_str()
        .map(|value| value.to_string())
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .or_else(|| value.as_u64().map(|value| value.to_string()))
}

fn required_string_at(raw: &Value, path: &[&str], label: &str) -> Result<String, ProtocolError> {
    string_at(raw, path)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ProtocolError::new(None, format!("missing {}", label)))
}

fn u32_at(raw: &Value, path: &[&str]) -> Option<u32> {
    let value = value_at(raw, path)?;
    value
        .as_u64()
        .map(|value| value as u32)
        .or_else(|| value.as_i64().and_then(|value| (value >= 0).then_some(value as u32)))
}

fn value_at<'a>(raw: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = raw;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn is_pending_message(message: &str) -> bool {
    message.contains("排队") || message.contains("处理中") || message.contains("稍后")
}

fn current_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
