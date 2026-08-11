<p align="center">
  <img src="assets/branding/prollyglot-logo.png" width="420" alt="Prollyglot logo" />
</p>

# Prollyglot

**Local, customizable subtitles for anything making sound on your computer.**

Prollyglot is a free and open-source desktop utility that captures audio from a selected playback device or application and turns it into live subtitles locally. It is designed for games, calls, browsers, media players, and other software that has missing, limited, or inaccessible captions.

> [!IMPORTANT]
> Prollyglot is in active pre-release development. There is not yet a supported binary release. Windows 11 is the primary target; the first Windows owner smoke confirmed that **Everything I hear** captions audible output from the selected playback device, while broader application, lifecycle, and release validation remains in progress.

## Why Prollyglot exists

Prollyglot exists to give people more control over how they understand media
across languages. Someone watching international news, a documentary, a
livestream, or a game should not have to place blind trust in one opaque
subtitle track. Keeping the original transcript visible beside its translation
helps people compare the two, notice possible errors, and judge whether a
translation is accurate and faithful. No model can guarantee perfection, so
Prollyglot should preserve the source and expose uncertainty instead of
presenting a machine's best guess as unquestionable truth.

It should also make language immersion available without a subscription or
metered service. Listening to speech while reading the original language and a
translation can help someone learn through the media they already enjoy.

The larger hope is that Prollyglot helps people understand and communicate with
others they otherwise could not. Languages differ, but the people speaking them
are still people; this project should help erase some of the lines language can
draw between us.

## What it is building toward

- **Everything I hear:** caption the mixed audio rendered through one selected playback device, with an option to follow the Windows system default.
- **Only this application:** caption the selected Windows process and its process tree without including unrelated application audio.
- **Local by default:** no account, telemetry requirement, cloud transcription, audio upload, or transcript upload.
- **Minimal and customizable:** one focused Start/Stop path plus an independent always-on-top overlay with readable appearance controls.
- **Selectable local speech models:** three English choices; smaller dedicated streaming models for Chinese, French, Korean, and Bengali; and an optional higher-resource Nemotron model covering 28 languages plus automatic detection.
- **Experimental visual text translation:** a separate Windows mode translates text already visible in a selected region, application window, or display—such as video subtitles, signs, menus, or Japanese text in a game HUD—through documented screen capture and local OCR.
- **Ubuntu after Windows:** one Ubuntu LTS release using PipeWire and a native `.deb`, once the Windows MVP is reliable.

## Current status

The repository currently contains:

- a Tauri 2 desktop shell and customizable caption-overlay proof;
- Windows playback-device capture through WASAPI loopback;
- Windows application/process-tree capture through the documented process-loopback API;
- follow-default-device behavior, endpoint reconnection, bounded capture queues, and local diagnostic logging;
- mono PCM normalization, band-limited streaming resampling to model rate, bounded low-latency buffering, energy VAD, and phrase boundaries;
- short-utterance-friendly speech gating with quiet-speech recall, pre-roll, and trailing decoder context;
- backend-neutral streaming ASR and stable provisional/final transcript contracts;
- an explicit first-run model flow with atomic downloads, safe path validation, size/SHA-256 verification, background startup inspection, working removal, and persistent selection among eight pinned local streaming models;
- a sherpa-onnx adapter that loads Zipformer or Nemotron streaming models, preserves phrase openings and decoder context, and exposes incremental and finalized hypotheses;
- original-language controls for 29 selectable spoken languages plus mixed-language automatic detection, with clear forced-language guidance and unsupported model/language combinations prevented before capture starts;
- optional pinned local translators for compact Japanese/Spanish-to-English, compact multilingual-to-English, and direct translation among the 29 selectable languages;
- a dedicated desktop model manager for speech, visual OCR, and translation packs, with one collapsible installed-model inventory and purpose/language-filtered choices for adding exactly one compatible model at a time;
- an experimental Windows visual-translation slice with explicit window, display, and drawn-region selection; change-gated local OCR; bounded latest-frame processing; and translated labels positioned near their recognized source text;
- Original, Translation, and Original + Translation output modes, with stable stacked or side-by-side pairs, independent colors, wrapped text, and zero to three fading prior caption rows;
- reproducible English and multilingual comparison tooling covering the same model choices exposed by the app;
- a bounded capture-to-inference bridge with visible backpressure and recovery behavior; and
- live provisional/final transcript updates wired to a latest-following, scrollback-safe transcript view and a customizable always-on-top overlay that retains bounded, line-separated conversational context.

