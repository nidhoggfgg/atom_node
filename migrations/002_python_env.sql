-- Add python venv metadata for plugin dependency management
ALTER TABLE plugins ADD COLUMN python_venv_path TEXT;
ALTER TABLE plugins ADD COLUMN python_dependencies TEXT;
