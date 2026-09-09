#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${PROLLYGLOT_VISUAL_OCR_MODEL_DIR:?Provide an installed OCR pack}"
: "${PROLLYGLOT_VISUAL_FIXTURE_DIR:?Provide synthetic subtitle images}"
for dependency in Xvfb weston tauri-driver WebKitWebDriver; do
  command -v "$dependency" >/dev/null || { echo "Missing $dependency"; exit 1; }
done
python3 scripts/build-wayland-fixture.py
pnpm --dir apps/desktop build
cargo build --locked -p prollyglot-desktop --features tauri/custom-protocol
cargo test --locked -p prollyglot-visual-pipewire --test desktop --no-run
export PROLLYGLOT_DESKTOP_TEST_BINARY="$(pwd)/target/debug/prollyglot-desktop"
python3 scripts/check-pipewire.py cargo test --locked -p prollyglot-visual-pipewire \
  --test desktop -- --ignored --nocapture --test-threads=1
