$ErrorActionPreference = "Stop"

function Invoke-CheckedNative {
    param(
        [Parameter(Mandatory = $true)][string]$Description,
        [Parameter(Mandatory = $true)][scriptblock]$Command
    )

    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $ProjectRoot

try {
    Invoke-CheckedNative "Rust formatting" { cargo fmt --all -- --check }
    Invoke-CheckedNative "Rust tests" {
        cargo test --locked `
            -p prollyglot-application-runtime `
            -p prollyglot-config `
            -p prollyglot-core `
            -p prollyglot-audio-pipeline `
            -p prollyglot-audio-windows `
            -p prollyglot-asr `
            -p prollyglot-asr-sherpa `
            -p prollyglot-transcript `
            -p prollyglot-model-manager `
            -p prollyglot-resource-coordinator `
            -p prollyglot-visual-pipeline `
            -p prollyglot-visual-ocr-rapid `
            -p prollyglot-visual-windows `
            --all-targets
    }
    Invoke-CheckedNative "Desktop Rust tests" {
        cargo test --locked -p prollyglot-desktop --lib
    }
    Invoke-CheckedNative "Generated runtime binding check" {
        cargo run --locked -p prollyglot-application-runtime --bin export-runtime-bindings -- --check
    }
    Invoke-CheckedNative "Frontend tests" { pnpm --dir apps/desktop test }
    Invoke-CheckedNative "Frontend production build" { pnpm --dir apps/desktop build }
    # This must perform a real MSVC link. `cargo check` and Clippy did not catch
    # collisions between the native speech and OCR inference runtimes.
    Invoke-CheckedNative "Native desktop build and link" {
        cargo build --locked -p prollyglot-desktop
    }
    Invoke-CheckedNative "Workspace Clippy" {
        cargo clippy --locked --workspace --all-targets -- -D warnings
    }
}
finally {
    Pop-Location
}
