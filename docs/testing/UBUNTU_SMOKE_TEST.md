# Experimental Ubuntu validation

The first Ubuntu slice is **0.2.0**, targeting **Ubuntu 26.04 LTS amd64** with
PipeWire and WirePlumber. Windows remains the first production target. Neither
a successful compile nor the checks below imply a supported binary release.

## Current scope

| Surface | Implemented | Acceptance still required |
| --- | --- | --- |
| Everything I hear | Follow the effective default output or pin one output monitor | Physical USB/Bluetooth/HDMI changes, service restart, suspend/resume, long sessions |
| Local captions | Shared speech/model pipeline, transcript, and caption overlay | Broader speech and Chinese/Spanish translation accuracy, latency, and resources |
| Overlay | Initial GTK X11/XWayland path, appearance controls, click-through setup | Native GNOME stacking, click-through, fullscreen, scaling, and multiple monitors |
| Native Wayland | Explicit GTK backend choice is honored | Positioning and overlay behavior are not yet supported/accepted |
| Application audio | Not implemented on Linux | Stable application grouping, isolation, and stream lifecycle |
| Screen translation | Windows-only capability message | Portal/PipeWire screen capture, region selection, OCR, and positioning on Linux |
| Debian package | Native build, private inference libraries, declared dependencies | Fresh GNOME install/upgrade/remove and complete release-wide license inventory |

Ubuntu 24.04 and other distributions are outside the initial supported-package
target. A `.deb` being installable is not a compatibility promise.

## Development checks

Install the [Ubuntu build prerequisites](../../BUILDING.md) first. From the
repository root:

```bash
rustup target add x86_64-pc-windows-msvc
./scripts/check-local.sh
python3 scripts/check-pipewire.py
pnpm --dir apps/desktop tauri build --bundles deb -- --locked
```

`check-pipewire.py` creates its own temporary runtime directory, PipeWire server,
session bus, and WirePlumber **policy** profile. It loads no audio hardware
monitor, changes only its private default output, and cleans up its processes.
The ignored native Rust test refuses to run without that private-session guard.
Synthetic PCM is generated and checked in memory; captured audio is not saved.

The native test creates two output monitors carrying different tones. It checks
default and pinned capture, changes the default while enumerating sources,
confirms the pinned source stays put, removes that source, checks recovery with
no fallback frames, recreates it with the same stable identity, and checks
discontinuity, monotonic sequence/time, and Stop within two seconds.

Ordinary unit tests cover identity ambiguity, missing default metadata, SPA
buffer bounds/wrapping, queue overflow, discontinuity, and advertised source
capabilities. These checks do not require speech models or a GUI.

## Native desktop smoke

Use the ordinary Ubuntu GNOME desktop session, not `sudo`. PipeWire and
WirePlumber must be available in that session. WSLg's PulseAudio service alone
does not supply the native PipeWire graph this backend requires.

1. Start Prollyglot. Check that output devices are listed and the selected model
   becomes ready. Install the Fast English model explicitly if needed.
2. Select **Everything I hear → Follow system default**, then **Start Captions**.
   Play a short, known English speech sample. Confirm the transcript and overlay
   both show useful partial and final text. One Stop click should hide the
   overlay and return the app to idle without late captions reappearing.
3. Pin a specific output and repeat. Change the OS default while sound continues
   through the pinned output; Prollyglot must keep captioning the pinned source.
4. Unplug the selected device during a session. Confirm waiting/recovery is
   visible, unrelated outputs are not captured, and ordinary reconnection resumes
   captions. Repeat with follow-default enabled and check the new default.
5. Open and close Appearance, change caption size/history, and try click-through
   over another application. Check fullscreen, two monitors, and differing scale
   factors. The initial app chooses X11/XWayland when `DISPLAY` is available;
   native Wayland is a separate acceptance task.
6. Open Screen translation. Confirm the Windows-only message explains why it is
   unavailable. Linux application audio should not appear as a selectable mode.
7. Check missing-service handling in an isolated test session: startup should
   preserve a PipeWire connection message below the source controls. Once the
   service is available, refreshing sources should clear it.
8. For translation acceptance, test known Chinese and Spanish material, preserving
   original text beside English. Measure first original and first translated
   caption separately, both cold and warm, with the actual selected models.

Do not interrupt a real desktop's audio services just to run an automated test.
The private graph can also host a custom smoke command:

```bash
python3 scripts/check-pipewire.py python3 /path/to/native-ui-smoke.py
```

