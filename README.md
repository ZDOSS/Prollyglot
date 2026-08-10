<p align="center">
  <img src="assets/branding/prollyglot-logo.png" width="420" alt="Prollyglot logo" />
</p>

# Prollyglot

**Local, customizable subtitles for anything making sound on your computer.**

Prollyglot is a free and open-source desktop utility that captures audio from a selected playback device or application and turns it into live subtitles locally. It is designed for games, calls, browsers, media players, and other software that has missing, limited, or inaccessible captions.

> [!IMPORTANT]
> Prollyglot is in active pre-release development. There is not yet a supported binary release. Windows 11 is the primary target; the first Windows owner smoke confirmed that **Everything I hear** captions audible output from the selected playback device, while broader application, lifecycle, and release validation remains in progress.

## What it is building toward

- **Everything I hear:** caption the mixed audio rendered through one selected playback device, with an option to follow the Windows system default.
- **Only this application:** caption the selected Windows process and its process tree without including unrelated application audio.
- **Local by default:** no account, telemetry requirement, cloud transcription, audio upload, or transcript upload.
- **Minimal and customizable:** one focused Start/Stop path plus an independent always-on-top overlay with readable appearance controls.
- **Modular speech models:** separately downloaded and integrity-checked language models rather than one enormous application package.
- **Ubuntu after Windows:** one Ubuntu LTS release using PipeWire and a native `.deb`, once the Windows MVP is reliable.

## Current status

The repository currently contains:

- a Tauri 2 desktop shell and customizable caption-overlay proof;
- Windows playback-device capture through WASAPI loopback;
- Windows application/process-tree capture through the documented process-loopback API;
- follow-default-device behavior, endpoint reconnection, bounded capture queues, and local diagnostic logging;
- mono PCM normalization, streaming resampling to model rate, bounded low-latency buffering, energy VAD, and phrase boundaries;
- short-utterance-friendly speech gating with quiet-speech recall, pre-roll, and trailing decoder context;
- backend-neutral streaming ASR and stable provisional/final transcript contracts;
- an explicit first-run model flow with atomic downloads, safe path validation, size/SHA-256 verification, removal, and a pinned Apache-2.0 English streaming-model candidate;
- a sherpa-onnx adapter that loads the pinned model, preserves phrase openings with internal stream pre-roll, and exposes incremental and finalized English hypotheses;
- a reproducible small-versus-standard model comparison harness with a separately pinned benchmark candidate;
- a bounded capture-to-inference bridge with visible backpressure and recovery behavior; and
- live provisional/final transcript updates wired to the transcript view and a customizable always-on-top overlay that retains bounded, line-separated conversational context.

The first real-Windows smokes confirmed selected-device capture and exposed startup-preview, overlay painting, Appearance dismissal, short-utterance, context-retention, and Settings-feedback defects. Those paths have been corrected and are ready for a focused re-smoke. The remaining Milestone 2 gates include application capture, lifecycle and overlay validation, partial-caption latency measurement, and representative conversational model evidence. See [BUILD_PLAN.md](BUILD_PLAN.md) for milestone status and use the [Windows development smoke test](docs/testing/WINDOWS_SMOKE_TEST.md) for ordinary pre-release checks.

The benchmark tooling and initial clean-reference result are documented in [docs/benchmarks/ENGLISH_MODELS.md](docs/benchmarks/ENGLISH_MODELS.md). That smoke result validates both runtimes but intentionally does not choose a default without conversational and noisy Windows evidence.

## Capture compatibility and protected media

Prollyglot uses documented operating-system capture paths. It does not classify applications by DRM status, maintain a protected-source blacklist, or refuse a source because of what it may be playing. If Windows exposes decoded PCM through its selected-device or process-loopback API, Prollyglot treats it like any other audio and attempts to caption it.

The project does not strip DRM, weaken protected-media controls, or promise that Windows will expose audio from every source. Current OBS device/application capture is the practical compatibility baseline: if OBS receives meaningful audio through an equivalent documented path and Prollyglot does not, that is a Prollyglot defect to investigate. A virtual audio device is not required for normal operation, though an already-installed virtual endpoint can be selected like any other playback device.

## Development

### Prerequisites

For the primary Windows target:

- Windows 11;
- Rust 1.88 or newer with the MSVC toolchain;
- Microsoft C++ Build Tools;
- Node.js and `pnpm`; and
- the Tauri 2 Windows prerequisites, including WebView2.

Clone the repository, then install the UI dependencies and launch the desktop app:

```powershell
pnpm --dir apps/desktop install
pnpm --dir apps/desktop tauri dev
```

Run the local Windows code checks from PowerShell when validating a change:

```powershell
./scripts/check-windows.ps1
```

For an ordinary native-Windows run, follow the five-minute [Windows development smoke test](docs/testing/WINDOWS_SMOKE_TEST.md). It requires no screenshots, recordings, generated fixtures, or evidence bundle for passing behavior. The exhaustive [Windows release and hardening plan](docs/testing/WINDOWS_TEST_PLAN.md) is reserved for formal milestone and release-candidate validation.

On a non-Windows development host, the shared core, frontend, and Windows cross-checks used by the project can be run with:

```bash
./scripts/check-local.sh
```

Physical WASAPI routing, process isolation, device switching, overlay layering, and end-to-end caption latency still require a real Windows machine. The [Milestone 1](docs/testing/WINDOWS_MILESTONE_1.md) and [Milestone 2](docs/testing/WINDOWS_MILESTONE_2.md) checklists summarize formal acceptance boundaries; they are not the routine tester loop.

## Repository map

```text
apps/desktop/          Tauri desktop shell, control window, and overlay UI
crates/audio-windows/  Windows endpoint and process-loopback capture
crates/audio-pipeline/ PCM normalization, resampling, buffering, and VAD
crates/asr/            Backend-neutral streaming speech contracts
crates/asr-sherpa/     sherpa-onnx streaming runtime adapter
crates/model-manager/  Explicit model installation and integrity checks
crates/transcript/     Provisional and committed transcript state
assets/                Branding and pinned model manifests
docs/                  Design, licenses/provenance, and manual test procedures
```

The full product definition is in [Prollyglot.md](Prollyglot.md). Product decisions discovered during implementation are kept there, while [BUILD_PLAN.md](BUILD_PLAN.md) defines delivery order and evidence required to complete each milestone.

## Privacy

Captured audio remains in bounded memory long enough to process it and is not recorded by default. Transcripts are not automatically persisted or uploaded. Network access is reserved for explicit actions such as downloading a selected model or, later, checking for application updates.

## License

Prollyglot source code is available under the [MIT License](LICENSE). Speech runtimes and model weights retain their own licenses; pinned provenance and redistribution notes are recorded in [docs/licenses/ASR_MODELS.md](docs/licenses/ASR_MODELS.md).
