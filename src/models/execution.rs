use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Execution {
    pub id: String,
    pub plugin_id: String,
    pub phase: ExecutionPhase,
    pub status: ExecutionStatus,
    pub pid: Option<i32>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub preview_payload: Option<String>,
    pub confirm_token: Option<String>,
    pub expires_at: Option<i64>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[repr(i32)]
pub enum ExecutionPhase {
    Prepare = 0,
    Apply = 1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[repr(i32)]
pub enum ExecutionStatus {
    Pending = 0,
    Running = 1,
    PreviewReady = 2,
    Applying = 3,
    Completed = 4,
    Failed = 5,
    Stopped = 6,
}
