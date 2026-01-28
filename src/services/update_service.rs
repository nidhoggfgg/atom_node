use crate::error::{AppError, Result};
use crate::paths;
use chrono::Utc;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const UPDATE_PENDING_FILE: &str = ".update_pending.json";
const UPDATE_STAGING_DIR: &str = ".update_staging";
const PRESERVE_DIRS: [&str; 4] = ["data", "plugins", "work_dir", "conf"];

#[derive(Debug, Serialize, Deserialize)]
struct PendingUpdate {
    staged_path: String,
    created_at: i64,
    package_version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateStatus {
    pub restart_required: bool,
    pub current_version: String,
    pub package_version: String,
}

#[derive(Clone)]
pub struct UpdateService;

impl UpdateService {
    pub fn new() -> Self {
        Self
    }

    pub async fn stage_update(&self, package_url: String) -> Result<UpdateStatus> {
        let install_root = paths::install_root()?;
        let pending_path = pending_update_path(&install_root);
        if pending_path.exists() {
            return Err(AppError::Execution(
                "An update is already pending. Restart to apply it first.".to_string(),
            ));
        }

        let bytes = fetch_bytes(&package_url, "update package").await?;

        let extract_dir = tempfile::Builder::new()
            .prefix("update_extract_")
            .tempdir_in(&install_root)
            .map_err(|e| {
                AppError::Execution(format!("Failed to create update extract dir: {}", e))
            })?;

        extract_zip(&bytes, extract_dir.path())?;
        let update_root = detect_update_root(extract_dir.path())?;
        let package_version = read_update_version(&update_root)?;
        validate_update_root(&update_root, &package_version)?;

        let extract_path = extract_dir.keep();
        let staging_dir = stage_update_root(&install_root, extract_path, update_root)?;

        let pending = PendingUpdate {
            staged_path: staging_dir.to_string_lossy().to_string(),
            created_at: Utc::now().timestamp_millis(),
            package_version: Some(package_version.clone()),
        };
        let payload = serde_json::to_vec_pretty(&pending).map_err(|e| {
            AppError::Execution(format!("Failed to serialize update metadata: {}", e))
        })?;
        fs::write(&pending_path, payload).map_err(|e| {
            AppError::Execution(format!(
                "Failed to write update metadata {}: {}",
                pending_path.display(),
                e
            ))
        })?;

        Ok(UpdateStatus {
            restart_required: true,
            current_version: current_version_string(),
            package_version,
        })
    }

    pub fn apply_pending_update() -> Result<Option<PathBuf>> {
        let install_root = paths::install_root()?;
        let pending_path = pending_update_path(&install_root);
        if !pending_path.is_file() {
            return Ok(None);
        }

        let content = fs::read_to_string(&pending_path).map_err(|e| {
            AppError::Execution(format!(
                "Failed to read update metadata {}: {}",
                pending_path.display(),
                e
            ))
        })?;
        let pending: PendingUpdate = serde_json::from_str(&content)
            .map_err(|e| AppError::Execution(format!("Invalid update metadata: {}", e)))?;

        let staged_path = PathBuf::from(&pending.staged_path);
        if !staged_path.is_dir() {
            return Err(AppError::Execution(format!(
                "Staged update not found: {}",
                staged_path.display()
            )));
        }
        if !staged_path.starts_with(&install_root) {
            return Err(AppError::Execution(
                "Staged update is outside install root".to_string(),
            ));
        }

        apply_update_from_staged(&staged_path, &install_root)?;
        fs::remove_file(&pending_path).map_err(|e| {
            AppError::Execution(format!(
                "Failed to remove update metadata {}: {}",
                pending_path.display(),
                e
            ))
        })?;

        if staged_path.exists() {
            fs::remove_dir_all(&staged_path).map_err(|e| {
                AppError::Execution(format!(
                    "Failed to remove staged update {}: {}",
                    staged_path.display(),
                    e
                ))
            })?;
        }

        Ok(Some(staged_path))
    }
}

fn pending_update_path(install_root: &Path) -> PathBuf {
    install_root.join(UPDATE_PENDING_FILE)
}

fn update_staging_root(install_root: &Path) -> PathBuf {
    install_root.join(UPDATE_STAGING_DIR)
}

fn stage_update_root(
    install_root: &Path,
    extract_root: PathBuf,
    update_root: PathBuf,
) -> Result<PathBuf> {
    let staging_root = update_staging_root(install_root);
    fs::create_dir_all(&staging_root).map_err(|e| {
        AppError::Execution(format!(
            "Failed to create update staging dir {}: {}",
            staging_root.display(),
            e
        ))
    })?;
    let staging_dir = staging_root.join(format!("update_{}", Uuid::new_v4()));

    if update_root == extract_root {
        fs::rename(&extract_root, &staging_dir).map_err(|e| {
            AppError::Execution(format!(
                "Failed to stage update {}: {}",
                extract_root.display(),
                e
            ))
        })?;
        return Ok(staging_dir);
    }

    fs::rename(&update_root, &staging_dir).map_err(|e| {
        AppError::Execution(format!(
            "Failed to stage update {}: {}",
            update_root.display(),
            e
        ))
    })?;

    if extract_root.exists() {
        fs::remove_dir_all(&extract_root).map_err(|e| {
            AppError::Execution(format!(
                "Failed to clean update temp dir {}: {}",
                extract_root.display(),
                e
            ))
        })?;
    }

    Ok(staging_dir)
}

fn detect_update_root(extract_dir: &Path) -> Result<PathBuf> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(extract_dir).map_err(|e| {
        AppError::Execution(format!(
            "Failed to read update contents {}: {}",
            extract_dir.display(),
            e
        ))
    })? {
        let entry = entry.map_err(|e| {
            AppError::Execution(format!(
                "Failed to read update contents {}: {}",
                extract_dir.display(),
                e
            ))
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "__MACOSX" || name == ".DS_Store" {
            continue;
        }
        entries.push(entry);
    }

    if entries.len() == 1 {
        let only = &entries[0];
        let path = only.path();
        if path.is_dir() {
            return Ok(path);
        }
    }

    Ok(extract_dir.to_path_buf())
}

fn validate_update_root(update_root: &Path, package_version: &str) -> Result<()> {
    if !update_root.is_dir() {
        return Err(AppError::Execution(
            "Update package has no root directory".to_string(),
        ));
    }

    ensure_newer_version(package_version)?;

    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_owned()))
        .ok_or_else(|| {
            AppError::Execution("Failed to resolve current executable name".to_string())
        })?;

