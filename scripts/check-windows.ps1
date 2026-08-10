$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $ProjectRoot

try {
    cargo fmt --all -- --check
    cargo test --locked -p prollyglot-core -p prollyglot-audio-pipeline
    pnpm --dir apps/desktop build
    cargo clippy --locked --workspace --all-targets -- -D warnings
}
finally {
    Pop-Location
}
