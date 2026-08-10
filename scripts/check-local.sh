#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

cargo fmt --all -- --check
cargo test --locked -p prollyglot-core -p prollyglot-audio-pipeline
cargo check --locked -p prollyglot-audio-windows
cargo check --locked -p prollyglot-audio-windows --target x86_64-pc-windows-msvc
pnpm --dir apps/desktop build
