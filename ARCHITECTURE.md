# Prollyglot architecture

This document describes the code that exists today. Product intent and platform
promises live in [Prollyglot.md](Prollyglot.md); delivery order and acceptance
evidence live in [BUILD_PLAN.md](BUILD_PLAN.md).

## System boundary

Prollyglot is one Tauri desktop process with several operating-system WebView
windows and native worker threads. Audio, screen frames, transcripts, OCR text,
and translations stay local. The only normal network operation is an explicit
model download.

Windows 11 is the first production target. An experimental Ubuntu 26.04 LTS
amd64 adapter now uses the same audio boundary. Neither platform has passed all
supported-release gates.

```text
Platform audio adapter                          Main WebView
  Windows: WASAPI / process loopback              UI + controllers
  Ubuntu: PipeWire output / application monitor
            │                                             │
            ▼                                             ▼
       normalized PCM ──► audio pipeline ──► ASR ──► transcript
                                                            │
Windows.Graphics.Capture                                    ▼
            │                                      presentation frame
            ▼                                             │
       latest frame ──► OCR ──► translation worker ────────┤
                                                            ▼
                                             caption / visual overlay WebView

                SessionSupervisor + resource coordinator
                    own lifecycle and inference loads
```

Audio captions and visual translation are mutually exclusive. One
`SessionSupervisor` is the authority for both modes; UI flags and legacy status
events are projections, not competing lifecycle state.

## Runtime ownership

| Owner | Responsibility |
| --- | --- |
| `SessionSupervisor` in `crates/application-runtime` | Session ID, mode, legal transitions, revision, cancellation, worker completion, failure, and cleanup |
| `RuntimeState` in `apps/desktop/src-tauri` | Native adapters and repositories shared by Tauri commands |
| `InferenceResourceCoordinator` | The speech, OCR, and translation runtimes loaded for the active session |
| `ConfigurationRepository` | The one durable, schema-versioned configuration snapshot |
| `ModelManager` | Manifest validation, bounded download, verification, atomic publication, removal, and native inventory |
| `TranscriptStore` | Current session's provisional and committed source-language segments |
| `AppStore` in the main WebView | Render-facing runtime, catalog, transcript, navigation, preferences, and notice state |
| Caption/visual presentation runtimes | The last accepted revisioned frame sent to each overlay |

The main WebView is the only presentation writer. Overlay WebViews render
accepted frames; they do not reconstruct captions from transcript events.

## Session lifecycle

Each start allocates a stable `SessionId` and cancellation token before model
loading begins.

```text
Stopped ──Start──► Starting ──ready──► Running
                       │                  │  ▲
                       │                  ▼  │ source returns
                       │                Waiting
                       │                  │
                       └────Stop──────────┤
                                          ▼
                                      Stopping ──joined──► Stopped

Any supervised worker failure ──► Failed ──cleanup──► next Start is legal
```

Every public snapshot has a monotonically increasing revision. The frontend
registers its listener before requesting bootstrap state and discards an older
snapshot. Every overlay frame also carries session, runtime, and presentation
revisions, so a delayed translation or publish cannot revive a stopped session.

Stop is idempotent. It acknowledges promptly, cancels current work, hides the
relevant overlay, and joins platform/inference workers in a supervised cleanup
thread. Cleanup has a 15-second failure boundary; a second click is never the
normal completion mechanism.

## Process, WebViews, and workers

Tauri creates these WebViews from `tauri.conf.json`:

- `main`: persistent full workspace or compact utility;
- `appearance`: compact mode's focused appearance utility;
- `overlay`: always-on-top audio-caption presentation;
- `visual-overlay`: positioned screen-translation labels; and
- `region-selector`: translucent live-region selection.

Native session work uses named threads rather than OS capture callback threads
for inference:

- audio capture/backend worker, capture-event forwarder, streaming
  transcription worker, supervisor monitor, and cleanup worker;
- visual capture worker, capture-event worker, OCR worker, supervisor monitor,
  and cleanup worker; and
- short-lived model inspection/download workers.

The main WebView uses separate Web Workers for translation model control and the
active inference session. A timed-out or replaced translation terminates only
the inference worker; catalog and download operations remain responsive.

## Audio path and portability seam

`prollyglot-core::AudioCaptureBackend` is the only desktop-facing audio capture
interface. It reports capabilities, enumerates sources, resolves a public
selection, starts a session, emits recovery events, and stops through
`CaptureSession`. `apps/desktop/src-tauri/src/audio.rs` does not call WASAPI
enumeration or capture functions directly.

