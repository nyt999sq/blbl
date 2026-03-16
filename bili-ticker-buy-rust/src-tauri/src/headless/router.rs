use crate::headless::auth;
use crate::headless::handlers;
use crate::headless::ws;
use crate::headless::HeadlessState;
use axum::middleware;
use axum::routing::{delete, get, post};
use axum::Router;
use std::path::PathBuf;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

pub fn build_router(state: HeadlessState, static_dir: PathBuf) -> Router {
    let public_routes = Router::new()
        .route("/api/auth/token-login", post(handlers::token_login))
        .route("/api/login/qrcode", get(handlers::get_login_qrcode))
        .route("/api/login/poll", get(handlers::poll_login_status))
        .route("/api/login/sms/countries", get(handlers::get_sms_login_countries))
        .route("/api/login/sms/captcha", get(handlers::get_sms_login_captcha))
        .route("/api/login/sms/send-code", post(handlers::send_sms_login_code))
        .route("/api/login/sms/verify-code", post(handlers::verify_sms_login_code))
        .route("/api/share/:token", get(handlers::get_share_preset_public))
        .route("/api/share/:token/buyers", post(handlers::fetch_share_buyers))
        .route(
            "/api/share/:token/addresses",
            post(handlers::fetch_share_addresses),
        )
        .route("/api/share/:token/user-info", post(handlers::fetch_share_user_info))
        .route("/api/share/:token/submit", post(handlers::submit_share_preset))
        .route("/api/ws", get(ws::ws_handler));

    let protected_routes = Router::new()
        .route("/api/accounts/import-cookie", post(handlers::import_cookie))
        .route("/api/accounts", get(handlers::get_accounts))
        .route("/api/accounts/:uid", delete(handlers::delete_account))
        .route("/api/project/fetch", post(handlers::fetch_project))
        .route("/api/project/buyers", post(handlers::fetch_buyers))
        .route("/api/project/addresses", post(handlers::fetch_addresses))
        .route("/api/user/info", post(handlers::get_user_info))
        .route("/api/time/sync", post(handlers::sync_time))
        .route("/api/tasks", get(handlers::list_tasks).post(handlers::create_task))
        .route(
            "/api/tasks/:id",
            delete(handlers::delete_task).patch(handlers::update_task),
        )
        .route("/api/tasks/:id/start", post(handlers::start_task_record))
        .route("/api/tasks/:id/stop", post(handlers::stop_task_record))
        .route("/api/task/start", post(handlers::start_task))
        .route("/api/task/stop", post(handlers::stop_task))
        .route(
            "/api/history",
            get(handlers::get_history).delete(handlers::clear_history),
        )
        .route("/api/project-history", get(handlers::get_project_history))
        .route("/api/project-history", post(handlers::add_project_history))
        .route(
            "/api/project-history",
            delete(handlers::delete_project_history),
        )
        .route(
            "/api/share/presets",
            get(handlers::list_share_presets).post(handlers::create_share_preset),
        )
        .route(
            "/api/share/presets/batch-delete",
            post(handlers::batch_delete_share_presets),
        )
        .route(
            "/api/share/presets/:id/close",
            post(handlers::close_share_preset),
        )
        .route(
            "/api/share/presets/:id/export-config",
            get(handlers::export_share_preset_config),
        )
        .route(
            "/api/share/presets/:id/start-task",
            post(handlers::start_share_preset_task),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_session,
        ));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .fallback_service(ServeDir::new(static_dir).append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
