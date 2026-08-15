# Changelog

Notable changes to Prollyglot are recorded here. The project follows Semantic
Versioning while it is in the `0.x` pre-release line.

## [Unreleased]

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