The first real-Windows smokes confirmed selected-device capture and exposed startup-preview, overlay painting, Appearance dismissal, short-utterance, context-retention, and Settings-feedback defects. The latest owner re-smoke found better results and confirmed that Appearance, Transcript, and Settings now open and close correctly. Recognition of some short speech and accented dialogue remains inconsistent. The Models workspace therefore offers Fast (43.1 MiB), Balanced (70.0 MiB), and Enhanced (181.4 MiB), plus dedicated streaming Chinese (29.5 MiB), French (123.0 MiB), Korean (134.4 MiB), and Bengali (89.8 MiB) options. An opt-in Nemotron 3.5 Streaming 0.6B model (650.6 MiB) covers 28 languages and automatic detection. Nemotron is a CPU path, is not the default, and does not yet have representative evidence for production-quality accuracy across that catalog. Its language selection guides recognition, so a known language should be selected when possible and Automatic is intended for mixed-language media.

Translation has its own **Translate to** control. Japanese-to-English (109.4 MiB) and Spanish-to-English (113.8 MiB) keep their compact direct models; a compact multilingual-to-English model (112.9 MiB) covers the other selectable source languages; and an optional universal model (610.3 MiB) translates directly among all 29 selectable languages. No translator is bundled or downloaded automatically, and the worker loads at most one at a time. Real-model development-WebView checks now cover all four route classes, including compact French-to-English from provisional text and universal Japanese-to-Spanish; representative native-Windows quality, cold-load, memory, and sustained-latency evaluation remain pending. Automatic mixed-language recognition remains original-only until ASR reports a dependable detected language for each segment. See [BUILD_PLAN.md](BUILD_PLAN.md) for milestone status and use the [Windows development smoke test](docs/testing/WINDOWS_SMOKE_TEST.md) for ordinary pre-release checks.

Translated output is enabled explicitly under **Caption output**; downloading a translator only makes a route available offline. Translation begins from a coalesced live partial after about 420 ms and is throttled to at most one new request every 900 ms, so changing words update the pending text without postponing translation until silence. Each finalized caption is translated again for stability, and Prollyglot now independently enforces a boundary after four seconds of continuous pause-light Nemotron speech instead of relying only on the model runtime's endpoint. The bounded final queue prioritizes the newest caption and skips stale backlog. Side-by-side source/translation columns wrap instead of ellipsizing, and **Appearance → Caption history** can retain zero to three complete, smaller, fading prior caption pairs without clipping either language independently. Appearance also controls how long a final caption remains after speech and how gently it fades; a late translation receives a fresh reading interval. Privacy-safe diagnostics include slow live translations without recording caption text.

