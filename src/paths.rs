use crate::error::{AppError, Result};
use std::path::PathBuf;

const BIN_DIR: &str = "bin";
const PLUGINS_DIR: &str = "plugins";
const WORK_DIR: &str = "work_dir";
const CONF_DIR: &str = "conf";
const DATA_DIR: &str = "data";
const PYTHON_ENVS_DIR: &str = "python_envs";
const HOME_ENV: &str = "ATOM_NODE_HOME";

pub fn install_root() -> Result<PathBuf> {
    if let Ok(home) = std::env::var(HOME_ENV) {
        if home.trim().is_empty() {
            return Err(AppError::Execution(
                "ATOM_NODE_HOME is set but empty".to_string(),
            ));
        }
        return Ok(PathBuf::from(home));
    }

    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| AppError::Execution("Failed to resolve executable directory".to_string()))?;

    if exe_dir.file_name().and_then(|name| name.to_str()) == Some(BIN_DIR) {
        let root = exe_dir.parent().ok_or_else(|| {
            AppError::Execution("Failed to resolve install root from bin".to_string())
        })?;
        return Ok(root.to_path_buf());
    }

    Ok(exe_dir.to_path_buf())
}

pub fn plugins_dir() -> Result<PathBuf> {
    Ok(install_root()?.join(PLUGINS_DIR))
}

pub fn work_dir() -> Result<PathBuf> {
    Ok(install_root()?.join(WORK_DIR))
}

pub fn conf_dir() -> Result<PathBuf> {
    Ok(install_root()?.join(CONF_DIR))
}

pub fn data_dir() -> Result<PathBuf> {
    Ok(install_root()?.join(DATA_DIR))
}

pub fn python_envs_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join(PYTHON_ENVS_DIR))
}
