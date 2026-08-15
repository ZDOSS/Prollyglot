# Prollyglot

**Local, cross-platform subtitles for anything making sound on your computer.**

Prollyglot is a free and open-source desktop application for Windows and Linux that captures audio from the whole system or a selected application and produces live subtitles locally.

The user should be able to select something such as Discord, a browser, a game, VLC, Spotify, a news stream, or the entire system output and receive live transcription without the source application needing native caption support.

Prollyglot should also support optional translation, allowing foreign-language audio to be transcribed in its original language and displayed either as original-language subtitles, translated subtitles, or both.

The project should prioritize local processing, low resource consumption, modular speech models, privacy, and a small native desktop footprint.

---

# 1. Core concept

The basic interaction should be:

1. Open Prollyglot.
2. Select an audio source.
3. Select the spoken language or automatic language detection.
4. Select the desired caption language.
5. Start captions.
6. A lightweight subtitle overlay appears over other applications.

For example:

```text
Source
Discord

Spoken language
English

Display
English

[ Start Captions ]
```

Or:

```text
Source
Firefox

Spoken language
Japanese

Display
Japanese + English

[ Start Captions ]
```

Prollyglot should not require:

- a browser extension,
- application plugins,
- virtual audio cables,
- Discord bots,
- accounts,
- cloud processing,
- subscription services,
- Electron,
- recordings to be uploaded anywhere.

The central product promise is:

> Select anything making sound on your computer and subtitle it locally.

---

# 2. Project goals

Prollyglot should provide:

- live transcription of arbitrary desktop audio,
- whole-system audio capture,
- per-application audio capture where the operating system allows it,
- Windows support,
- Linux support,
- local inference,
- lightweight transcription options,
- modular ASR backends,
- downloadable language models,
- optional automatic language detection,
- optional translation,
- original-language subtitles,
- translated subtitles,
- dual-language subtitles,
- a configurable always-on-top subtitle overlay,
- optional transcript history,
- transcript export,
- simple installation,
- sensible defaults.

Prollyglot should remain useful on ordinary consumer hardware.

A dedicated high-end GPU should improve performance but should not be a fundamental requirement for basic English captioning.

---

# 3. Project philosophy

Prollyglot should remain a focused desktop utility.

The project exists to give people meaningful control over cross-language media.
Original speech or visible text should remain available beside translation so a
viewer can compare them and judge whether a result is accurate and faithful.
No model can guarantee a perfect translation; Prollyglot should expose the
source and uncertainty rather than presenting one opaque output as unquestionable.

Local, free-to-use original-plus-translation output should also support language
learning through immersion in news, films, streams, games, and other media. The
larger purpose is to help people understand and communicate with others they
otherwise might not, reducing the lines that language can draw between us.

It should not gradually become:

- an AI meeting assistant,
- a productivity suite,
- a note-taking platform,
- a cloud SaaS,
- a Discord client,
- an OBS replacement,
- an audio editor,
- a voice recorder,
- an LLM chat application.

Additional capabilities should be evaluated based on whether they improve the core job:

> Turning arbitrary computer audio into useful subtitles.

The project should prefer small independent components over a monolithic architecture.

---

# 4. Supported platforms

## Platform roadmap

### Windows

Primary development and first production target:

- Windows 11

Potential later support:

- sufficiently recent Windows 10 versions where required capture APIs are available.

Audio capture should use native Windows audio facilities rather than virtual devices.

The Windows version should reach MVP quality before release work shifts to Linux-specific capture, packaging, and overlay behavior.

### Linux

Secondary target after the Windows MVP is reliable:

- one current Ubuntu LTS release, selected when Linux work begins,
- PipeWire
- Wayland
- X11 where practical

The first official Linux package should be a native `.deb` for the supported Ubuntu release.

Debian and other Debian-based distributions may be compatible, but should initially be considered community-supported rather than guaranteed. Prollyglot should not promise support based only on a distribution being able to install a `.deb` file.

Prollyglot should not initially spend substantial engineering effort supporting:

- multiple Linux packaging formats,
- Flatpak, Snap, or distribution-specific repositories,
- obsolete PulseAudio-only systems,
- JACK-specific configurations,
- unusual custom desktop stacks,
- extremely old distributions.

Linux support should be real, but deliberately scoped.

---

# 5. Cross-platform architecture

The platform-specific portion of Prollyglot should be kept as small as possible.

The overall architecture should resemble:

```text
                     PROLLYGLOT

                ┌─────────────────┐
                │  Audio Capture  │
                └────────┬────────┘
                         │
            ┌────────────┴────────────┐
            │                         │
        Windows                    Linux
         WASAPI                  PipeWire
            │                         │
            └────────────┬────────────┘
                         │
                  Normalized PCM
                         │
                  Audio Pipeline
                         │
                  VAD / buffering
                         │
                    ASR Engine
                         │
                Transcript Manager
                         │
       ┌─────────────────┼───────────────────┐
       │                 │                   │
   Caption Overlay   Transcript View    Translation
       │                                     │
       └─────────────────┬───────────────────┘
                         │
                  Display / Export
```

Everything downstream from raw audio capture should ideally use shared code.

## 5.1 Runtime and application integrity

The application layer must have one explicit, testable account of what work is
starting, active, waiting, stopping, or failed. Tauri commands and WebView
controllers should adapt that state for the operating system and interface; they
should not each maintain competing session truth.

Runtime requirements:

- Audio captions and visual translation use supervised sessions with stable
  session identifiers, monotonic revisions, cancellable startup, bounded
  shutdown, and terminal worker completion reporting. They remain mutually
  exclusive until a later resource gate explicitly permits simultaneous use.
- Live work such as transcription, OCR, and translation has bounded queues,
  deadlines or cancellation, and an explicit stale-work policy. Stopping a
  session must invalidate every result that belongs to it.
- The control window, native runtime, and overlays exchange generated or
  schema-derived contracts. User-facing failures retain a stable error code,
  recoverability, and a suggested action instead of being flattened into
  unclassified strings.
- Each overlay has one versioned presentation stream. Original text may render
  before translation, but later translation is an update to the same identified
  caption or visual region rather than a second competing source of display
  truth.
- One native model store owns manifests, explicit installation, verification,
  removal, and disk inventory for speech, OCR, and translation artifacts.
  Inference runtimes load only the active verified model and are isolated from
  catalog and download work so a slow inference cannot block model management.
- Production settings use one locally stored, versioned schema with validation
  and migrations. Individual WebViews may cache transient view state, but must
  not become independent owners of durable application configuration.
- The full desktop interface uses persistent workspace pages backed by shared
  application state. Compact mode may use contained dialogs, but full mode must
  not simulate navigation by repeatedly rebuilding one modal surface.
- Session coordination, scheduling, revision handling, and configuration
  migration must be testable without native capture hardware or a running Tauri
  WebView. Manual Windows checks remain necessary for real WASAPI, capture,
  overlay, and hardware-inference behavior.

The current pre-release runtime implements this lifecycle boundary for both
audio captions and visual translation. Legacy audio/visual status events remain
temporary interface projections, while the versioned runtime bootstrap and
monotonic runtime event are authoritative. Session-facing source, status,
selection, visual-text, and error payloads are generated from Rust contracts;
the frontend rejects older runtime snapshots and visual results from a stopped,
waiting, or replaced session.

