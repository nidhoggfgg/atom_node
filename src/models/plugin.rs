use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub plugin_type: PluginType,
    pub description: String,
    pub author: String,
    pub plugin_path: String,
    pub entry_point: String,
    pub enabled: bool,
    pub parameters: Option<String>,
    pub metadata: Option<String>,
    pub python_venv_path: Option<String>,
    pub python_dependencies: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[repr(i32)]
pub enum PluginType {
    Python = 0,
    JavaScript = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PluginParamType {
    String,
    Number,
    Integer,
    Boolean,
    Json,
}

impl PluginParamType {
    pub fn matches(&self, value: &Value) -> bool {
        match self {
            Self::String => value.is_string(),
            Self::Number => value.is_number(),
            Self::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
            Self::Boolean => value.is_boolean(),
            Self::Json => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: PluginParamType,
    pub description: Option<String>,
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum PythonDependencies {
    Requirements { path: String },
    Pyproject { path: String },
}
