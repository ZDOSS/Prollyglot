# Changelog

Notable changes to Prollyglot are recorded here. The project follows Semantic
Versioning while it is in the `0.x` pre-release line.

## [Unreleased]

## [0.5.1] - 2026-09-08

- Parent the Ubuntu sharing picker to the main window on native Wayland using
  a session-owned GDK export. Bound and cancel export waits, fall back to an
  unparented picker when unavailable, and release exports after Stop or failed
  startup.
- Add isolated, software-rendered native Wayland reader and cancellation checks,
  including missing/stalled exporters and handle cleanup before app exit. Keep
  GNOME dialog stacking, anchoring, and real-media acceptance open.
- Update Ubuntu build and package descriptions for drawn regions and conditional
  anchors. Headless test tooling stays outside the shipped application.

## [0.5.0] - 2026-09-08

- Add Ubuntu monitor-region selection using a transient native preview, mouse
  drawing, accessible pixel-coordinate controls, and cancellable startup.
  Crop before OCR and require a new selection after capture geometry changes.
- Add X11/XWayland monitor and region translation anchors when portal logical
  geometry uniquely matches the desktop. Verify actual placement before showing
  labels, and fall back to the movable reader when positioning is unavailable
  or changes. Native Wayland and shared-window output remain reader-based.
- Keep captured pixels out of IPC and recordings. Extend isolated GTK/PipeWire
  fixtures for regions, anchoring, fallback, cancellation, and stale output.
  Fresh GNOME, physical scaling, fullscreen stacking, and real-media acceptance
  remain owner-run checks.

## [0.4.0] - 2026-09-08

### Added

- Experimental Ubuntu screen translation through the desktop's window/monitor
  sharing picker, restricted PipeWire capture, and shared local OCR/translation
  pipeline. A normal movable reader presents translations independently of
  compositor coordinates; source text remains visible in the original window.
- Cancellable picker startup, normal stopped state after dismissal, Stop on
  reader close, and rejection of late output after cleanup. Every Start asks
  for a new source; permissions and restore tokens are not saved.
- Bounded reader feedback filtering that checks rendered text and local
  background pixels, plus isolated portal, headless browser, and native virtual
  display fixtures. Drawn regions, anchored Wayland overlays, GPU-only video,
  and owner GNOME/real-media acceptance remain pending.

### Fixed

- Finish OCR scanning and text confirmation when an active portal supplies a
  static image without duplicate frames. Repeated samples preserve capture
  timestamps and do not inflate received-frame counts.
- Subscribe before fetching current visual presentation so a slow overlay or
  reader webview cannot miss its first output.

## [0.3.1] - 2026-09-08

### Fixed

- Preserve a missing selected window or display during visual-source refresh.
  Start stays disabled until that source returns or another source is explicitly
  chosen. Refresh no longer redirects screen capture to an unrelated source.
- Invalidate a drawn region when its display selection or refreshed pixel size
  changes, reject out-of-bounds regions, and recheck source availability after
  waiting for audio captions to stop.

### Development

- Implement the Ubuntu XDG ScreenCast/PipeWire capture backend with cancellable
  picker requests, restricted-remote capture, newest-frame delivery, CPU pixel
  conversion, and source/session-loss cleanup. Private headless tests cover the
  portal protocol, native video, and Chinese/Spanish OCR. Desktop screen
  translation remains Windows-only until Linux selection and overlay integration
  are complete; this backend does not change the current package's support scope.
- Isolate native test buses under their temporary runtime directories and remove
  inherited desktop display and playback environment variables from fixtures.

## [0.3.0] - 2026-09-08

### Added

- Experimental Ubuntu application audio capture through native PipeWire ports,
  preserving normal playback and mixing the selected application's streams on
  one graph clock, including streams routed to different outputs.
- Stable application grouping and recovery after stream recreation or player
  restart. Independent instances with the same identity pause capture until the
  selection becomes unambiguous.
- Private-graph checks for multi-stream isolation, synchronized mixing, original
  playback routes, process restart, ambiguity, and capture cleanup.

### Fixed

- Preserve an unavailable application or pinned playback device during source
  refresh on Windows and Ubuntu. Starting captions requires that source to be
  available or an explicit new selection; refresh no longer silently changes it
  to Everything I hear.