Caption and visual overlays now consume separate generated presentation-frame
contracts carrying session, runtime, and presentation revisions. Only the main
WebView may publish them. Native code validates the active session before
forwarding one authoritative event to each overlay, and each overlay independently
rejects duplicates and delayed frames. Transcript events no longer write raw
caption text directly to the overlay. Reading time and fade are derived from the
frame's newest-readable timestamp, so a late translation receives its own full
reading interval without competing with another clear timer.

The desktop frontend now consumes these contracts through one reducer-backed
application store. Runtime, source, model-inventory, transcript, translation,
visual, navigation, preference, and notice state no longer live as independent
module-level authorities in the entry point. Native Tauri access and the
browser development preview implement the same typed bridge; preview catalogs
are small fictional fixtures rather than a hand-copied production inventory.
Durable preferences now use one native schema-v1 configuration repository with
validated defaults, immutable atomic revision publication, a retained fallback,
corrupt-revision recovery, and generated TypeScript contracts. Existing WebView
values and selected-model files migrate only through verified write/readback;
accepted revisions are broadcast to every WebView, rapid changes coalesce, and
stale frontend writes rebase without erasing a newer native model selection.
The full workspace now uses persistent destination pages backed by focused
feature controllers, while compact mode retains contained utilities. The
stylesheet is separated into design-token, shell, feature, utility-window, and
overlay layers so those surfaces no longer depend on one monolithic rule file.

---

# 6. Audio capture

## 6.1 Whole-system capture

Prollyglot should support:

> Caption everything I hear.

This mode captures the mixed audio being sent to one user-selected playback device.

The default selection should follow the operating system's current default playback device. The user should also be able to pin capture to a specific device such as speakers, headphones, an HDMI display, or a USB audio interface.

“Everything I hear” does not mean combining every playback device simultaneously. It means everything being rendered through the selected device.

Example:

```text
Audio source
Everything I hear

Playback device
Follow system default — Speakers
```

Possible use cases:

- movies,
- browser videos,
- games,
- Discord calls,
- music,
- livestreams,
- news,
- accessibility.

---

# 6.2 Per-application capture

This should be one of Prollyglot's defining features.

The user should be able to select:

```text
Audio source

○ Everything I hear
○ Discord
○ Firefox
○ Chrome
○ VLC
○ Spotify
○ Game.exe
```

This allows someone to caption Discord while ignoring a game, or caption a game while ignoring Discord.

The abstraction exposed by the shared application should be something like:

```text
AudioSource
- SystemOutput(device)
- Application
- InputDevice
```

Platform-specific implementations should translate native audio objects into this shared representation.

---

# 6.3 Windows capture backend

Windows should use WASAPI.

The backend should support:

- ordinary output loopback for the selected playback device,
- per-process/application loopback where supported,
- input-device capture,
- device enumeration,
- default-device and explicit-device selection,
- application enumeration,
- stream lifecycle monitoring.

The rest of Prollyglot should not need to know that WASAPI exists.

---

# 6.4 Linux capture backend

Linux should use PipeWire.

The backend should support:

- output-monitor capture,
- application/stream selection,
- input devices,
- stream enumeration,
- changing application streams,
- device hot-plugging.

Linux applications may expose multiple PipeWire nodes.

Prollyglot should therefore group related streams into a user-facing application where possible rather than exposing raw PipeWire internals.

For example, instead of:

```text
Firefox AudioStream 372
Firefox AudioStream 391
Firefox AudioStream 405
```

the UI should attempt to present:

```text
Firefox
```

with an advanced stream selector available if needed.

---

# 6.5 Protected and unavailable audio

Prollyglot should use documented operating-system capture APIs and should not attempt to disable, strip, or bypass DRM or protected-media controls.

Prollyglot should not maintain a protected-source blacklist, inspect a source's DRM status, or refuse capture based on the application or media being played. If Windows exposes decoded PCM through the selected playback-device or process-loopback API, Prollyglot should capture and transcribe it like any other audio. This includes protected-media playback when Windows makes its rendered audio available through those documented paths.

Protected content, exclusive-mode output, unusual application routing, or another OS/driver restriction may still produce silence or an unavailable stream even though the user can hear playback. That is a compatibility condition to observe, not a reason to skip the capture attempt.

OBS is the practical Windows compatibility baseline for the two MVP capture modes. When current OBS device-output or application-audio capture receives meaningful audio from the same source and routing configuration but Prollyglot does not, treat the difference as a Prollyglot defect to investigate rather than assuming DRM made the audio unavailable.

When capture is unavailable, Prollyglot should:

- keep the rest of the application responsive,
- retry ordinary device invalidation and routing changes where safe,
- explain that the selected capture path is not currently receiving audio,
- avoid claiming that every protected source is supported,
- offer comparison diagnostics that make an OBS parity failure actionable,
- suggest source-provided captions when available,
- allow deliberate microphone capture as an accessibility fallback without enabling it automatically.

Installed virtual audio endpoints may be selected like any other playback device, but Prollyglot should not require or bundle a virtual driver for its normal path. A third-party virtual cable may later be documented as an advanced compatibility option for applications whose routing is incompatible with process capture; it is not a DRM-removal mechanism or a substitute for making native capture reliable.

The core product should attempt protected and unprotected sources uniformly without promising that Windows will expose every source on every system.

---

# 7. Microphone support

Microphone transcription may be supported, but should not distract from system audio.

Possible source choices:

```text
Audio

● Selected application
○ Entire system
○ Microphone
○ Application + microphone
```

Combined microphone/system transcription could eventually support conversations where the user wants both their own speech and remote speech captured.

This is not required for the first proof of concept.

---

# 8. Audio processing pipeline

Captured audio should be normalized into a common internal representation before reaching speech recognition.

Preferred internal format:

- mono,
- PCM,
- model-appropriate sample rate,
- fixed frame size internally where practical.

Pipeline:

```text
OS audio
   ↓
capture backend
   ↓
channel conversion
   ↓
resampling
   ↓
ring buffer
   ↓
VAD
   ↓
ASR
```

The audio processing path should avoid unnecessary copies.

---

# 9. Voice activity detection

Prollyglot should use VAD where beneficial.

VAD can:

- prevent inference during silence,
- lower CPU/GPU use,
- reduce unnecessary caption updates,
- provide natural phrase segmentation,
- improve transcript formatting.

VAD should be modular rather than tightly coupled to one ASR backend.

For desktop playback, the initial gate should favor speech recall over aggressive silence suppression. It should retain a short pre-roll so quiet phrase openings are not discarded, allow enough trailing context for the recognizer to finish short remarks, and avoid splitting on every tiny pause.

---

# 10. Speech recognition architecture

Speech recognition should be implemented behind a backend interface.

Conceptually:

```text
trait SpeechEngine {
    initialize()
    load_model()
    unload_model()
    start_stream()
    push_audio()
    get_partial()
    get_final()
    stop_stream()
}
```

Exact implementation language may vary, but the architectural contract should remain similar.

This allows Prollyglot to support multiple engines without rewriting the application.

---

# 11. ASR engine strategy

Prollyglot should not permanently tie itself to one speech-recognition model family.

The speech-model landscape changes quickly.

Instead, engines should be replaceable.

Initial candidates include:

- Moonshine,
- Nemotron / Parakeet-style streaming models,
- whisper.cpp,
- Voxtral Realtime,
- future compatible models.

The application should present simple quality choices while retaining advanced engine controls.

---

# 12. Initial model strategy

## Recommended POC

Start with `sherpa-onnx` behind Prollyglot's replaceable speech-engine contract and genuinely streaming English models. The Windows MVP exposes three pinned Apache-2.0 Zipformer choices in Settings:

