# Batch Create Directories Plugin

This example plugin creates multiple directories in one run.

## Parameters

- `base_dir` (`directory`): Base directory for relative paths.
- `directories` (`textarea`): One directory path per line.
- `exist_ok` (`boolean`): Treat existing directories as success.
- `create_parents` (`boolean`): Create missing parent directories automatically.
- `allow_absolute_paths` (`boolean`): Allow absolute paths in `directories`.

## Example `directories` Input

```text
logs
output/reports
tmp/cache
```

## Phase Behavior

- `prepare`: Validates input and returns a preview plan.
- `apply`: Creates directories based on preview plan (or recalculates if absent).
