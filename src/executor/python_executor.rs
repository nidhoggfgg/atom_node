use super::PluginExecutor;
use crate::error::{AppError, Result};
use crate::models::Plugin;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct PythonExecutor {
    python_path: String,
}

impl PythonExecutor {
    pub fn new(python_path: Option<String>) -> Self {
        Self {
            python_path: python_path.unwrap_or_else(|| "python3".to_string()),
        }
    }
}

impl Default for PythonExecutor {
    fn default() -> Self {
        Self::new(None)
    }
}

impl PluginExecutor for PythonExecutor {
    async fn execute(
        &self,
        plugin: &Plugin,
        args: Vec<String>,
        env: HashMap<String, String>,
        work_dir: &Path,
    ) -> Result<(u32, tokio::process::Child)> {
        let script_path = Path::new(&plugin.plugin_path).join(&plugin.entry_point);
        if !script_path.is_file() {
            return Err(AppError::Execution(format!(
                "Entry point not found: {}",
                script_path.display()
            )));
        }

        let (python_path, venv_root) = match &plugin.python_venv_path {
            Some(venv_path) if !venv_path.is_empty() => {
                let venv_root = PathBuf::from(venv_path);
                let venv_python = Self::python_executable_path(&venv_root);
                if !venv_python.is_file() {
                    return Err(AppError::Execution(format!(
                        "Python venv not found: {}",
                        venv_python.display()
                    )));
                }
                (venv_python, Some(venv_root))
            }
            _ => (PathBuf::from(&self.python_path), None),
        };

        // Build the command
        let mut cmd = tokio::process::Command::new(&python_path);
        cmd.arg(&script_path);
        cmd.current_dir(work_dir);

        for arg in args {
            cmd.arg(arg);
        }

        // Set environment variables
        let mut env = env;
        if let Some(venv_root) = venv_root {
            let bin_dir = Self::python_bin_dir(&venv_root);
            env.insert(
                "VIRTUAL_ENV".to_string(),
                venv_root.to_string_lossy().to_string(),
            );
            let path_separator = if cfg!(windows) { ";" } else { ":" };
            let existing_path = env
                .get("PATH")
                .cloned()
                .or_else(|| std::env::var("PATH").ok());
            let new_path = match existing_path {
                Some(current) if !current.is_empty() => {
                    format!("{}{}{}", bin_dir.display(), path_separator, current)
                }
                _ => bin_dir.to_string_lossy().to_string(),
            };
            env.insert("PATH".to_string(), new_path);
        }

        for (key, value) in env {
            cmd.env(key, value);
        }

        // Capture stdout and stderr
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let child = cmd.spawn()?;

        let pid = child
            .id()
            .ok_or_else(|| AppError::Execution("Failed to get process ID".to_string()))?;

        Ok((pid, child))
    }
}

impl PythonExecutor {
    fn python_executable_path(venv_dir: &Path) -> PathBuf {
        if cfg!(windows) {
            venv_dir.join("Scripts").join("python.exe")
        } else {
            venv_dir.join("bin").join("python")
        }
    }

    fn python_bin_dir(venv_dir: &Path) -> PathBuf {
        if cfg!(windows) {
            venv_dir.join("Scripts")
        } else {
            venv_dir.join("bin")
        }
    }
}