`crates/audio-windows` implements the interface with:

- selected-endpoint and follow-default WASAPI loopback;
- documented process-tree loopback;
- opaque application identity derived from available application-model,
  package-family, or executable identity; and
- bounded device/application re-resolution while the session is `Waiting`.

PIDs, process roots, package identity, executable paths, `IMMDevice` objects,
and other platform handles remain inside the adapter. More than one live match
for an application identity is ambiguous and is never selected silently.

`crates/audio-pipewire` implements default-output and pinned-output monitoring on
Ubuntu. The output path reads `Audio/Sink` nodes and session-manager metadata,
uses opaque node-name identities across recreation, and targets the current
object serial. Session-manager fallback is disabled: a missing pinned output
enters recovery rather than switching sources. One cancellable worker owns each
PipeWire loop and stream; capture callbacks publish through bounded channels
without running on PipeWire's real-time thread. Reconnection retains the session
clock and marks a discontinuity. Effective-default metadata is also refreshed
periodically on the same loop to cover a missed subscription update observed
during concurrent source enumeration.

The application path observes playback nodes, clients, and DSP ports.
`identity.rs` reads same-user executable/process-start evidence; `graph.rs`
groups nodes by declared application identity or executable/name fallback and
refuses independent matching process roots. Unknown clients use a server-cookie
and client-serial identity that cannot silently reconnect across server lifetimes.
Paths, PIDs, and native graph IDs remain inside the adapter. No command lines,
environment values, media titles, or raw PCM are persisted.

`application.rs` owns one native `pw_filter` and passive links from all selected
playback channels. It uses the existing generated PipeWire FFI because the Rust
wrapper does not expose filters. Stable boxed listener data, mapped-buffer
bounds checks, and RAII buffer return define the unsafe boundary. Callbacks and
port updates run on one non-realtime worker loop. Each graph quantum downmixes
individual streams, sums them, clamps finite mono PCM, and publishes one block;
streams are never concatenated into a longer audio timeline. Removing a stream
leaves other inputs active; disappearing or ambiguous application identity tears
down the monitor and waits for safe re-resolution. Stop removes only owned
capture links and the filter, leaving the player's output connections intact.

The desktop selects its adapter by platform and queries sources off the UI event
thread. Refresh retains missing selected applications and pinned outputs as
unavailable options; Start cannot silently switch to system-wide capture.

After capture, shared code downmixes PCM, resamples to the selected model rate,
buffers bounded frames, applies speech-window/VAD policy, and drives the
backend-neutral `SpeechEngine`/`SpeechStream` contracts.

On Ubuntu, hidden GTK windows are realized before click-through settings are
applied. The initial caption overlay uses X11/XWayland when available and honors
an explicitly selected GTK backend. Native Wayland overlay behavior and Linux
visual-mode integration remain separate work. The Ubuntu `.deb` carries speech/ONNX
libraries in `/usr/lib/prollyglot`; relative runtime search paths keep the package
independent of the build checkout.

## Visual path

`crates/visual-windows` captures an explicitly selected window, display, or
display crop through `Windows.Graphics.Capture`. Captured frames are transient.
A capacity-one latest-frame channel replaces stale input if OCR is slower than
capture.

`crates/visual-pipewire` implements the Ubuntu capture backend independently of
desktop windows. An asynchronous, cancellable portal worker creates a dedicated
DBus session, selects one monitor/window, and gives the portal's restricted FD
to a native PipeWire worker. Version 6 streams target `object.serial`; older
portals use their node ID without reconnect or fallback after source loss.
The workers retain no permission/restore tokens and close requests, sessions,
native listeners, buffers, and connections on cancellation or failure.

The native adapter negotiates CPU-readable packed RGB, strips padding, applies
valid crops and SPA transforms, and publishes the same transient `VisualFrame`
used by Windows. Frame storage remains capacity one, capture metadata remains
bounded, and allocations are checked against an 8K-sized pixel budget. It
prefers the shared capture rate but accepts fixed-rate sources; higher input
rates replace pending frames instead of extending an OCR backlog. No GPU
buffer import or recording path is implemented.