    let bin_path = update_root.join("bin").join(exe_name);
    if !bin_path.is_file() {
        return Err(AppError::Execution(format!(
            "Update package missing binary at {}",
            bin_path.display()
        )));
    }

    ensure_executable(&bin_path)?;
    let wrapper_path = update_root.join(bin_path.file_name().unwrap());
    if wrapper_path.is_file() {
        ensure_executable(&wrapper_path)?;
    }
    Ok(())
}

fn read_update_version(update_root: &Path) -> Result<String> {
    let version_path = update_root.join("VERSION");
    let content = fs::read_to_string(&version_path).map_err(|e| {
        AppError::Execution(format!(
            "Failed to read update version {}: {}",
            version_path.display(),
            e
        ))
    })?;
    let version = content.trim();
    if version.is_empty() {
        return Err(AppError::Execution(
            "Update version cannot be empty".to_string(),
        ));
    }
    Ok(version.to_string())
}

fn ensure_newer_version(package_version: &str) -> Result<()> {
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| {
        AppError::Execution(format!(
            "Invalid current version '{}': {}",
            env!("CARGO_PKG_VERSION"),
            e
        ))
    })?;
    let candidate = Version::parse(package_version).map_err(|e| {
        AppError::Execution(format!(
            "Invalid update version '{}': {}",
            package_version, e
        ))
    })?;
    if candidate <= current {
        return Err(AppError::Execution(format!(
            "Update version {} is not newer than current version {}",
            candidate, current
        )));
    }
    Ok(())
}

fn current_version_string() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn extract_zip(bytes: &[u8], target_dir: &Path) -> Result<()> {
    let reader = io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| AppError::Execution(format!("Invalid update archive: {}", e)))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| AppError::Execution(format!("Failed to read update archive: {}", e)))?;

        let Some(relative_path) = file.enclosed_name().as_deref().map(Path::to_path_buf) else {
            return Err(AppError::Execution(
                "Invalid file path in update archive".to_string(),
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
        io::copy(&mut file, &mut outfile)?;

        #[cfg(unix)]
        if let Some(mode) = file.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))?;
        }
    }

    Ok(())
}

fn apply_update_from_staged(staged_root: &Path, install_root: &Path) -> Result<()> {
    let entries = fs::read_dir(staged_root).map_err(|e| {
        AppError::Execution(format!(
            "Failed to read staged update {}: {}",
            staged_root.display(),
            e
        ))
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| {
            AppError::Execution(format!(
                "Failed to read staged update {}: {}",
                staged_root.display(),
                e
            ))
        })?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if PRESERVE_DIRS.contains(&name_str.as_ref()) {
            let dest = install_root.join(&name);
            if dest.exists() {
                continue;
            }
        }

        let source = entry.path();
        let dest = install_root.join(&name);
        if dest.exists() {
            remove_path(&dest)?;
        }

        if let Err(err) = fs::rename(&source, &dest) {
            if err.kind() == io::ErrorKind::CrossesDevices {
                copy_path(&source, &dest)?;
                remove_path(&source)?;
            } else {
                return Err(AppError::Execution(format!(
                    "Failed to apply update {} -> {}: {}",
                    source.display(),
                    dest.display(),
                    err
                )));
            }
        }
    }

    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn copy_path(source: &Path, dest: &Path) -> Result<()> {
    if source.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let child_source = entry.path();
            let child_dest = dest.join(entry.file_name());
            copy_path(&child_source, &child_dest)?;
        }
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::copy(source, dest)?;
    ensure_executable(dest)?;
    Ok(())
}

async fn fetch_bytes(url: &str, label: &str) -> Result<Vec<u8>> {
    if let Some(path) = resolve_local_path(url) {
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

    let response = reqwest::get(url)
        .await
        .map_err(|e| AppError::Execution(format!("Failed to download {}: {}", label, e)))?;
    let response = response
        .error_for_status()
        .map_err(|e| AppError::Execution(format!("Failed to download {}: {}", label, e)))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Execution(format!("Failed to read {} bytes: {}", label, e)))?;

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
    if let Some(path) = local_path_from_url(url) {
        return Some(path);
    }

    if url.starts_with("http://") || url.starts_with("https://") {
        return None;
    }

    Some(PathBuf::from(url))
}

fn ensure_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path)?;
        let mut permissions = metadata.permissions();
        let mode = permissions.mode();
        if mode & 0o111 == 0 {
            permissions.set_mode(mode | 0o755);
            fs::set_permissions(path, permissions)?;
        }
    }
    Ok(())
}
