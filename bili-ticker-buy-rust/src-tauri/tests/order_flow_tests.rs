use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use bili_ticker_buy_rust::buy::{start_buy_task, TicketInfo};
use bili_ticker_buy_rust::core::events::TaskEventSink;
use bili_ticker_buy_rust::order_protocol::{OrderProtocol, SubmitOrderResult};
use bili_ticker_buy_rust::util::CTokenGenerator;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;

fn sample_ticket_info(is_hot_project: bool) -> TicketInfo {
    TicketInfo {
        project_id: "project-1".to_string(),
        project_name: Some("测试项目".to_string()),
        screen_id: "screen-1".to_string(),
        sku_id: "sku-1".to_string(),
        count: 2,
        buyer_info: json!([{ "id": "buyer-1", "name": "Alice", "tel": "13800000000" }]),
        deliver_info: json!({ "name": "Alice", "phone": "13800000000" }),
        cookies: vec!["SESSDATA=abc".to_string(), "bili_jct=csrf".to_string()],
        is_hot_project: Some(is_hot_project),
        pay_money: Some(6800),
        contact_name: Some("Alice".to_string()),
        contact_tel: Some("13800000000".to_string()),
    }
}

fn sample_generator() -> CTokenGenerator {
    CTokenGenerator::new(1_700_000_000, 0, 2500)
}

async fn start_server(app: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{}", addr)
}

#[derive(Default)]
struct NoopSink;

impl TaskEventSink for NoopSink {
    fn emit_log(&self, _task_id: &str, _message: &str) {}

    fn emit_payment_qrcode(&self, _task_id: &str, _url: &str) {}

    fn emit_task_result(&self, _task_id: &str, _success: bool, _message: &str) {}
}

#[tokio::test]
async fn submit_order_completes_non_hot_success_flow() {
    async fn prepare_handler(Json(payload): Json<Value>) -> Json<Value> {
        assert_eq!(payload["project_id"], "project-1");
        Json(json!({
            "errno": 0,
            "data": {
                "token": "prepare-token"
            }
        }))
    }

    async fn confirm_handler(Query(query): Query<HashMap<String, String>>) -> Json<Value> {
        assert_eq!(query.get("token"), Some(&"prepare-token".to_string()));
        Json(json!({
            "errno": 0,
            "data": {
                "count": 2,
                "pay_money": 6800
            }
        }))
    }

    async fn create_handler(Json(payload): Json<Value>) -> Json<Value> {
        assert_eq!(payload["token"], "prepare-token");
        assert!(payload.get("ctoken").is_none());
        Json(json!({
            "errno": 0,
            "data": {
                "orderId": "order-1"
            }
        }))
    }

    async fn pay_handler(Query(query): Query<HashMap<String, String>>) -> Json<Value> {
        assert_eq!(query.get("order_id"), Some(&"order-1".to_string()));
        Json(json!({
            "errno": 0,
            "data": {
                "code_url": "https://pay.example/order-1"
            }
        }))
    }

    let base_url = start_server(
        Router::new()
            .route("/api/ticket/order/prepare", post(prepare_handler))
            .route("/api/ticket/order/confirmInfo", get(confirm_handler))
            .route("/api/ticket/order/createV2", post(create_handler))
            .route("/api/ticket/order/getPayParam", get(pay_handler)),
    )
    .await;

    let protocol = OrderProtocol::with_base_url(base_url);
    let client = reqwest::Client::new();
    let stop_flag = AtomicBool::new(false);

    let result = protocol
        .submit_order(
            &client,
            &sample_ticket_info(false),
            &mut sample_generator(),
            "device-1",
            &stop_flag,
        )
        .await
        .expect("submit order");

    assert_eq!(
        result,
        SubmitOrderResult::Success {
            order_id: "order-1".to_string(),
            payment_url: Some("https://pay.example/order-1".to_string()),
        }
    );
}

