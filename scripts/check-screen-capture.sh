#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test --locked -p prollyglot-visual-pipewire --test portal --no-run
test_args=(--ignored --nocapture --test-threads=1)
if [[ -z "${PROLLYGLOT_VISUAL_OCR_MODEL_DIR:-}" || -z "${PROLLYGLOT_VISUAL_FIXTURE_DIR:-}" ]]; then
  echo "Skipping model-backed OCR: set PROLLYGLOT_VISUAL_OCR_MODEL_DIR and PROLLYGLOT_VISUAL_FIXTURE_DIR to opt in."
  test_args+=(--skip native_ocr)
fi
python3 scripts/check-pipewire.py cargo test --locked -p prollyglot-visual-pipewire \
  --test portal -- "${test_args[@]}"
