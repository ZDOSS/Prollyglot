$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $ProjectRoot

try {
    cargo fmt --all -- --check
    cargo test --locked `
        -p prollyglot-core `
        -p prollyglot-audio-pipeline `
        -p prollyglot-asr `
        -p prollyglot-asr-sherpa `
        -p prollyglot-transcript `
        -p prollyglot-model-manager `
        -p prollyglot-visual-pipeline `
        -p prollyglot-visual-ocr-rapid `
        -p prollyglot-visual-windows `
        --all-targets
    pnpm --dir apps/desktop build
    cargo clippy --locked --workspace --all-targets -- -D warnings
}
finally {
    Pop-Location
}
