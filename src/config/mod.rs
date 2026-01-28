use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub host: String,
    pub port: u16,
    pub uv_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        let database_url = crate::paths::data_dir()
            .map(|dir| format!("sqlite:{}", dir.join("atom_node.db").display()))
            .unwrap_or_else(|_| "sqlite:atom_node.db".to_string());
        Self {
            database_url,
            host: "127.0.0.1".to_string(),
            port: 6701,
            uv_path: None,
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();

        if let Some(file_config) = Self::from_conf_file()? {
            config.apply_file(file_config);
        }

        if let Ok(db_url) = std::env::var("DATABASE_URL") {
            config.database_url = db_url;
        }

        if let Ok(host) = std::env::var("HOST") {
            config.host = host;
        }

        if let Ok(port) = std::env::var("PORT") {
            config.port = port.parse().unwrap_or(6701);
        }

        config.normalize_database_url()?;
        config.normalize_uv_path()?;
        Ok(config)
    }

    fn from_conf_file() -> Result<Option<FileConfig>> {
        let path = crate::paths::conf_dir()?.join("config.json");
        if !path.is_file() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file {}", path.display()))?;
        let file_config = serde_json::from_str(&content)
            .with_context(|| format!("Invalid config file {}", path.display()))?;
        Ok(Some(file_config))
    }

    fn apply_file(&mut self, file_config: FileConfig) {
        if let Some(database_url) = file_config.database_url {
            self.database_url = database_url;
        }
        if let Some(host) = file_config.host {
            self.host = host;
        }
        if let Some(port) = file_config.port {
            self.port = port;
        }
        if let Some(uv_path) = file_config.uv_path {
            self.uv_path = Some(PathBuf::from(uv_path));
        }
    }

    fn normalize_database_url(&mut self) -> Result<()> {
        let Some(path_str) = self.database_url.strip_prefix("sqlite:") else {
            return Ok(());
        };

        let path = Path::new(path_str);
        let root = crate::paths::install_root()?;

        if path.is_absolute() {
            if !path.starts_with(&root) {
                anyhow::bail!(
                    "SQLite database path must be under install root: {}",
                    root.display()
                );
            }
            return Ok(());
        }

        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            anyhow::bail!("SQLite database path cannot contain '..'");
        }

        let absolute = root.join(path);
        self.database_url = format!("sqlite:{}", absolute.display());
        Ok(())
    }

    fn normalize_uv_path(&mut self) -> Result<()> {
        let Some(path) = self.uv_path.as_ref() else {
            return Ok(());
        };

        let path_str = path.to_string_lossy();
        if path_str.trim().is_empty() {
            anyhow::bail!("uv_path in config cannot be empty");
        }

        if path.is_absolute() {
            anyhow::bail!("uv_path must be relative to install root");
        }

        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            anyhow::bail!("uv_path cannot contain '..'");
        }

        let root = crate::paths::install_root()?;
        self.uv_path = Some(root.join(path));
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    database_url: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    uv_path: Option<String>,
}
