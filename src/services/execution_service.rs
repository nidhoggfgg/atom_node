use crate::error::{AppError, Result};
use crate::executor::{NodeExecutor, PluginExecutor, PythonExecutor};
use crate::models::{Execution, ExecutionStatus, PluginParameter};
use crate::repository::{ExecutionRepository, PluginRepository};
use crate::paths;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
pub struct ExecutionService {
    exec_repo: ExecutionRepository,
    plugin_repo: PluginRepository,
    python_executor: PythonExecutor,
    node_executor: NodeExecutor,
}

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
        // Get plugin
        let plugin = self.plugin_repo.get(plugin_id).await?;

        if !plugin.enabled {
            return Err(AppError::PluginDisabled);
        }

        // Create execution record
        let execution = self.exec_repo.create(plugin_id).await?;

        let work_dir = Self::work_dir_for(&execution.id)?;
        std::fs::create_dir_all(&work_dir)?;

        let resolved_params = Self::resolve_parameters(&plugin.parameters, params)?;
        let mut env = HashMap::new();
        if !resolved_params.is_empty() {
            let params_json = serde_json::to_string(&resolved_params).map_err(|e| {
                AppError::Execution(format!("Failed to serialize parameters: {}", e))
            })?;
            env.insert("ATOM_PLUGIN_PARAMS".to_string(), params_json);
        }

        // Select executor based on plugin type
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

        // Update execution with pid
        self.exec_repo.update_pid(&execution.id, pid).await?;

        // Spawn background task to monitor execution
        let exec_id = execution.id.clone();
        let exec_repo_clone = self.exec_repo.clone();

        tokio::spawn(async move {
            // Take stdout and stderr before waiting for process
            let mut stdout_child = child.stdout.take();
            let mut stderr_child = child.stderr.take();

            // Wait for process to complete
            let status_result = child.wait().await;

            match status_result {
                Ok(status) => {
                    let exit_code = status.code();

                    // Read stdout and stderr from child process
                    use tokio::io::AsyncReadExt;
                    let mut stdout_buf = String::new();
                    let mut stderr_buf = String::new();

                    // Read stdout if available
                    if let Some(ref mut stdout) = stdout_child {
                        let _ = stdout.read_to_string(&mut stdout_buf).await;
                    }

                    // Read stderr if available
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

                    let exec_status = if exit_code == Some(0) {
                        ExecutionStatus::Completed
                    } else {
                        ExecutionStatus::Failed
                    };

                    exec_repo_clone
                        .update_result(&exec_id, stdout, stderr, exit_code, exec_status)
                        .await
                        .ok();
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
                }
            }

            if let Err(e) = std::fs::remove_dir_all(&work_dir) {
                tracing::warn!("Failed to remove work dir {}: {}", work_dir.display(), e);
            }
        });

        Ok(execution)
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
                return Err(AppError::Execution(format!(
                    "Unknown parameter: {}",
                    name
                )));
            };
            if !schema_param.param_type.matches(&value) {
                return Err(AppError::Execution(format!(
                    "Parameter '{}' does not match type {:?}",
                    name, schema_param.param_type
                )));
            }
            resolved.insert(name, value);
        }

        for param in &schema {
            if resolved.contains_key(&param.name) {
                continue;
            }
            if let Some(default) = &param.default {
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

    fn parse_parameters(raw: &Option<String>) -> Result<Vec<PluginParameter>> {
        let Some(raw) = raw else {
            return Ok(Vec::new());
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let parameters = serde_json::from_str(trimmed).map_err(|e| {
            AppError::Execution(format!("Invalid plugin parameters: {}", e))
        })?;
        Ok(parameters)
    }
}
