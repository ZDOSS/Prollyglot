$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $ProjectRoot

try {
    cargo fmt --all -- --check
    cargo test --locked -p prollyglot-core -p prollyglot-audio-pipeline
    cargo check --locked -p prollyglot-audio-windows
    pnpm --dir apps/desktop build
    cargo check --locked -p prollyglot-desktop
}
finally {
    Pop-Location
}
