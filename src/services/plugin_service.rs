use crate::error::{AppError, Result};
use crate::models::{Plugin, PluginParameter, PluginType, PythonDependencies};
use crate::repository::PluginRepository;
use crate::paths;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct MetadataInstallPlugin {
    name: String,
    version: String,
    plugin_type: String,
    description: String,
    author: String,
    package_url: String,
    entry_point: String,
    metadata: Option<Value>,
    parameters: Option<Vec<PluginParameter>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MetadataPayload {
    Multi { install_plugins: Vec<MetadataInstallPlugin> },
    Single(MetadataInstallPlugin),
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

    pub async fn install_plugin(
        &self,
        name: String,
        version: String,
        plugin_type: PluginType,
        description: String,
        author: String,
        package_url: String,
        entry_point: String,
        metadata: Option<String>,
        parameters: Option<Vec<PluginParameter>>,
    ) -> Result<Plugin> {
        // Check if plugin already exists
        if self.repo.get_by_name(&name).await.is_ok() {
            return Err(crate::error::AppError::PluginAlreadyExists(name));
        }

        if entry_point.trim().is_empty() {
            return Err(crate::error::AppError::Execution(
                "Entry point cannot be empty".to_string(),
            ));
        }
        Self::validate_entry_point(&entry_point)?;

        let plugin_id = Uuid::new_v4().to_string();
        let plugin_dir = Self::plugin_dir_for(&plugin_id)?;

        fs::create_dir_all(&plugin_dir)?;

        let parameters_json = Self::validate_parameters(parameters)?;

        if let Err(err) = self.download_and_extract(&package_url, &plugin_dir).await {
            let _ = fs::remove_dir_all(&plugin_dir);
            return Err(err);
        }

        let entry_path = plugin_dir.join(&entry_point);
        if !entry_path.is_file() {
            let _ = fs::remove_dir_all(&plugin_dir);
            return Err(crate::error::AppError::Execution(format!(
                "Entry point not found: {}",
                entry_path.display()
            )));
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
            id: plugin_id,
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

    pub async fn install_from_metadata_url(&self, metadata_url: &str) -> Result<Vec<Plugin>> {
        let bytes = Self::fetch_bytes(metadata_url, "metadata").await?;
        let payload: MetadataPayload = serde_json::from_slice(&bytes).map_err(|e| {
            AppError::Execution(format!("Invalid metadata JSON: {}", e))
        })?;

        let mut specs = match payload {
            MetadataPayload::Single(spec) => vec![spec],
            MetadataPayload::Multi { install_plugins } => install_plugins,
        };

        if specs.is_empty() {
            return Err(AppError::Execution(
                "Metadata file did not contain any install_plugins entries".to_string(),
            ));
        }

        let mut plugins = Vec::with_capacity(specs.len());
        for spec in specs.drain(..) {
            let plugin_type = Self::parse_plugin_type(&spec.plugin_type)?;
            let metadata = spec.metadata.map(Self::stringify_metadata);
            let package_url = Self::resolve_package_url(metadata_url, &spec.package_url);
            let plugin = self
                .install_plugin(
                    spec.name,
                    spec.version,
                    plugin_type,
                    spec.description,
                    spec.author,
                    package_url,
                    spec.entry_point,
                    metadata,
                    spec.parameters,
                )
                .await?;
            plugins.push(plugin);
        }

        Ok(plugins)
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

    async fn download_and_extract(&self, url: &str, target_dir: &Path) -> Result<()> {
        let bytes = Self::fetch_bytes(url, "package").await?;
        Self::extract_zip(&bytes, target_dir)
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

    fn resolve_package_url(metadata_url: &str, package_url: &str) -> String {
        if package_url.starts_with("http://")
            || package_url.starts_with("https://")
            || package_url.starts_with("file://")
        {
            return package_url.to_string();
        }

        let path = Path::new(package_url);
        if path.is_absolute() {
            return package_url.to_string();
        }

        if let Some(local_metadata) = Self::resolve_local_path(metadata_url) {
            if let Some(parent) = local_metadata.parent() {
                return parent.join(package_url).to_string_lossy().to_string();
            }
        }

        if let Ok(base_url) = reqwest::Url::parse(metadata_url) {
            if let Ok(joined) = base_url.join(package_url) {
                return joined.to_string();
            }
        }

        package_url.to_string()
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
