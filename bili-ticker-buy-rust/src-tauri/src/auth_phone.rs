use anyhow::{anyhow, Result};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::SET_COOKIE;
use reqwest::redirect::Policy;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;

const COUNTRY_URL: &str = "https://passport.bilibili.com/x/passport-login/web/country";
const CAPTCHA_URL: &str = "https://passport.bilibili.com/x/passport-login/captcha?source=main-fe";
const SMS_SEND_URL: &str = "https://passport.bilibili.com/x/passport-login/web/sms/send";
const SMS_LOGIN_URL: &str = "https://passport.bilibili.com/x/passport-login/web/login/sms";
const USER_INFO_URL: &str = "https://api.bilibili.com/x/web-interface/nav";
const DEFAULT_SOURCE: &str = "main_web";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneCountryItem {
    pub id: i64,
    pub country_code: String,
    pub cname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneCountriesResponse {
    pub default: Option<PhoneCountryItem>,
    pub list: Vec<PhoneCountryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeetestPayload {
    pub challenge: String,
    pub gt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsCaptchaPayload {
    #[serde(rename = "type")]
    pub captcha_type: String,
    pub token: String,
    pub geetest: Option<GeetestPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendSmsCodeRequest {
    pub tel: String,
    pub cid: String,
    pub go_url: Option<String>,
    pub source: Option<String>,
    pub token: String,
    pub validate: Option<String>,
    pub seccode: Option<String>,
    pub challenge: Option<String>,
    pub captcha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendSmsCodeResponse {
    pub captcha_key: String,
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifySmsCodeRequest {
    pub tel: String,
    pub cid: String,
    pub code: String,
    pub captcha_key: String,
    pub go_url: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifySmsCodeResponse {
    pub cookies: Vec<String>,
    pub user_info: SmsLoginUserInfo,
    pub is_new: Option<bool>,
    pub status: Option<i64>,
    pub redirect_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsLoginUserInfo {
    pub mid: String,
    pub uname: String,
    pub face: Option<String>,
}

fn default_headers(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    request.header(
        "User-Agent",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36",
    )
}

fn normalized_source(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(DEFAULT_SOURCE)
        .to_string()
}

fn extract_cookie_strings(headers: &reqwest::header::HeaderMap) -> Vec<String> {
    headers
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next().map(str::trim))
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect()
}

fn build_cookie_client() -> Result<(Client, Arc<Jar>)> {
    let jar = Arc::new(Jar::default());
    let client = Client::builder()
        .cookie_provider(jar.clone())
        .redirect(Policy::limited(10))
        .build()?;
    Ok((client, jar))
}

fn resolve_redirect_url(value: &str) -> Option<Url> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Url::parse(trimmed)
        .ok()
        .or_else(|| Url::parse("https://www.bilibili.com").ok()?.join(trimmed).ok())
}

fn collect_cookie_strings_from_jar(jar: &Jar, extra_urls: &[Url]) -> Vec<String> {
    let mut urls = vec![
        Url::parse("https://passport.bilibili.com/").expect("passport url"),
        Url::parse("https://api.bilibili.com/").expect("api url"),
        Url::parse("https://show.bilibili.com/").expect("show url"),
        Url::parse("https://www.bilibili.com/").expect("www url"),
    ];
    urls.extend(extra_urls.iter().cloned());

    let mut seen = HashSet::new();
    let mut cookies = Vec::new();
    for url in urls {
        let Some(header_value) = jar.cookies(&url) else {
            continue;
        };
        let Ok(raw) = header_value.to_str() else {
            continue;
        };
        for item in raw.split(';').map(str::trim).filter(|value| !value.is_empty()) {
            let Some((name, _)) = item.split_once('=') else {
                continue;
            };
            let key = name.trim().to_string();
            if seen.insert(key) {
                cookies.push(item.to_string());
            }
        }
    }
    cookies
}

fn normalize_mid(value: &Value) -> String {
    value
        .as_str()
        .map(|mid| mid.to_string())
        .or_else(|| value.as_i64().map(|mid| mid.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

async fn fetch_user_info_with_url(url: &str, cookies: Vec<String>) -> Result<Value> {
    let client = Client::new();
    let mut req = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        );

    if !cookies.is_empty() {
        req = req.header("Cookie", cookies.join("; "));
    }

    Ok(req.send().await?.json().await?)
}

pub async fn fetch_phone_countries() -> Result<PhoneCountriesResponse> {
    let client = Client::new();
    fetch_phone_countries_from_url(&client, COUNTRY_URL).await
}

async fn fetch_phone_countries_from_url(
    client: &Client,
    url: &str,
) -> Result<PhoneCountriesResponse> {
    let res: Value = default_headers(client.get(url)).send().await?.json().await?;
    if res["code"].as_i64().unwrap_or(-1) != 0 {
        return Err(anyhow!(
            "{}",
            res["message"].as_str().unwrap_or("获取国家区号失败")
        ));
    }

    let default = serde_json::from_value::<PhoneCountryItem>(res["data"]["default"].clone()).ok();
    let list = serde_json::from_value::<Vec<PhoneCountryItem>>(res["data"]["list"].clone())
        .unwrap_or_default();

    Ok(PhoneCountriesResponse { default, list })
}

pub async fn fetch_sms_captcha() -> Result<SmsCaptchaPayload> {
    let client = Client::new();
    fetch_sms_captcha_from_url(&client, CAPTCHA_URL).await
}

async fn fetch_sms_captcha_from_url(client: &Client, url: &str) -> Result<SmsCaptchaPayload> {
    let res: Value = default_headers(client.get(url)).send().await?.json().await?;
    if res["code"].as_i64().unwrap_or(-1) != 0 {
        return Err(anyhow!(
            "{}",
            res["message"].as_str().unwrap_or("获取人机验证参数失败")
        ));
    }

    serde_json::from_value::<SmsCaptchaPayload>(res["data"].clone())
        .map_err(|err| anyhow!("解析 captcha 响应失败: {}", err))
}

pub async fn send_sms_code(payload: SendSmsCodeRequest) -> Result<SendSmsCodeResponse> {
    let client = Client::new();
    send_sms_code_to_url(&client, SMS_SEND_URL, payload).await
}

async fn send_sms_code_to_url(
    client: &Client,
    url: &str,
    payload: SendSmsCodeRequest,
) -> Result<SendSmsCodeResponse> {
    let mut form = vec![
        ("source".to_string(), normalized_source(payload.source.as_deref())),
        ("tel".to_string(), payload.tel),
        ("cid".to_string(), payload.cid),
        ("token".to_string(), payload.token),
        ("go_url".to_string(), payload.go_url.unwrap_or_default()),
    ];

    if let Some(value) = payload.validate.filter(|value| !value.trim().is_empty()) {
        form.push(("validate".to_string(), value));
    }
    if let Some(value) = payload.seccode.filter(|value| !value.trim().is_empty()) {
        form.push(("seccode".to_string(), value));
    }
    if let Some(value) = payload.challenge.filter(|value| !value.trim().is_empty()) {
        form.push(("challenge".to_string(), value));
    }
    if let Some(value) = payload.captcha.filter(|value| !value.trim().is_empty()) {
        form.push(("captcha".to_string(), value));
    }

    let res: Value = default_headers(client.post(url))
        .form(&form)
        .send()
        .await?
        .json()
        .await?;

    let code = res["code"].as_i64().unwrap_or(-1);
    let message = res["message"]
        .as_str()
        .unwrap_or("发送短信验证码失败")
        .to_string();
    if code != 0 {
        return Err(anyhow!("{}", message));
    }

    let captcha_key = res["data"]["captcha_key"]
        .as_str()
        .ok_or_else(|| anyhow!("发送验证码成功但缺少 captcha_key"))?
        .to_string();

    Ok(SendSmsCodeResponse {
        captcha_key,
        code,
        message,
    })
}

pub async fn verify_sms_code(payload: VerifySmsCodeRequest) -> Result<VerifySmsCodeResponse> {
    let (client, jar) = build_cookie_client()?;
    verify_sms_code_to_url(&client, jar, SMS_LOGIN_URL, USER_INFO_URL, payload).await
}

async fn verify_sms_code_to_url(
    client: &Client,
    jar: Arc<Jar>,
    url: &str,
    user_info_url: &str,
    payload: VerifySmsCodeRequest,
) -> Result<VerifySmsCodeResponse> {
    let form = vec![
        ("source".to_string(), normalized_source(payload.source.as_deref())),
        ("tel".to_string(), payload.tel),
        ("cid".to_string(), payload.cid),
        ("code".to_string(), payload.code),
        ("captcha_key".to_string(), payload.captcha_key),
        ("go_url".to_string(), payload.go_url.unwrap_or_default()),
    ];

    let resp = default_headers(client.post(url)).form(&form).send().await?;
    let headers = resp.headers().clone();
    let res: Value = resp.json().await?;

    let code = res["code"].as_i64().unwrap_or(-1);
    if code != 0 {
        return Err(anyhow!(
            "{}",
            res["message"].as_str().unwrap_or("短信验证码登录失败")
        ));
    }

    let redirect_url = res["data"]["url"]
        .as_str()
        .map(|value| value.to_string())
        .or_else(|| res["data"]["redirectUrl"].as_str().map(|value| value.to_string()));

    let redirect_target = redirect_url
        .as_deref()
        .and_then(resolve_redirect_url);

    if let Some(url) = redirect_target.as_ref() {
        let _ = default_headers(client.get(url.clone())).send().await;
    }

    let mut cookies = collect_cookie_strings_from_jar(
        jar.as_ref(),
        &redirect_target.iter().cloned().collect::<Vec<_>>(),
    );
    if cookies.is_empty() {
        cookies = extract_cookie_strings(&headers);
    }
    if cookies.is_empty() {
        return Err(anyhow!("短信登录成功，但未获取到登录 Cookies"));
    }

    let user_info = fetch_user_info_with_url(user_info_url, cookies.clone()).await?;
    if user_info["code"].as_i64().unwrap_or(-1) != 0 {
        return Err(anyhow!(
            "{}",
            user_info["message"]
                .as_str()
                .unwrap_or("短信登录成功，但登录态校验失败")
        ));
    }

    let data = &user_info["data"];
    let user_info = SmsLoginUserInfo {
        mid: normalize_mid(&data["mid"]),
        uname: data["uname"]
            .as_str()
            .unwrap_or("未知账号")
            .to_string(),
        face: data["face"].as_str().map(|value| value.to_string()),
    };

    Ok(VerifySmsCodeResponse {
        cookies,
        user_info,
        is_new: res["data"]["is_new"].as_bool(),
        status: res["data"]["status"].as_i64(),
        redirect_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Form;
    use axum::http::header::SET_COOKIE;
    use axum::http::HeaderValue;
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde::Deserialize;
    use serde_json::json;
    use tokio::net::TcpListener;

    async fn start_server(app: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn fetch_phone_countries_returns_default_and_list() {
        async fn handler() -> Json<Value> {
            Json(json!({
                "code": 0,
                "data": {
                    "default": { "id": 1, "country_code": "86", "cname": "中国大陆" },
                    "list": [{ "id": 1, "country_code": "86", "cname": "中国大陆" }]
                }
            }))
        }

        let base = start_server(Router::new().route("/country", get(handler))).await;
        let client = Client::new();
        let result = fetch_phone_countries_from_url(&client, &format!("{}/country", base))
            .await
            .expect("countries");

        assert_eq!(result.default.unwrap().country_code, "86");
        assert_eq!(result.list.len(), 1);
    }

    #[tokio::test]
    async fn fetch_sms_captcha_returns_geetest_payload() {
        async fn handler() -> Json<Value> {
            Json(json!({
                "code": 0,
                "data": {
                    "type": "geetest",
                    "token": "token-1",
                    "geetest": {
                        "challenge": "challenge-1",
                        "gt": "gt-1"
                    }
                }
            }))
        }

        let base = start_server(Router::new().route("/captcha", get(handler))).await;
        let client = Client::new();
        let result = fetch_sms_captcha_from_url(&client, &format!("{}/captcha", base))
            .await
            .expect("captcha");

        assert_eq!(result.captcha_type, "geetest");
        assert_eq!(result.token, "token-1");
        assert_eq!(result.geetest.unwrap().gt, "gt-1");
    }

    #[derive(Debug, Deserialize)]
    struct SmsSendForm {
        source: String,
        tel: String,
        cid: String,
        token: String,
        validate: Option<String>,
        seccode: Option<String>,
        challenge: Option<String>,
        captcha: Option<String>,
    }

    #[tokio::test]
    async fn send_sms_code_posts_form_and_returns_captcha_key() {
        async fn handler(Form(payload): Form<SmsSendForm>) -> Json<Value> {
            assert_eq!(payload.source, "main_web");
            assert_eq!(payload.tel, "13800138000");
            assert_eq!(payload.cid, "86");
            assert_eq!(payload.token, "token-1");
            assert_eq!(payload.validate.as_deref(), Some("validate-1"));
            assert_eq!(payload.seccode.as_deref(), Some("seccode-1"));
            assert_eq!(payload.challenge.as_deref(), Some("challenge-1"));
            assert_eq!(payload.captcha, None);

            Json(json!({
                "code": 0,
                "message": "OK",
                "data": { "captcha_key": "captcha-key-1" }
            }))
        }

        let base = start_server(Router::new().route("/send", post(handler))).await;
        let client = Client::new();
        let result = send_sms_code_to_url(
            &client,
            &format!("{}/send", base),
            SendSmsCodeRequest {
                tel: "13800138000".to_string(),
                cid: "86".to_string(),
                go_url: None,
                source: None,
                token: "token-1".to_string(),
                validate: Some("validate-1".to_string()),
                seccode: Some("seccode-1".to_string()),
                challenge: Some("challenge-1".to_string()),
                captcha: None,
            },
        )
        .await
        .expect("send sms");

        assert_eq!(result.captcha_key, "captcha-key-1");
    }

    #[derive(Debug, Deserialize)]
    struct SmsVerifyForm {
        source: String,
        tel: String,
        cid: String,
        code: String,
        captcha_key: String,
    }

    #[tokio::test]
    async fn verify_sms_code_collects_cookies_from_login_and_redirect_and_validates_user() {
        async fn verify_handler(
            headers: axum::http::HeaderMap,
            Form(payload): Form<SmsVerifyForm>,
        ) -> axum::response::Response {
            assert_eq!(payload.source, "main_web");
            assert_eq!(payload.tel, "13800138000");
            assert_eq!(payload.cid, "86");
            assert_eq!(payload.code, "123456");
            assert_eq!(payload.captcha_key, "captcha-key-1");

            let host = headers
                .get("host")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("127.0.0.1");

            let mut response = Json(json!({
                "code": 0,
                "message": "OK",
                "data": { "status": 0, "url": format!("http://{}/finish", host) }
            }))
            .into_response();
            response.headers_mut().append(
                SET_COOKIE,
                HeaderValue::from_static("SESSDATA=abc; Path=/; HttpOnly"),
            );
            response
        }

        async fn finish_handler() -> axum::response::Response {
            let mut response = Json(json!({
                "code": 0,
                "message": "OK"
            }))
            .into_response();
            response.headers_mut().append(
                SET_COOKIE,
                HeaderValue::from_static("bili_jct=def; Path=/"),
            );
            response
        }

        async fn user_info_handler(headers: axum::http::HeaderMap) -> Json<Value> {
            let cookie = headers
                .get("cookie")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            assert!(cookie.contains("SESSDATA=abc"));
            assert!(cookie.contains("bili_jct=def"));
            Json(json!({
                "code": 0,
                "data": {
                    "mid": 123456,
                    "uname": "测试账号",
                    "face": "https://example.com/face.jpg"
                }
            }))
        }

        let base = start_server(
            Router::new()
                .route("/verify", post(verify_handler))
                .route("/finish", get(finish_handler))
                .route("/user-info", get(user_info_handler)),
        )
        .await;
        let (client, jar) = build_cookie_client().expect("cookie client");
        let result = verify_sms_code_to_url(
            &client,
            jar,
            &format!("{}/verify", base),
            &format!("{}/user-info", base),
            VerifySmsCodeRequest {
                tel: "13800138000".to_string(),
                cid: "86".to_string(),
                code: "123456".to_string(),
                captcha_key: "captcha-key-1".to_string(),
                go_url: None,
                source: None,
            },
        )
        .await
        .expect("verify sms");

        assert_eq!(result.cookies.len(), 2);
        assert!(result.cookies.iter().any(|value| value == "SESSDATA=abc"));
        assert!(result.cookies.iter().any(|value| value == "bili_jct=def"));
        assert_eq!(result.user_info.mid, "123456");
        assert_eq!(result.user_info.uname, "测试账号");
        assert_eq!(result.status, Some(0));
    }

    #[tokio::test]
    async fn verify_sms_code_rejects_when_user_info_validation_fails() {
        async fn verify_handler(Form(_payload): Form<SmsVerifyForm>) -> axum::response::Response {
            let mut response = Json(json!({
                "code": 0,
                "message": "OK",
                "data": { "status": 0, "url": "" }
            }))
            .into_response();
            response.headers_mut().append(
                SET_COOKIE,
                HeaderValue::from_static("SESSDATA=abc; Path=/; HttpOnly"),
            );
            response
        }

        async fn user_info_handler() -> Json<Value> {
            Json(json!({
                "code": -101,
                "message": "账号未登录"
            }))
        }

        let base = start_server(
            Router::new()
                .route("/verify", post(verify_handler))
                .route("/user-info", get(user_info_handler)),
        )
        .await;
        let (client, jar) = build_cookie_client().expect("cookie client");
        let error = verify_sms_code_to_url(
            &client,
            jar,
            &format!("{}/verify", base),
            &format!("{}/user-info", base),
            VerifySmsCodeRequest {
                tel: "13800138000".to_string(),
                cid: "86".to_string(),
                code: "123456".to_string(),
                captcha_key: "captcha-key-1".to_string(),
                go_url: None,
                source: None,
            },
        )
        .await
        .expect_err("user info validation should fail");

        assert!(error.to_string().contains("账号未登录"));
    }
}