- **Fast:** the 20M model and current first-run default, prioritizing the smallest download and lowest CPU cost;
- **Balanced:** a standard-size streaming model for users who can spend more resources; and
- **Enhanced:** the largest initial option, trained on both LibriSpeech and GigaSpeech to provide a broader recognition candidate for varied dialogue and accents.

These names describe product profiles and training/resource tradeoffs, not a guarantee that the larger model will transcribe every speaker more accurately. The user can install more than one model, explicitly select which one the next caption session loads, remove unused models, and keep that selection across restarts. Model changes are disabled during an active caption session.

Fast remains the initial default until representative Windows comparisons justify changing it. Comparisons should cover accented and unaccented conversation, media, and noisy game/call audio, and should include accuracy, partial-caption latency, real-time factor, memory use, and download size. Every model's exact upstream revision, runtime files, sizes, SHA-256 digests, and license must remain recorded in a versioned manifest.

The pre-release catalog also includes four smaller Apache-2.0 streaming choices for languages where compatible online transducers are available: **Chinese Small** (29.5 MiB), **French Compact** (123.0 MiB), **Korean Compact** (134.4 MiB), and **Bengali Compact** (89.8 MiB). These are independent optional downloads and use the same install/select/remove lifecycle as the English choices. They avoid requiring a 0.6B multilingual model for a user who needs only one of those languages, but their catalog presence does not claim better real-media accuracy until representative Windows comparisons exist.

The catalog also includes an explicit opt-in **Multilingual** trial: the INT8 560 ms conversion of NVIDIA Nemotron 3.5 ASR Streaming 0.6B. Here `0.6B` means approximately 600 million parameters, not a 600 MB file; its verified download is 650.6 MiB. The current integration runs locally on CPU and exposes NVIDIA's 15 transcription-ready languages plus 13 broad-coverage languages, for 28 unique forced-language choices, as well as automatic detection. Adaptation-ready languages that require fine-tuning are not exposed. Nemotron is not the default. Initial development-host evidence is promising for a Spanish publisher fixture but weaker for one English fixture and poor for the first Japanese and unconstrained automatic-detection fixtures, so the whole catalog remains pre-release and broad coverage plus automatic detection are explicitly less certain. The 560 ms checkpoint is intentionally preferred over the 1120 ms variant while caption delay is already a reported concern.

Initial goal:

```text
English audio
↓
local speech recognition
↓
live subtitles
```

The first successful milestone should not require multilingual support.

Models remain separate from the application binary. Downloads occur only after an explicit user action, incomplete downloads must never appear installed, and every required artifact must pass its manifest size and hash checks before an engine can load it.

---

# 13. Language packs

Prollyglot should support independently downloadable language packs whenever the selected model architecture permits it. A multilingual engine may instead cover several languages in one optional download; Nemotron follows that architecture and must never be downloaded merely because an English-only user installed the app. The current catalog combines both approaches: small dedicated Chinese, French, Korean, and Bengali models plus the wider Nemotron option.

Example:

```text
Languages

Installed

✓ English — Fast
✓ Japanese — Multilingual

Available

  Chinese — Small       Download
  French — Compact      Download
  Korean — Compact      Download
  Bengali — Compact     Download
  Multilingual          Download
```

A user should not have to install speech models for languages they will never use.

---

# 14. Language profiles

Users should be able to create convenient language sets.

Example:

```text
Language Profiles

Everyday
English

Mine
English
Spanish
Japanese

European News
English
Spanish
French
German
Italian

East Asia
Japanese
Mandarin
Korean
```

These profiles can control:

- available ASR models,
- automatic language-detection constraints,
- translation options,
- preferred output languages.

---

# 15. Model loading behavior

Installing a model and loading a model should be treated separately.

A user may have:

```text
English
Spanish
Japanese
```

installed on disk without all three consuming memory.

For monolingual backends:

```text
Japanese selected

Unload English
↓
Load Japanese
```

Only active models should consume substantial RAM or VRAM.

This is particularly important for low-resource machines.

Installed-model inventory and integrity checks must not hold the application
window closed. A successful full SHA-256 verification may write a small local
marker containing the pinned manifest digest and artifact size/modification
metadata. Unchanged artifacts can use that marker on later launches; missing,
changed, or unreadable metadata must fall back to full verification. Existing
models may need one background full pass after this mechanism changes. This is
separate from loading the selected model into the inference runtime when the
user starts captions, which can remain noticeably slower for a large model.

---

# 16. Multilingual engines

Some backends will support many languages using one model.

Prollyglot should support this architecture too.

For such models:

```text
Spoken language

● Auto detect

○ English
○ Spanish
○ Japanese
○ French
○ Korean

○ Allowed languages...
```

Allowed-language detection might contain:

```text
☑ English
☑ Spanish
☑ Japanese
☐ French
☐ German
☐ Mandarin
```

Restricting likely languages may improve UX by preventing nonsensical language switching.

For a prompt-conditioned multilingual recognizer such as the current Nemotron
trial, the spoken-language control actively guides decoding; it is not merely a
label applied after recognition. A user who knows the program is speaking
Japanese should choose Japanese for the best chance of useful Japanese text.
Selecting Spanish is not expected to recognize Japanese reliably, even though
strong speech in another language may occasionally pass through the bias.

Automatic detection is the explicit choice for mixed-language media. It may add
latency or choose the wrong language, so a forced known language remains the
preferred accuracy path. Automatic detection also needs to expose the detected
language on each committed segment before automatic source-to-target translation
can be considered dependable.

---

# 17. Automatic backend selection

Eventually Prollyglot should provide:

```text
Engine

● Automatic
○ Lightweight
○ High accuracy
○ Advanced
```

Automatic mode may consider:

- installed models,
- CPU capabilities,
- GPU vendor,
- available VRAM,
- available RAM,
- selected language,
- requested latency,
- power-saving preferences.

The first version does not need sophisticated benchmarking.

Static heuristics are acceptable initially.

---

# 18. Caption states

Live speech recognition should distinguish between:

- provisional text,
- committed text.

Example:

```text
Committed:

"We're probably going there tomorrow."

Provisional:

"Maybe around eigh..."
```

The UI should avoid repeatedly rewriting already-finalized sentences.

This is essential for captions that feel stable.

The live overlay should retain a bounded amount of recent finalized context while the next provisional utterance develops. Finalized utterances should begin on separate visual lines, with the newest line visually strongest, so quick conversational turns remain readable instead of replacing one another immediately.

Appearance should offer the current caption alone or up to three prior caption
rows. Prior rows fade with age and may be removed as complete units when a long
current caption needs the space; the overlay must never reveal more history by
clipping a row midway. This visual grouping is conversational context, not
speaker identification.

---

# 19. Caption overlay

Prollyglot should provide an independent subtitle overlay window.

The overlay should support:

- always-on-top,
- borderless display,
- draggable position,
- click-through mode,
- configurable width,
- configurable text size,
- configurable opacity,
- background opacity,
- text alignment,
- optional original + translated text,
- independent original and translated text colors,
- stacked or side-by-side bilingual layout,
- zero to three fading prior caption rows,
- a selectable post-speech reading interval,
- a selectable fade-out duration,
- multi-monitor placement,
- temporary hiding,
- fullscreen applications where possible.

Default placement:

- lower center of the active display.

---

# 20. Overlay styles

Initial modes might include:

### Standard

