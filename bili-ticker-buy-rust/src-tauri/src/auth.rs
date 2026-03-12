use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

const QRCODE_GENERATE_URL: &str =
    "https://passport.bilibili.com/x/passport-login/web/qrcode/generate";
const QRCODE_POLL_URL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/poll";

pub async fn generate_qrcode() -> Result<(String, String)> {
    let client = Client::new();
    generate_qrcode_from_url(&client, QRCODE_GENERATE_URL).await
}

async fn generate_qrcode_from_url(client: &Client, url: &str) -> Result<(String, String)> {
    let res: Value = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await?
        .json()
        .await?;

    if res["code"].as_i64().unwrap_or(-1) == 0 {
        let url = res["data"]["url"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing qrcode url in response"))?
            .to_string();
        let qrcode_key = res["data"]["qrcode_key"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing qrcode key in response"))?
            .to_string();
        Ok((url, qrcode_key))
    } else {
        let message = res["message"]
            .as_str()
            .unwrap_or("Failed to generate QR code");
        Err(anyhow!(message.to_string()))
    }
}

pub async fn poll_login(qrcode_key: &str) -> Result<String> {
    let client = Client::new();
    poll_login_from_url(
        &client,
        QRCODE_POLL_URL,
        qrcode_key,
        120,
        Duration::from_secs(1),
    )
    .await
}

async fn poll_login_from_url(
    client: &Client,
    url: &str,
    qrcode_key: &str,
    max_attempts: usize,
    poll_interval: Duration,
) -> Result<String> {
    for _ in 0..max_attempts {
        let resp = client
            .get(url)
            .query(&[("qrcode_key", qrcode_key)])
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await?;

        let headers = resp.headers().clone();
        let res_json: Value = resp.json().await?;

        if let Some(code) = res_json["data"]["code"].as_i64() {
            if code == 0 {
                // B 站会通过多个 Set-Cookie 头返回登录态，这里统一收集并序列化给前端。
                let mut cookie_strings = Vec::new();
                for value in headers.get_all(reqwest::header::SET_COOKIE).iter() {
                    if let Ok(cookie) = value.to_str() {
                        cookie_strings.push(cookie.to_string());
                    }
                }
                return Ok(serde_json::to_string(&cookie_strings)?);
            }

            // 86101/86090 都表示二维码状态仍在等待（未扫码/已扫码待确认），继续轮询。
            if code == 86101 || code == 86090 {
                tokio::time::sleep(poll_interval).await;
                continue;
            }

            let message = res_json["data"]["message"]
                .as_str()
                .unwrap_or("Unknown login error");
            return Err(anyhow!("Login failed: {}", message));
        }

        tokio::time::sleep(poll_interval).await;
    }

    Err(anyhow!("Login timeout"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Query, State};
    use axum::http::header::SET_COOKIE;
    use axum::http::HeaderValue;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::{Json, Router};
    use serde::Deserialize;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::net::TcpListener;

    async fn start_server(app: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("get local addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn generate_qrcode_returns_url_and_key() {
        async fn handler() -> Json<serde_json::Value> {
            Json(json!({
                "code": 0,
                "data": {
                    "url": "https://example.com/qr",
                    "qrcode_key": "key-123"
                }
            }))
        }

        let base = start_server(Router::new().route("/generate", get(handler))).await;
        let client = reqwest::Client::new();

        let got = generate_qrcode_from_url(&client, &format!("{}/generate", base))
            .await
            .expect("generate qrcode should succeed");

        assert_eq!(got.0, "https://example.com/qr");
        assert_eq!(got.1, "key-123");
    }

    #[derive(Debug, Deserialize)]
    struct PollReq {
        qrcode_key: String,
    }

    #[tokio::test]
    async fn poll_login_retries_then_returns_cookie_json() {
        async fn handler(
            State(counter): State<Arc<AtomicUsize>>,
            Query(query): Query<PollReq>,
        ) -> axum::response::Response {
            assert_eq!(query.qrcode_key, "test-key");
            let attempt = counter.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                return Json(json!({
                    "data": { "code": 86101 }
                }))
                .into_response();
            }

            let mut response = Json(json!({
                "data": { "code": 0 }
            }))
            .into_response();
            response.headers_mut().append(
                SET_COOKIE,
                HeaderValue::from_static("SESSDATA=abc; Path=/; HttpOnly"),
            );
            response
                .headers_mut()
                .append(SET_COOKIE, HeaderValue::from_static("bili_jct=def; Path=/"));
            response
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route("/poll", get(handler))
            .with_state(counter.clone());
        let base = start_server(app).await;
        let client = reqwest::Client::new();

        let raw = poll_login_from_url(
            &client,
            &format!("{}/poll", base),
            "test-key",
            2,
            std::time::Duration::from_millis(1),
        )
        .await
        .expect("poll login should succeed");

        let cookies: Vec<String> = serde_json::from_str(&raw).expect("cookie json");
        assert_eq!(cookies.len(), 2);
        assert!(cookies.iter().any(|s| s.starts_with("SESSDATA=abc")));
        assert!(cookies.iter().any(|s| s.starts_with("bili_jct=def")));
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
