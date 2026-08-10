# Prollyglot build plan

This is the executable delivery plan for Prollyglot. `Prollyglot.md` remains the product specification; this document defines the order in which the product is built and the evidence required to call each large milestone complete.

## Execution contract

- Build in substantial, integrated milestones. Internal experiments and checks are part of a milestone, not reasons to pause for repeated approval.
- Continue autonomously while the spec, this plan, and existing architecture provide a safe direction.
- Make reversible implementation choices without asking first, document them, and change them when evidence demands it.
- Commit at meaningful integration points inside a large milestone and push each such commit to `origin/main`. Do not publish placeholder-only or broken checkpoints.
- Keep routine validation local. GitHub Actions minutes are limited, so do not use per-push builds or broad hosted matrices as the development loop; reserve a consolidated, manually dispatched Windows workflow for milestone packaging or evidence that cannot be produced locally.
- Stop and request direction only when progress requires an irreversible product decision, unavailable credentials or hardware, an unclear dependency or model license, contradictory requirements, or external runtime evidence that cannot be obtained locally.
- A milestone is complete only when its end-to-end acceptance criteria pass. Compiling one module or drawing one screen is not a completed milestone.

## Initial technical direction

These choices are concrete enough to begin work but remain replaceable behind narrow interfaces:

- **Core:** stable Rust workspace with separate crates for shared audio types, Windows capture, the audio pipeline, ASR, transcripts, configuration, and application orchestration.
- **Windows capture:** the Microsoft `windows` crate over WASAPI and process-loopback APIs. C++ is an escape hatch only if a required API cannot be used reliably from Rust.
- **Desktop shell:** Tauri 2 using the operating system WebView, not Electron.
- **Interface:** vanilla TypeScript and CSS unless application complexity later proves a small UI framework worthwhile. The main control window and subtitle overlay are separate windows backed by the same Rust state.
- **Overlay behavior:** use Tauri window controls where reliable and native Windows window flags where always-on-top, click-through, focus, or fullscreen behavior requires them.
- **Audio pipeline:** bounded buffers, normalized mono floating-point PCM at the active model's sample rate, explicit resampling, and no raw-audio persistence by default.
- **First ASR integration:** `sherpa-onnx` behind Prollyglot's `SpeechEngine` contract, starting with a genuinely streaming English model whose redistribution license is recorded. Benchmark at least a small and standard model internally before selecting the default.
- **Fallback ASR candidate:** `whisper.cpp` remains a quality and multilingual comparison backend; it is not allowed to shape the shared transcript contract around its implementation details.
- **State and privacy:** local configuration and model storage, no account, no telemetry requirement, and no network activity except explicit model or application update actions.

Why this direction:

- Tauri uses the system WebView and Rust backend while supporting multiple controllable desktop windows: <https://v2.tauri.app/concept/architecture/>
- Windows exposes selected-endpoint WASAPI loopback and process-tree loopback through documented APIs: <https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording> and <https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/>
- `sherpa-onnx` provides local streaming ASR, Windows/Linux support, and a maintained Rust API: <https://github.com/k2-fsa/sherpa-onnx>

## Milestone map

| Milestone | Integrated outcome | Status |
| --- | --- | --- |
| 1. Windows capture foundation | A real Windows desktop shell can enumerate and capture either a selected output device or selected application | Selected-device Windows smoke passed; application and lifecycle validation remain |
| 2. Live English captions | Captured audio becomes stable partial and final English captions locally | Device-to-caption smoke passed; UI fixes await re-smoke and representative model evidence remains |
| 3. Minimal customizable Windows app | The complete daily-use interface, overlay customization, transcript view, and controls work together | Pending |
| 4. Windows MVP release | A reliable installable Windows build is ready for outside testing | Pending |
| 5. Ubuntu port | The Windows-proven core runs on one supported Ubuntu LTS release through PipeWire | Pending |
| 6. Multilingual captions and translation | Downloadable language support, local translation, and dual captions are production-ready | Pending |

### Windows smoke checkpoint — 2026-08-10

The repository owner's first native-Windows run confirmed that **Everything I hear** receives audible audio from the selected playback device and produces local captions. It also exposed an unsolicited startup preview, unstable painting while partial captions changed, and Settings actions with no visible result. Those UI paths were corrected in the next integration point and require a focused re-smoke.

Routine development now uses [`docs/testing/WINDOWS_SMOKE_TEST.md`](docs/testing/WINDOWS_SMOKE_TEST.md). Interrupted-download recovery, formal latency measurement, screenshots, OBS parity, and sustained-resource evidence are intentionally deferred to milestone hardening or release boundaries rather than imposed on every pre-release build.

## Milestone 1 — Windows capture foundation

Build the application foundation and retire the two largest Windows risks: native audio capture and reliable overlay window behavior.

### Included outcome

