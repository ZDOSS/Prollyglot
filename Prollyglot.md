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

Protected content, exclusive-mode output, or another OS-enforced restriction may produce silence or an unavailable stream even though the user can hear playback.

When capture is unavailable, Prollyglot should:

- keep the rest of the application responsive,
- explain that the operating system did not expose capturable audio,
- avoid claiming that every protected source is supported,
- suggest source-provided captions when available,
- allow deliberate microphone capture as an accessibility fallback without enabling it automatically.

The core product should not depend on protected-content capture working.

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

Start with Moonshine or another lightweight local model for English.

Initial goal:

```text
English audio
↓
local speech recognition
↓
live subtitles
```

The first successful milestone should not require multilingual support.

---

# 13. Language packs

Prollyglot should support independently downloadable language packs whenever the selected model architecture permits it.

Example:

```text
Languages

Installed

✓ English
✓ Spanish
✓ Japanese

Available

  French       Download
  German       Download
  Korean       Download
  Mandarin     Download
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

The default Prollyglot window should be small and approachable.

Example:

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
│ Captions                               │
│ English                             ▾  │
│                                        │
│ Engine                                 │
│ Automatic                           ▾  │
│                                        │
│          [ Start Captions ]            │
│                                        │
└────────────────────────────────────────┘
```

Advanced configuration should not dominate the main interface.

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

Prollyglot should include a model manager.

Example:

```text
Models

Moonshine English Small
123 MB
Installed
[ Remove ]

Moonshine Japanese
58 MB
Installed
[ Remove ]

Nemotron Multilingual
Not installed
[ Download ]

Voxtral Realtime
Not installed
[ Download ]
```

The model manager should show:

- model name,
- supported languages,
- download size,
- approximate memory requirement where known,
- backend,
- installed state.

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
    process_id?
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

Application-level details should not leak outside platform capture modules.

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

- Spanish model,
- Japanese model,
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
- automatic language detection,
- multilingual ASR backend,
- translation,
- dual subtitles,
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

Benchmark real conversational audio and prioritize streaming-native engines.

## Translation latency

Translation can make otherwise responsive captions feel delayed.

Mitigation:

Transcription should render immediately; translated captions can update independently.

## Hardware fragmentation

Inference performance will vary significantly.

Mitigation:

Support small CPU-friendly models first and treat large models as optional.

## Protected or unavailable capture

Some audible Windows content may be unavailable to documented loopback APIs because of protected-media policy, exclusive-mode output, or driver behavior.

Mitigation:

Treat this as an explicit compatibility boundary, report sustained silent or unavailable capture clearly, and do not build or recommend a DRM-bypass path.

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
