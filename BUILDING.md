# Building Prollyglot

Windows 11 with the MSVC toolchain is the primary build and runtime target.
Ubuntu 26.04 LTS amd64 is an experimental native build target. WSL can run its
Linux checks and use Windows interop for native MSVC/WASAPI/WGC fixture tests;
cross-compilation alone cannot prove capture or overlay behavior. WSLg checks
also do not replace native Ubuntu GNOME and physical-device acceptance.

There is no supported binary release yet.

## Experimental Ubuntu prerequisites

Use **Ubuntu 26.04 LTS amd64**, Rust 1.88 or newer with `rustfmt`/`clippy`,
Node.js 22.12 or newer, and pnpm 11. The initial app uses PipeWire/WirePlumber
output/application monitoring and X11/XWayland captions. Screen translation
remains Windows-only; native Wayland overlay acceptance is pending.
Ubuntu 24.04 and other distributions are not supported package targets.

Install native build and runtime dependencies:

```bash
sudo apt-get update
sudo apt-get install build-essential pkg-config libclang-dev libssl-dev \
  libpipewire-0.3-dev pipewire pipewire-bin wireplumber dbus-daemon \
  libwebkit2gtk-4.1-dev libxdo-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf
pnpm --dir apps/desktop install --frozen-lockfile
pnpm --dir apps/desktop tauri dev
```

Run from the repository root in the same desktop session as PipeWire and
WirePlumber. Do not run the app with `sudo`. A normal Ubuntu desktop supplies
these user services; WSLg's PulseAudio server alone is not a PipeWire session.
Missing PipeWire connections appear below the playback-device controls.
When `DISPLAY` is present the app selects GTK's X11 backend; an explicit
`GDK_BACKEND` is honored. Native Wayland is not yet an accepted overlay path.

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
rustup target add x86_64-pc-windows-msvc
./scripts/check-local.sh
```

The script tests and lints shared crates and the PipeWire adapter, checks the
Windows audio/visual/ASR adapters with the MSVC cross target, verifies generated
contracts, runs frontend tests, and builds the production frontend. With the
Ubuntu native dependencies above, it also runs desktop Rust tests and Clippy.
Under WSL,
it also locates a mounted Windows SDK `rc.exe` and checks the complete desktop
Windows library. If no compatible resource compiler is available, it says that
the desktop cross-check was skipped; this is not native Windows acceptance.

Install the Ubuntu prerequisites and MSVC Rust target before using the script
on Linux. These compile checks cannot link a native Windows application without
the Windows build tools. Full Linux workspace tests are also
available after installing the GTK/WebKit/PipeWire development packages.

Run the native routing/recovery test separately:

```bash
python3 scripts/check-pipewire.py
```

This launches a private PipeWire graph, session bus, and WirePlumber policy with
synthetic null outputs. It checks default changes, pinned-output isolation,
output removal/recreation, application isolation, synchronized multi-stream
mixing across outputs, process restart/ambiguity, original playback links, and
bounded Stop. No hardware monitor is loaded, desktop audio routing is unchanged,
and captured PCM is not written to disk. The native tests are ignored in ordinary
Rust checks and refuse to run outside this isolated setup. See
[Ubuntu validation](docs/testing/UBUNTU_SMOKE_TEST.md) for the desktop smoke.

For the experimental screen-capture backend, run
`bash scripts/check-screen-capture.sh`. It exercises a fake portal and native
video on the same isolated services, with no desktop picker or windows. Optional
installed-model OCR checks and the current integration boundary are documented
in [Ubuntu screen capture](docs/testing/UBUNTU_SCREEN_CAPTURE.md).

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

### Ubuntu development package

Build on Ubuntu 26.04 LTS amd64, matching the oldest Ubuntu release this package
targets. Tauri automatically merges `tauri.linux.conf.json`:

```bash
pnpm --dir apps/desktop tauri build --bundles deb -- --locked
dpkg-deb --info target/release/bundle/deb/Prollyglot_0.3.0_amd64.deb
```

The pre-bundle hook copies the linked sherpa-onnx and ONNX Runtime libraries into
a private `/usr/lib/prollyglot` directory and gives them relative runtime search
paths. The `.deb` declares native PipeWire and desktop dependencies and includes
the project license and recorded runtime/model notices. It does not include or
download speech/translation models. A complete release-wide notice inventory
and fresh-machine installation acceptance remain release work.

For local development, `--debug --bundles deb` produces the corresponding
package under `target/debug/bundle/deb`. Debug packages are larger and are not
representative performance artifacts. Install a deliberately chosen build with
`sudo apt install ./target/release/bundle/deb/Prollyglot_0.3.0_amd64.deb`, and remove
it with `sudo apt remove prollyglot`. Uninstalling does not remove the user's
downloaded models or preferences.

### Windows package

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