Visual text translation now exists as an experimental, separately enabled Windows slice. **Screen translation** continuously watches one explicitly selected top-level window, display, or drawn live display region through `Windows.Graphics.Capture`; feeds transient frames through a capacity-one latest-frame queue, change gate, and PP-OCRv6 Small; and places a local translation near the original text already visible on screen. **Prominent text** is the default media mode: it accepts the first high-confidence pass, joins nearby same-line and stacked OCR fragments into one phrase, ranks and caps the six most useful regions, and drops pending translations from older frames. OCR input is bounded to 1280 pixels on its longest side, upright desktop text skips the direction-classifier pass, and the local translator begins loading alongside OCR startup. **All detected text** retains the more conservative stabilizer when small interface text matters. A small scanning indicator appears before the first result; a disappeared label remains readable for up to eight seconds, except text that was continuously visible for at least twelve seconds is removed immediately. The optional OCR pack is a 30.4 MiB explicit download, and translation reuses the selected local language pack. Nothing is downloaded automatically. Audio and visual sessions are mutually exclusive for now. Application, display, and region selections share a camel-case-tested native contract, and the region selector is translucent. The control and overlay surfaces remain visible to ordinary screenshots; the capture loop instead filters translations Prollyglot currently draws, avoiding a feedback loop without making the app disappear from full-screen captures. Screen translation drains to the newest frame before each OCR pass and suppresses a result that is over 1.5 seconds behind after its source changes, rather than showing text from a departed scene; static text can still receive a slow first result. The CPU-heavy OCR path is optimized in `tauri dev`, and one Stop click now preserves its button, cancels active ONNX inference, hides the overlay, and finishes cleanup in the background. An owner run found the earlier full-frame path too slow and too fragmented for moving Spanish media; this bounded live-media correction passes local pipeline and rendered checks, but native Windows speed, OCR quality, DPI/multi-monitor positioning, and representative media usefulness still require re-testing.

The control app now opens as a full desktop workspace with persistent navigation for Captions, Screen translation, Transcript, Models, Appearance, and Settings. Captions and screen translation use desktop-width grouped panels instead of one long mobile-style form, and Appearance is an in-place full-workspace page with a live preview rather than another modal window. A title-bar control switches to a focused compact utility that retains the current Start/Stop path and bottom navigation; compact Appearance remains a separate focused utility window. The app remembers the chosen layout and restores an appropriate window size for each mode.

Installed recognition models are no longer all hashed before the app window appears. Model inspection runs in the background and records a small verification marker after a successful full SHA-256 pass; later launches use file size, modification metadata, and the pinned manifest to avoid re-hashing unchanged model files. Existing installations perform one background full check after this update. Only the selected recognition model is loaded when captions start, so the larger Nemotron choice can still take noticeably longer than an English model at that point; only the translator requested by the current source/output choice is loaded. Startup and translation timing are written to the privacy-safe diagnostic log without caption text. The custom Windows title bar also has the explicit Tauri permissions required for dragging and its minimize, maximize, close, and full/compact sizing controls; native Windows remains the final check for those operating-system interactions.

The English benchmark tooling and initial clean-reference results are documented in [docs/benchmarks/ENGLISH_MODELS.md](docs/benchmarks/ENGLISH_MODELS.md). All three English choices stream comfortably faster than real time on the development host, but the clean fixture does not establish an accuracy winner. The separate [Nemotron multilingual trial](docs/benchmarks/MULTILINGUAL_NEMOTRON.md) records resource cost and provisional English, Spanish, Japanese, and automatic-detection results. The [translation model record](docs/benchmarks/TRANSLATION_MODELS.md) distinguishes integration checks from the representative quality and latency evidence still needed. Representative Windows listening still decides what is genuinely useful.

## Capture compatibility and protected media

Prollyglot uses documented operating-system capture paths. It does not classify applications by DRM status, maintain a protected-source blacklist, or refuse a source because of what it may be playing. If Windows exposes decoded PCM through its selected-device or process-loopback API, Prollyglot treats it like any other audio and attempts to caption it.

The project does not strip DRM, weaken protected-media controls, or promise that Windows will expose audio from every source. Current OBS device/application capture is the practical compatibility baseline: if OBS receives meaningful audio through an equivalent documented path and Prollyglot does not, that is a Prollyglot defect to investigate. A virtual audio device is not required for normal operation, though an already-installed virtual endpoint can be selected like any other playback device.

