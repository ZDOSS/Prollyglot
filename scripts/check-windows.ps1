$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $ProjectRoot

try {
    cargo fmt --all -- --check
    cargo test --locked `
        -p prollyglot-application-runtime `
        -p prollyglot-config `
        -p prollyglot-core `
        -p prollyglot-audio-pipeline `
        -p prollyglot-asr `
        -p prollyglot-asr-sherpa `
        -p prollyglot-transcript `
        -p prollyglot-model-manager `
        -p prollyglot-resource-coordinator `
        -p prollyglot-visual-pipeline `
        -p prollyglot-visual-ocr-rapid `
        -p prollyglot-visual-windows `
        --all-targets
    cargo test --locked -p prollyglot-desktop --lib
    cargo run --locked -p prollyglot-application-runtime --bin export-runtime-bindings -- --check
    pnpm --dir apps/desktop test
    pnpm --dir apps/desktop build
    # This must perform a real MSVC link. `cargo check` and Clippy did not catch
    # collisions between the native speech and OCR inference runtimes.
    cargo build --locked -p prollyglot-desktop
    cargo clippy --locked --workspace --all-targets -- -D warnings
}
finally {
    Pop-Location
}