## [0.2.0] - 2026-09-08

### Added

- An experimental Ubuntu 26.04 LTS amd64 audio backend using native PipeWire
  output monitoring, with default-output following, pinned-output capture,
  bounded cancellation, and recovery after output recreation.
- Private-graph capture checks for source isolation, default changes, device
  removal/recreation, continuous timing, and Stop without changing desktop audio.
- Ubuntu desktop build and native Debian packaging with the shared speech/ONNX
  runtimes. Application audio selection and screen translation remain Windows
  features while their Linux implementations are pending.

### Changed

- Select the audio adapter by platform while reusing transcription, local
  translation, transcript storage, and session supervision.
- Enumerate audio sources off the desktop event thread.

### Fixed

- Realize hidden GTK overlay windows before applying click-through settings,
  preventing an Ubuntu startup crash.
- Keep audio-source connection failures visible after startup and skip screen
  source enumeration on platforms without screen-capture support.

## [0.1.15] - 2026-09-08

### Fixed

- Prevent a native heap-corruption crash when starting Windows application audio
  capture: the asynchronous activation variant now borrows its Rust-owned buffer
  without letting Windows free it.
- Refresh visual detection after a capture resize even when the sampled pixels
  match, and avoid retranslation caused only by OCR whitespace changes.
- Keep overlapping new text observations distinct within the same frame.

### Added

- An explicit native Windows audio fixture check for default/selected-device
  mixing, application isolation, exit/restart recovery, monotonic capture time,
  and bounded Stop. Only synthetic signal measurements are printed; captured
  audio stays in memory.

## [0.1.14] - 2026-09-08

### Fixed

- Preserve small full-display text with native-resolution contrast regions and
  overlapping detection tiles instead of shrinking every display to 1280 pixels.
- Bound each OCR pass, continue pending scans on current frames, and alternate
  known-text priority with fair discovery of the rest of a busy display.
- Reuse unchanged recognition only after checking its actual source pixels;
  invalidate changed text and release cached pixels at tracking reset or Stop.
- Keep readable 24-pixel subtitles eligible on 4K displays in Prominent text
  mode, and prevent a large false background detection from absorbing a subtitle.

### Added

- Chinese/Spanish OCR acceptance fixtures for corners, tile seams, short words,
  and textured backgrounds, with expected text and capture-space geometry.
- Native Windows fixture verification for window, display, and region capture,
  repeated Start/Stop, detection latency, and bounded capture shutdown.

## [0.1.13] - 2026-09-07

### Added

- Architecture, build, and contribution guides describing the implemented
  runtime boundaries, local-first workflow, Windows prerequisites, generated
  contracts, and temporary direct-to-main policy.
- A focused, no-screenshot Windows lifecycle soak and automated log auditor for
  supervised-session coverage, inference cleanup, privacy fields, and bounded
  post-stop resident-memory growth.

### Changed

- Log every accepted runtime revision and each session's post-cleanup resident
  memory so native lifecycle testing can be evaluated without recording media.
- Provide a development-only translation-delay switch for deterministic timeout
  and worker-recovery testing; production builds ignore it.
- Replace the ordered Windows lifecycle script with plain-language areas to
  exercise in any order, and judge repetition by comparable cleanup evidence
  instead of an arbitrary total session count.
- Make the native Windows validation script fail immediately when any Cargo or
  pnpm command returns a nonzero exit code, and include Windows audio-adapter
  tests in that gate.

- Recognize detected regions from original-resolution pixels and cap recognition
  crops before expensive inference (24 prominent / 48 all-text candidates).
- Add OCR stage timings, Chinese/Spanish static and timed fixtures, a local OCR
  evaluator, and repeatable browser checks. Small full-display text remains an
  observed detector limitation; Windows live-media acceptance is still pending.
- Advance the generated runtime contract to version 5 for source age and measured
  overlay geometry.

### Fixed

- Normalize checkout line endings when verifying generated runtime bindings so
  the same committed contract passes on Linux and Windows.
- Give the desktop library and executable distinct artifact names, removing the
  Windows PDB collision warning, and keep the visual session monitor within the
  current stable Clippy boundary.
