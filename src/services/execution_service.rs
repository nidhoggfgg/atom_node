use crate::error::{AppError, Result};
use crate::executor::{NodeExecutor, PluginExecutor, PythonExecutor};
use crate::models::{Execution, ExecutionPhase, ExecutionStatus, PluginParameter};
use crate::paths;
use crate::repository::{ExecutionRepository, PluginRepository};
use chrono::Utc;
use semver::Version;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

#[derive(Clone)]
pub struct ExecutionService {
    exec_repo: ExecutionRepository,
    plugin_repo: PluginRepository,
    python_executor: PythonExecutor,
    node_executor: NodeExecutor,
}

const PREVIEW_TTL_MS: i64 = 10 * 60 * 1000;

impl ExecutionService {
    pub fn new(exec_repo: ExecutionRepository, plugin_repo: PluginRepository) -> Self {
        Self {
            exec_repo,
            plugin_repo,
            python_executor: PythonExecutor::default(),
            node_executor: NodeExecutor::default(),
        }
    }

    pub async fn execute_plugin(
        &self,
        plugin_id: &str,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<Execution> {
        // 直接执行（无预览）的快捷接口，保持向后兼容
        let plugin = self.plugin_repo.get(plugin_id).await?;
        if !plugin.enabled {
            return Err(AppError::PluginDisabled);
        }
        Self::ensure_min_atom_node_version(&plugin.min_atom_node_version)?;

        let resolved_params = Self::resolve_parameters(&plugin.parameters, params)?;
        let mut env = HashMap::new();
        if !resolved_params.is_empty() {
            let params_json = serde_json::to_string(&resolved_params).map_err(|e| {
                AppError::Execution(format!("Failed to serialize parameters: {}", e))
            })?;
            env.insert("ATOM_PLUGIN_PARAMS".to_string(), params_json);
        }
        env.insert("ATOM_PHASE".to_string(), "apply".to_string());

        self.start_process(
            plugin,
            ExecutionPhase::Apply,
            ExecutionStatus::Completed,
            env,
            true,
        )
        .await
    }

    pub async fn prepare_plugin(
        &self,
        plugin_id: &str,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<Execution> {
        let plugin = self.plugin_repo.get(plugin_id).await?;
        if !plugin.enabled {
            return Err(AppError::PluginDisabled);
        }
        Self::ensure_min_atom_node_version(&plugin.min_atom_node_version)?;

        let resolved_params = Self::resolve_parameters(&plugin.parameters, params)?;
        let mut env = HashMap::new();
        if !resolved_params.is_empty() {
            let params_json = serde_json::to_string(&resolved_params).map_err(|e| {
                AppError::Execution(format!("Failed to serialize parameters: {}", e))
            })?;
            env.insert("ATOM_PLUGIN_PARAMS".to_string(), params_json);
        }
        env.insert("ATOM_PHASE".to_string(), "prepare".to_string());

        self.start_process(
            plugin,
            ExecutionPhase::Prepare,
            ExecutionStatus::PreviewReady,
            env,
            false,
        )
        .await
    }

    pub async fn apply_execution(
        &self,
        id: &str,
        confirm_token: &str,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<Execution> {
        let execution = self.exec_repo.get(id).await?;
        if execution.phase != ExecutionPhase::Prepare {
            return Err(AppError::Execution(
                "Only preview executions can be applied".to_string(),
            ));
        }
        if execution.status != ExecutionStatus::PreviewReady {
            return Err(AppError::Execution(
                "Execution is not ready to apply".to_string(),
            ));
        }
        if execution.confirm_token.as_deref() != Some(confirm_token) {
            return Err(AppError::Execution("Invalid confirm token".to_string()));
        }
        if let Some(expires_at) = execution.expires_at {
            if Utc::now().timestamp_millis() > expires_at {
                return Err(AppError::Execution(
                    "Preview has expired, please run prepare again".to_string(),
                ));
            }
        }

        let plugin = self.plugin_repo.get(&execution.plugin_id).await?;
        if !plugin.enabled {
            return Err(AppError::PluginDisabled);
        }
        Self::ensure_min_atom_node_version(&plugin.min_atom_node_version)?;

        let resolved_params = Self::resolve_parameters(&plugin.parameters, params)?;
        let mut env = HashMap::new();
        if !resolved_params.is_empty() {
            let params_json = serde_json::to_string(&resolved_params).map_err(|e| {
                AppError::Execution(format!("Failed to serialize parameters: {}", e))
            })?;
            env.insert("ATOM_PLUGIN_PARAMS".to_string(), params_json);
        }
        env.insert("ATOM_PHASE".to_string(), "apply".to_string());
        if let Some(plan) = execution.preview_payload.clone() {
            env.insert("ATOM_PREVIEW_PLAN".to_string(), plan);
        }

        self.exec_repo.begin_apply(id).await?;

        let updated_execution = self.exec_repo.get(id).await?;

        self.spawn_process(
            updated_execution.clone(),
            plugin,
            ExecutionStatus::Completed,
            env,
            true,
        )
        .await?;

        Ok(updated_execution)
    }

    pub async fn get_execution(&self, id: &str) -> Result<Execution> {
        self.exec_repo.get(id).await
    }

    pub async fn list_executions(&self, plugin_id: Option<String>) -> Result<Vec<Execution>> {
        if let Some(pid) = plugin_id {
            self.exec_repo.list_by_plugin(&pid).await
        } else {
            self.exec_repo.list_all().await
        }
    }

    pub async fn wait_for_states(
        &self,
        id: &str,
        targets: &[ExecutionStatus],
        timeout_ms: u64,
    ) -> Result<Execution> {
        let deadline = Utc::now().timestamp_millis() + timeout_ms as i64;
        loop {
            let current = self.exec_repo.get(id).await?;
            if targets.iter().any(|t| *t == current.status) {
                return Ok(current);
            }
            if Utc::now().timestamp_millis() > deadline {
                return Ok(current);
            }
            sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn stop_execution(&self, id: &str) -> Result<()> {
        let execution = self.exec_repo.get(id).await?;

        if let Some(pid) = execution.pid {
            // Try to kill the process
            // TODO: Implement proper process management
            tracing::info!("Stopping execution {} with pid {}", id, pid);
        }

        self.exec_repo
            .update_status(id, ExecutionStatus::Stopped)
            .await?;

        Ok(())
    }

    async fn start_process(
        &self,
        plugin: crate::models::Plugin,
        phase: ExecutionPhase,
        success_status: ExecutionStatus,
        env: HashMap<String, String>,
        cleanup_on_success: bool,
    ) -> Result<Execution> {
        let execution = self
            .exec_repo
            .create_with_phase(&plugin.plugin_id, phase)
            .await?;
        self.spawn_process(
            execution.clone(),
            plugin,
            success_status,
            env,
            cleanup_on_success,
        )
        .await?;
        Ok(execution)
    }

    async fn spawn_process(
        &self,
        execution: Execution,
        plugin: crate::models::Plugin,
        success_status: ExecutionStatus,
        env: HashMap<String, String>,
        cleanup_on_success: bool,
    ) -> Result<()> {
        let work_dir = Self::work_dir_for(&execution.id)?;
        std::fs::create_dir_all(&work_dir)?;

        let exec_result = match plugin.plugin_type {
            crate::models::PluginType::Python => {
                self.python_executor
                    .execute(&plugin, Vec::new(), env, &work_dir)
                    .await
            }
            crate::models::PluginType::JavaScript => {
                self.node_executor
                    .execute(&plugin, Vec::new(), env, &work_dir)
                    .await
            }
        };

        let (pid, mut child) = match exec_result {
            Ok(output) => output,
            Err(err) => {
                let _ = std::fs::remove_dir_all(&work_dir);
                return Err(err);
            }
        };

        self.exec_repo.update_pid(&execution.id, pid).await?;

        let exec_id = execution.id.clone();
        let exec_repo_clone = self.exec_repo.clone();
        let keep_on_success =
            !cleanup_on_success && success_status == ExecutionStatus::PreviewReady;

        tokio::spawn(async move {
            let mut stdout_child = child.stdout.take();
            let mut stderr_child = child.stderr.take();

            let status_result = child.wait().await;

            match status_result {
                Ok(status) => {
                    let exit_code = status.code();

                    use tokio::io::AsyncReadExt;
                    let mut stdout_buf = String::new();
                    let mut stderr_buf = String::new();

                    if let Some(ref mut stdout) = stdout_child {
                        let _ = stdout.read_to_string(&mut stdout_buf).await;
                    }
                    if let Some(ref mut stderr) = stderr_child {
                        let _ = stderr.read_to_string(&mut stderr_buf).await;
                    }

                    let stdout = if !stdout_buf.is_empty() {
                        Some(stdout_buf)
                    } else {
                        None
                    };

                    let stderr = if !stderr_buf.is_empty() {
                        Some(stderr_buf)
                    } else {
                        None
                    };

                    if exit_code == Some(0) && success_status == ExecutionStatus::PreviewReady {
                        let confirm_token = uuid::Uuid::new_v4().to_string();
                        let expires_at = Utc::now().timestamp_millis() + PREVIEW_TTL_MS;
                        exec_repo_clone
                            .mark_preview_ready(
                                &exec_id,
                                stdout,
                                stderr,
                                exit_code,
                                confirm_token,
                                expires_at,
                            )
                            .await
                            .ok();
                        if !keep_on_success {
                            let _ = std::fs::remove_dir_all(&work_dir);
                        }
                        return;
                    }

                    let exec_status = if exit_code == Some(0) {
                        success_status
                    } else {
                        ExecutionStatus::Failed
                    };

                    exec_repo_clone
                        .update_result(&exec_id, stdout, stderr, exit_code, exec_status)
                        .await
                        .ok();

                    if exit_code != Some(0) || cleanup_on_success {
                        if let Err(e) = std::fs::remove_dir_all(&work_dir) {
                            tracing::warn!(
                                "Failed to remove work dir {}: {}",
                                work_dir.display(),
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error waiting for process: {}", e);
                    exec_repo_clone
                        .update_result(
                            &exec_id,
                            None,
                            Some(format!("Error: {}", e)),
                            None,
                            ExecutionStatus::Failed,
                        )
                        .await
                        .ok();
                    if let Err(err) = std::fs::remove_dir_all(&work_dir) {
                        tracing::warn!("Failed to remove work dir {}: {}", work_dir.display(), err);
                    }
                }
            }
        });

        Ok(())
    }

    fn work_dir_for(execution_id: &str) -> Result<PathBuf> {
        let base_dir = paths::work_dir()?;
        Ok(base_dir.join(execution_id))
    }

    fn resolve_parameters(
        raw_parameters: &Option<String>,
        provided: HashMap<String, serde_json::Value>,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let schema = Self::parse_parameters(raw_parameters)?;
        if schema.is_empty() {
            if provided.is_empty() {
                return Ok(HashMap::new());
            }
            return Err(AppError::Execution(
                "Plugin does not declare parameters".to_string(),
            ));
        }

        let mut schema_map = HashMap::new();
        for param in &schema {
            let name = param.name.trim();
            if name.is_empty() {
                return Err(AppError::Execution(
                    "Parameter name cannot be empty".to_string(),
                ));
            }
            if name != param.name {
                return Err(AppError::Execution(format!(
                    "Parameter name has leading/trailing whitespace: {}",
                    param.name
                )));
            }
            if schema_map.insert(name.to_string(), param).is_some() {
                return Err(AppError::Execution(format!(
                    "Duplicate parameter name: {}",
                    name
                )));
            }
        }

        let mut resolved = HashMap::new();
        for (name, value) in provided {
            let Some(schema_param) = schema_map.get(&name) else {
                return Err(AppError::Execution(format!("Unknown parameter: {}", name)));
            };
            if !schema_param.param_type.matches(&value) {
                return Err(AppError::Execution(format!(
                    "Parameter '{}' does not match type {:?}",
                    name, schema_param.param_type
                )));
            }
            Self::ensure_choice(schema_param, &value)?;
            resolved.insert(name, value);
        }

        for param in &schema {
            if resolved.contains_key(&param.name) {
                continue;
            }
            if let Some(default) = &param.default {
                Self::ensure_choice(param, default)?;
                resolved.insert(param.name.clone(), default.clone());
            } else {
                return Err(AppError::Execution(format!(
                    "Missing required parameter: {}",
                    param.name
                )));
            }
        }

        Ok(resolved)
    }

    fn ensure_choice(param: &PluginParameter, value: &serde_json::Value) -> Result<()> {
        let Some(choices) = &param.choices else {
            return Ok(());
        };
        if choices.iter().any(|choice| choice == value) {
            return Ok(());
        }
        Err(AppError::Execution(format!(
            "Parameter '{}' must be one of the choices",
            param.name
        )))
    }

    fn ensure_min_atom_node_version(required: &Option<String>) -> Result<()> {
        let Some(required) = required.as_deref() else {
            return Ok(());
        };
        let trimmed = required.trim();
        if trimmed.is_empty() {
            return Err(AppError::Execution(
                "Minimum atom_node version cannot be empty".to_string(),
            ));
        }
        let required = Version::parse(trimmed).map_err(|e| {
            AppError::Execution(format!(
                "Invalid minimum atom_node version '{}': {}",
                trimmed, e
            ))
        })?;
        let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| {
            AppError::Execution(format!(
                "Invalid current atom_node version '{}': {}",
                env!("CARGO_PKG_VERSION"),
                e
            ))
        })?;
        if current < required {
            return Err(AppError::Execution(format!(
                "Plugin requires atom_node >= {}, current version is {}",
                required, current
            )));
        }
        Ok(())
    }

    fn parse_parameters(raw: &Option<String>) -> Result<Vec<PluginParameter>> {
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let parameters = serde_json::from_str(trimmed)
            .map_err(|e| AppError::Execution(format!("Invalid plugin parameters: {}", e)))?;
        Ok(parameters)
    }
}