```text
┌───────────────────────────────────┐
│ Are you coming over later today? │
└───────────────────────────────────┘
```

### Dual language

```text
┌─────────────────────────────────────────┐
│ 今日は何をする予定ですか？             │
│ What are you planning to do today?     │
└─────────────────────────────────────────┘
```

Dual language should also offer a side-by-side layout when the selected width
can keep both columns readable. The source and translation colors should be
independently configurable so the two outputs remain easy to distinguish.
Each source caption and its translation form one stable visual row. A new or
provisional source must not span the bilingual grid, displace the translated
half of an older pair, or allow one language to be clipped independently.
Both columns wrap in full. Prior pairs may use a smaller faded type size and the
oldest complete pair may be removed when space is exhausted, but neither column
may be independently ellipsized or truncated.

The default final-caption reading interval is 15 seconds, followed by an 800ms
fade. Appearance should offer 6, 10, 15, and 30 second reading choices plus an
instant or graduated fade. The interval is measured from the newest readable
source or translation result, not merely from the end of captured speech. A
translation that arrives after its source therefore receives a fresh full
reading interval. Original text is the first revision of one structured frame;
pending and completed translation update the same identified rows. There is no
second raw-caption display path that can collapse a bilingual layout into a
full-width original-only frame.

### Minimal

```text
Are you coming over later today?
```

Do not build a full visual theme ecosystem initially.

Basic readability matters more.

---

# 21. Caption positioning

Users should be able to choose:

- bottom-center,
- top-center,
- bottom-left,
- bottom-right,
- custom position.

Prollyglot should remember the preferred position.

Eventually application-specific positioning may be useful.

Example:

```text
Discord → bottom center
Game → top center
```

Not required for initial release.

---

# 22. Transcript view

Prollyglot should optionally retain the current session transcript.

Example:

```text
17:42:11  We're going live now.
17:42:15  The vote has finished.
17:42:19  Results should be available shortly.
```

The transcript panel should support:

- copy,
- clear,
- search,
- save.

During a live session, the panel should open at and follow the newest caption by
default and use most of the available window height. If the user scrolls upward,
incoming captions must not pull the view away from that older context; a visible
Latest action should return to live-follow mode.

---

# 23. Transcript export

Initial export formats:

- `.txt`,
- `.srt`,
- `.vtt`.

Optional later formats:

- JSON,
- structured transcript.

Timestamps should be preserved when possible.

Prollyglot should not automatically persist transcripts unless the user enables that behavior.

---

# 24. Privacy

Prollyglot should be local-first.

Default behavior:

- captured audio remains on device,
- no account,
- no telemetry requirement,
- no transcript upload,
- no audio upload,
- no cloud model requirement.

If cloud backends are ever supported, they should be optional plugins or explicitly enabled providers.

The UI should make it obvious when an engine processes information remotely.

---

# 25. Recording policy

Prollyglot should not record raw audio by default.

The application only needs transient audio buffers for inference.

Default:

```text
Audio
Captured → processed → discarded
```

Optional recording can be considered later but should be treated as a separate feature.

---

# 26. Translation

Translation should be separate from transcription.

Architecture:

```text
audio
 ↓
ASR
 ↓
source-language text
 ↓
optional translation engine
 ↓
translated text
```

This avoids coupling speech recognition quality to translation capabilities.

---

# 27. Translation modes

Users should be able to choose:

```text
Caption display

● Original
○ Translation
○ Original + Translation
```

Example:

```text
Spoken language
Japanese

Translate to
English
```

Result:

```text
「今朝、政府は新しい計画を発表しました。」

"The government announced a new plan this morning."
```

The pre-release implementation supports a selectable translation target for
each of the 29 forced spoken-language choices. Japanese-to-English and
Spanish-to-English retain compact direct q8 models. One compact multilingual
OPUS model handles the remaining supported sources to English. An optional
larger M2M100 q8 model translates directly among all 29 selectable languages.
These are explicit downloads: the route resolver prefers an installed compact
model and never silently downloads the universal model. Inventory, download,
verification, and removal use the native `ModelManager`; a separate disposable
inference worker belongs to one active caption or visual translation session
and keeps at most one translator loaded. Model revisions and every required
artifact's size and SHA-256 digest are pinned. Native downloads stream through a
64 KiB buffer into verified sidecars and become visible only after atomic
publication. The main inference WebView reads only manifest-listed native files
through a private protocol capped at 4 MiB per range and runs them on CPU through
WebAssembly. The runtime necessarily materializes an ONNX graph for inference,
but model downloads and native-to-WebView transfers do not create an additional
artifact-sized command payload.

Translation packs installed by an older pre-release remain available from the
legacy WebView cache as a read-only migration fallback. New installs always use
native storage. The Models workspace identifies a legacy pack and offers an
explicit move action; the old copy is removed only after the native replacement
passes verification or when the user explicitly removes the pack from both
stores.

Original text renders immediately. Once a provisional caption has at least four
characters, translation can begin from the newest coalesced partial after about
420 ms. Changing partials replace the pending text without restarting that first
deadline; further live requests are throttled to no more than one every 900 ms
so translation cannot build a per-word backlog. Finalized source text has
priority and is translated again unless an exact live result can be reused. For
Nemotron, the adapter enforces a four-second continuous pause-light finalization
safety boundary, but live translation no longer depends on reaching it.

Translation jobs carry their request and session identity, source revision,
workload profile, priority, coalescing identity, enqueue time, and deadline.
The bounded scheduler prioritizes finalized captions over provisional work.
Changing partials replace queued text for the same utterance, and current visual
text replaces queued work for the same track. If translation falls behind,
expired or superseded work is rejected so the live display returns to the
current edge instead of exhaustively replaying old work. A workload deadline
terminates and recreates only the active inference worker; model control remains
responsive, and delayed output from the old session is ignored. The translated
line fills its existing source/translation pair independently.
Translation failure falls back to original text and writes privacy-safe timing
and failure diagnostics without logging caption contents.

The implemented display choices are Original, Translation, and Original + Translation.
The bilingual choice supports stacked and side-by-side layouts plus independent
source and translated colors. Side-by-side text wraps in full, and zero to three
complete prior pairs can remain above the live pair at a smaller fading size.
Automatic mixed-language recognition remains
original-only until the recognizer reports a dependable detected language on
each finalized segment; silently guessing which translator to run would make
both latency and output quality worse.

---

# 28. Multiple translation outputs

This could be supported later:

```text
Input
Japanese

Output
☑ Japanese original
☑ English
☑ Spanish
```

ASR should run once.

Translations can then branch from the recognized text.

This is not required for MVP.

---

# 28.1 Visual text translation

Visual text translation is an experimental Windows-first extension of
Prollyglot, kept focused on the same media-accessibility job: recognize text
already shown by another application, translate it locally, and place the
result near that text. It is not a mandate to become a general screen recorder,
OBS replacement, or open-ended visual assistant.

This specifically includes text rendered into video subtitles, signs visible
inside a video, menus, title cards, and text drawn into a game or application's
HUD. It does not require a general vision model to describe non-text objects or
events in the scene.

The first Windows slice is a separate mode with its own action:

```text
[ Start Audio Captions ]
[ Translate Screen… ]
```

Audio captions and visual translation are initially mutually exclusive. The
architecture should permit both later, but concurrent mode is enabled only
after CPU, GPU, memory, and overlay-behavior evidence shows that neither path
starves the other.

Visual sources:

- a user-drawn region on one display;
- one selected application window; or
- one selected display.