- Exercise the resident-memory probe on Windows instead of allowing its desktop
  test to become an empty platform-gated pass.

- Give translator loading its own deadline, preserve visual priority in the queue,
  and retry current visual text up to three times with a bounded backoff.
- Suppress late translations after their source disappears; verify old OCR against
  its text areas and reject output from outdated capture dimensions.
- Detect thin subtitle changes between sampling points, periodically refresh static
  OCR, preserve short confident Spanish/Chinese text, and correct word order under
  vertical OCR-box jitter.
- Filter overlay echoes using measured label positions before grouping source lines.
  Clamp rendered labels to capture edges and keep narrow setup controls accessible.
- Preserve one audio timestamp/sequence across WASAPI reconnects and explicitly
  mark the first recovered packet as discontinuous.
- Limit OCR source choices to scripts the installed dictionary supports while
  preserving the broader translation target choices.

## [0.1.12] - 2026-08-14

### Added

- A session-scoped native inference-resource coordinator for speech, visual OCR,
  and WebView translation runtimes, including cold-start and process resident
  memory diagnostics.
- Generated resource ownership/status commands and deterministic native and
  frontend tests for overlap, forced cleanup, stale unloads, and report ordering.

### Changed

- Bind every heavyweight inference load to the active supervised audio or
  visual session and force-release any remaining ownership when that session
  reaches its terminal cleanup boundary.
- Serialize translation worker load/unload reports across the native bridge so
  a delayed event cannot evict a newer session's model.
- Include resource-coordinator tests in both local validation scripts.

### Fixed

- Stop preloading a translation model while captions are inactive.
- Delay visual translator preparation until its native visual session exists,
  and unload it automatically when that presentation session ends.

## [0.1.11] - 2026-08-14

### Added

- A platform-neutral audio-capture backend contract covering capabilities,
  source enumeration, selection resolution, session start, recovery events,
  and stop.
- Stable opaque Windows application identities derived from available package,
  application-model, or executable identity without exposing process IDs or
  private executable paths.
- Bounded application restart recovery that enters Waiting and reconnects to
  the same unambiguous application identity after its process tree changes.
- A WSL Windows-SDK resource-compiler adapter so the local suite can type-check
  the complete native Tauri desktop target without consuming hosted CI.

### Changed

- Route desktop audio orchestration exclusively through `AudioCaptureBackend`
  instead of invoking the Windows crate directly.
- Advance the generated runtime contract to version 4 and replace application
  `processId` fields with opaque `sourceId` values.

### Fixed

- Refuse to bind silently when multiple current process trees match the same
  application identity; the source picker explains that duplicate instances
  must be closed.
- Keep ordinary application exit/restart in a recoverable session rather than
  failing captions permanently when the replacement process receives a new PID.

## [0.1.10] - 2026-08-14

### Added

- Persistent full-view pages for Captions, Screen translation, Transcript,
  Models, Appearance, and Settings, with state-preserving navigation and focus
  restoration.
- Focused modules and deterministic tests for workspace navigation, caption
  setup, transcript following, runtime bootstrap ordering, and title-bar input.

### Changed

- Keep compact secondary tools in a contained dialog while full view navigates
  mounted desktop pages without reconstructing active controls.
- Split the desktop shell, feature controllers, and the monolithic stylesheet
  into maintainable feature and presentation layers.

### Fixed

- Prevent full/compact switching from leaving a trapped dialog or replacing the
  current native session and transcript state.
- Preserve a safe translation-target fallback when saved language preferences
  do not describe a valid local route.

## [0.1.9] - 2026-08-14

### Added

- A native schema-v1 configuration repository with validated defaults,
  immutable revision files, atomic publication, retained fallback revisions,
  corrupt-file quarantine, and version-zero migration.
- Generated TypeScript configuration types, command/event names, and defaults
  from the Rust contract, plus deterministic migration, stale-write, readback,
  recovery, and rapid-change coalescing tests.

### Changed

- Make one native configuration snapshot authoritative for full/compact mode,
  caption and translation choices, playback-device preference, visual setup,
  caption appearance, and selected models across every WebView.
- Move speech-model selection out of standalone preference files. Existing
  files are imported and removed only after the accepted native selection is
  read back successfully.
