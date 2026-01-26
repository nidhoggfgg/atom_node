#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Package atom_node with uv (no bundled Python), then zip it.

Usage:
  scripts/package_bundle.sh [--uv-version X] [--uv-url URL] [--output-dir DIR] [--target TARGET] [--skip-build]

Options:
  --uv-version X   uv version to bundle (default: 0.9.26 or $UV_VERSION)
  --uv-url URL     Override uv download URL (default: release asset for target)
  --output-dir DIR Output directory for the bundle (default: ./dist)
  --target TARGET  Cargo target triple for cross builds (e.g. x86_64-pc-windows-msvc)
  --skip-build     Skip cargo build step
  -h, --help       Show this help
EOF
}

UV_VERSION="${UV_VERSION:-0.9.26}"
UV_URL="${UV_URL:-}"
OUTPUT_DIR=""
CARGO_TARGET=""
SKIP_BUILD=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --uv-version)
      UV_VERSION="$2"
      shift 2
      ;;
    --uv-url)
      UV_URL="$2"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --target)
      CARGO_TARGET="$2"
      shift 2
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CARGO_TOML="$ROOT_DIR/Cargo.toml"

read_cargo_field() {
  local key="$1"
  awk -F= -v key="$key" '
    $0 ~ /^\[package\]/ {in_pkg=1; next}
    in_pkg && $0 ~ /^\[/ {in_pkg=0}
    in_pkg && $1 ~ "^[[:space:]]*" key "[[:space:]]*$" {
      val=$2
      sub(/^[[:space:]]*/, "", val)
      sub(/[[:space:]]*$/, "", val)
      gsub(/"/, "", val)
      print val
      exit
    }
  ' "$CARGO_TOML"
}

detect_host_os() {
  local os_raw
  os_raw="$(uname -s)"
  case "$os_raw" in
    Linux) echo "linux" ;;
    Darwin) echo "darwin" ;;
    MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
    *) echo "$(echo "$os_raw" | tr '[:upper:]' '[:lower:]')" ;;
  esac
}

detect_host_arch() {
  local arch_raw
  arch_raw="$(uname -m)"
  case "$arch_raw" in
    x86_64|amd64) echo "x86_64" ;;
    aarch64|arm64) echo "aarch64" ;;
    *) echo "$arch_raw" ;;
  esac
}

if [[ ! -f "$CARGO_TOML" ]]; then
  echo "Cargo.toml not found at $CARGO_TOML" >&2
  exit 1
fi

NAME="$(read_cargo_field name)"
VERSION="$(read_cargo_field version)"

if [[ -z "$NAME" || -z "$VERSION" ]]; then
  echo "Failed to read package name/version from $CARGO_TOML" >&2
  exit 1
fi

HOST_OS="$(detect_host_os)"
HOST_ARCH="$(detect_host_arch)"

if [[ -n "$CARGO_TARGET" ]]; then
  TARGET_ARCH="${CARGO_TARGET%%-*}"
  case "$CARGO_TARGET" in
    *-pc-windows-*|*-windows-*) OS="windows" ;;
    *-apple-darwin) OS="darwin" ;;
    *-unknown-linux-*|*-linux-*) OS="linux" ;;
    *) OS="unknown" ;;
  esac
  case "$TARGET_ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) ARCH="$TARGET_ARCH" ;;
  esac
  if [[ "$OS" == "unknown" ]]; then
    OS="$HOST_OS"
  fi
  if [[ -z "$ARCH" ]]; then
    ARCH="$HOST_ARCH"
  fi
else
  OS="$HOST_OS"
  ARCH="$HOST_ARCH"
fi

BUNDLE_NAME="${NAME}-${VERSION}-${OS}-${ARCH}"
DIST_DIR="${OUTPUT_DIR:-$ROOT_DIR/dist}"
BUNDLE_DIR="$DIST_DIR/$BUNDLE_NAME"

download_file() {
  local url="$1"
  local dest="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$dest"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$dest" "$url"
  else
    echo "curl or wget is required to download $url" >&2
    exit 1
  fi
}

extract_zip() {
  local zip_path="$1"
  local dest_dir="$2"

  if command -v unzip >/dev/null 2>&1; then
    unzip -q "$zip_path" -d "$dest_dir"
  elif command -v python3 >/dev/null 2>&1; then
    python3 - "$zip_path" "$dest_dir" <<'PY'
import sys
import zipfile

zip_path, dest_dir = sys.argv[1:3]
with zipfile.ZipFile(zip_path, "r") as zf:
    zf.extractall(dest_dir)
PY
  else
    echo "unzip or python3 is required to extract $zip_path" >&2
    exit 1
  fi
}

extract_targz() {
  local archive_path="$1"
  local dest_dir="$2"

  if command -v tar >/dev/null 2>&1; then
    tar -xzf "$archive_path" -C "$dest_dir"
  else
    echo "tar is required to extract $archive_path" >&2
    exit 1
  fi
}

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  if [[ -n "$CARGO_TARGET" && "$OS" == "windows" ]]; then
    if command -v cargo-xwin >/dev/null 2>&1; then
      cargo xwin build --release --target "$CARGO_TARGET"
    else
      echo "cargo-xwin not found. Install with: cargo install cargo-xwin" >&2
      exit 1
    fi
  elif [[ -n "$CARGO_TARGET" ]]; then
    cargo build --release --target "$CARGO_TARGET"
  else
    cargo build --release
  fi
fi

if [[ -n "$CARGO_TARGET" ]]; then
  BIN_DIR="$ROOT_DIR/target/$CARGO_TARGET/release"
