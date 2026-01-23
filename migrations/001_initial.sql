-- 插件表
CREATE TABLE IF NOT EXISTS plugins (
    id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    plugin_type INTEGER NOT NULL,
    description TEXT,
    author TEXT,
    code TEXT NOT NULL,
    plugin_path TEXT NOT NULL,
    entry_point TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    metadata TEXT,
    parameters TEXT,
    python_venv_path TEXT,
    python_dependencies TEXT
);

-- 执行记录表
CREATE TABLE IF NOT EXISTS executions (
    id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL,
    status INTEGER NOT NULL,
    pid INTEGER,
    exit_code INTEGER,
    stdout TEXT,
    stderr TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    error_message TEXT,
    FOREIGN KEY (plugin_id) REFERENCES plugins(plugin_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_executions_plugin_id ON executions(plugin_id);
CREATE INDEX IF NOT EXISTS idx_plugins_enabled ON plugins(enabled);