The current Windows slice uses the documented `Windows.Graphics.Capture` path
for selected top-level application windows and displays. A region is a crop of
a captured display rather than a separate injection or application hook. The
desktop app enumerates eligible `HWND` and `HMONITOR` sources and requires an
explicit choice before Win32 interop creates a capture item. The Windows system
picker remains a compatibility and accessibility follow-up because its
ownership and UI-thread lifecycle have not yet been integrated safely into the
Tauri command path.

Selected-display capture is a first-class compatibility path. A documented
DXGI Desktop Duplication display backend still needs to be compared because an
application's window surface and the composed monitor can behave differently,
and OBS Display Capture is the practical parity baseline.
If window capture is blank while one of the display paths returns useful pixels,
Prollyglot should offer **Switch to Monitor capture** and crop the requested
region from that display. If equivalent OBS Display Capture succeeds while both
Prollyglot display backends fail, treat it as a Prollyglot compatibility defect.

This is compatibility engineering, not a protected-content bypass. Microsoft
documents that Desktop Duplication protects access to protected video and that
protected swap chains can be excluded from desktop duplication. Protected or
unavailable pixels therefore follow the same policy as audio: process whatever
the documented API exposes, accept a black or excluded frame as an unavailable
capture condition, do not classify a source as DRM from blank pixels alone, and
do not add process injection, capture hooks, or a protection bypass.

The initial pipeline uses specialized OCR rather than a general-purpose
vision-language model:

```text
Windows.Graphics.Capture frame
        ↓
selected crop + frame-change detection
        ↓
text detection and OCR
        ↓
spatial line grouping, profile-specific stabilization, and region ranking
        ↓
existing local translation service
        ↓
translated label anchored above or beside the source text
```

The current optional OCR pack is pinned PP-OCRv6 Small: detection,
classification, unified recognition, and dictionary artifacts total 31,824,456
bytes (30.4 MiB). The manifest exposes the same 29 language choices currently
available to the translation UI and records source provenance, byte size, and
SHA-256 for every artifact (the dictionary's mutable upstream URL is pinned by
content hash). The pack is never bundled or downloaded without an explicit user
action. Its `rapidocr-core` runtime is vendored at the exact
0.2.2 source release with packaging-only dependency changes documented beside
the source.

Windows' built-in `Windows.Media.Ocr` remains a potentially useful
zero-download comparison because it returns word positions and uses installed
recognizers. Microsoft documents that desktop use requires package identity,
however, so the current unpackaged development and MSI/NSIS paths cannot assume
it is available. It is an optional packaged-build experiment rather than the
baseline, and it would not provide the later Linux path.

Relevant primary references:

- <https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture>
- <https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/>
- <https://learn.microsoft.com/en-us/windows-hardware/drivers/display/desktop-duplication-api>
- <https://learn.microsoft.com/en-us/windows/win32/api/dxgi/ne-dxgi-dxgi_swap_chain_flag>
- <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity>
- <https://learn.microsoft.com/en-us/uwp/api/windows.media.ocr.ocrengine>
- <https://www.paddleocr.ai/latest/en/version3.x/algorithm/PP-OCRv6/PP-OCRv6.html>
- <https://obsproject.com/kb/display-capture-sources>

Visual translation continuously watches the selected live source; the region
selector chooses a crop to monitor, not a still image. Windows capture samples
at up to 12 FPS by default so movement and short-lived text remain current.
Expensive OCR is independently capped at four changed-frame opportunities per
second, and a capacity-one latest-frame queue replaces stale work if inference
is slower. A 64 × 36 luminance fingerprint detects both broad scene movement
and small localized text-like changes. The PP-OCR live configuration bounds the
longest detector side to 1280 pixels, uses up to four local inference threads,
limits detector candidates, and skips the direction classifier because desktop
media text is expected to be upright. Nearby horizontally split or vertically
stacked OCR lines are joined into one reading-order phrase before filtering and
translation, so a sign containing three stacked title lines becomes one request.
Before every recognition pass the worker drains the queue to its newest frame.
If a completed pass is already more than three seconds behind and the live
source has undergone a broad scene change, its text is discarded instead of
being placed over a later scene; an unchanged source or small localized update
may still use the result. The ordinary gate remains sensitive to subtitle-sized
changes, while stale-result rejection uses a deliberately stronger threshold so
cursor movement, controls, clocks, and counters do not repeatedly clear useful
translations. Discarding an old result clears its text tracks, cancels pending
translation for that generation, and returns the overlay to **Scanning for
text…** while the newest frame is processed. The latest visual presentation is
generated by the main translation controller, validated and cached by native
code, then delivered as one revisioned event to the overlay window. Only the
main translation controller owns clear/rescan events, and both native and
overlay consumers reject stale presentation revisions, preventing a late
broadcast or publish from erasing newer labels. Runtime status reports recognized OCR regions
and delivered overlay labels separately. The CPU-heavy image, ndarray, OCR
post-processing, and visual-pipeline crates use optimized development profiles
so the owner's normal `tauri dev` loop exercises representative OCR code rather
than debug-speed pixel loops.

**Prominent text** promotes its first high-confidence OCR pass, ranks regions by
confidence, size, area, and proximity to the source center, and keeps at most six
current regions. This removes the previous requirement to wait for a second
expensive inference before showing a useful result. **All detected text** keeps
two-pass stabilization and the lower size/confidence thresholds because its
purpose is deliberately broader, with a twelve-region live-output budget so a
dense desktop cannot create an unbounded translation or overlay surface. An
unchanged confirmation pass is still allowed before static content goes idle.
Translator preparation begins when visual translation is requested, but live
capture and OCR no longer sit idle behind its cold start. Audio-caption
translation is suspended while visual mode owns the inference session, and
pending work is continuously rebuilt from the highest-value current snapshot
instead of draining stale fragments. Short, prominent labels receive priority
over long paragraphs. Only the request actively running displays
**Translating…**; queued regions appear progressively when their translations
complete rather than covering the source with placeholders that imply parallel
work. These defaults
should be tuned from native Windows latency, CPU, and recall evidence rather
than exposed as premature performance controls.

Translation generation is bounded in proportion to recognized input length so
a short OCR label that misses an end token cannot consume the full 192-token
ceiling. Once a route is loaded, a compact visual inference that has not returned
within its 3.5-second live deadline is abandoned and the session worker is
recreated; the optional universal model receives eight seconds because its performance
profile is materially heavier. This is a liveness ceiling, not the ordinary
latency target: a stable compact-route region should normally reach the overlay
within two seconds. One timed-out region must not strand later regions, and the
privacy-safe diagnostic log records route, queue wait, inference time, remaining
region count, and recovery without source or translated text.

The translated visual layer is separate from the bottom audio-caption surface.
Each result retains a capture-space bounding box mapped through crop and current
source geometry into screen coordinates. The source text is not redrawn because
it is already visible in the captured application or display. Translation
appears just above the source box when space permits, moves below it when
required, and clamps to the captured bounds. The overlay shows one immediate
**Scanning for text…** state before the first OCR result. Once text is confirmed
absent, its translation remains readable for at most eight seconds. If the same
text had already remained continuously visible for twelve seconds or longer, it
is removed immediately when absent rather than receiving another grace period.
Live status counters update their existing values in place instead of rebuilding
the active control surface on every capture event. This keeps the Stop control
stable between pointer-down and pointer-up. One Stop action hides and clears the
overlay immediately, sends a cooperative termination request to an active ONNX
Runtime recognition call, stops capture, and completes cleanup away from the
command/UI thread.