Native GUI automation uses Tauri's documented `tauri-driver` and Ubuntu 26.04's
`webkitgtk-webdriver` package (`WebKitWebDriver`). It tests the actual
`tauri://localhost` WebViews and native commands. A browser-only preview cannot
validate GTK window creation, PipeWire routing, native model loading, or package
library resolution. Keep automation profiles, screenshots, fixture audio, and
copied test models outside the checkout and the user's application profile.

## Evidence recorded on 2026-09-08

Environment: Ubuntu 26.04 LTS under WSL2/WSLg, PipeWire 1.6.2, GTK 3.24.52,
WebKitGTK 2.52.6, and native Tauri/WebKitWebDriver automation. This is a native
Linux process and audio graph, but not a native GNOME or physical-device test.

- `check-local.sh`: **153 Rust tests passed**, five explicitly ignored
  model/private-graph tests; **58 frontend tests passed**. Formatting, Clippy,
  generated bindings, frontend production build, and available Windows cross
  checks passed.
- Private routing/recreation fixture: all signal and lifecycle assertions
  passed. The unwanted tone remained below 0.05% of the selected signal in this
  run; backend Stop took approximately **1–15 ms**.
- Native desktop, Fast English model: both follow-default and pinned-output
  sessions produced 18 recognized words from the same 6.625-second public speech
  fixture, displayed the text in the caption overlay, and stopped successfully.
  First captions appeared **1,368 ms** and **1,267 ms** after playback started;
  UI Stop-to-idle took **121 ms** in both sessions. These are single-run
  development measurements, not latency guarantees or translation benchmarks.
- Native window checks: page identity, rendered transcript/overlay, repeated
  Start/Stop, Windows-only visual capability, Appearance open/close, and no
  recorded JavaScript errors passed. Screenshots showed readable wrapped
  captions. WSLg emitted DRI3 acceleration warnings while rendering successfully.
- Missing-service startup: the PipeWire message remained visible after bootstrap
  settled. Launch also succeeded without an explicit `GDK_BACKEND` override.
- Native Windows regression checks also passed: **148 Rust tests**, four ignored
  model tests, **58 frontend tests**, generated bindings, formatting, workspace
  Clippy, and a complete MSVC desktop build/link.

The speech sample is the pinned sherpa-onnx English model's
[public test WAV](https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/d42f2d9f7ca24806fb667456a18a9f1b60f70d16/test_wavs/0.wav).
The final word was recognized incorrectly, so this run establishes the working
capture-to-overlay path, not perfect recognition or representative accuracy.
No user audio or media was used for these checks.

## Package validation

Inspect the built package before installation:

```bash
dpkg-deb --info target/release/bundle/deb/Prollyglot_0.2.0_amd64.deb
dpkg-deb --contents target/release/bundle/deb/Prollyglot_0.2.0_amd64.deb
```

Extract it into a temporary directory and run the executable with no
`LD_LIBRARY_PATH` override to check that the private speech/ONNX libraries resolve
from the extracted package. Repeat a caption session from that executable.
This is necessary package evidence but does not prove package-manager behavior.
Before a supported Ubuntu release, install on a fresh Ubuntu 26.04 GNOME machine,
test the desktop launcher, explicitly install a model, caption offline, upgrade,
and remove the package. Preserve user models and preferences during uninstall.

The 0.2.0 optimized `.deb` built locally is approximately **30.5 MiB**
(about **81.6 MiB** installed, without models). It includes a desktop launcher,
application icon, the two private inference libraries, and the recorded notices.
Extraction and `ldd` checks confirmed that both libraries resolve inside the
package with no `LD_LIBRARY_PATH` override. The extracted release executable
passed the same two native caption/overlay sessions: first captions at
**1,369 ms / 1,257 ms**, UI Stop-to-idle at **117 ms / 121 ms**, and no recorded
JavaScript errors. This verifies the packaged runtime independently of Cargo's
build directory.

On the development Ubuntu/WSL system, `apt-get install` installed the new
`prollyglot` package without upgrading or removing other packages. `dpkg --verify`
passed, and the installed `/usr/bin/prollyglot-desktop` launched successfully
without a library-path override. `apt-get remove prollyglot` removed the binary,
private libraries, and launcher while preserving the separate application test
data. A fresh native GNOME install/upgrade/removal remains an acceptance gate.

Follow the [PipeWire capture API](https://docs.pipewire.org/audio-capture_8c-example.html),
[WirePlumber linking policy](https://pipewire.pages.freedesktop.org/wireplumber/policies/linking.html),
and [Tauri Debian packaging](https://v2.tauri.app/distribute/debian/) when extending
this slice. Native Wayland and application capture must meet their own acceptance
criteria before being advertised as supported.
