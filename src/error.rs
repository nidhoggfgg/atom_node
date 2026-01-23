use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    #[error("Plugin already exists: {0}")]
    PluginAlreadyExists(String),

    #[error("Execution not found: {0}")]
    ExecutionNotFound(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid plugin type")]
    InvalidPluginType,

    #[error("Plugin is disabled")]
    PluginDisabled,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Database(e) => {
                tracing::error!("Database error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            AppError::PluginNotFound(id) => {
                (StatusCode::NOT_FOUND, format!("Plugin '{}' not found", id))
            }
            AppError::PluginAlreadyExists(id) => (
                StatusCode::CONFLICT,
                format!("Plugin id '{}' already exists", id),
            ),
            AppError::ExecutionNotFound(id) => (
                StatusCode::NOT_FOUND,
                format!("Execution '{}' not found", id),
            ),
            AppError::Execution(e) => (StatusCode::BAD_REQUEST, e),
            AppError::Io(e) => {
                tracing::error!("IO error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            AppError::InvalidPluginType => {
                (StatusCode::BAD_REQUEST, "Invalid plugin type".to_string())
            }
            AppError::PluginDisabled => (StatusCode::FORBIDDEN, "Plugin is disabled".to_string()),
        };

        let body = json!({
            "error": message
        });

        (status, Json(body)).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
