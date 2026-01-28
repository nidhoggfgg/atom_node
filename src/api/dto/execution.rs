use crate::models::Execution;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct ExecutePluginRequest {
    pub params: Option<HashMap<String, Value>>,
}

#[derive(Debug, Deserialize)]
pub struct ApplyExecutionRequest {
    pub confirm_token: String,
    pub params: Option<HashMap<String, Value>>,
}

#[derive(Debug, Serialize)]
pub struct ExecutionResponse {
    pub id: String,
    pub plugin_id: String,
    pub phase: String,
    pub status: String,
    pub pid: Option<i32>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_payload: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

impl From<Execution> for ExecutionResponse {
    fn from(execution: Execution) -> Self {
        Self {
            id: execution.id,
            plugin_id: execution.plugin_id,
            phase: format!("{:?}", execution.phase),
            status: format!("{:?}", execution.status),
            pid: execution.pid,
            exit_code: execution.exit_code,
            stdout: execution.stdout,
            stderr: execution.stderr,
            preview_payload: execution.preview_payload,
            confirm_token: execution.confirm_token,
            expires_at: execution.expires_at,
            started_at: execution.started_at,
            finished_at: execution.finished_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExecutionsListResponse {
    pub data: Vec<ExecutionResponse>,
}