#[tokio::test]
async fn submit_order_uses_hot_ticket_fields_only_for_hot_projects() {
    #[derive(Clone)]
    struct HotBodies {
        prepare: Arc<Mutex<Option<Value>>>,
        create: Arc<Mutex<Option<Value>>>,
    }

    let prepare_body = Arc::new(Mutex::new(None::<Value>));
    let create_body = Arc::new(Mutex::new(None::<Value>));

    async fn prepare_handler(
        State(state): State<HotBodies>,
        Json(payload): Json<Value>,
    ) -> Json<Value> {
        *state.prepare.lock().unwrap() = Some(payload);
        Json(json!({
            "errno": 0,
            "data": {
                "token": "prepare-token",
                "ptoken": "project-token"
            }
        }))
    }

    async fn confirm_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "count": 2,
                "pay_money": 6800
            }
        }))
    }

    async fn create_handler(
        State(state): State<HotBodies>,
        Query(query): Query<HashMap<String, String>>,
        Json(payload): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(query.get("ptoken"), Some(&"project-token".to_string()));
        *state.create.lock().unwrap() = Some(payload);
        Json(json!({
            "errno": 0,
            "data": {
                "orderId": "order-hot"
            }
        }))
    }

    async fn pay_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "code_url": "https://pay.example/hot"
            }
        }))
    }

    let base_url = start_server(
        Router::new()
            .route("/api/ticket/order/prepare", post(prepare_handler))
            .route("/api/ticket/order/confirmInfo", get(confirm_handler))
            .route("/api/ticket/order/createV2", post(create_handler))
            .route("/api/ticket/order/getPayParam", get(pay_handler))
            .with_state(HotBodies {
                prepare: prepare_body.clone(),
                create: create_body.clone(),
            }),
    )
    .await;

    let protocol = OrderProtocol::with_base_url(base_url);
    let client = reqwest::Client::new();
    let stop_flag = AtomicBool::new(false);

    let result = protocol
        .submit_order(
            &client,
            &sample_ticket_info(true),
            &mut sample_generator(),
            "device-hot",
            &stop_flag,
        )
        .await
        .expect("submit hot order");

    assert_eq!(
        result,
        SubmitOrderResult::Success {
            order_id: "order-hot".to_string(),
            payment_url: Some("https://pay.example/hot".to_string()),
        }
    );

    let prepare = prepare_body.lock().unwrap().clone().expect("prepare payload");
    let create = create_body.lock().unwrap().clone().expect("create payload");
    assert!(prepare["token"].as_str().is_some_and(|value| !value.is_empty()));
    assert!(create["ctoken"].as_str().is_some_and(|value| !value.is_empty()));
    assert_eq!(create["ptoken"], "project-token");
    assert_eq!(
        create["orderCreateUrl"],
        "https://show.bilibili.com/api/ticket/order/createV2"
    );
}

#[tokio::test]
async fn submit_order_polls_create_status_until_success() {
    let status_calls = Arc::new(AtomicUsize::new(0));

    async fn prepare_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "token": "prepare-token"
            }
        }))
    }

    async fn confirm_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "count": 2,
                "pay_money": 6800
            }
        }))
    }

    async fn create_handler() -> Json<Value> {
        Json(json!({
            "errno": 100079,
            "msg": "处理中",
            "data": {
                "token": "pay-token"
            }
        }))
    }

    async fn status_handler(
        State(calls): State<Arc<AtomicUsize>>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Json<Value> {
        assert_eq!(query.get("token"), Some(&"pay-token".to_string()));
        let current = calls.fetch_add(1, Ordering::SeqCst);
        if current == 0 {
            return Json(json!({
                "errno": 100079,
                "msg": "排队中"
            }));
        }

        Json(json!({
            "errno": 0,
            "data": {
                "order_id": "order-pending"
            }
        }))
    }

    async fn pay_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "code_url": "https://pay.example/pending"
            }
        }))
    }

    let base_url = start_server(
        Router::new()
            .route("/api/ticket/order/prepare", post(prepare_handler))
            .route("/api/ticket/order/confirmInfo", get(confirm_handler))
            .route("/api/ticket/order/createV2", post(create_handler))
            .route("/api/ticket/order/createstatus", get(status_handler))
            .route("/api/ticket/order/getPayParam", get(pay_handler))
            .with_state(status_calls.clone()),
    )
    .await;

    let protocol = OrderProtocol::with_base_url(base_url)
        .with_status_poll_interval(Duration::from_millis(1))
        .with_status_poll_attempts(3);
    let client = reqwest::Client::new();
    let stop_flag = AtomicBool::new(false);

    let result = protocol
        .submit_order(
            &client,
            &sample_ticket_info(false),
            &mut sample_generator(),
            "device-pending",
            &stop_flag,
        )
        .await
        .expect("submit pending order");

    assert_eq!(
        result,
        SubmitOrderResult::Success {
            order_id: "order-pending".to_string(),
            payment_url: Some("https://pay.example/pending".to_string()),
        }
    );
    assert_eq!(status_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn submit_order_returns_price_changed_when_create_reports_new_price() {
    async fn prepare_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "token": "prepare-token"
            }
        }))
    }

    async fn confirm_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "count": 2,
                "pay_money": 6800
            }
        }))
    }

    async fn create_handler() -> Json<Value> {
        Json(json!({
            "errno": 100034,
            "msg": "价格变化",
            "data": {
                "pay_money": 7200
            }
        }))
    }

    let base_url = start_server(
        Router::new()
            .route("/api/ticket/order/prepare", post(prepare_handler))
            .route("/api/ticket/order/confirmInfo", get(confirm_handler))
            .route("/api/ticket/order/createV2", post(create_handler)),
    )
    .await;

    let protocol = OrderProtocol::with_base_url(base_url);
    let client = reqwest::Client::new();
    let stop_flag = AtomicBool::new(false);

    let result = protocol
        .submit_order(
            &client,
            &sample_ticket_info(false),
            &mut sample_generator(),
            "device-price",
            &stop_flag,
        )
        .await
        .expect("submit price changed order");

    assert_eq!(result, SubmitOrderResult::PriceChanged { pay_money: 7200 });
}