- A Rust workspace and Tauri desktop application with clean boundaries between UI, application orchestration, capture, and shared audio types.
- A minimal control window that shows available playback devices and active audio-producing applications.
- “Everything I hear” capture from the current default or a pinned playback device through WASAPI loopback.
- “Only this application” capture for a selected process tree through Windows process loopback.
- A bounded capture pipeline that converts native audio formats into normalized frames and exposes levels, timestamps, discontinuities, and lifecycle events.
- Start, stop, source switching, selected-device disappearance, selected-process exit, and repeated restart behavior.
- Follow-system-default capture that moves to the new default endpoint, plus safe retry when a selected endpoint is temporarily invalidated.
- Compatibility behavior aligned with current OBS use of documented endpoint and process-loopback APIs, without source blacklists or protected-media refusal logic.
- A separate transparent subtitle window that can show test text, remain above ordinary windows, move between monitors, and toggle click-through without stealing focus.
- Local diagnostic logging that contains technical errors but never captured audio.
- Local host-independent tests and Windows cross-compilation where practical, with a manually dispatched Windows verification workflow reserved for substantial milestone integration rather than every push.

### Acceptance boundary

Run the manual acceptance procedure in [`docs/testing/WINDOWS_MILESTONE_1.md`](docs/testing/WINDOWS_MILESTONE_1.md) on Windows 11. This is one substantial milestone gate, not a per-commit hosted workflow.

- On Windows 11, the application launches without administrator access or a virtual audio device.
- The displayed device list agrees with Windows playback devices and identifies the default device.
- Capturing a chosen device produces audio frames only while audio is rendered through that device.
- Capturing a chosen application produces its process-tree audio while unrelated application audio is absent.
- Capture can start and stop repeatedly, survive ordinary device/application lifecycle changes, and run for 30 minutes without a crash or unbounded memory growth.
- When equivalent current OBS device or application capture receives the same routed audio, Prollyglot also receives it; an OBS-only success is recorded as a Prollyglot capture defect rather than accepted as a protected-content limitation.
- The overlay proof works on a normal desktop and across two monitors; any exclusive-fullscreen limitation is recorded rather than hidden.
- Local checks pass and the manually invoked Windows build succeeds when milestone packaging evidence is needed. Final milestone acceptance still requires a real Windows 11 run because WSL and hosted automation cannot validate physical audio routing or desktop layering.

## Milestone 2 — Live English captions

Turn the capture foundation into the first complete product vertical slice: selected source to useful local English subtitles.

### Included outcome

- Resampling, channel conversion, a bounded ring buffer, voice activity detection, phrase segmentation, and backpressure behavior.
- A modular `SpeechEngine` contract with lifecycle, engine metadata, partial results, committed results, and structured failures.
- A model manifest and manager that downloads, verifies, loads, unloads, and removes the initial English streaming model.
- License and provenance records for the runtime and model weights before either is distributed.
- Stable provisional versus committed transcript state with timestamps.
- End-to-end captions from both Windows capture modes to the overlay and transcript store.
- Internal benchmarks comparing candidate small and standard English models on conversational, media, and noisy game/call samples.
- Useful errors for silence, unsupported capture, missing models, corrupt downloads, and insufficient memory.

### Acceptance boundary

Run the end-to-end procedure in [`docs/testing/WINDOWS_MILESTONE_2.md`](docs/testing/WINDOWS_MILESTONE_2.md) on the reference Windows 11 machine and retain the benchmark results.

- A new user can install or download the English model from inside the app and caption real Windows system or application audio without a cloud service.
- Captions update incrementally, finalized text does not churn, and silence does not trigger continuous inference.
- On the reference Windows test machine, lightweight mode is at least real-time and reaches a measured median partial-caption latency below two seconds on the benchmark set.
- Model downloads are integrity-checked and interrupted downloads recover safely.
- No raw audio is retained after its bounded inference buffers expire.
- Automated transcript-state, buffer, resampling, model-manifest, and failure-path tests pass.

## Milestone 3 — Minimal customizable Windows app

Build the complete daily-use experience around the working caption pipeline.

### UI rule

The application is minimal by default and customizable by choice. A user should see source, spoken language, caption output, and one primary Start/Stop action without navigating a setup maze. Customization lives in a clear secondary surface and updates a live preview immediately.

### Included outcome

- A compact main window for source selection, language, live state, Start/Stop, transcript access, and settings.
- A restrained visual system with strong typography, generous spacing, clear hierarchy, light and dark modes, and no decorative clutter.
- Overlay controls for font family, font size, weight, line height, text color, outline or shadow, background color and opacity, width, maximum lines, alignment, screen position, monitor, and click-through.
- Readable built-in presets, a reset-to-default action, and persistent local settings.
- Drag-to-position with a lock mode, plus keyboard-accessible positioning where direct dragging is unsuitable.
- A transcript view with copy, clear, search, and `.txt`, `.srt`, and `.vtt` export.
- System tray operation and configurable shortcuts for Start/Stop, pause, and overlay visibility.
- Keyboard navigation, focus visibility, screen-reader labels, high contrast, and large-caption testing.
- Recovery UI for source disappearance, device changes, model failures, and sustained unavailable audio.

### Acceptance boundary

