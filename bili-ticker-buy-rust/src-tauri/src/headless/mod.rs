pub mod auth;
pub mod handlers;
pub mod router;
pub mod ws;

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct HeadlessState {
    pub server_token: Option<String>,
    pub sessions: auth::SessionStore,
    pub tasks: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    pub ws_hub: ws::WsHub,
}