Prollyglot does not mark its app or overlays with
`WDA_EXCLUDEFROMCAPTURE`. That affinity caused full-screen troubleshooting
captures to omit the app or the surface under its transparent visual overlay.
The app, overlay, controls, and translucent selector must remain visible to an
ordinary user screenshot. To prevent an OCR feedback loop, the capture worker
instead tracks the bounded set of translated strings currently drawn by the
visual overlay and rejects matching OCR observations before stabilization and
translation. The list and text-match threshold are deliberately bounded; native
media testing must watch for false suppression when genuine source text happens
to match a currently displayed translation.

Visual OCR has two explicit recognition profiles. **Prominent text** is the
default for media and filters low-confidence observations, tiny interface text,
common URL/interface noise, and results whose dominant script conflicts with a
forced source language. It also groups nearby lines, prioritizes prominent
regions, and caps the live result set so browser chrome cannot create an
unbounded translation queue. **All detected text** lowers the size and
confidence thresholds for users who intentionally need small text. Neither
profile silently changes the user's selected capture region.

The integrated slice is accepted only if representative Japanese and Spanish
video subtitles, game UI, and signs remain positionally stable and useful on a
midrange Windows machine. Measure capture-to-OCR latency, translation latency,
text accuracy, box jitter, CPU/GPU load, and memory. A context-aware visual model
may later help ambiguous text or non-text scene understanding, but it is a
separate heavy option; the first useful version should not pay that cost merely
to read visible words.

Frames remain transient, bounded, absent from diagnostic logs, and are not saved
by default. Visual text and its translation follow the same transcript/privacy
rules as audio captions. Current local validation covers the platform-neutral
pipeline, rendered WebView workflows, and real OCR model initialization; native
Windows capture, DPI and multi-monitor mapping, protected/blank surfaces,
quality, latency, and resource use remain required evidence.

---

# 29. News use case

News and live media should be considered an important target.

Example:

```text
Firefox
↓
Japanese news livestream
↓
Japanese transcription
↓
English translation
↓
Prollyglot overlay
```

Potential sources include:

- international news,
- press conferences,
- livestreams,
- foreign-language documentaries,
- archived broadcasts,
- podcasts.

The user should not need the source website to provide captions.

---

# 30. Gaming use case

Prollyglot should work with games where technically possible.

Use cases:

- games lacking subtitles,
- inaccessible subtitle size,
- foreign-language games,
- untranslated dialogue,
- voice chat.

Example:

```text
Source
Game.exe

Spoken language
Japanese

Display
English
```

Prollyglot should not inject itself into game processes.

It should operate through the OS audio layer and overlay system.

This minimizes compatibility and anti-cheat concerns.

---

# 31. Discord / call use case

Discord should require no Discord-specific integration.

Pipeline:

```text
Discord
↓
OS audio stream
↓
Prollyglot
↓
captions
```

Possible later enhancement:

- speaker diarization.

Application APIs should not be required simply to transcribe conversation audio.

---

# 32. Speaker diarization

Speaker diarization should not block the initial release.

Initial output:

```text
Are you going?
Yeah, probably.
Okay.
```

Possible future output:

```text
Speaker 1:
Are you going?

Speaker 2:
Yeah, probably.

Speaker 1:
Okay.
```

Actual human names should not be inferred unless Prollyglot has reliable external information.

Before full diarization exists, the overlay should use reliable ASR/VAD utterance boundaries as a modest readability cue: each finalized utterance starts a new line and a few recent lines remain visible. This is pause-based turn formatting, not a claim that Prollyglot knows when the speaker changed. Simultaneous voices or changes without a usable pause may remain in one utterance.

---

# 33. Browser extension

A browser extension is not required for Prollyglot.

The desktop application should already be able to caption browser audio through OS capture.

A future extension could improve browser integration by:

- positioning captions inside pages,
- associating transcripts with tabs,
- selecting a specific tab,
- adding browser-specific controls.

The extension should remain optional.

---

# 34. User interface

The default Prollyglot window is a desktop workspace, not a phone-shaped stack
of every available control. It opens at approximately 1180 × 760 and uses a
persistent left sidebar for **Captions**, **Screen translation**,
**Transcript**, **Models**, **Appearance**, and **Settings**. A compact-mode
control in the title bar switches to an approximately 440 × 640 focused utility
that preserves the existing quick Start/Stop flow and bottom navigation. The
chosen mode persists between launches.

The UI principle is:

> Minimal by default, customizable by choice.

The full workspace should use the width available on a desktop. Caption setup
pairs the source and language controls with a live transcript/status panel.
Screen translation separates capture-source controls from language/output
controls. Transcript and model management use dedicated pages rather than
nested mobile-style sheets. Appearance is likewise a dedicated page with its
controls and preview inside the full workspace; it must not open a second modal
or utility window from full view. Compact mode exposes only the decisions
required to start useful captions, while secondary destinations use contained
dialogs and Appearance may use its focused utility window. Visual customization
and advanced engine controls remain easy to find without crowding either
primary path.

Compact example:

```text
┌────────────────────────────────────────┐
│ PROLLYGLOT                             │
│                                        │
│ Audio                                  │
│ Discord                             ▾  │
│                                        │
│ Spoken language                        │
│ English                             ▾  │
│                                        │
│ Translate to                           │
│ Off                                 ▾  │
│                                        │
│ Caption output                         │
│ Original                            ▾  │
│                                        │
│ Engine                                 │
│ Automatic                           ▾  │
│                                        │
│          [ Start Captions ]            │
│                                        │
└────────────────────────────────────────┘
```

Advanced configuration should not dominate the main interface.

The interface should favor:

- clear typography,
- generous spacing,
- restrained color,
- one obvious primary action,
- progressive disclosure,
- immediate preview for visual changes,
- readable defaults and an easy reset path.

Customization should improve caption readability and fit rather than turn the application into a theme-building platform.

---

# 35. Active-caption interface

While running:

```text
┌────────────────────────────────────────┐
│ PROLLYGLOT                      ● LIVE │
│                                        │
│ Discord                                │
│ English → English                      │
│                                        │
│ [ Stop ]   [ Transcript ]   [ ⚙ ]     │
└────────────────────────────────────────┘
```

System-tray operation should eventually be supported.

In full mode the active session keeps the same persistent navigation and
replaces the setup action with one obvious Stop action. In compact mode it keeps
the smaller live summary shown above. Changing view mode must not start or stop
capture, lose transcript state, or leave a secondary dialog trapped open.

---

# 36. Advanced settings

Advanced users may configure:

- ASR backend,
- specific model,
- inference device,
- CPU/GPU preference,
- VAD,
- chunk size,
- latency target,
- language-detection behavior,
- translation engine,
- model cache path,
- overlay behavior.

These settings should stay out of the standard setup path.

---

# 37. Model manager

Prollyglot includes a dedicated **Models** workspace that remains usable as the
catalog grows. It must not mix model inventory into unrelated application
settings or render every model as a permanently expanded card.

The page starts with a collapsed **Installed on this PC** disclosure showing
the installed count and total disk use. Opening it shows every installed speech,
translation, and visual OCR pack together, with its purpose, language scope,
size, selected/in-use state, and individual Remove action.

Below that, **Add a model** uses purpose tabs for **Speech**,
**Translation**, and **Screen text**. Within the chosen purpose the user first
selects a language or route, then a second selector contains only compatible
models. Exactly one model's size, coverage, tradeoff, state, and lifecycle
action is presented at a time. Search may supplement this flow later, but must
not restore an unbounded wall of cards.