else
  BIN_DIR="$ROOT_DIR/target/release"
fi

BIN_NAME="$NAME"
if [[ "$OS" == "windows" ]]; then
  BIN_NAME="${NAME}.exe"
fi
BIN_PATH="$BIN_DIR/$BIN_NAME"

if [[ ! -f "$BIN_PATH" ]]; then
  echo "Binary not found: $BIN_PATH" >&2
  exit 1
fi

rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR/bin"
mkdir -p "$BUNDLE_DIR/conf"

cp "$BIN_PATH" "$BUNDLE_DIR/bin/$BIN_NAME"
echo "$VERSION" > "$BUNDLE_DIR/VERSION"

UV_EXT=""
UV_URL_RESOLVED="$UV_URL"
if [[ -n "$UV_URL_RESOLVED" ]]; then
  case "$UV_URL_RESOLVED" in
    *.zip) UV_EXT="zip" ;;
    *.tar.gz) UV_EXT="tar.gz" ;;
    *.tgz) UV_EXT="tgz" ;;
    *)
      echo "Unsupported uv URL (expected .zip or .tar.gz): $UV_URL_RESOLVED" >&2
      exit 1
      ;;
  esac
else
  case "$OS" in
    windows)
      UV_TARGET="${ARCH}-pc-windows-msvc"
      UV_EXT="zip"
      ;;
    linux)
      UV_TARGET="${ARCH}-unknown-linux-gnu"
      UV_EXT="tar.gz"
      ;;
    darwin)
      UV_TARGET="${ARCH}-apple-darwin"
      UV_EXT="tar.gz"
      ;;
    *)
      echo "Unsupported OS for uv download: $OS" >&2
      exit 1
      ;;
  esac
  UV_URL_RESOLVED="https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-${UV_TARGET}.${UV_EXT}"
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

UV_ARCHIVE="$TMP_DIR/uv.$UV_EXT"
UV_EXTRACT_DIR="$TMP_DIR/uv_extract"
mkdir -p "$UV_EXTRACT_DIR"

download_file "$UV_URL_RESOLVED" "$UV_ARCHIVE"

if [[ "$UV_EXT" == "zip" ]]; then
  extract_zip "$UV_ARCHIVE" "$UV_EXTRACT_DIR"
else
  extract_targz "$UV_ARCHIVE" "$UV_EXTRACT_DIR"
fi

if [[ "$OS" == "windows" ]]; then
  UV_BIN_PATH="$(find "$UV_EXTRACT_DIR" -type f -name "uv.exe" | head -n1)"
  if [[ -z "$UV_BIN_PATH" ]]; then
    echo "uv.exe not found in $UV_URL_RESOLVED" >&2
    exit 1
  fi
  cp "$UV_BIN_PATH" "$BUNDLE_DIR/bin/uv.exe"
else
  UV_BIN_PATH="$(find "$UV_EXTRACT_DIR" -type f -name "uv" | head -n1)"
  if [[ -z "$UV_BIN_PATH" ]]; then
    echo "uv binary not found in $UV_URL_RESOLVED" >&2
    exit 1
  fi
  cp "$UV_BIN_PATH" "$BUNDLE_DIR/bin/uv"
  chmod +x "$BUNDLE_DIR/bin/uv"
fi

if [[ "$OS" == "windows" ]]; then
  UV_RELATIVE_PATH="bin/uv.exe"
else
  UV_RELATIVE_PATH="bin/uv"
fi

cat > "$BUNDLE_DIR/conf/config.json" <<EOF
{
  "database_url": "sqlite:data/atom_node.db",
  "host": "127.0.0.1",
  "port": 6701,
  "uv_path": "$UV_RELATIVE_PATH"
}
EOF

if [[ "$OS" == "windows" ]]; then
  cat > "$BUNDLE_DIR/$NAME.cmd" <<EOF
@echo off
setlocal
set "ROOT_DIR=%~dp0"
set "PATH=%ROOT_DIR%bin;%PATH%"
"%ROOT_DIR%bin\\$BIN_NAME" %*
EOF
else
  cat > "$BUNDLE_DIR/$NAME" <<EOF
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=\$(cd "\$(dirname "\${BASH_SOURCE[0]}")" && pwd)
export PATH="\$ROOT_DIR/bin:\$PATH"

exec "\$ROOT_DIR/bin/$BIN_NAME" "\$@"
EOF
  chmod +x "$BUNDLE_DIR/$NAME"
fi

ZIP_PATH="$DIST_DIR/$BUNDLE_NAME.zip"
rm -f "$ZIP_PATH"

if command -v zip >/dev/null 2>&1; then
  (cd "$DIST_DIR" && zip -r "$ZIP_PATH" "$BUNDLE_NAME")
else
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$DIST_DIR" "$BUNDLE_NAME" "$ZIP_PATH" <<'PY'
import os
import sys
import zipfile

dist_dir, bundle_name, zip_path = sys.argv[1:4]
bundle_dir = os.path.join(dist_dir, bundle_name)

with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    for root, _, files in os.walk(bundle_dir):
        for file_name in files:
            full_path = os.path.join(root, file_name)
            rel_path = os.path.relpath(full_path, dist_dir)
            zf.write(full_path, rel_path)
PY
  else
    echo "zip not found and python3 is unavailable to create the archive." >&2
    exit 1
  fi
fi

echo "Bundle created:"
echo "  $BUNDLE_DIR"
echo "  $ZIP_PATH"