- Import valid legacy WebView settings once, discard invalid values with a
  diagnostic, and remove old keys only after native write and readback agree.
- Route Appearance, caption overlay, caption controls, and visual controls
  through the shared configuration controller; rapid changes are coalesced and
  stale writes rebase over concurrent native model updates.
- Include the configuration crate in both local validation scripts and advance
  the generated application runtime contract to version 3.

### Fixed

- Prevent separate WebViews and legacy storage keys from silently competing as
  durable settings authorities.
- Recover from an incomplete or corrupt newest configuration revision by using
  the retained last-good revision instead of poisoning later launches.

## [0.1.8] - 2026-08-14

### Added

- A reducer-backed application store that owns runtime, source, model,
  transcript, translation, visual, navigation, preference, and notice state.
- Deterministic store tests for stale bootstrap/runtime rejection, transcript
  revision ordering, feature-state independence, navigation, and subscriptions.
- Dedicated, generated-contract-compatible preview fixture builders whose
  fictional catalogs are isolated from the production model inventory.

### Changed

- Split desktop host access into one typed `DesktopBridge` contract with
  separate native Tauri and browser-preview implementations.
- Inject the host bridge into translation control so feature code can be tested
  without importing or branching on live Tauri commands.
- Route session projections, catalogs, transcript, visual state, navigation,
  preferences, and user notices through the application store instead of
  mutable module-level state in the desktop entry point.

### Fixed

- Prevent an older transcript snapshot from replacing a newer application-store
  revision, matching the existing monotonic runtime bootstrap behavior.

## [0.1.7] - 2026-08-14

### Added

- Generated, session-scoped caption and visual presentation contracts carrying
  runtime and presentation revisions, plus one stable event name per overlay.
- Fake-clock tests for caption reading/fade boundaries and cursor tests for
  duplicate, delayed-revision, and replaced-session presentation frames.
- Native validation that only the main WebView can publish presentation state
  for the currently active audio or visual session.

### Changed

- Publish original caption text, pending translation, completed translation,
  history, phase, and newest-readable time as one replaceable-latest frame.
- Derive final-caption hold and fade from the presentation timestamp, giving a
  delayed translation a fresh reading interval without a competing native timer.
- Route positioned visual labels through the same revisioned native boundary and
  revalidate their session at the overlay before painting.

### Fixed

- Remove the competing raw-caption event that could flash original text at full
  size, displace bilingual rows, or clear a newer translated caption.
- Prevent queued caption or visual output from a stopped or replaced session
  from repainting an overlay after its terminal clear.
- Clear and hide both overlays immediately from native Stop/failure handling so
  one Stop action does not depend on a delayed frontend publish.

## [0.1.6] - 2026-08-14

### Added

- Native manifests, inventory, background inspection, download progress,
  integrity verification, and removal for all four local translation packs.
- A private, main-window-only model protocol that exposes verified translation
  artifacts through bounded byte ranges instead of command payloads.
- Deterministic tests for native range reconstruction, missing artifacts, range
  limits, safe manifest paths, and the pinned native translation catalog.

### Changed

- Store new translation downloads beside speech and visual OCR models through
  the native `ModelManager`, using 64 KiB download buffers, verified sidecars,
  and atomic publication.
- Read native translation artifacts first while retaining old WebView packs as
  a read-only migration fallback. Settings identifies legacy packs and offers
  an explicit **Move to native storage** action.
- Let native model inventory complete desktop startup without waiting for the
  legacy-cache worker, and avoid probing model-sized artifacts before a native
  translator begins loading.

### Fixed

- Remove a legacy translation copy only after its native replacement is
  verified, while an explicit Remove action clears both stores.
- Replace a translation session invalidated by timeout or removal before the
  next caption attempts to use it.

## [0.1.5] - 2026-08-14

### Added

- A session-scoped translation scheduler with typed workload profiles,
  priorities, source revisions, coalescing keys, enqueue/deadline timestamps,
  bounded queues, and privacy-safe load/unload/queue/inference telemetry.
- Deterministic fast, slow, failing, never-resolving, replacement, and Stop
  tests for caption and visual translation scheduling.

### Changed

- Split translation inventory, download, verification, and removal into a
  lightweight control worker that remains responsive while a separate
  disposable worker owns live WebAssembly inference.
