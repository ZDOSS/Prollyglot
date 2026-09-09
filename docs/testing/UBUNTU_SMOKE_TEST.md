# Experimental Ubuntu validation

The current Ubuntu slice is **0.3.0**, targeting **Ubuntu 26.04 LTS amd64** with
PipeWire and WirePlumber. Windows remains the first production target. Neither
a successful compile nor the checks below imply a supported binary release.

## Current scope

| Surface | Implemented | Acceptance still required |
| --- | --- | --- |
| Everything I hear | Follow the effective default output or pin one output monitor | Physical USB/Bluetooth/HDMI changes, service restart, suspend/resume, long sessions |
| Local captions | Shared speech/model pipeline, transcript, and caption overlay | Broader speech and Chinese/Spanish translation accuracy, latency, and resources |
| Overlay | Initial GTK X11/XWayland path, appearance controls, click-through setup | Native GNOME stacking, click-through, fullscreen, scaling, and multiple monitors |
| Native Wayland | Explicit GTK backend choice is honored | Positioning and overlay behavior are not yet supported/accepted |
| Application audio | Grouped playback streams, synchronized mixing, stream/process restart recovery, ambiguity handling | Real browser/Electron/PulseAudio-bridge/sandbox identities, permissions, hardware clocks, and long sessions |
| Screen translation | Separate portal/PipeWire backend with headless native-frame/OCR checks; desktop mode remains Windows-only | Desktop picker/region controls, Linux presentation and coordinate mapping, GNOME/Wayland acceptance |
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
bash scripts/check-screen-capture.sh
pnpm --dir apps/desktop tauri build --bundles deb -- --locked
```

`check-pipewire.py` creates its own temporary runtime directory, PipeWire server,
session bus, and WirePlumber **policy** profile. It loads no audio hardware
monitor, changes only its private default output, and cleans up its processes.
The ignored native Rust tests refuse to run without that private-session guard.
Synthetic PCM is generated and checked in memory; captured audio is not saved.
The fixtures also remove desktop display/playback environment variables and
create their DBus socket inside the private runtime directory. The separate
[Ubuntu screen-capture checks](UBUNTU_SCREEN_CAPTURE.md) use a fake portal on
that bus and synthetic native video; they never contact the desktop portal.

The output test creates two output monitors carrying different tones. It checks
default and pinned capture, changes the default while enumerating sources,
confirms the pinned source stays put, removes that source, checks recovery with
no fallback frames, recreates it with the same stable identity, and checks
discontinuity, monotonic sequence/time, and Stop within two seconds.

The application test creates multiple playback clients from one process and an
unrelated player. It checks that two selected streams routed to separate outputs
are summed on one clock while rejecting the unrelated tone; normal playback
links stay active. It removes/recreates individual streams, introduces an
ambiguous independent instance, removes all selected streams, and resumes. A
separate real player exits and restarts with a different PID but the same stable
application selection. Resumed PCM is discontinuous with monotonic time; Stop
removes the monitor and preserves playback routes.

Ordinary unit tests cover application grouping, ambiguity, server-lifetime
isolation for unknown clients, process-start parsing, missing default metadata, SPA
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
   unavailable.
7. Check missing-service handling in an isolated test session: startup should
   preserve a PipeWire connection message below the source controls. Once the
   service is available, refreshing sources should clear it.
8. Start audio in two applications, refresh sources, and select **Only [player]**.
   Confirm only its speech reaches the transcript and overlay, including when it
   exposes several playback streams or outputs. Close/reopen the player and
   confirm waiting followed by automatic recovery. If independent instances
   share its identity, capture should wait instead of mixing them.
9. Stop, close the selected application, then refresh. Its selection must remain
   visible as unavailable; Start must not capture Everything I hear. Repeat with
   a removed pinned output. Reopening/reconnecting and refreshing should restore
   the same selection. This also applies on Windows.
10. For translation acceptance, test known Chinese and Spanish material, preserving
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

## Initial 0.2.0 evidence — 2026-09-08

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

## Application capture evidence — 0.3.0, 2026-09-08

The same Ubuntu/WSLg environment above was used with WirePlumber 0.5.13. All
checks used a private audio graph and a separate temporary application profile.

- Shared/Linux validation: **157 Rust tests**, six explicitly ignored native or
  model checks, **58 frontend tests**, formatting, Clippy, generated contracts,
  production frontend build, and available Windows cross-checks passed. Focused
  PipeWire unit tests and Clippy were rerun after the identity-lifetime review.
- Both private native tests passed. Selected application tones remained present
  while unrelated tones stayed below 0.1% of the selected amplitude. Mixing
  approximately one second of two-stream PCM advanced capture time by about
  0.98 seconds, without doubling it. Stream removal/recreation, independent
  instance ambiguity, a real player PID change, preserved output links, and
  monitor cleanup passed. Application backend Stop took **19 ms** in this run.
- The **extracted 0.3.0 release package** ran through native Tauri/WebKit automation
  with no library-path override. A silent selected player produced no captions
  while another player spoke on the same output. Selected speech then produced
  18 recognized words in the transcript and overlay; process exit/restart resumed
  the same capture session and source identity.

| Packaged desktop session | First caption after speech began | UI Stop to idle |
| --- | ---: | ---: |
| Follow default | 1,389 ms | 119 ms |
| Pinned output | 1,358 ms | 246 ms |
| Selected application | 1,373 ms | Continued through player restart |
| Selected application after restart | 1,358 ms | 125 ms |

- Native UI checks confirmed a missing selected application or pinned device
  remains selected and blocks Start, without switching to another source.
  Recreated sources recover their original IDs. Transcript/overlay rendering,
  Appearance open/close, and the Windows-only screen-translation message passed;
  no JavaScript errors were recorded.
- Native Windows regression validation passed: **148 Rust tests**, four ignored
  model checks, **58 frontend tests**, formatting, generated contracts, workspace
  Clippy, and the complete MSVC desktop build/link.
- The `.deb` is approximately **30.6 MiB**, **81.8 MiB installed** without models.
  Both private inference libraries resolve inside the extracted package.
  Package-manager scripts/dependency declarations did not change in this slice;
  the install/remove evidence below is specifically from 0.2.0.

These single-run timings establish capture, recovery, and presentation behavior.
They do not establish real-media accuracy, translation latency, native GNOME
compositor behavior, or independent physical-device clock acceptance. The final
word of the public fixture was still misrecognized. Windows Chinese/Spanish
visual media, physical 4K/mixed-DPI positioning, OBS/DXGI comparison, and owner
lifecycle soak remain separate acceptance work.

## Package validation

Inspect the built package before installation:

```bash
dpkg-deb --info target/release/bundle/deb/Prollyglot_0.3.0_amd64.deb
dpkg-deb --contents target/release/bundle/deb/Prollyglot_0.3.0_amd64.deb
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

For 0.2.0 on the development Ubuntu/WSL system, `apt-get install` installed the new
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
