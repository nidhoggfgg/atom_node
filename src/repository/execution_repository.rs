use crate::error::{AppError, Result};
use crate::models::{Execution, ExecutionPhase, ExecutionStatus};
use crate::repository::DbPool;
use chrono::Utc;

#[derive(Clone)]
pub struct ExecutionRepository {
    pool: DbPool,
}

impl ExecutionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create_with_phase(
        &self,
        plugin_id: &str,
        phase: ExecutionPhase,
    ) -> Result<Execution> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();

        let execution = Execution {
            id: id.clone(),
            plugin_id: plugin_id.to_string(),
            phase,
            status: ExecutionStatus::Pending,
            pid: None,
            exit_code: None,
            stdout: None,
            stderr: None,
            preview_payload: None,
            confirm_token: None,
            expires_at: None,
            started_at: now,
            finished_at: None,
        };

        sqlx::query(
            r#"
            INSERT INTO executions (id, plugin_id, phase, status, started_at, finished_at)
            VALUES (?, ?, ?, ?, ?, NULL)
            "#,
        )
        .bind(&execution.id)
        .bind(&execution.plugin_id)
        .bind(execution.phase as i32)
        .bind(execution.status as i32)
        .bind(execution.started_at)
        .execute(&self.pool)
        .await?;

        Ok(execution)
    }

    pub async fn get(&self, id: &str) -> Result<Execution> {
        let execution = sqlx::query_as::<_, Execution>("SELECT * FROM executions WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::ExecutionNotFound(id.to_string()))?;

        Ok(execution)
    }

    pub async fn list_by_plugin(&self, plugin_id: &str) -> Result<Vec<Execution>> {
        let executions = sqlx::query_as::<_, Execution>(
            "SELECT * FROM executions WHERE plugin_id = ? ORDER BY started_at DESC",
        )
        .bind(plugin_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(executions)
    }

    pub async fn list_all(&self) -> Result<Vec<Execution>> {
        let executions =
            sqlx::query_as::<_, Execution>("SELECT * FROM executions ORDER BY started_at DESC")
                .fetch_all(&self.pool)
                .await?;

        Ok(executions)
    }

    pub async fn update_pid(&self, id: &str, pid: u32) -> Result<()> {
        sqlx::query("UPDATE executions SET pid = ?, status = ? WHERE id = ?")
            .bind(pid as i32)
            .bind(ExecutionStatus::Running as i32)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn update_result(
        &self,
        id: &str,
        stdout: Option<String>,
        stderr: Option<String>,
        exit_code: Option<i32>,
        status: ExecutionStatus,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE executions
            SET stdout = ?, stderr = ?, exit_code = ?, status = ?, finished_at = ?
            WHERE id = ?
            "#,
        )
        .bind(stdout)
        .bind(stderr)
        .bind(exit_code)
        .bind(status as i32)
        .bind(Utc::now().timestamp_millis())
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_preview_ready(
        &self,
        id: &str,
        stdout: Option<String>,
        stderr: Option<String>,
        exit_code: Option<i32>,
        confirm_token: String,
        expires_at: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE executions
            SET stdout = ?, stderr = ?, exit_code = ?, status = ?, finished_at = ?, preview_payload = ?, confirm_token = ?, expires_at = ?
            WHERE id = ?
            "#,
        )
        .bind(stdout.clone())
        .bind(stderr)
        .bind(exit_code)
        .bind(ExecutionStatus::PreviewReady as i32)
        .bind(Utc::now().timestamp_millis())
        .bind(stdout)
        .bind(confirm_token)
        .bind(expires_at)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn begin_apply(&self, id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE executions
            SET phase = ?, status = ?, pid = NULL, exit_code = NULL, stdout = NULL, stderr = NULL, started_at = ?, finished_at = NULL, confirm_token = NULL
            WHERE id = ?
            "#,
        )
        .bind(ExecutionPhase::Apply as i32)
        .bind(ExecutionStatus::Pending as i32)
        .bind(Utc::now().timestamp_millis())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_status(&self, id: &str, status: ExecutionStatus) -> Result<()> {
        sqlx::query("UPDATE executions SET status = ? WHERE id = ?")
            .bind(status as i32)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