- Route final captions ahead of queued provisional work, coalesce changing live
  utterances and visual tracks to their newest text, and publish visual overlay
  changes through one in-flight plus one replaceable-latest update.
- Begin visual OCR capture while the selected translator prepares instead of
  keeping screen recognition idle behind translator cold-start time.

### Fixed

- Terminate and recreate only the active inference worker when a translation
  exceeds its workload deadline, allowing current work to continue without an
  application restart or a blocked model catalog.
- Reject delayed inference from stopped or replaced caption/visual translation
  sessions before it can update either output.

## [0.1.4] - 2026-08-14

### Added

- Rust-derived desktop contracts and centralized IPC names for audio and visual
  session commands, source selections, status projections, region selection,
  OCR updates, clears, bootstrap state, and structured failures.
- Browser-level runtime reducer tests in the normal local and Windows check
  scripts, covering contract mismatches, out-of-order snapshots, replacement
  sessions, and stale visual-result epochs.

### Changed

- Route visual-translation startup, capture/OCR workers, source loss, stopping,
  failure cleanup, and compatibility status through the same session supervisor
  already used by audio captions.
- Register the runtime listener before fetching a versioned bootstrap snapshot;
  the interface now applies only the newest monotonic revision and rejects
  delayed visual output from stopped, waiting, or replaced sessions.
- Treat the legacy audio and visual status events as UI compatibility
  projections instead of independent lifecycle authorities.

### Fixed

- Keep one visual Stop action available while the OCR model is loading, cancel
  late startup work, hide and clear the overlay immediately, and complete native
  cleanup once in the background.
- Surface visual source, region-selector, overlay, capture, model, worker, and
  shutdown failures with stable codes and recovery guidance instead of flattening
  session errors into strings.

## [0.1.3] - 2026-08-14

### Added

- A platform-neutral application runtime foundation with typed audio/visual
  session identity, legal lifecycle transitions, startup cancellation,
  idempotent stopping, supervised worker completion, and structured recovery
  errors.
- Deterministic Rust-derived TypeScript runtime contracts plus local and Windows
  checks that fail when the checked-in bindings are stale.

### Changed

- Route production audio-caption sessions through one supervisor that owns
  lifecycle, session identity, cancellation, health, and terminal failure state;
  the previous capture status is now only a compatibility projection.
- Supervise capture-event and transcription workers so unexpected exits, panics,
  and cleanup timeouts produce structured recovery guidance instead of leaving
  the interface claiming a dead session is live.

### Fixed

- Make one Stop click acknowledge immediately while model loading or capture is
  active, invalidate late startup work, hide the overlay, and complete bounded
  cleanup in the background.

## [0.1.2] - 2026-08-11

### Fixed

- Bound visual translation to the current highest-value OCR regions and show a
  pending label only for the region actually being translated instead of
  covering a dense source with indefinite `Translating…` placeholders.
- Preload the selected translator before visual capture starts, suspend stale
  audio-caption translation work while screen translation owns the worker, and
  prioritize shorter live labels so dense pages make visible progress.
- Restart a stalled local translator after a five-second compact-route
  inference deadline (twelve seconds for the optional universal model) so one
  problematic OCR region cannot freeze every later translation.
- Scale the translation generation budget to input length, preventing a short
  OCR label with a missed end token from consuming the full 192-token ceiling.

### Added

- Privacy-safe visual translation timing, queue-wait, remaining-work, and
  timeout diagnostics without logging recognized or translated text.

## [0.1.1] - 2026-08-11

### Fixed

- Deliver screen-translation state directly to the native overlay window and
  cache the newest output so window setup cannot lose recognized text.
- Make the main controller the sole owner of visual clear/rescan events,
  preventing a late broadcast clear from erasing newer translated labels.
- Reject delayed OCR only after a broad scene change and a three-second lag;
  cursor movement, controls, counters, and small text changes no longer clear a
  useful static result.

### Added

- Distinct OCR-region and overlay-label diagnostics for screen translation.
- A synchronized version check and documented pre-release bump policy.

## [0.1.0] - 2026-08-09

- Established the initial Windows-first pre-release baseline for local audio
  captions, optional translation, model management, transcript history,
  customizable overlays, and experimental visual text translation.