The portal's optional compositor geometry lives in `StreamInfo`; pixel crop and
orientation live in `FrameGeometry`. Neither assumes a global physical position
for a Wayland window. The desktop's `visual_capture` adapter resolves a portal
selection on a cancellable startup worker, then owns it under the same session
supervisor as Windows. Ubuntu uses a normal movable reader; only Windows maps
capture-space bounds onto the desktop. Reader feedback requires both rendered
text and local background-pixel evidence, with bounded recent-text history.
A separate OCR sampling clock permits static portal images to finish scanning
without changing their capture evidence or received-frame counts. No pixels
cross IPC. See the [headless screen-capture checks](docs/testing/UBUNTU_SCREEN_CAPTURE.md).

`crates/visual-pipeline` owns change gating, crop/geometry rules, stabilization,
and tracking. `crates/visual-ocr-rapid` adapts the verified PP-OCRv6 model. The
desktop worker drains to the newest frame before inference and rejects a result
that is materially stale after a broad scene change.

Recognized regions reach `VisualTranslationController`, which prioritizes and
coalesces current text, then publishes one replaceable-latest presentation
frame. The native presentation adapter validates the active session before the
visual overlay can render it.

## Translation, queues, and cancellation

Translation inventory and inference are separate:

- native `ModelManager` owns new translation artifacts;
- a private `prollyglot-model` protocol exposes only verified, manifest-listed
  files in ranges capped at 4 MiB;
- the control Web Worker inspects legacy cache and model metadata; and
- one disposable inference Web Worker loads at most one translator for the
  active session.

Translation jobs carry request ID, session ID, source revision, workload,
priority, enqueue time, coalescing key, and deadline. The queue holds at most 16
jobs. New partial text replaces older partial text for the same utterance;
visual work replaces older text for the same track. Final captions outrank
provisional captions. Deadlines are 2.5 seconds for live captions, 5 seconds for
final captions, 3.5 seconds for compact visual routes, and 8 seconds for the
universal visual route. Preparation has a separate 120-second ceiling.

Other bounded boundaries include:

- 128 audio frames between capture and transcription, with one-step stale
  backlog recovery;
- 12 capture events during audio startup/session forwarding;
- one pending visual frame; and
- one in-flight plus one replaceable-latest overlay publish.

No queue is allowed to grow in proportion to playback duration.

## Inference resource ownership

`crates/resource-coordinator` supplies RAII leases for three runtime kinds:
speech, visual OCR, and translation. Resources from the same active session may
coexist where required (for example speech plus translation); another session
or mode is rejected.

Speech and OCR leases move into their native inference workers and drop when
those workers end. Translation load/unload telemetry is serialized by the main
WebView and includes both native session identity and disposable worker owner
identity, preventing a stale unload from evicting a replacement. Terminal
session cleanup force-releases any remaining record.

Logs record model ID, cold-start time, resident process memory, session, and
resource kind. They do not record media text or samples. The coordinator is an
ownership and diagnostics boundary; it is not evidence that the three current
ONNX runtime stacks can be replaced safely by one runtime.

## Contracts and host bridge

Rust contracts and IPC names are defined in `crates/application-runtime`.
`export-runtime-bindings` generates
`apps/desktop/src/generated/runtime.ts`. Do not edit that file by hand.

The frontend depends on `DesktopBridge`, with separate native and browser
preview implementations. Feature code should not import Tauri APIs directly or
branch on preview/native behavior. When a public contract changes:

1. change the Rust contract and its JSON round-trip test;
2. update the native command/event adapter;
3. regenerate TypeScript bindings;
4. update both bridge implementations and controller tests; and
5. advance the runtime contract version only when an older frontend cannot
   safely consume the new native shape.

## Configuration and model storage

Configuration lives under Tauri's local application-data directory as immutable
revisions plus a retained last-good fallback. Writes validate schema and
expected revision before atomic publication. The main and Appearance WebViews
may request changes; overlay and selector WebViews cannot.

Models live separately under the application-data model root. Every artifact is
declared by a versioned manifest under `assets/model-manifests`, validated for a
safe relative path, exact byte size, SHA-256, provenance, and license. A verified
metadata marker avoids hashing unchanged large files on every launch. Inventory
does not imply runtime loading.

## Privacy and security invariants

- Raw audio and screen frames are bounded, transient, and not persisted.
- Captions, OCR text, and translations are absent from diagnostic logs.
- No account, cloud inference, virtual driver, injected hook, or application
  plugin is required.
- Capture uses documented OS paths and attempts protected and unprotected
  sources uniformly; it does not weaken protected-media controls.
- Model downloads require explicit user action and become visible only after
  complete verification and atomic publication.
- Only the main presentation controller may publish overlay frames.
- Public application identities contain no PID or private executable path.

These invariants are architectural requirements, not optional implementation
details.
