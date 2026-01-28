use crate::repository::DbPool;
use anyhow::Result;
use sqlx::Row;

pub async fn establish_connection(database_url: &str) -> Result<DbPool> {
    // Ensure the database URL has the correct format
    let db_url = if database_url.starts_with("sqlite:") {
        database_url.to_string()
    } else {
        format!("sqlite:{}", database_url)
    };

    // Create connection with create_if_missing option
    let connection_string = format!("{}?mode=rwc", db_url);
    let pool = sqlx::SqlitePool::connect(&connection_string).await?;

    // Run migrations
    sqlx::query(
        r#"
        -- 插件表
        CREATE TABLE IF NOT EXISTS plugins (
            id TEXT PRIMARY KEY,
            plugin_id TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            version TEXT NOT NULL,
            min_atom_node_version TEXT,
            plugin_type INTEGER NOT NULL,
            description TEXT,
            author TEXT,
            plugin_path TEXT NOT NULL,
            entry_point TEXT NOT NULL,
            enabled BOOLEAN NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            parameters TEXT,
            python_venv_path TEXT,
            python_dependencies TEXT
        );

        -- 执行记录表
        CREATE TABLE IF NOT EXISTS executions (
            id TEXT PRIMARY KEY,
            plugin_id TEXT NOT NULL,
            phase INTEGER NOT NULL DEFAULT 0,
            status INTEGER NOT NULL,
            pid INTEGER,
            exit_code INTEGER,
            stdout TEXT,
            stderr TEXT,
            preview_payload TEXT,
            confirm_token TEXT,
            expires_at INTEGER,
            started_at INTEGER NOT NULL,
            finished_at INTEGER,
            FOREIGN KEY (plugin_id) REFERENCES plugins(plugin_id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_executions_plugin_id ON executions(plugin_id);
        CREATE INDEX IF NOT EXISTS idx_plugins_enabled ON plugins(enabled);
        CREATE INDEX IF NOT EXISTS idx_plugins_plugin_id ON plugins(plugin_id);
        CREATE INDEX IF NOT EXISTS idx_plugins_name ON plugins(name);
        "#,
    )
    .execute(&pool)
    .await?;

    ensure_min_atom_node_version_column(&pool).await?;
    ensure_execution_new_columns(&pool).await?;

    Ok(pool)
}

async fn ensure_min_atom_node_version_column(pool: &DbPool) -> Result<()> {
    let columns = sqlx::query("PRAGMA table_info(plugins)")
        .fetch_all(pool)
        .await?;
    let has_column = columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "min_atom_node_version");
    if !has_column {
        sqlx::query("ALTER TABLE plugins ADD COLUMN min_atom_node_version TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn ensure_execution_new_columns(pool: &DbPool) -> Result<()> {
    let columns = sqlx::query("PRAGMA table_info(executions)")
        .fetch_all(pool)
        .await?;

    let mut has_phase = false;
    let mut has_preview_payload = false;
    let mut has_confirm_token = false;
    let mut has_expires_at = false;

    for row in &columns {
        let name: String = row.get("name");
        match name.as_str() {
            "phase" => has_phase = true,
            "preview_payload" => has_preview_payload = true,
            "confirm_token" => has_confirm_token = true,
            "expires_at" => has_expires_at = true,
            _ => {}
        }
    }

    if !has_phase {
        sqlx::query("ALTER TABLE executions ADD COLUMN phase INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await?;
    }
    if !has_preview_payload {
        sqlx::query("ALTER TABLE executions ADD COLUMN preview_payload TEXT")
            .execute(pool)
            .await?;
    }
    if !has_confirm_token {
        sqlx::query("ALTER TABLE executions ADD COLUMN confirm_token TEXT")
            .execute(pool)
            .await?;
    }
    if !has_expires_at {
        sqlx::query("ALTER TABLE executions ADD COLUMN expires_at INTEGER")
            .execute(pool)
            .await?;
    }

    Ok(())
}