For visual text translation, **Whole display** is a first-class source rather
than a last-minute workaround. The current experimental slice captures selected
application windows and displays through `Windows.Graphics.Capture`, then crops
a selected region from the display frame. A documented DXGI Desktop Duplication
backend remains a planned comparison and fallback. Equivalent OBS **Display Capture**
is the compatibility baseline (see the
[OBS source documentation](https://obsproject.com/kb/display-capture-sources)).
If OBS can see useful pixels from the same display while Prollyglot cannot, that
is a compatibility defect to investigate; the planned second display path and
privacy-safe frame diagnostics must distinguish an app defect from pixels the
operating system does not expose.

Monitor capture is not guaranteed to expose every protected surface. Windows
[documents protected-video handling in Desktop Duplication](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/desktop-duplication-api)
and allows protected swap chains or windows to be excluded from public capture
APIs. Prollyglot will process whatever pixels Windows supplies, clearly report
a blank or excluded region, and will not add process injection, capture hooks,
or a protection bypass.

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

For an ordinary native-Windows run, follow the five-minute [Windows development smoke test](docs/testing/WINDOWS_SMOKE_TEST.md). The separate [experimental visual-translation smoke](docs/testing/WINDOWS_VISUAL_SMOKE_TEST.md) is only needed when checking that feature. Neither requires screenshots, recordings, generated fixtures, or an evidence bundle for passing behavior. The exhaustive [Windows release and hardening plan](docs/testing/WINDOWS_TEST_PLAN.md) is reserved for formal milestone and release-candidate validation.

On a non-Windows development host, the shared core, frontend, and Windows cross-checks used by the project can be run with:

```bash
./scripts/check-local.sh
```

Physical WASAPI routing, process isolation, device switching, screen capture, DPI/multi-monitor mapping, overlay layering, and end-to-end latency still require a real Windows machine. The [Milestone 1](docs/testing/WINDOWS_MILESTONE_1.md) and [Milestone 2](docs/testing/WINDOWS_MILESTONE_2.md) checklists summarize formal acceptance boundaries; they are not the routine tester loop.

### Windows diagnostic log

Prollyglot writes a rolling local log containing lifecycle, capture, model, and backlog diagnostics. It does not include captured audio or transcript text. From any PowerShell directory, show the newest log with:

```powershell
$LogRoot = Join-Path $env:LOCALAPPDATA "com.prollyglot.desktop\logs"
$LatestLog = Get-ChildItem $LogRoot -Filter *.log |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1
Get-Content $LatestLog.FullName -Tail 200
```

Use this only when troubleshooting a failure. A normal smoke-test pass does not require saving or submitting logs.

## Repository map

```text
apps/desktop/          Tauri desktop shell, control window, and overlay UI
crates/audio-windows/  Windows endpoint and process-loopback capture
crates/audio-pipeline/ PCM normalization, resampling, buffering, and VAD
crates/asr/            Backend-neutral streaming speech contracts
crates/asr-sherpa/     sherpa-onnx streaming runtime adapter
crates/model-manager/  Explicit model installation and integrity checks
crates/transcript/     Provisional and committed transcript state
crates/visual-pipeline/ Frame gating, OCR contracts, and text stabilization
crates/visual-windows/ Windows window/display/region capture adapter
crates/visual-ocr-rapid/ Local PP-OCRv6 adapter
assets/                Branding and pinned model manifests
docs/                  Design, licenses/provenance, and manual test procedures
```

The full product definition is in [Prollyglot.md](Prollyglot.md). Product decisions discovered during implementation are kept there, while [BUILD_PLAN.md](BUILD_PLAN.md) defines delivery order and evidence required to complete each milestone.

## Privacy

Captured audio and screen pixels remain in bounded memory only long enough to process them and are not recorded by default. Transcripts and recognized visual text are not automatically persisted or uploaded. Network access is reserved for explicit actions such as downloading a selected model or, later, checking for application updates.

## License

Prollyglot source code is available under the [MIT License](LICENSE). Speech, OCR, and translation runtimes and model weights retain their own licenses; pinned provenance and redistribution notes are recorded in [docs/licenses/ASR_MODELS.md](docs/licenses/ASR_MODELS.md), [docs/licenses/VISUAL_OCR_MODELS.md](docs/licenses/VISUAL_OCR_MODELS.md), and [docs/licenses/TRANSLATION_MODELS.md](docs/licenses/TRANSLATION_MODELS.md).
