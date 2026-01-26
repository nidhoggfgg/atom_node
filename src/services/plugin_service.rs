use crate::error::{AppError, Result};
use crate::models::{Plugin, PluginParameter, PluginType, PythonDependencies};
use crate::repository::PluginRepository;
use crate::paths;
use chrono::Utc;
use semver::Version;
use serde::Deserialize;
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
        self.install_plugin_from_bytes(bytes).await
    }

    pub async fn update_plugin(&self, id: &str, package_url: String) -> Result<Plugin> {
        let existing = self.repo.get(id).await?;
        let bytes = Self::fetch_bytes(&package_url, "package").await?;
        let temp_dir = tempfile::Builder::new()
            .prefix("plugin_update_")
            .tempdir()
            .map_err(|e| {
                AppError::Execution(format!("Failed to create temp dir: {}", e))
            })?;

        Self::extract_zip(&bytes, temp_dir.path())?;
        let (spec, metadata_dir) = Self::read_metadata_from_dir(temp_dir.path())?;
        let PackageMetadata {
            plugin_id,
            name,
            version,
            plugin_type,
            description: _,
            author: _,
            entry_point,
            parameters,
        } = spec;

        let plugin_id = Self::normalize_plugin_id(plugin_id, &name)?;
        if plugin_id != id {
            return Err(AppError::Execution(format!(
                "Plugin id '{}' does not match update target '{}'",
                plugin_id, id
            )));
        }
        if entry_point.trim().is_empty() {
            return Err(AppError::Execution(
                "Entry point cannot be empty".to_string(),
            ));
        }
        let _ = Self::parse_plugin_type(&plugin_type)?;
        let _ = Self::validate_parameters(parameters)?;
        let _ = Self::resolve_entry_point(
            &entry_point,
            temp_dir.path(),
            metadata_dir.as_deref(),
        )?;
        Self::ensure_newer_version(&version, &existing.version)?;

        self.uninstall_plugin(id).await?;
        self.install_plugin_from_bytes(bytes).await
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

    async fn install_plugin_from_bytes(&self, bytes: Vec<u8>) -> Result<Plugin> {
        let (spec, metadata_dir) = Self::read_metadata_from_zip(&bytes)?;
        let PackageMetadata {
            plugin_id,
            name,
            version,
            plugin_type,
            description,
            author,
            entry_point,
            parameters,
        } = spec;

        let plugin_id = Self::normalize_plugin_id(plugin_id, &name)?;
        if self.repo.get(&plugin_id).await.is_ok() {
            return Err(crate::error::AppError::PluginAlreadyExists(
                plugin_id.clone(),
            ));
        }

        if entry_point.trim().is_empty() {
            return Err(crate::error::AppError::Execution(
                "Entry point cannot be empty".to_string(),
            ));
        }

        let plugin_type = Self::parse_plugin_type(&plugin_type)?;
        let parameters_json = Self::validate_parameters(parameters)?;

        let internal_id = Uuid::new_v4().to_string();
        let plugin_dir = Self::plugin_dir_for(&plugin_id)?;

        fs::create_dir_all(&plugin_dir)?;

        if let Err(err) = Self::extract_zip(&bytes, &plugin_dir) {
            let _ = fs::remove_dir_all(&plugin_dir);
            return Err(err);
        }

        let entry_point = match Self::resolve_entry_point(
            &entry_point,
            &plugin_dir,
            metadata_dir.as_deref(),
        ) {
            Ok(entry_point) => entry_point,
            Err(err) => {
                let _ = fs::remove_dir_all(&plugin_dir);
                return Err(err);
            }
        };

        let mut python_venv_path = None;
        let mut python_dependencies_json = None;
        if plugin_type == PluginType::Python {
            let venv_dir = Self::python_env_dir_for(&plugin_id)?;
            let resolved_deps = Self::resolve_python_dependencies(
                &plugin_dir,
                metadata_dir.as_deref(),
                &entry_point,
            );
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

        let now = Utc::now().timestamp_millis();
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

    fn read_metadata_from_dir(
        root: &Path,
    ) -> Result<(PackageMetadata, Option<PathBuf>)> {
        let mut matches = Vec::new();
        let mut stack = vec![root.to_path_buf()];

        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir).map_err(|e| {
                AppError::Execution(format!(
                    "Failed to read extracted package: {}",
                    e
                ))
            })?;
            for entry in entries {
                let entry = entry.map_err(|e| {
                    AppError::Execution(format!(
                        "Failed to read extracted package: {}",
                        e
                    ))
                })?;
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.file_name() == Some(OsStr::new("metadata.json")) {
                    matches.push(path);
                }
            }
        }

        if matches.is_empty() {
            return Err(AppError::Execution(
                "metadata.json not found in package".to_string(),
            ));
        }
        if matches.len() > 1 {
            return Err(AppError::Execution(
                "Multiple metadata.json files found in package".to_string(),
            ));
        }

        let metadata_path = matches.remove(0);
        let buffer = fs::read(&metadata_path).map_err(|e| {
            AppError::Execution(format!("Failed to read metadata.json: {}", e))
        })?;
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
            .parent()
            .and_then(|parent| parent.strip_prefix(root).ok())
            .map(PathBuf::from)
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

    fn resolve_entry_point(
        entry_point: &str,
        root_dir: &Path,
        metadata_dir: Option<&Path>,
    ) -> Result<String> {
        Self::validate_entry_point(entry_point)?;
        let entry_path = root_dir.join(entry_point);
        if entry_path.is_file() {
            return Ok(entry_point.to_string());
        }
        if let Some(dir) = metadata_dir {
            let candidate = dir.join(entry_point);
            let candidate_str = candidate.to_string_lossy().to_string();
            Self::validate_entry_point(&candidate_str)?;
            let candidate_path = root_dir.join(&candidate_str);
            if candidate_path.is_file() {
                return Ok(candidate_str);
            }
        }
        Err(AppError::Execution(format!(
            "Entry point not found: {}",
            entry_path.display()
        )))
    }

    fn normalize_plugin_id(
        plugin_id: Option<String>,
        name: &str,
    ) -> Result<String> {
        let plugin_id_raw = plugin_id.unwrap_or_else(|| name.to_string());
        let plugin_id = plugin_id_raw.trim();
        if plugin_id.is_empty() {
            return Err(AppError::Execution(
                "Plugin id cannot be empty".to_string(),
            ));
        }
        if plugin_id != plugin_id_raw {
            return Err(AppError::Execution(format!(
                "Plugin id has leading/trailing whitespace: {}",
                plugin_id_raw
            )));
        }
        Self::validate_plugin_id(plugin_id)?;
        Ok(plugin_id.to_string())
    }

    fn ensure_newer_version(candidate: &str, current: &str) -> Result<()> {
        let candidate = candidate.trim();
        if candidate.is_empty() {
            return Err(AppError::Execution(
                "Plugin version cannot be empty".to_string(),
            ));
        }
        let current = current.trim();
        let candidate = Version::parse(candidate).map_err(|e| {
            AppError::Execution(format!(
                "Invalid plugin version '{}': {}",
                candidate, e
            ))
        })?;
        let current = Version::parse(current).map_err(|e| {
            AppError::Execution(format!(
                "Invalid installed plugin version '{}': {}",
                current, e
            ))
        })?;
        if candidate <= current {
            return Err(AppError::Execution(format!(
                "Plugin version {} is not newer than installed version {}",
                candidate, current
            )));
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
        metadata_dir: Option<&Path>,
        entry_point: &str,
    ) -> Option<PythonDependencies> {
        let mut search_dirs: Vec<PathBuf> = Vec::new();
        if let Some(dir) = metadata_dir {
            Self::push_unique_dir(&mut search_dirs, dir.to_path_buf());
        }
        if let Some(entry_dir) = Path::new(entry_point).parent() {
            if !entry_dir.as_os_str().is_empty() {
                Self::push_unique_dir(&mut search_dirs, entry_dir.to_path_buf());
            }
        }
        Self::push_unique_dir(&mut search_dirs, PathBuf::new());

        if let Some(path) =
            Self::find_dependency_in_dirs(plugin_dir, &search_dirs, "pyproject.toml")
        {
            return Some(PythonDependencies::Pyproject { path });
        }

        if let Some(path) =
            Self::find_dependency_in_dirs(plugin_dir, &search_dirs, "requirements.txt")
        {
            return Some(PythonDependencies::Requirements { path });
        }

        None
    }

    fn push_unique_dir(target: &mut Vec<PathBuf>, dir: PathBuf) {
        if !target.iter().any(|existing| existing == &dir) {
            target.push(dir);
        }
    }

    fn find_dependency_in_dirs(
        plugin_dir: &Path,
        search_dirs: &[PathBuf],
        filename: &str,
    ) -> Option<String> {
        for dir in search_dirs {
            let relative = dir.join(filename);
            let candidate = plugin_dir.join(&relative);
            if candidate.is_file() {
                return Some(relative.to_string_lossy().to_string());
            }
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
                Some(plugin_dir.to_path_buf())
            }
            PythonDependencies::Pyproject { path } => {
                args.push("-e".to_string());
                args.push(".".to_string());
                let project_root = plugin_dir.join(path);
                let project_root = project_root.parent().unwrap_or(plugin_dir);
                Some(project_root.to_path_buf())
            }
        };

        Self::run_uv_command(&args, current_dir.as_deref()).await?;
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