- The default path from launch to captions requires only choosing a source and pressing Start.
- Every overlay appearance control updates a live preview and survives restart.
- The control window remains uncluttered at default settings and advanced engine controls remain out of the primary path.
- The overlay remains readable across common DPI scales and multi-monitor layouts and does not take keyboard focus while click-through is enabled.
- Core workflows are keyboard-operable and pass an automated accessibility scan where tooling supports it, followed by manual keyboard and high-contrast validation.
- Transcript exports preserve committed timestamps and do not include provisional duplicates.

## Milestone 4 — Windows MVP release

Harden the complete Windows application into an installable public beta.

### Included outcome

- A repeatable Windows release build and installer using Tauri's supported Windows bundling path.
- First-run model setup, storage management, uninstall behavior, version information, licenses, and privacy documentation.
- Automated unit and integration tests for shared logic and Windows lifecycle behavior, plus a repeatable manual compatibility script.
- Soak testing, failure injection, sleep/resume, default-device switching, Bluetooth/headphone changes, application restarts, display changes, and offline startup.
- Performance profiles for representative low-, middle-, and high-capability Windows hardware.
- Local diagnostic export suitable for bug reports without audio or transcript content unless the user explicitly includes transcript text.
- User documentation for installation, source modes, customization, known capture-compatibility limits, troubleshooting, and model storage.
- Reproducible versioned release artifacts with checksums, produced locally on Windows or by one manually dispatched release workflow.

### Acceptance boundary

- A clean Windows 11 machine can install, caption system output and one application, customize the overlay, export a transcript, restart, and uninstall cleanly.
- The release passes the compatibility script on at least two materially different Windows machines.
- No known crash, data-loss bug, or unbounded resource issue remains in the core caption path.
- Offline use works after models are installed.
- Signing or store publication may require owner-provided credentials; unsigned test artifacts must still be reproducible before that external gate.

## Milestone 5 — Ubuntu port

Port the proven product rather than designing Windows and Linux simultaneously.

### Included outcome

- One explicitly selected Ubuntu LTS version and a documented support matrix.
- PipeWire enumeration and capture for a selected output monitor and selected application stream/group.
- Reuse of the same normalized audio, ASR, transcript, configuration, and model-management core.
- Ubuntu-specific application grouping and stream-recreation recovery.
- Overlay behavior validated separately on the supported Ubuntu Wayland session and X11 where practical.
- A native `.deb`, dependency documentation, and local validation on the supported Ubuntu release; hosted release verification remains optional and manual.

### Acceptance boundary

- The Windows MVP's two source modes and daily-use workflow pass on the supported Ubuntu LTS release.
- PipeWire stream recreation and device hot-plugging recover without restarting Prollyglot in ordinary cases.
- Wayland limitations are explicit in the UI and documentation instead of being represented as cross-desktop guarantees.
- The `.deb` installs and removes cleanly on a fresh supported Ubuntu system; other distributions remain best-effort community territory.

## Milestone 6 — Multilingual captions and translation

Expand language capability only after the base application is dependable on its supported platforms.

### Included outcome

- Downloadable model manifests for selected additional languages, beginning with Spanish and Japanese when licensing and quality are acceptable.
- Automatic language detection with an allowed-language constraint.
- A separate local translation-engine contract and model manager.
- Original, translated, and dual-language overlay modes with independent update timing.
- Language profiles and resource-aware automatic engine/model selection.
- Benchmarks covering transcription accuracy, translation usefulness, latency, RAM, VRAM, and package/model sizes.

### Acceptance boundary

- Each advertised language has a documented model license, supported hardware profile, benchmark result, and tested download lifecycle.
- Original captions render without waiting for translation, and translated lines update without destabilizing committed source text.
- English-only users do not download or load multilingual or translation models.
- Failure of translation leaves original-language transcription fully usable.

## Cross-milestone quality rules

- Do not place operating-system objects, ASR-engine types, or UI payload shapes directly into shared domain contracts.
- Do not perform blocking model inference or filesystem/network work on audio callback threads.
- Use bounded queues and make overflow behavior visible in diagnostics.
- Keep provisional and committed transcript data distinct from the first ASR integration onward.
- Record dependency and model licenses when they enter the repository, not at release time.
- Keep fixtures redistributable and free of private conversation audio.
- Treat Windows runtime validation as required evidence; a cross-compile or hosted CI build alone cannot prove audio routing or overlay behavior.
- Prefer one local command that reproduces milestone checks over duplicating that work across GitHub-hosted jobs.

## Known external gates

Work should continue until one of these gates is actually reached:

- A real Windows 11 machine is required to accept Milestone 1 and later Windows milestones. The current development environment is WSL2, so it can build and test shared code but cannot validate WASAPI loopback or native overlay stacking.
- GitHub-hosted runner minutes are intentionally conserved. A lack of continuous hosted validation is not a blocker when equivalent local checks pass; manually dispatched jobs are used only where their environment or artifact is materially useful.
- Windows signing and store publication require owner-controlled identity and credentials.
- Model distribution stops if commercial use, redistribution, or derivative rights are unclear.
- Exact Ubuntu LTS selection is deferred until Milestone 5 so the support window is current when porting begins.
