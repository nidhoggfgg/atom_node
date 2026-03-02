#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod api;
mod config;
mod error;
mod executor;
mod models;
mod paths;
mod repository;
mod services;
#[cfg(target_os = "windows")]
mod windows_tray;

use crate::config::Config;
use crate::repository::{ExecutionRepository, PluginRepository, establish_connection};
use crate::services::{ExecutionService, PluginService, UpdateService};
use api::create_router;
use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const AUTH_COOKIE_NAME: &str = "anthill_token";
const AUTH_QUERY_KEY: &str = "token";
const AUTH_HEADER_NAME: &str = "x-anthill-token";

#[derive(Clone)]
struct AuthState {
    token: Arc<String>,
}

#[derive(Clone)]
struct SharedFileWriter {
    file: Arc<Mutex<File>>,
}

impl Write for SharedFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| std::io::Error::other("failed to lock log file"))?;
        file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| std::io::Error::other("failed to lock log file"))?;
        file.flush()
    }
}

fn init_tracing() -> anyhow::Result<PathBuf> {
    let logs_dir = crate::paths::install_root()?.join("logs");
    std::fs::create_dir_all(&logs_dir)?;
    let log_path = logs_dir.join("anthill.log");
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let shared_writer = SharedFileWriter {
        file: Arc::new(Mutex::new(log_file)),
    };

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "anthill=debug,tower_http=debug,axum=debug".into());

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(move || shared_writer.clone());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(log_path)
}

fn token_from_query(query: &str) -> Option<&str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        match (parts.next(), parts.next()) {
            (Some(key), Some(value)) if key == AUTH_QUERY_KEY => Some(value),
            _ => None,
        }
    })
}

fn token_from_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|cookie| {
        let trimmed = cookie.trim();
        let mut parts = trimmed.splitn(2, '=');
        match (parts.next(), parts.next()) {
            (Some(name), Some(value)) if name == AUTH_COOKIE_NAME => Some(value.to_string()),
            _ => None,
        }
    })
}

fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(token) = headers
        .get(AUTH_HEADER_NAME)
        .and_then(|value| value.to_str().ok())
    {
        return Some(token.to_string());
    }

    let auth_header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    auth_header
        .strip_prefix("Bearer ")
        .map(std::string::ToString::to_string)
}

async fn auth_middleware(
    axum::extract::State(auth_state): axum::extract::State<AuthState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let expected_token = auth_state.token.as_str();
    let query_token = request
        .uri()
        .query()
        .and_then(token_from_query)
        .map(std::string::ToString::to_string);
    let header_token = token_from_headers(request.headers());
    let cookie_token = token_from_cookie(request.headers());

    let has_valid_token = header_token.as_deref() == Some(expected_token)
        || cookie_token.as_deref() == Some(expected_token)
        || query_token.as_deref() == Some(expected_token);

    if !has_valid_token {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let mut response = next.run(request).await;

    // Allow first browser visit using query token, then persist via HttpOnly cookie.
    if query_token.as_deref() == Some(expected_token) {
        let cookie_value = format!(
            "{}={}; Path=/; HttpOnly; SameSite=Lax",
            AUTH_COOKIE_NAME, expected_token
        );
        if let Ok(cookie) = HeaderValue::from_str(&cookie_value) {
            response.headers_mut().append(header::SET_COOKIE, cookie);
        }
    }

    response
}

fn open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()?;

    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(url).status()?;

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let status = std::process::Command::new("xdg-open").arg(url).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(
            "browser command exited with non-zero status",
        ))
    }
}

fn prepend_bin_to_path() -> anyhow::Result<()> {
    let bin_dir = crate::paths::install_root()?.join("bin");
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();

    if !paths.iter().any(|path| path == &bin_dir) {
        paths.insert(0, bin_dir);
        let new_path = std::env::join_paths(paths)?;
        // SAFETY: We only mutate PATH at startup before spawning child processes.
        unsafe {
            std::env::set_var("PATH", new_path);
        }
    }

    Ok(())
}

async fn run_server<F>(shutdown: F) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    // Always persist logs to file so Windows GUI builds can be diagnosed.
    let log_path = init_tracing()?;
    tracing::info!("Log file: {}", log_path.display());

    prepend_bin_to_path()?;

    if let Err(err) = UpdateService::apply_pending_update() {
        tracing::error!("Failed to apply pending update: {}", err);
    }

    // Load configuration
    let config = Config::from_env()?;
    tracing::info!("Starting anthill with config: {:?}", config);

    if let Some(path) = config.database_url.strip_prefix("sqlite:") {
        let path = std::path::Path::new(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // Establish database connection
    let db_pool = establish_connection(&config.database_url).await?;
    tracing::info!("Database connected: {}", config.database_url);

    // Initialize repositories
    let plugin_repo = PluginRepository::new(db_pool.clone());
    let execution_repo = ExecutionRepository::new(db_pool);

    // Initialize services
    let plugin_service = PluginService::new(plugin_repo.clone(), config.uv_path.clone());
    let execution_service = ExecutionService::new(execution_repo, plugin_repo);

    let runtime_token = uuid::Uuid::new_v4().to_string();
    let auth_state = AuthState {
        token: Arc::new(runtime_token.clone()),
    };

    // Create router
    let app = create_router(plugin_service, execution_service);
    let app = app.layer(middleware::from_fn_with_state(auth_state, auth_middleware));
    let app = app.layer(TraceLayer::new_for_http());

    // Start server
    let addr = format!("{}:{}", config.host, config.port);
    let addr = addr.parse::<SocketAddr>()?;
    tracing::info!("Server listening on {}", addr);
    let launch_url = format!(
        "http://{}:{}?token={}",
        config.host, config.port, runtime_token
    );
    tracing::info!("Frontend URL: {}", launch_url);
    #[cfg(target_os = "windows")]
    windows_tray::set_launch_url(launch_url.clone());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    if let Err(err) = open_browser(&launch_url) {
        tracing::warn!("Failed to open browser automatically: {}", err);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_server(std::future::pending::<()>()).await
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let server_handle = runtime.spawn(run_server(async move {
        let _ = shutdown_rx.await;
    }));

    let _tray_thread = std::thread::spawn(move || {
        if let Err(err) = windows_tray::run_tray_loop(shutdown_tx) {
            eprintln!("tray loop failed: {err}");
        }
    });

    match runtime.block_on(async { server_handle.await }) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(anyhow::anyhow!(err)),
    }
}
