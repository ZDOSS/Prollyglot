# Changelog

Notable changes to Prollyglot are recorded here. The project follows Semantic
Versioning while it is in the `0.x` pre-release line.

## [Unreleased]

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