#[tokio::test]
async fn submit_order_returns_token_expired_when_create_reports_expired_token() {
    async fn prepare_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "token": "prepare-token"
            }
        }))
    }

    async fn confirm_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "count": 2,
                "pay_money": 6800
            }
        }))
    }

    async fn create_handler() -> Json<Value> {
        Json(json!({
            "errno": 100051,
            "msg": "token expired"
        }))
    }

    let base_url = start_server(
        Router::new()
            .route("/api/ticket/order/prepare", post(prepare_handler))
            .route("/api/ticket/order/confirmInfo", get(confirm_handler))
            .route("/api/ticket/order/createV2", post(create_handler)),
    )
    .await;

    let protocol = OrderProtocol::with_base_url(base_url);
    let client = reqwest::Client::new();
    let stop_flag = AtomicBool::new(false);

    let result = protocol
        .submit_order(
            &client,
            &sample_ticket_info(false),
            &mut sample_generator(),
            "device-expired",
            &stop_flag,
        )
        .await
        .expect("submit expired order");

    assert_eq!(result, SubmitOrderResult::TokenExpired);
}

#[tokio::test]
async fn submit_order_stops_while_polling_create_status() {
    let status_calls = Arc::new(AtomicUsize::new(0));
    let stop_flag = Arc::new(AtomicBool::new(false));

    async fn prepare_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "token": "prepare-token"
            }
        }))
    }

    async fn confirm_handler() -> Json<Value> {
        Json(json!({
            "errno": 0,
            "data": {
                "count": 2,
                "pay_money": 6800
            }
        }))
    }

    async fn create_handler() -> Json<Value> {
        Json(json!({
            "errno": 100079,
            "msg": "处理中",
            "data": {
                "token": "pay-token"
            }
        }))
    }

    async fn status_handler(
        State((calls, stop_flag)): State<(Arc<AtomicUsize>, Arc<AtomicBool>)>,
    ) -> (StatusCode, Json<Value>) {
        calls.fetch_add(1, Ordering::SeqCst);
        stop_flag.store(true, Ordering::SeqCst);
        (
            StatusCode::OK,
            Json(json!({
                "errno": 100079,
                "msg": "排队中"
            })),
        )
    }

    let base_url = start_server(
        Router::new()
            .route("/api/ticket/order/prepare", post(prepare_handler))
            .route("/api/ticket/order/confirmInfo", get(confirm_handler))
            .route("/api/ticket/order/createV2", post(create_handler))
            .route("/api/ticket/order/createstatus", get(status_handler))
            .with_state((status_calls.clone(), stop_flag.clone())),
    )
    .await;

    let protocol = OrderProtocol::with_base_url(base_url)
        .with_status_poll_interval(Duration::from_millis(1))
        .with_status_poll_attempts(3);
    let client = reqwest::Client::new();

    let result = protocol
        .submit_order(
            &client,
            &sample_ticket_info(false),
            &mut sample_generator(),
            "device-stop",
            stop_flag.as_ref(),
        )
        .await
        .expect("submit stop order");

    assert_eq!(result, SubmitOrderResult::Stopped);
    assert_eq!(status_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn start_buy_task_returns_when_stopped_during_scheduled_wait() {
    let stop_flag = Arc::new(AtomicBool::new(true));
    let sink: Arc<dyn TaskEventSink + Send + Sync> = Arc::new(NoopSink);

    let result = start_buy_task(
        sink,
        "task-wait-stop".to_string(),
        stop_flag,
        sample_ticket_info(false),
        50,
        0,
        1,
        Some("2099-01-01 00:00:00".to_string()),
        None,
        None,
        None,
    )
    .await;

    assert!(result.is_ok());
}
