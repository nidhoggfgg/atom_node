use crate::error::{AppError, Result};
use crate::models::Plugin;
use crate::repository::DbPool;
use chrono::Utc;

#[derive(Clone)]
pub struct PluginRepository {
    pool: DbPool,
}

impl PluginRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Plugin>> {
        let plugins = sqlx::query_as::<_, Plugin>(
            r#"
            SELECT id, name, version, plugin_type, description, author, plugin_path, entry_point,
                   enabled, created_at, updated_at, metadata, parameters,
                   python_venv_path, python_dependencies
            FROM plugins
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(plugins)
    }

    pub async fn get(&self, id: &str) -> Result<Plugin> {
        let plugin = sqlx::query_as::<_, Plugin>(
            r#"
            SELECT id, name, version, plugin_type, description, author, plugin_path, entry_point,
                   enabled, created_at, updated_at, metadata, parameters,
                   python_venv_path, python_dependencies
            FROM plugins
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::PluginNotFound(id.to_string()))?;

        Ok(plugin)
    }

    pub async fn get_by_name(&self, name: &str) -> Result<Plugin> {
        let plugin = sqlx::query_as::<_, Plugin>(
            r#"
            SELECT id, name, version, plugin_type, description, author, plugin_path, entry_point,
                   enabled, created_at, updated_at, metadata, parameters,
                   python_venv_path, python_dependencies
            FROM plugins
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::PluginNotFound(name.to_string()))?;

        Ok(plugin)
    }

    pub async fn create(&self, plugin: &Plugin) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO plugins (id, name, version, plugin_type, description, author, code, plugin_path, entry_point, enabled, created_at, updated_at, metadata, parameters, python_venv_path, python_dependencies)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&plugin.id)
        .bind(&plugin.name)
        .bind(&plugin.version)
        .bind(plugin.plugin_type as i32)
        .bind(&plugin.description)
        .bind(&plugin.author)
        .bind("")
        .bind(&plugin.plugin_path)
        .bind(&plugin.entry_point)
        .bind(plugin.enabled)
        .bind(plugin.created_at)
        .bind(plugin.updated_at)
        .bind(&plugin.metadata)
        .bind(&plugin.parameters)
        .bind(&plugin.python_venv_path)
        .bind(&plugin.python_dependencies)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    #[allow(unused)]
    pub async fn update(&self, plugin: &Plugin) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE plugins
            SET name = ?, version = ?, plugin_type = ?, description = ?, author = ?, plugin_path = ?, entry_point = ?, enabled = ?, updated_at = ?, metadata = ?, parameters = ?, python_venv_path = ?, python_dependencies = ?
            WHERE id = ?
            "#,
        )
        .bind(&plugin.name)
        .bind(&plugin.version)
        .bind(plugin.plugin_type as i32)
        .bind(&plugin.description)
        .bind(&plugin.author)
        .bind(&plugin.plugin_path)
        .bind(&plugin.entry_point)
        .bind(plugin.enabled)
        .bind(Utc::now())
        .bind(&plugin.metadata)
        .bind(&plugin.parameters)
        .bind(&plugin.python_venv_path)
        .bind(&plugin.python_dependencies)
        .bind(&plugin.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM plugins WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::PluginNotFound(id.to_string()));
        }

        Ok(())
    }

    pub async fn update_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        sqlx::query("UPDATE plugins SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled)
            .bind(Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
