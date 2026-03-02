use crate::error::AppError;
use crate::models::{Plugin, PluginParameter, PluginParameterGroup, PythonDependencies};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct InstallPluginRequest {
    pub package_url: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePluginRequest {
    pub package_url: String,
}

#[derive(Debug, Serialize)]
pub struct PluginResponse {
    pub id: String,
    pub name: String,
    pub version: String,
    pub min_anthill_version: Option<String>,
    pub plugin_type: String,
    pub description: String,
    pub author: String,
    pub entry_point: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub parameters: Option<Vec<PluginParameter>>,
    pub groups: Option<Vec<PluginParameterGroup>>,
    pub metadata: Option<Value>,
    pub python_dependencies: Option<PythonDependencies>,
}

impl TryFrom<Plugin> for PluginResponse {
    type Error = AppError;

    fn try_from(plugin: Plugin) -> Result<Self, Self::Error> {
        let parameters = parse_parameters(&plugin.parameters)?;
        let groups = parse_groups(&plugin.parameter_groups)?;
        let metadata = parse_metadata(&plugin.metadata)?;
        let python_dependencies = parse_python_dependencies(&plugin.python_dependencies)?;
        Ok(Self {
            id: plugin.plugin_id,
            name: plugin.name,
            version: plugin.version,
            min_anthill_version: plugin.min_anthill_version,
            plugin_type: format!("{:?}", plugin.plugin_type),
            description: plugin.description,
            author: plugin.author,
            entry_point: plugin.entry_point,
            enabled: plugin.enabled,
            created_at: plugin.created_at,
            updated_at: plugin.updated_at,
            parameters,
            groups,
            metadata,
            python_dependencies,
        })
    }
}

fn parse_parameters(raw: &Option<String>) -> Result<Option<Vec<PluginParameter>>, AppError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parameters = serde_json::from_str(trimmed)
        .map_err(|e| AppError::Execution(format!("Invalid plugin parameters: {}", e)))?;
    Ok(Some(parameters))
}

fn parse_python_dependencies(raw: &Option<String>) -> Result<Option<PythonDependencies>, AppError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let dependencies = serde_json::from_str(trimmed)
        .map_err(|e| AppError::Execution(format!("Invalid python dependencies: {}", e)))?;
    Ok(Some(dependencies))
}

fn parse_groups(raw: &Option<String>) -> Result<Option<Vec<PluginParameterGroup>>, AppError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let groups = serde_json::from_str(trimmed)
        .map_err(|e| AppError::Execution(format!("Invalid plugin groups: {}", e)))?;
    Ok(Some(groups))
}

fn parse_metadata(raw: &Option<String>) -> Result<Option<Value>, AppError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let metadata = serde_json::from_str(trimmed)
        .map_err(|e| AppError::Execution(format!("Invalid plugin metadata: {}", e)))?;
    Ok(Some(metadata))
}

#[derive(Debug, Serialize)]
pub struct PluginsListResponse {
    pub data: Vec<PluginResponse>,
}
