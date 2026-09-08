param([Parameter(Mandatory = $true)][string]$ModelDirectory)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$models = (Resolve-Path $ModelDirectory).Path
Push-Location $root
try {
    cargo build --locked -p prollyglot-visual-ocr-rapid --example capture_check
    if ($LASTEXITCODE -ne 0) { throw 'Native capture verifier did not build.' }
    foreach ($language in @('zh', 'es')) {
        $fixtureScript = Join-Path $PSScriptRoot 'visual-fixture.ps1'
        $fixture = Start-Process powershell.exe -ArgumentList @('-NoLogo', '-NoProfile', '-NonInteractive',
            '-File', ('"' + $fixtureScript + '"'), '-Language', $language, '-Seconds', '90') -PassThru -WindowStyle Hidden
        try {
            cargo run --locked -p prollyglot-visual-ocr-rapid --example capture_check -- $models $language
            if ($LASTEXITCODE -ne 0) { throw "Native $language capture verification failed." }
        }
        finally {
            if (!$fixture.HasExited) { Stop-Process -Id $fixture.Id }
            $fixture.Dispose()
        }
    }
}
finally { Pop-Location }
