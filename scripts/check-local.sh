#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

cargo fmt --all -- --check
cargo test --locked \
  -p prollyglot-application-runtime \
  -p prollyglot-config \
  -p prollyglot-core \
  -p prollyglot-audio-pipeline \
  -p prollyglot-asr \
  -p prollyglot-asr-sherpa \
  -p prollyglot-transcript \
  -p prollyglot-model-manager \
  -p prollyglot-visual-pipeline \
  -p prollyglot-visual-ocr-rapid \
  -p prollyglot-visual-windows \
  --all-targets
cargo clippy --locked \
  -p prollyglot-application-runtime \
  -p prollyglot-config \
  -p prollyglot-core \
  -p prollyglot-audio-pipeline \
  -p prollyglot-asr \
  -p prollyglot-asr-sherpa \
  -p prollyglot-transcript \
  -p prollyglot-model-manager \
  -p prollyglot-visual-pipeline \
  -p prollyglot-visual-ocr-rapid \
  -p prollyglot-visual-windows \
  --all-targets -- -D warnings
cargo check --locked -p prollyglot-audio-windows
cargo clippy --locked -p prollyglot-audio-windows --target x86_64-pc-windows-msvc -- -D warnings
cargo clippy --locked -p prollyglot-visual-windows --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo clippy --locked -p prollyglot-asr-sherpa --target x86_64-pc-windows-msvc --lib -- -D warnings
if command -v llvm-rc >/dev/null 2>&1; then
  cargo check --locked -p prollyglot-desktop --lib --tests --target x86_64-pc-windows-msvc
elif windres_path="$(command -v x86_64-w64-mingw32-windres 2>/dev/null)"; then
  RC_x86_64_pc_windows_msvc="$windres_path" \
    cargo check --locked -p prollyglot-desktop --lib --tests --target x86_64-pc-windows-msvc
elif [[ -d "/mnt/c/Program Files (x86)/Windows Kits/10/bin" ]]; then
  windows_sdk_rc="$(
    find "/mnt/c/Program Files (x86)/Windows Kits/10/bin" \
      -type f -path '*/x64/rc.exe' -print 2>/dev/null \
      | sort -V \
      | tail -n 1
  )"
  if [[ -n "$windows_sdk_rc" ]]; then
    PROLLYGLOT_WINDOWS_RC="$windows_sdk_rc" \
      RC_x86_64_pc_windows_msvc="$project_root/scripts/windows-sdk-llvm-rc.sh" \
      cargo check --locked -p prollyglot-desktop --lib --tests --target x86_64-pc-windows-msvc
  else
    echo "Skipping the desktop Windows cross-check: no x64 Windows SDK rc.exe was found."
  fi
else
  echo "Skipping the desktop Windows cross-check: install llvm-rc or x86_64-w64-mingw32-windres."
fi
cargo run --locked -p prollyglot-application-runtime --bin export-runtime-bindings -- --check
pnpm --dir apps/desktop test
pnpm --dir apps/desktop build
