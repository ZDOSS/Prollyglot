# Building Prollyglot

Windows 11 with the MSVC toolchain is the primary build and runtime target.
Ubuntu/WSL can validate shared code, the frontend, and the complete Windows
Tauri library target when a Windows SDK is mounted, but it cannot prove WASAPI,
screen capture, overlay, or hardware behavior.

There is no supported binary release yet.

## Windows prerequisites

Install:

- 64-bit Windows 11;
- Rust 1.88 or newer using `stable-x86_64-pc-windows-msvc`, with `rustfmt` and
  `clippy`;
- Microsoft Visual Studio Build Tools with **Desktop development with C++** and
  a current Windows SDK;
- Microsoft Edge WebView2 Runtime;
- Node.js 22.12 or newer; and
- pnpm 11.

Confirm the Rust host is MSVC, not GNU:

```powershell
rustup default stable-msvc
rustup component add rustfmt clippy
rustup show active-toolchain
```

## First checkout

Run every command in this section from the repository root—the directory that
contains `Cargo.toml`, `Prollyglot.md`, and `scripts`.

```powershell
Set-Location C:\path\to\Prollyglot
pnpm --dir apps/desktop install --frozen-lockfile
```

Launch the current development build:

```powershell
pnpm --dir apps/desktop tauri dev
```

The first native build is expected to be much slower than later incremental
builds because the speech and OCR runtimes are compiled and linked. Installed
model count does not change Rust compile time. A selected large model can still
take longer to load after the app starts.

## Required local validation

On native Windows, run:

```powershell
.\scripts\check-windows.ps1
```

This checks formatting, Rust and desktop tests, generated bindings, frontend
tests and build, a real native MSVC link, and workspace Clippy. It is the normal
pre-push gate and consumes no GitHub Actions minutes.

Do not wrap the script in `*>&1 | Tee-Object`. Cargo writes ordinary progress to
stderr, and merging streams under strict PowerShell handling can surface a
non-failure such as `Updating crates.io index` as `NativeCommandError`.

On Ubuntu or WSL, run:

```bash
./scripts/check-local.sh
```

The script tests and lints every platform-neutral crate, checks the Windows
audio/visual/ASR adapters with the MSVC cross target, verifies generated
contracts, runs frontend tests, and builds the production frontend. Under WSL,
it also locates a mounted Windows SDK `rc.exe` and checks the complete desktop
Windows library. If no compatible resource compiler is available, it says that
the desktop cross-check was skipped; this is not native Windows acceptance.

Do not substitute `cargo test --workspace` on an arbitrary Linux host for the
documented script. Tauri's Linux shell dependencies may require GTK/WebKit and
`pkg-config`, while the current product target and desktop adapters are Windows.

## Focused commands

Use focused commands while iterating, then run the appropriate full script
before publishing.

```powershell
cargo fmt --all -- --check
cargo test --locked -p prollyglot-application-runtime -p prollyglot-resource-coordinator
cargo clippy --locked --workspace --all-targets -- -D warnings
pnpm --dir apps/desktop test
pnpm --dir apps/desktop build
node scripts/check-version.mjs
```

The desktop browser preview is useful for layout and controller work that does
not require native capture:

```powershell
pnpm --dir apps/desktop dev
```

Open the Vite URL printed in the terminal. Preview catalogs are fictional test
fixtures and do not represent installed native models.

## Generated runtime contracts

`apps/desktop/src/generated/runtime.ts` is generated from Rust and must not be
edited manually.

Regenerate it after changing a public runtime/configuration/presentation type or
central command/event name:

```powershell
cargo run --locked -p prollyglot-application-runtime --bin export-runtime-bindings
```

Verify that the committed file is current:

```powershell
cargo run --locked -p prollyglot-application-runtime --bin export-runtime-bindings -- --check
```

## Model-dependent tests

Ordinary checks do not download hundreds of megabytes. Tests marked `ignored`
exercise real pinned models and document their required environment variables in
the relevant benchmark or test guide:

- [English models](docs/benchmarks/ENGLISH_MODELS.md)
- [Nemotron multilingual](docs/benchmarks/MULTILINGUAL_NEMOTRON.md)
- [translation models](docs/benchmarks/TRANSLATION_MODELS.md)

Model downloads are explicit, verified, and stored outside the repository.
Never commit model weights, partial downloads, private media, transcripts, or
test recordings.

## Native Windows validation

Use the [five-minute smoke test](docs/testing/WINDOWS_SMOKE_TEST.md) for ordinary
changes. It requires no screenshots or evidence folder. Use the focused
[lifecycle soak](docs/testing/WINDOWS_LIFECYCLE_SOAK.md) when changing session,
capture recovery, inference ownership, or shutdown behavior. The exhaustive
[release plan](docs/testing/WINDOWS_TEST_PLAN.md) is reserved for a deliberate
release or milestone gate.

The lifecycle soak has an optional development-only translation delay. It is
compiled out of production builds and activates only when starting `tauri dev`
with this environment variable:

```powershell
$env:VITE_PROLLYGLOT_TRANSLATION_TEST_DELAY_MS = "3000"
pnpm --dir apps/desktop tauri dev
```

Three seconds intentionally exceeds the 2.5-second live-caption deadline while
remaining inside the 5-second finalized-caption deadline for a compact,
already-installed translator. This exercises timeout and worker replacement
without making every later job impossible to finish.

Close that terminal or remove the variable before ordinary testing:

```powershell
Remove-Item Env:VITE_PROLLYGLOT_TRANSLATION_TEST_DELAY_MS -ErrorAction SilentlyContinue
```

## Packaging

On native Windows, after `check-windows.ps1` and the required smoke/soak gate:

```powershell
pnpm --dir apps/desktop tauri build
```

Tauri inherits the native version from the workspace; do not add a separate
version to `tauri.conf.json`. A successful local bundle is not automatically a
supported release. Release status also requires the acceptance boundaries in
`BUILD_PLAN.md`, synchronized version files, a changelog entry, and native owner
validation.

## Troubleshooting

### Command runs from the wrong directory

If PowerShell cannot find `scripts`, `Cargo.toml`, or `apps/desktop`, first run:

```powershell
Set-Location C:\path\to\Prollyglot
Get-ChildItem Cargo.toml, Prollyglot.md, scripts
```

### MSVC linker errors (`LNK4098`, `LNK1169`, or missing libraries)

Confirm `rustup show active-toolchain` ends in `pc-windows-msvc` and Visual
Studio Build Tools includes the C++ workload and Windows SDK. Stop every running
`tauri dev` instance before rebuilding. Do not mix MinGW/GNU linker variables
into the MSVC shell. If the toolchain is correct and the error persists, send
the first duplicate-symbol lines as well as the final linker line; the final
`LNK1169` line alone does not identify the colliding libraries.

### Frontend dependencies or lockfile changed

Use the committed lockfile:

```powershell
pnpm --dir apps/desktop install --frozen-lockfile
```

Do not update packages merely to make a missing local install disappear.

### Generated binding check fails

Regenerate with the command above, inspect the Rust and TypeScript diff together,
and commit both sides of the contract change.

### Native behavior differs from cross-checks

Cross-compilation cannot validate physical routing, process isolation, sleep,
display scale, overlay stacking, or capture latency. Reproduce on native Windows
and use the newest privacy-safe application log only when diagnosing a failure.