Example:

```text
Models                                    3 installed

› Installed on this PC · 3 models · 793 MiB

Add a model
[ Speech ] [ Translation ] [ Screen text ]

Language or coverage
Japanese                               ▾

Compatible model
Nemotron multilingual                 ▾

650.6 MiB · streaming · broad coverage
[ Download ]
```

Model progress updates preserve the selected purpose, language, model, scroll
position, and a sensible keyboard focus target. Download/removal feedback must
remain visible in the active detail panel or installed inventory. The installed
disclosure exposes `aria-expanded` and `aria-controls`, its content is
associated with its button, and every action has a model-specific accessible
name.

The model manager should show:

- model name,
- supported languages,
- download size,
- approximate memory requirement where known,
- backend,
- streaming/hardware capability,
- installed state,
- selected state,
- a short, evidence-based explanation of the tradeoff.

No model is downloaded merely because it appears in search, is expanded, or is
required by a newly selected route. Downloads always require an explicit action.

---

# 38. Model storage

Models should live separately from the application binary.

Example:

```text
prollyglot/
models/
cache/
config/
```

Platform-appropriate application-data directories should actually be used in production.

The project should not distribute a gigantic executable containing every speech model.

---

# 39. Updates

Application updates and model updates should be separate.

Model changes should not require reinstalling Prollyglot.

Future engine plugins should ideally be independently versioned.

Application builds follow Semantic Versioning. `0.x` identifies the active
pre-release development line: substantial integrated fixes increment the patch
version, new product milestones or compatibility promises increment the minor
version, and `1.0.0` is reserved for the supported Windows release boundary.
`[workspace.package].version` in `Cargo.toml` is the native source of truth;
the desktop package must match it, the Tauri bundle inherits it, and the UI
receives it at build time rather than hard-coding a second display version.
Every published version has a changelog entry. Documentation-only corrections
and internal checkpoints do not require an application bump.

---

# 40. Performance goals

Exact performance targets should be established through benchmarking rather than assumed.

General goals:

### Lightweight mode

- usable on CPU,
- low background usage,
- suitable for ordinary laptops and living-room PCs.

### Standard mode

- stronger quality,
- still reasonable local inference requirements.

### Heavy mode

- optional GPU-oriented models,
- highest available transcription quality.

Prollyglot should degrade gracefully.

Failure to have a powerful GPU should not make the application useless.

---

# 41. Latency

Live captions must prioritize useful latency.

The pipeline should be designed to minimize:

```text
audio capture delay
+
buffering
+
speech inference
+
translation
+
rendering
```

Different modes may intentionally trade latency for accuracy.

Large installed model files should not add synchronous work before the control
window appears. Model inventory verification runs in the background, and the
diagnostic log distinguishes that time from the selected model's inference
runtime load. Translation diagnostics separately record queue wait, inference
time, and stale-work skips without caption content.

Possible setting:

```text
Caption latency

● Live
○ Balanced
○ Accurate
```

Do not expose raw millisecond tuning to ordinary users unless necessary.

---

# 42. Hardware acceleration

Engine implementations may support:

- CPU,
- CUDA,
- DirectML,
- Vulkan,
- ROCm,
- other acceleration APIs.

Prollyglot's core should not assume NVIDIA hardware.

This matters especially for:

- AMD Windows systems,
- AMD Linux systems,
- Intel integrated graphics,
- low-power systems.

Backends should advertise their supported execution providers.

---

# 43. FOSS requirements

Prollyglot itself should use an OSI-compatible open-source license.

Dependencies should be checked for:

- redistribution rights,
- commercial-use restrictions,
- model-weight licensing,
- derivative-work requirements.

The project should avoid making a non-commercial model mandatory.

Optional engines with unusual licensing should be clearly marked and ideally downloaded separately.

---

# 44. Dependency policy

Avoid large frameworks unless they provide significant value.

Particularly avoid:

- Electron.

Prefer:

- native or lightweight UI,
- Rust/C/C++ core components where appropriate,
- mature system APIs,
- independently replaceable model runtimes.

The project should not accumulate five frameworks simply to display subtitles.

---

# 45. Suggested implementation stack

A strong initial direction is:

### Core

Rust.

Responsibilities:

- application state,
- audio abstraction,
- buffering,
- model management,
- transcript state,
- configuration,
- IPC if needed.

### Windows audio

Rust bindings around Windows WASAPI APIs, with lower-level C++ only where necessary.

### Linux audio

PipeWire integration.

### UI

Use a lightweight native or lightweight cross-platform toolkit capable of:

- transparent windows,
- always-on-top overlays,
- click-through behavior,
- multi-monitor support.

The final UI technology should be selected based primarily on overlay reliability.

---

# 46. Internal modules

Possible project layout:

```text
prollyglot/

crates/
    core/
    audio/
    audio-windows/
    audio-pipewire/
    asr/
    asr-moonshine/
    asr-whisper/
    translation/
    transcript/
    overlay/
    model-manager/
    config/

app/
    desktop/

assets/

docs/
```

Exact layout can evolve.

The important distinction is keeping platform audio and model implementations modular.

---

# 47. Shared audio-source abstraction

Example conceptual type:

```text
AudioSource {
    id
    name
    source_type
    instance_count?
    icon?
    active
}
```

Source types:

```text
System
Application
Input
```

Application IDs are stable, opaque backend identities. Process IDs, executable
paths, package identities, and current process-tree roots remain inside the
platform adapter. If more than one current process tree matches an identity,
the selection is ambiguous and Prollyglot must wait or ask the user rather than
choosing one silently. An ordinary application restart keeps the session in
Waiting and re-resolves the same identity with bounded backoff.

---

# 48. ASR backend contract

Each engine should expose metadata.

Example:

```text
ASREngineInfo {
    name
    languages
    streaming
    cpu_support
    gpu_support
    model_requirements
}
```

Runtime behavior should expose:

```text
load
unload
start
push_audio
partial_result
final_result
stop
```

---

# 49. Transcript representation

Transcript data should be structured internally.

Example:

```text
TranscriptSegment {
    start_time
    end_time
    source_language
    text
    final
    speaker?
    translation?
}
```

This makes export and future features much easier than storing one giant string.

---

# 50. Error handling

User-facing errors should be understandable.

Bad:

```text
HRESULT 0x88890004
```

Better:

```text
Discord stopped producing audio.

Prollyglot is waiting for it to resume.
```

Or:

```text
The selected transcription model could not fit in available memory.

Try Lightweight mode.
```

---

# 51. Application lifecycle handling

Prollyglot should gracefully handle:

- selected application closing,
- selected application restarting,
- playback device changing,
- headphones disconnecting,
- Linux stream recreation,
- GPU failure,
- model load failure,
- sleep/resume,
- display changes.

The application should attempt recovery where safe instead of simply exiting.

---

# 52. Hotkeys

Useful global shortcuts:

```text
Toggle captions
Pause transcription
Hide/show overlay
Clear current captions
```

Hotkeys should be configurable.

---

# 53. Accessibility

Because Prollyglot is fundamentally an accessibility-adjacent application, the UI should support:

- keyboard navigation,
- scalable text,
- high-contrast captions,
- screen-reader-friendly controls,
- large caption sizes,
- configurable background opacity.

Accessibility should be treated as a core quality requirement rather than a later theme.

---

