#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

cargo fmt --all -- --check
cargo test --locked \
  -p prollyglot-core \
  -p prollyglot-audio-pipeline \
  -p prollyglot-asr \
  -p prollyglot-transcript \
  -p prollyglot-model-manager
cargo clippy --locked \
  -p prollyglot-core \
  -p prollyglot-audio-pipeline \
  -p prollyglot-asr \
  -p prollyglot-transcript \
  -p prollyglot-model-manager \
  --all-targets -- -D warnings
cargo check --locked -p prollyglot-audio-windows
cargo clippy --locked -p prollyglot-audio-windows --target x86_64-pc-windows-msvc -- -D warnings
pnpm --dir apps/desktop build
