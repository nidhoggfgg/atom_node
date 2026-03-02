import json
import os
import sys
from pathlib import Path


def load_params() -> dict:
    raw_params = os.getenv("ANTHILL_PLUGIN_PARAMS")
    if not raw_params:
        return {}
    try:
        return json.loads(raw_params)
    except json.JSONDecodeError:
        raise ValueError("ANTHILL_PLUGIN_PARAMS is not valid JSON")


def parse_directory_lines(value: str) -> list[str]:
    if not isinstance(value, str):
        return []
    lines = []
    for line in value.splitlines():
        item = line.strip()
        if not item or item.startswith("#"):
            continue
        lines.append(item)
    return lines


def resolve_targets(base_dir: str, items: list[str], allow_absolute_paths: bool) -> list[Path]:
    base_path = Path(base_dir).expanduser()
    targets = []
    for item in items:
        candidate = Path(item).expanduser()
        if candidate.is_absolute():
            if not allow_absolute_paths:
                raise ValueError(f"Absolute path is not allowed: {item}")
            target = candidate
        else:
            target = base_path / candidate
        targets.append(target)
    return targets


def build_plan(params: dict) -> dict:
    base_dir = str(params.get("base_dir", "."))
    exist_ok = bool(params.get("exist_ok", True))
    create_parents = bool(params.get("create_parents", True))
    allow_absolute_paths = bool(params.get("allow_absolute_paths", False))
    directories = parse_directory_lines(params.get("directories", ""))

    if not directories:
        raise ValueError("Parameter 'directories' is required and cannot be empty")

    targets = resolve_targets(base_dir, directories, allow_absolute_paths)
    existing = [str(path) for path in targets if path.exists()]
    to_create = [str(path) for path in targets if not path.exists()]

    message = (
        f"Will create {len(to_create)} directory(s)"
        f" and keep {len(existing)} existing directory(s)."
    )

    warnings = []
    if existing and not exist_ok:
        warnings.append("Some directories already exist and apply phase may fail.")
    if not create_parents:
        warnings.append("Parent directories will not be auto-created.")

    return {
        "phase": "prepare",
        "operation": "create_directories",
        "message": message,
        "base_dir": base_dir,
        "total": len(targets),
        "targets": [str(path) for path in targets],
        "existing": existing,
        "to_create": to_create,
        "options": {
            "exist_ok": exist_ok,
            "create_parents": create_parents,
            "allow_absolute_paths": allow_absolute_paths,
        },
        "warnings": warnings,
    }


def apply_plan(params: dict, preview_plan_raw: str | None) -> dict:
    plan = None
    if preview_plan_raw:
        try:
            plan = json.loads(preview_plan_raw)
        except json.JSONDecodeError:
            raise ValueError("ANTHILL_PREVIEW_PLAN is not valid JSON")

    if not plan:
        plan = build_plan(params)

    options = plan.get("options", {})
    exist_ok = bool(options.get("exist_ok", True))
    create_parents = bool(options.get("create_parents", True))
    targets = [Path(path) for path in plan.get("targets", [])]

    created = []
    skipped_existing = []
    failed = []

    for target in targets:
        try:
            existed_before = target.exists()
            target.mkdir(parents=create_parents, exist_ok=exist_ok)
            if existed_before:
                skipped_existing.append(str(target))
            else:
                created.append(str(target))
        except Exception as exc:  # noqa: BLE001
            failed.append({"path": str(target), "error": str(exc)})

    result = {
        "phase": "apply",
        "status": "success" if not failed else "partial_failure",
        "created": created,
        "skipped_existing": skipped_existing,
        "failed": failed,
    }
    return result


def main() -> None:
    try:
        params = load_params()
        phase = os.getenv("ANTHILL_PHASE", "apply")
        preview_plan_raw = os.getenv("ANTHILL_PREVIEW_PLAN")

        if phase == "prepare":
            plan = build_plan(params)
            print(json.dumps(plan, ensure_ascii=False))
            return

        if phase == "apply":
            result = apply_plan(params, preview_plan_raw)
            print(json.dumps(result, ensure_ascii=False))
            if result["failed"]:
                sys.exit(1)
            return

        raise ValueError(f"Unknown ANTHILL_PHASE: {phase}")
    except Exception as exc:  # noqa: BLE001
        print(json.dumps({"status": "error", "error": str(exc)}, ensure_ascii=False))
        sys.exit(1)


if __name__ == "__main__":
    main()
