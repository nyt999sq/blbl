use bili_ticker_buy_rust::core::storage::configure_data_dir;
use bili_ticker_buy_rust::headless::auth::new_session_store;
use bili_ticker_buy_rust::headless::router::build_router;
use bili_ticker_buy_rust::headless::ws::WsHub;
use bili_ticker_buy_rust::headless::HeadlessState;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[derive(Debug, Parser)]
#[command(name = "headless")]
#[command(about = "Bili ticker buy headless runner")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 18080)]
        port: u16,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
    Run {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Serve {
            host,
            port,
            token,
            data_dir,
        } => {
            if host != "127.0.0.1" && host != "localhost" && host != "::1" {
                if token.as_ref().map(|t| t.trim().is_empty()).unwrap_or(true) {
                    anyhow::bail!("--host 非本地地址时必须提供 --token");
                }
            }

            let data_root = data_dir.unwrap_or_else(|| PathBuf::from("./data"));
            configure_data_dir(Some(data_root.clone()));

            let static_dir = if PathBuf::from("../dist").exists() {
                PathBuf::from("../dist")
            } else {
                PathBuf::from("./dist")
            };

            let state = HeadlessState {
                server_token: token,
                sessions: new_session_store(),
                tasks: Arc::new(Mutex::new(HashMap::<String, Arc<AtomicBool>>::new())),
                ws_hub: WsHub::new(),
            };

            let app = build_router(state, static_dir);
            let listener = TcpListener::bind(format!("{}:{}", host, port)).await?;
            println!(
                "headless server listening on {}:{} data_dir={}",
                host,
                port,
                data_root.display()
            );
            axum::serve(listener, app).await?;
        }
        Commands::Run { config, data_dir } => {
            let data_root = data_dir.unwrap_or_else(|| PathBuf::from("./data"));
            configure_data_dir(Some(data_root.clone()));
            println!(
                "headless run 暂为占位实现，config={} data_dir={}",
                config.display(),
                data_root.display()
            );
        }
    }
    Ok(())
}