# 54. Configuration

Preferences should be stored locally.

Possible settings:

```text
config
├── audio defaults
├── overlay position
├── overlay appearance
├── preferred language
├── selected engine
├── language profiles
├── model paths
└── hotkeys
```

No account should be necessary to synchronize configuration.

---

# 55. Initial proof of concept

The executable delivery sequence and milestone acceptance criteria live in `BUILD_PLAN.md`. The POCs below describe product risk, while the build plan groups them into substantial integrated releases.

The first POC should deliberately be ugly.

Goal:

> Demonstrate reliable capture → transcription → overlay.

### POC 1 — Windows

- enumerate output sources,
- capture full system audio from the selected playback device,
- run English speech recognition,
- print transcript to console,
- display simple overlay.

### POC 2 — Windows per-process

- enumerate active applications producing audio,
- select application,
- capture only that application,
- transcribe it.

The first usable release should be built and hardened from these Windows POCs before Linux-specific implementation becomes release-critical.

### POC 3 — Linux

- capture system output through PipeWire,
- run the same shared transcription pipeline,
- display subtitles.

### POC 4 — Linux application capture

- enumerate relevant PipeWire streams,
- select application,
- transcribe only that source.

At this point the core technical risk has largely been addressed.

---

# 56. MVP

After the Windows POCs succeed:

### Required for the first Windows release

- Windows 11,
- selected-device system capture,
- application capture,
- English transcription,
- lightweight ASR backend,
- subtitle overlay,
- start/stop controls,
- basic settings,
- local-only inference,
- model downloader,
- simple transcript view.

### Strongly desired

- multilingual original-language captioning (29 forced choices now exist through dedicated models and Nemotron, but production quality approval remains),
- `.txt` export,
- `.srt` export,
- `.vtt` export,
- system tray,
- global hotkey.

### Deferred until the Windows MVP is reliable

- Ubuntu/PipeWire implementation,
- Ubuntu `.deb` packaging,
- Wayland- and X11-specific overlay validation,
- Linux application-stream grouping and lifecycle handling.

---

# 57. Linux follow-up and Version 0.2

The first post-MVP platform milestone should bring the shared pipeline to one supported Ubuntu LTS release using PipeWire and a native `.deb` package.

Once that baseline is reliable, Version 0.2 feature work may include:

- language profiles,
- production approval and allowed-language constraints for automatic detection,
- additional or improved multilingual ASR backends,
- production approval and optimization of compact-to-English and many-to-many local translation,
- richer language-profile behavior,
- improved model selection,
- improved overlay,
- GPU acceleration options.

---

# 58. Version 0.3+

Potential features:

- speaker diarization,
- OBS text output,
- browser extension,
- subtitle history search,
- multiple simultaneous audio sources,
- application-specific presets,
- plugin API,
- macOS investigation.

These should only be pursued after the basic application is reliable.

---

# 59. Explicit non-goals for initial releases

Do not prioritize:

- accounts,
- cloud storage,
- cloud transcription,
- cloud translation,
- LLM summaries,
- meeting summaries,
- calendar integrations,
- task extraction,
- sentiment analysis,
- Discord bots,
- direct Discord API integration,
- browser-only implementation,
- macOS,
- mobile apps,
- audio recording,
- voice cloning,
- speaker identification by real-world identity,
- massive plugin ecosystem.

---

# 60. Testing philosophy

Testing should focus on important behavior.

High-value tests:

- audio buffer handling,
- resampling correctness,
- transcript state transitions,
- model loading,
- export generation,
- configuration parsing,
- language switching.

Do not spend early development cycles constructing enormous mock suites around unstable prototypes.

Manual integration testing is acceptable for:

- WASAPI capture,
- PipeWire capture,
- overlays,
- fullscreen behavior,
- hardware-specific inference.

The early project should prioritize getting a working caption pipeline into users' hands.

---

# 61. Documentation philosophy

Documentation should be sufficient for:

- building the project,
- understanding architecture,
- contributing new backends,
- packaging releases.

Avoid documenting speculative systems before they exist.

Initial documentation:

```text
README.md
CONTRIBUTING.md
BUILDING.md
ARCHITECTURE.md
```

Add additional documents only when they solve an actual maintenance problem.

---

# 62. Success criteria

Prollyglot is successful when a user can install it and do this:

```text
Open Prollyglot.

Select:
Firefox

Choose:
Japanese → English

Press:
Start Captions

Play:
Japanese news livestream

See:
English subtitles over the screen.
```

And another user can do:

```text
Select:
Discord

Choose:
English → English

Start captions.

Play a game simultaneously.

Only Discord is transcribed.
```

And both should work without:

- sending audio to a server,
- installing a virtual sound device,
- modifying Discord,
- modifying Firefox,
- installing a browser extension,
- owning a high-end GPU.

These scenarios must first meet release quality on Windows 11. The same scenarios become the acceptance target for the later supported Ubuntu release.

---

# 63. Primary technical risks

## Per-application capture differences

Windows and Linux expose applications differently.

Mitigation:

Keep the capture abstraction narrow and platform-specific.

## Linux stream instability

PipeWire application streams may appear and disappear dynamically.

Mitigation:

Track application identity separately from transient node IDs.

## Model latency

Some speech models may technically work but feel terrible as live captions.

Mitigation:

Benchmark real conversational audio and prioritize streaming-native engines. The bounded inference queue should absorb ordinary decoder bursts without adding unbounded delay. If the queue does overflow, Prollyglot should discard stale queued audio in one operation, abandon any incomplete hypothesis without forcing an expensive final decode, resume near the current live edge, and record both the drop and later recovery in the privacy-safe diagnostic log. It should not remain trapped repeatedly finalizing discontinuous audio while captions stop advancing.

## Translation latency

Translation can make otherwise responsive captions feel delayed.

Mitigation:

Transcription should render immediately; translated captions update independently.
Coalesce changing partial text into a short throttled live-translation cadence
instead of waiting for the speaker to stop or queuing every partial. Translate
final text again for stability, process finalized captions before provisional
work, give pause-light multilingual speech a bounded finalization point, cap the
backlog, discard stale translations when necessary, and expose timing in
privacy-safe logs without caption contents.

## Hardware fragmentation

Inference performance will vary significantly.

Mitigation:

Support small CPU-friendly models first and treat large models as optional.

## Protected or unavailable capture

Some audible Windows content may be unavailable to documented loopback APIs because of protected-media policy, exclusive-mode output, application routing, or driver behavior. Other protected-media playback is exposed through the normal decoded device mix and should work without any special-case handling.

Mitigation:

Attempt every selected source through the documented capture path without DRM detection or source blacklists. Compare failures against equivalent current OBS device and application capture, treat OBS-only success as a Prollyglot compatibility defect, report sustained unavailable audio clearly, and do not build a DRM-bypass path.

---

# 64. Project identity

**Name:** Prollyglot

The name should not force the interface into excessive novelty.

A restrained visual identity could use a subtle language or sound motif without turning an accessibility utility into a novelty application.

The software itself should feel competent and lightweight.

Possible short description:

> Prollyglot is a FOSS desktop subtitle layer for Windows and Linux. Select an application or your entire system, transcribe its audio locally, and optionally translate it in real time.

Possible shorter repository description:

> Local subtitles for anything making sound on Windows or Linux.

---

# 65. Guiding development rule

Whenever a proposed feature appears, ask:

> Does this make arbitrary computer audio easier to caption?

If not, it probably does not belong in Prollyglot yet.
