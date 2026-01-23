use crate::error::{AppError, Result};
use crate::models::{Plugin, PluginParameter, PluginType, PythonDependencies};
use crate::repository::PluginRepository;
use crate::paths;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::ffi::OsStr;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct PackageMetadata {
    plugin_id: Option<String>,
    name: String,
    version: String,
    plugin_type: String,
    description: String,
    author: String,
    entry_point: String,
    metadata: Option<Value>,
    parameters: Option<Vec<PluginParameter>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PackageMetadataPayload {
    Multi { install_plugins: Vec<PackageMetadata> },
    Single(PackageMetadata),
}

#[derive(Clone)]
pub struct PluginService {
    repo: PluginRepository,
}

impl PluginService {
    pub fn new(repo: PluginRepository) -> Self {
        Self { repo }
    }

    pub async fn list_plugins(&self) -> Result<Vec<Plugin>> {
        self.repo.list().await
    }

    pub async fn get_plugin(&self, id: &str) -> Result<Plugin> {
        self.repo.get(id).await
    }

    #[allow(unused)]
    pub async fn get_plugin_by_name(&self, name: &str) -> Result<Plugin> {
        self.repo.get_by_name(name).await
    }

    pub async fn install_plugin(&self, package_url: String) -> Result<Plugin> {
        let bytes = Self::fetch_bytes(&package_url, "package").await?;
        let (spec, metadata_dir) = Self::read_metadata_from_zip(&bytes)?;
        let PackageMetadata {
            plugin_id,
            name,
            version,
            plugin_type,
            description,
            author,
            entry_point,
            metadata,
            parameters,
        } = spec;

        let plugin_id_raw = plugin_id.unwrap_or_else(|| name.clone());
        let plugin_id = plugin_id_raw.trim();
        if plugin_id.is_empty() {
            return Err(crate::error::AppError::Execution(
                "Plugin id cannot be empty".to_string(),
            ));
        }
        if plugin_id != plugin_id_raw {
            return Err(crate::error::AppError::Execution(format!(
                "Plugin id has leading/trailing whitespace: {}",
                plugin_id_raw
            )));
        }
        Self::validate_plugin_id(plugin_id)?;
        if self.repo.get(plugin_id).await.is_ok() {
            return Err(crate::error::AppError::PluginAlreadyExists(
                plugin_id.to_string(),
            ));
        }

        if entry_point.trim().is_empty() {
            return Err(crate::error::AppError::Execution(
                "Entry point cannot be empty".to_string(),
            ));
        }

        let plugin_type = Self::parse_plugin_type(&plugin_type)?;
        let metadata = metadata.map(Self::stringify_metadata);
        let parameters_json = Self::validate_parameters(parameters)?;
        let mut entry_point = entry_point;
        Self::validate_entry_point(&entry_point)?;

        let plugin_id = plugin_id.to_string();
        let internal_id = Uuid::new_v4().to_string();
        let plugin_dir = Self::plugin_dir_for(&plugin_id)?;

        fs::create_dir_all(&plugin_dir)?;

        if let Err(err) = Self::extract_zip(&bytes, &plugin_dir) {
            let _ = fs::remove_dir_all(&plugin_dir);
            return Err(err);
        }

        let entry_path = plugin_dir.join(&entry_point);
        if !entry_path.is_file() {
            if let Some(dir) = metadata_dir.as_deref() {
                let candidate = dir.join(&entry_point);
                let candidate_str = candidate.to_string_lossy().to_string();
                Self::validate_entry_point(&candidate_str)?;
                let candidate_path = plugin_dir.join(&candidate_str);
                if candidate_path.is_file() {
                    entry_point = candidate_str;
                } else {
                    let _ = fs::remove_dir_all(&plugin_dir);
                    return Err(crate::error::AppError::Execution(format!(
                        "Entry point not found: {}",
                        entry_path.display()
                    )));
                }
            } else {
                let _ = fs::remove_dir_all(&plugin_dir);
                return Err(crate::error::AppError::Execution(format!(
                    "Entry point not found: {}",
                    entry_path.display()
                )));
            }
        }

        let mut python_venv_path = None;
        let mut python_dependencies_json = None;
        if plugin_type == PluginType::Python {
            let venv_dir = Self::python_env_dir_for(&plugin_id)?;
            let resolved_deps = Self::resolve_python_dependencies(&plugin_dir);
            python_dependencies_json = match resolved_deps.as_ref() {
                Some(deps) => match Self::serialize_python_dependencies(deps) {
                    Ok(json) => Some(json),
                    Err(err) => {
                        let _ = fs::remove_dir_all(&plugin_dir);
                        let _ = fs::remove_dir_all(&venv_dir);
                        return Err(err);
                    }
                },
                None => None,
            };
            if let Err(err) =
                Self::prepare_python_env(&venv_dir, &plugin_dir, resolved_deps.as_ref()).await
            {
                let _ = fs::remove_dir_all(&plugin_dir);
                let _ = fs::remove_dir_all(&venv_dir);
                return Err(err);
            }
            python_venv_path = Some(venv_dir.to_string_lossy().to_string());
        }

        let now = Utc::now();
        let plugin = Plugin {
            id: internal_id,
            plugin_id: plugin_id.clone(),
            name,
            version,
            plugin_type,
            description,
            author,
            plugin_path: plugin_dir.to_string_lossy().to_string(),
            entry_point,
            enabled: true,
            created_at: now,
            updated_at: now,
            metadata,
            parameters: parameters_json,
            python_venv_path,
            python_dependencies: python_dependencies_json,
        };

        if let Err(err) = self.repo.create(&plugin).await {
            let _ = fs::remove_dir_all(&plugin.plugin_path);
            if let Some(venv_path) = &plugin.python_venv_path {
                let _ = fs::remove_dir_all(venv_path);
            }
            return Err(err);
        }
        Ok(plugin)
    }

    pub async fn uninstall_plugin(&self, id: &str) -> Result<()> {
        let plugin = self.repo.get(id).await?;
        if !plugin.plugin_path.is_empty() {
            match fs::remove_dir_all(&plugin.plugin_path) {
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        if let Some(venv_path) = &plugin.python_venv_path {
            if !venv_path.is_empty() {
                match fs::remove_dir_all(venv_path) {
                    Ok(_) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err.into()),
                }
            }
        }
        self.repo.delete(id).await
    }

    pub async fn enable_plugin(&self, id: &str) -> Result<()> {
        self.repo.update_enabled(id, true).await
    }

    pub async fn disable_plugin(&self, id: &str) -> Result<()> {
        self.repo.update_enabled(id, false).await
    }

    fn plugin_dir_for(plugin_id: &str) -> Result<PathBuf> {
        let base_dir = paths::plugins_dir()?;
        Ok(base_dir.join(plugin_id))
    }

    fn extract_zip(bytes: &[u8], target_dir: &Path) -> Result<()> {
        let reader = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader).map_err(|e| {
            crate::error::AppError::Execution(format!("Invalid zip archive: {}", e))
        })?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| {
                crate::error::AppError::Execution(format!("Failed to read archive: {}", e))
            })?;

            let Some(relative_path) = file.enclosed_name().as_deref().map(Path::to_path_buf) else {
                return Err(crate::error::AppError::Execution(
                    "Invalid file path in archive".to_string(),
                ));
            };

            let out_path = target_dir.join(relative_path);
            if file.name().ends_with('/') {
                fs::create_dir_all(&out_path)?;
                continue;
            }

            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut outfile = fs::File::create(&out_path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            outfile.write_all(&buffer)?;
        }

        Ok(())
    }

    fn read_metadata_from_zip(
        bytes: &[u8],
    ) -> Result<(PackageMetadata, Option<PathBuf>)> {
        let reader = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader).map_err(|e| {
            AppError::Execution(format!("Invalid zip archive: {}", e))
        })?;

        let mut metadata_index = None;
        let mut metadata_path = None;

        for i in 0..archive.len() {
            let file = archive.by_index(i).map_err(|e| {
                AppError::Execution(format!("Failed to read archive: {}", e))
            })?;
            let Some(path) = file.enclosed_name().as_deref().map(Path::to_path_buf) else {
                return Err(AppError::Execution(
                    "Invalid file path in archive".to_string(),
                ));
            };
            if path.file_name() == Some(OsStr::new("metadata.json")) {
                if metadata_index.is_some() {
                    return Err(AppError::Execution(
                        "Multiple metadata.json files found in package".to_string(),
                    ));
                }
                metadata_index = Some(i);
                metadata_path = Some(path);
            }
        }

        let Some(index) = metadata_index else {
            return Err(AppError::Execution(
                "metadata.json not found in package".to_string(),
            ));
        };

        let mut file = archive.by_index(index).map_err(|e| {
            AppError::Execution(format!("Failed to read metadata.json: {}", e))
        })?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let payload: PackageMetadataPayload =
            serde_json::from_slice(&buffer).map_err(|e| {
                AppError::Execution(format!("Invalid metadata JSON: {}", e))
            })?;
        let spec = match payload {
            PackageMetadataPayload::Single(spec) => spec,
            PackageMetadataPayload::Multi { install_plugins } => {
                if install_plugins.len() != 1 {
                    return Err(AppError::Execution(
                        "Package metadata must describe exactly one plugin".to_string(),
                    ));
                }
                install_plugins.into_iter().next().unwrap()
            }
        };

        let metadata_dir = metadata_path
            .as_deref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .filter(|dir| !dir.as_os_str().is_empty());

        Ok((spec, metadata_dir))
    }

    async fn fetch_bytes(url: &str, label: &str) -> Result<Vec<u8>> {
        if let Some(path) = Self::resolve_local_path(url) {
            let bytes = fs::read(&path).map_err(|e| {
                AppError::Execution(format!(
                    "Failed to read local {} {}: {}",
                    label,
                    path.display(),
                    e
                ))
            })?;
            return Ok(bytes);
        }

        let response = reqwest::get(url).await.map_err(|e| {
            AppError::Execution(format!("Failed to download {}: {}", label, e))
        })?;
        let response = response.error_for_status().map_err(|e| {
            AppError::Execution(format!("Failed to download {}: {}", label, e))
        })?;

        let bytes = response.bytes().await.map_err(|e| {
            AppError::Execution(format!("Failed to read {} bytes: {}", label, e))
        })?;

        Ok(bytes.to_vec())
    }

    fn local_path_from_url(url: &str) -> Option<PathBuf> {
        if let Some(path) = url.strip_prefix("file://") {
            let path = path.strip_prefix("localhost/").unwrap_or(path);
            return Some(PathBuf::from(path));
        }
        None
    }

    fn resolve_local_path(url: &str) -> Option<PathBuf> {
        if let Some(path) = Self::local_path_from_url(url) {
            return Some(path);
        }

        if url.starts_with("http://") || url.starts_with("https://") {
            return None;
        }

        Some(PathBuf::from(url))
    }

    fn parse_plugin_type(raw: &str) -> Result<PluginType> {
        match raw {
            "python" => Ok(PluginType::Python),
            "javascript" | "js" => Ok(PluginType::JavaScript),
            _ => Err(AppError::InvalidPluginType),
        }
    }

    fn stringify_metadata(value: Value) -> String {
        match value {
            Value::String(s) => s,
            other => other.to_string(),
        }
    }

    fn validate_entry_point(entry_point: &str) -> Result<()> {
        let path = Path::new(entry_point);
        if path.is_absolute() {
            return Err(crate::error::AppError::Execution(
                "Entry point must be a relative path".to_string(),
            ));
        }
        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(crate::error::AppError::Execution(
                "Entry point cannot contain '..'".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_plugin_id(plugin_id: &str) -> Result<()> {
        if plugin_id.contains('/') || plugin_id.contains('\\') {
            return Err(crate::error::AppError::Execution(
                "Plugin id cannot contain path separators".to_string(),
            ));
        }
        let path = Path::new(plugin_id);
        if path.is_absolute() {
            return Err(crate::error::AppError::Execution(
                "Plugin id must be a relative identifier".to_string(),
            ));
        }
        let mut components = path.components();
        match components.next() {
            Some(Component::Normal(_)) => {}
            _ => {
                return Err(crate::error::AppError::Execution(
                    "Plugin id must be a valid identifier".to_string(),
                ))
            }
        }
        if components.next().is_some() {
            return Err(crate::error::AppError::Execution(
                "Plugin id must be a single path segment".to_string(),
            ));
        }
        Ok(())
    }

    fn python_env_dir_for(plugin_id: &str) -> Result<PathBuf> {
        let base_dir = paths::python_envs_dir()?;
        Ok(base_dir.join(plugin_id))
    }

    fn resolve_python_dependencies(
        plugin_dir: &Path,
    ) -> Option<PythonDependencies> {
        let pyproject = plugin_dir.join("pyproject.toml");
        if pyproject.is_file() {
            return Some(PythonDependencies::Pyproject {
                path: "pyproject.toml".to_string(),
            });
        }

        let requirements = plugin_dir.join("requirements.txt");
        if requirements.is_file() {
            return Some(PythonDependencies::Requirements {
                path: "requirements.txt".to_string(),
            });
        }

        None
    }

    fn serialize_python_dependencies(deps: &PythonDependencies) -> Result<String> {
        serde_json::to_string(deps).map_err(|e| {
            crate::error::AppError::Execution(format!(
                "Failed to serialize python dependencies: {}",
                e
            ))
        })
    }

    async fn prepare_python_env(
        venv_dir: &Path,
        plugin_dir: &Path,
        dependencies: Option<&PythonDependencies>,
    ) -> Result<()> {
        if let Some(parent) = venv_dir.parent() {
            fs::create_dir_all(parent)?;
        }

        let venv_dir_str = venv_dir.to_string_lossy().to_string();
        Self::run_uv_command(&vec!["venv".to_string(), venv_dir_str], None).await?;

        let python_path = Self::python_executable_path(venv_dir);
        if !python_path.is_file() {
            return Err(crate::error::AppError::Execution(format!(
                "Python executable not found in venv: {}",
                python_path.display()
            )));
        }

        let python_path_str = python_path.to_string_lossy().to_string();
        let Some(dependencies) = dependencies else {
            return Ok(());
        };

        let mut args = vec![
            "pip".to_string(),
            "install".to_string(),
            "--python".to_string(),
            python_path_str,
        ];
        let current_dir = match dependencies {
            PythonDependencies::Requirements { path } => {
                args.push("-r".to_string());
                args.push(path.clone());
                Some(plugin_dir)
            }
            PythonDependencies::Pyproject { path: _ } => {
                args.push("-e".to_string());
                args.push(".".to_string());
                Some(plugin_dir)
            }
        };

        Self::run_uv_command(&args, current_dir).await?;
        Ok(())
    }

    fn python_executable_path(venv_dir: &Path) -> PathBuf {
        if cfg!(windows) {
            venv_dir.join("Scripts").join("python.exe")
        } else {
            venv_dir.join("bin").join("python")
        }
    }

    async fn run_uv_command(args: &[String], current_dir: Option<&Path>) -> Result<()> {
        let mut cmd = tokio::process::Command::new("uv");
        cmd.args(args);
        if let Some(dir) = current_dir {
            cmd.current_dir(dir);
        }
        let output = cmd.output().await.map_err(|e| {
            crate::error::AppError::Execution(format!(
                "Failed to run uv {}: {}",
                args.join(" "),
                e
            ))
        })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let details = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        let message = if details.is_empty() {
            format!("uv {} failed", args.join(" "))
        } else {
            format!("uv {} failed: {}", args.join(" "), details)
        };
        Err(crate::error::AppError::Execution(message))
    }

    fn validate_parameters(
        parameters: Option<Vec<PluginParameter>>,
    ) -> Result<Option<String>> {
        let Some(parameters) = parameters else {
            return Ok(None);
        };

        let mut seen = std::collections::HashSet::new();
        for param in &parameters {
            let name = param.name.trim();
            if name.is_empty() {
                return Err(crate::error::AppError::Execution(
                    "Parameter name cannot be empty".to_string(),
                ));
            }
            if name != param.name {
                return Err(crate::error::AppError::Execution(format!(
                    "Parameter name has leading/trailing whitespace: {}",
                    param.name
                )));
            }
            if !seen.insert(name.to_string()) {
                return Err(crate::error::AppError::Execution(format!(
                    "Duplicate parameter name: {}",
                    name
                )));
            }
            if let Some(default) = &param.default {
                if !param.param_type.matches(default) {
                    return Err(crate::error::AppError::Execution(format!(
                        "Default value for parameter '{}' does not match type {:?}",
                        name, param.param_type
                    )));
                }
            }
        }

        let json = serde_json::to_string(&parameters).map_err(|e| {
            crate::error::AppError::Execution(format!(
                "Failed to serialize parameters: {}",
                e
            ))
        })?;
        Ok(Some(json))
    }

}
