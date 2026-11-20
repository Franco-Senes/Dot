#!/usr/bin/env bash
set -euo pipefail

# Minimal bash wrapper to build and install the Dot Flatpak locally.
# Usage:
#   ./scripts/flatpak-local.sh            # build & install in --user scope
#   ./scripts/flatpak-local.sh --bundle   # also export dev.senes.Dot.flatpak

ROOT="$(cd -- "$(dirname -- "$0")/.." && pwd)"
APP_ID="dev.senes.Dot"
MANIFEST="$ROOT/build-aux/flatpak/$APP_ID.yml"
BUILD_DIR="$ROOT/target/flatpak-build"
REPO_DIR="$ROOT/target/flatpak-repo"
BUNDLE_PATH="$ROOT/target/$APP_ID.flatpak"

MAKE_BUNDLE=0
for arg in "$@"; do
  case "$arg" in
    --bundle) MAKE_BUNDLE=1 ;;
    -h|--help)
      echo "Usage: $0 [--bundle]"
      exit 0
      ;;
    *)
      echo "error: unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

if ! command -v flatpak-builder >/dev/null 2>&1; then
  echo "error: flatpak-builder is not installed" >&2
  exit 1
fi

if [ ! -f "$ROOT/build-aux/flatpak/cargo-sources.json" ]; then
  echo "error: missing build-aux/flatpak/cargo-sources.json" >&2
  echo "Generate it first with flatpak-cargo-generator against Cargo.lock." >&2
  exit 1
fi

flatpak-builder \
  --user \
  --install \
  --force-clean \
  --repo="$REPO_DIR" \
  "$BUILD_DIR" \
  "$MANIFEST"

if [ "$MAKE_BUNDLE" -eq 1 ]; then
  flatpak build-bundle "$REPO_DIR" "$BUNDLE_PATH" "$APP_ID"
  echo "Bundle created: $BUNDLE_PATH"
fi

echo "Installed $APP_ID. Run with: flatpak run $APP_ID"
