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
| S. Structural integrity program | Replace patched orchestration with supervised sessions, bounded translation and presentation work, generated contracts, maintainable desktop pages, unified local state, and portable capture boundaries | In progress; S1 runtime foundation and production audio cutover integrated, visual/bootstrap cutover next |
| 1. Windows capture foundation | A real Windows desktop shell can enumerate and capture either a selected output device or selected application | Selected-device Windows smoke passed; application and lifecycle validation remain |
| 2. Live English captions | Captured audio becomes stable partial and final English captions locally | Device-to-caption and corrected UI/context re-smokes passed; accented/conversational model evidence and application/lifecycle validation remain |
| 3. Minimal customizable Windows app | The complete daily-use interface, overlay customization, transcript view, and controls work together | Pending |
| 4. Windows MVP release | A reliable installable Windows build is ready for outside testing | Pending |
| 5. Ubuntu port | The Windows-proven core runs on one supported Ubuntu LTS release through PipeWire | Pending |
| 6. Multilingual captions and translation | Downloadable language support, local translation, and dual captions are production-ready | 29 forced spoken languages, four compact language models, compact-to-English and 29-language many-to-many routes integrated; Windows quality, latency, and automatic-language constraints remain pending |
| 7. Visual text translation | A selected region, application window, or display becomes locally translated positioned text | Experimental WGC/OCR/positioned-overlay slice integrated; native Windows media, DPI, performance, and OBS/DXGI parity remain pending |

## Structural integrity program — next execution sequence

The capture, audio-processing, transcript, native model-verification, and visual
pipeline crates are worth preserving. The structural debt is concentrated in
application orchestration: session lifecycle is split across locks and UI flags,
translation uses one global serialized worker, overlays reconcile competing
event paths, native and TypeScript contracts are copied by hand, and automated
tests stop below the layer where recent regressions occurred.

This program is therefore an incremental replacement of the application spine,
not a rewrite of the working media engines. Execute S1 through S4 in order. S1
and S2 are prerequisites for adding more model families, simultaneous
audio/visual operation, or another major visual-translation feature. Narrow
correctness fixes may still land while the program is active, but they should
use the new boundary when that boundary already exists.

### Program coverage

| Review concern | Primary milestone | Completion evidence |
| --- | --- | --- |
| One translation worker serializes catalog, installation, loading, and all live inference without general cancellation | S2 | A never-resolving translation is cancelled within its deadline, current original output remains usable, and later work proceeds without restarting the application |
| Audio and visual lifecycle truth is split across native locks, status objects, UI booleans, generations, and detached workers | S1 | One supervisor owns legal transitions, startup can be cancelled, one Stop action is sufficient, and worker completion always reaches a terminal state |
| Raw and bilingual captions compete at the overlay; visual output can publish stale FIFO snapshots | S2 | Each overlay accepts one session-scoped, revisioned stream and rejects delayed results from an older revision or session |
| Rust/TypeScript IPC shapes, command names, event names, and errors are manually duplicated | S1 | TypeScript bindings are schema-derived, contract tests cover every public command/event payload, and failures preserve code/recoverability/action metadata |
| Automated checks miss Tauri orchestration and all TypeScript schedulers/controllers | S1 and S3 | Tauri-free runtime tests and fake-clock frontend tests run from the normal local check scripts; the Windows script also executes native desktop tests it can support |
| Translation artifacts use WebView storage and artifact-sized buffers while speech/OCR use the native streaming model manager | S2 | One native inventory owns all model kinds, downloads remain bounded-memory and atomic, and the translation runtime reads only verified local artifacts |
| `main.ts`, `bridge.ts`, and one generic dialog own too much UI and runtime behavior | S3 | Full-view destinations are persistent pages, production and preview bridges are separate, and feature controllers consume a shared application store |
| Concurrent startup snapshots and listener registration can apply older state after a newer event | S1 and S3 | Bootstrap and all runtime events carry monotonic revisions; the frontend ignores older state deterministically |
| Per-application capture exposes an ephemeral PID and the desktop calls the Windows backend directly | S4 | The UI selects a stable application identity through a capture-backend contract and an ordinary process restart can be re-resolved |
| Durable settings are split across unversioned WebView storage and native files; architecture/build/contribution documents are absent | S3 and S4 | A validated configuration schema migrates existing preferences, and the required repository documentation matches the implemented boundaries |

### Migration rules

- Keep `prollyglot-core`, the audio and visual pipelines, ASR and OCR adapters,
  transcript store, and native `ModelManager` operational throughout the work.
  Refactor their callers unless a failing contract test proves a lower-level
  defect.
- Use a strangler migration: add a new contract or coordinator beside the old
  route, move one complete vertical path, verify parity, and only then delete the
  old state or event path. Do not maintain two authorities after the cutover.
- Every asynchronous result carries the session and revision that created it.
  A consumer must reject stale results; clearing a JavaScript array or boolean is
  not cancellation.
- Keep at most one substantial translator loaded while audio and visual modes
  remain mutually exclusive. The new scheduler improves ownership and recovery;
  it does not silently expand the current resource promise.
- Preserve explicit model downloads, local-only inference, no raw-audio or frame
  persistence, and privacy-safe logs without caption or OCR text.
- Existing installed models and preferences receive a deliberate migration. A
  legacy translation cache may remain read-only during a transition, but it must
  not be silently destroyed or copied through an artifact-sized memory buffer.
- Keep each published integration point runnable. Commit and push coherent
  slices within a structural milestone, but do not publish placeholder crates,
  half-migrated event paths, or a UI that requires old and new state to agree.
- Run routine checks locally. Native Windows smokes occur at the milestone gates
  below and require notes only for failures or material measurements, not a
  screenshot or evidence bundle for every passing action.

## S1 — Typed runtime spine and supervised sessions

### Integrated outcome

Create one platform-neutral application runtime that owns session identity,
legal state transitions, cancellation, completion, revisions, and structured
failures. Tauri becomes an adapter around that runtime instead of the place
where lifecycle policy is assembled.

### Included work

- Add a Tauri-free runtime/coordinator crate. Its public state includes a
  `SessionId`, monotonic revision, active mode, source identity, lifecycle state,
  progress/health summary, and optional structured failure.
- Define one mutually exclusive supervisor for audio and visual sessions with
  explicit `Stopped`, `Starting`, `Running`, `Waiting`, `Stopping`, and `Failed`
  transitions. Illegal commands return a typed conflict rather than consulting
  several locks and booleans independently.
- Allocate cancellation before model loading begins. Stop during `Starting`
  cancels or invalidates model preparation; Stop during `Running` acknowledges
  promptly, initiates bounded background cleanup, and publishes completion.
- Route capture, transcription, OCR, and event-forwarder exit or panic through a
  supervised completion channel. A dead worker cannot leave the UI claiming that
  the session is still live.
- Define a structured application error contract containing a stable code,
  understandable message, recoverability, suggested action, and related session
  where applicable. Preserve existing `SpeechError` information and map capture,
  model, translation, configuration, and window errors into the same envelope.
- Adopt one schema-generation path for Rust-to-TypeScript payloads and record the
  choice. Generate command/event payload types and centralize command/event names;
  remove hand-copied production shapes only after parity tests pass.
- Add a versioned bootstrap snapshot. Register runtime listeners before fetching
  it, attach revisions to subsequent events, and reject any snapshot or event
  older than the state already applied.
- Move current audio orchestration first, then visual orchestration, through the
  supervisor. Preserve existing bounded media queues and native adapters.
- Extract state-transition, cancellation, stale-event, panic, and recovery tests
  so they run without Tauri, GTK/WebKit, WASAPI, WGC, or model downloads.

### Migration and rollback

The existing Tauri commands initially delegate to the supervisor while retaining
their current external names. Audio moves completely before visual moves. For
each mode, old `session`/`status` ownership is removed in the same integration
point that switches its final command and event. If the new runtime fails parity,
the adapter can be reverted without changing capture, ASR, OCR, or transcript
implementations.

### Acceptance boundary

- Start and Stop each produce one legal, monotonic transition sequence; rapid
  repeated clicks cannot create duplicate sessions or contradictory active flags.
- Starting a large ASR or OCR model can be cancelled without waiting for the
  model to finish loading before the UI acknowledges the request.
- Audio and visual Stop return control promptly while cleanup completes once in
  the background; no additional click is needed.
- Simulated capture loss, inference error, worker panic, and shutdown timeout end
  in a typed, recoverable state with one user-facing action.
- An event from an older session or revision cannot overwrite current state.
- Generated TypeScript contracts round-trip every public session command, event,
  selection, status, and error shape.
- The normal local check runs the new runtime tests, and the Windows check runs
  supported desktop orchestration tests in addition to the real native link.

### Progress — 2026-08-14

The first S1 integration point is present as the Tauri-free
`prollyglot-application-runtime` crate. It owns opaque session identity,
monotonic snapshots, mutually exclusive audio/visual transitions, cancellation
allocated before startup work, idempotent Stop requests, supervised worker
completion, and structured recovery errors. Deterministic Rust-derived
TypeScript contracts are checked by both local scripts.

The production audio path now delegates lifecycle authority to that supervisor.
Model preparation is invalidated by Stop, runtime resources clean up once in the
background, capture and transcription worker outcomes are supervised, and the
legacy `CaptureStatus` is only a compatibility projection. The visual path still
uses its previous state owner, and the frontend still needs the revision-aware
bootstrap cutover; S1 is not yet complete.

## S2 — Bounded translation, unified model storage, and authoritative overlays

### Integrated outcome

Make translation a session-scoped, cancellable inference service and make each
overlay consume one authoritative, revisioned presentation stream. Catalog and
download work remain responsive even if inference stalls.

### Included work

- Split translation control from translation inference. Model status,
  installation, verification, and removal no longer share the live inference
  queue.
- Move translation manifests and artifact lifecycle under the native
  `ModelManager`. Prove a cache-only, read-only path from verified native files to
  Transformers.js/ONNX Runtime in WebView2 before retiring the existing cache.
  Prefer a streaming Tauri custom protocol or equivalent bounded path; do not
  transfer a whole model artifact through command payloads.
- Keep a legacy translation cache readable during migration. New installs use
  the native store; an old pack is removed only after its native replacement is
  verified or the user explicitly removes it.
- Create one inference worker per active translation session. Terminating that
  worker is the hard cancellation boundary for a runtime call that does not
  cooperatively cancel. Stopping or replacing a session rejects all of its
  pending work without affecting model inventory.
- Introduce typed translation jobs with request ID, session ID, source revision,
  workload profile, priority, enqueue time, and deadline. Final caption work
  outranks provisional work; provisional text coalesces to the newest utterance;
  visual work coalesces to the current recognized snapshot.
- Give caption-live, caption-final, compact visual, and universal visual work
  separate bounded generation/deadline policies. A timeout restarts only the
  active inference session and records timing without source text.
- Replace raw-caption plus bilingual-caption races with one caption presentation
  frame. Original text is emitted immediately as the first revision; pending or
  completed translation updates the same caption identity.
- Add the same session/revision contract to visual overlay output. Replace the
  FIFO `publishSerial` chain with one in-flight publish and one replaceable latest
  value; native and overlay consumers reject older revisions.
- Centralize active inference ownership and document the three current runtime
  stacks: sherpa-onnx for speech, native ONNX Runtime for OCR, and WebAssembly ONNX
  Runtime for translation. Record load, unload, cold-start, memory, queue, and
  timeout telemetry without media text.
- Add deterministic tests using fast, slow, failing, and never-resolving fake
  translators plus delayed overlay publishers.

### Migration and rollback

First make the scheduler drive the existing verified WebView cache. Next prove
native-store loading for one compact route, then migrate every route and model
action. Only after all routes pass offline load, removal, repair, and rollback
checks should the old write path be deleted. Likewise, publish the new overlay
frame beside the old route only in a test adapter; production must cut over in
one integration point so two presentation authorities never remain active.

### Acceptance boundary

- A never-resolving translation reaches its deadline, clears or reports its
  pending state, and allows current work to continue without restarting the app.
- Model status, installation, and removal remain responsive during slow or hung
  inference.
- Translation downloads and verification use bounded memory and atomic files;
  no WebView allocation scales to the size of a model artifact.
- At most one substantial translator is loaded, and switching or stopping modes
  releases the previous inference session deterministically.
- Original captions remain immediate. Final translation is not trapped behind
  stale partials, and visual translation never exhaustively replays old scenes.
- Delayed caption or visual results from a stopped/replaced session never appear.
- Side-by-side, stacked, history, reading-time, fade, visual retention, and clear
  behavior all consume the same versioned presentation state and pass fake-clock
  tests.
- On Windows, one Stop click succeeds during normal work, slow inference, and a
  forced timeout. Compact routes meet the existing live target; universal-route
  latency is measured and reported separately rather than hidden.

## S3 — Maintainable desktop workspace and versioned configuration

### Integrated outcome

Rebuild the application shell around the stable runtime snapshot so full view is
a real desktop workspace, compact view remains a focused utility, and feature
controllers can be tested without a live native backend.

### Included work

- Introduce a small application store/reducer that consumes the versioned
  bootstrap snapshot and runtime events. Session state, model catalogs,
  transcript, translation state, visual state, navigation, and notices have one
  owner instead of module-level flags spread through `main.ts`.
- Split startup/bootstrap, captions, screen translation, transcript, models,
  appearance, settings, window controls, and navigation into feature modules with
  narrow inputs and actions. Keep vanilla TypeScript unless implementation
  evidence shows a framework would reduce rather than move complexity.
- Split the production Tauri bridge from the preview/test bridge behind one typed
  interface. Remove copied production model catalogs from the bridge; preview
  fixtures consume the same generated contracts and dedicated fixture builders.
- Replace the full-view generic `<dialog>` router with persistent page containers
  under the desktop navigation. Full-view Appearance, Transcript, Models,
  Settings, and Screen translation retain DOM state while inactive. Compact mode
  may continue using contained modal/dialog surfaces where appropriate.
- Divide the shared stylesheet into design tokens, shell/layout, feature, caption
  overlay, visual overlay, and utility-window layers so changes do not depend on
  one global selector file.
- Add one native, locally stored configuration document with a schema version,
  defaults, validation, atomic writes, and migrations. Move overlay appearance,
  caption mode, translation target, view mode, visual preferences, selected
  models, and future source defaults into that repository.
- Import existing valid `localStorage` and selected-model preferences once. Write
  durable state only after validation succeeds, broadcast the accepted snapshot
  to every WebView, and retain `localStorage` only for development preview or
  disposable view state.
- Add focused TypeScript tests with fake timers and injected bridges for the
  caption/visual controllers, store revision handling, translation scheduling,
  Stop/start interactions, settings migration, focus/scroll preservation, and
  full/compact navigation. Add a small rendered interaction layer for critical
  controls rather than an enormous snapshot suite.
- Add `pnpm test` and make it part of both local check scripts. Keep browser/native
  visual checks manual only where DOM simulation cannot prove the behavior.

### Migration and rollback

Move one full-view destination at a time into the persistent shell while compact
mode remains available as a fallback. Route all new pages through the store before
removing their old global variables. Configuration migration first reads and
validates old values without deleting them; legacy keys are removed only after a
successful atomic write and restart readback.

### Acceptance boundary

- Full-view navigation changes persistent pages without opening a modal, making
  the caption workspace inert, losing scroll/focus, or rebuilding active controls.
- Compact/full switching never changes the native session or transcript and
  leaves no trapped dialog.
- Startup cannot regress to an older snapshot after a newer event.
- Invalid or older settings migrate or fall back once with a clear diagnostic;
  they do not poison every launch. Every accepted setting reaches all relevant
  windows from one configuration snapshot.
- Native and preview bridges satisfy the same typed interface without copied
  production catalog data.
- The standard frontend test command covers timing and ordering failures that
  previously produced stuck translation, stale overlays, and ineffective Stop.
- Keyboard navigation, focus restoration, screen-reader semantics, high contrast,
  and large text remain intact through the new shell.

## S4 — Capture portability, resource hardening, and maintainership

### Integrated outcome

Finish the structural program with a Windows-proven capture boundary that can
support Ubuntu later, explicit inference-resource ownership, durable architecture
documentation, and one practical lifecycle soak.

### Included work

- Add an `AudioCaptureBackend` contract for capability reporting, source
  enumeration, selection resolution, session start, recovery events, and stop.
  The desktop runtime depends on that contract rather than calling the Windows
  crate directly.
- Replace the application selection contract's public PID with a stable opaque
  application identity and display metadata. Keep PID/process-tree resolution
  inside the Windows backend. Use executable identity, package/application
  identity, and current process roots as available without exposing private paths
  in logs.
- On ordinary application exit/restart, enter `Waiting`, re-enumerate with bounded
  backoff, re-resolve the stable identity, and resume the process-loopback session
  when safe. A genuinely unavailable or ambiguous application produces a typed
  recovery choice instead of silently binding another process.
- Preserve selected-device recovery behind the same backend contract and define
  the interface PipeWire will later implement. Do not implement or claim Ubuntu
  support in this structural milestone.
- Add an inference resource coordinator that records which speech, OCR, and
  translation runtime is loaded, enforces current audio/visual exclusivity,
  unloads inactive models, and supplies cold-start/RAM diagnostics. Evaluate
  runtime convergence only with measured packaging and compatibility evidence;
  do not replace working engines merely to reduce the runtime count.
- Create `ARCHITECTURE.md` describing process/WebView/thread ownership, session
  transitions, contracts, queues, cancellation, model storage, configuration,
  overlays, and platform adapters. Create `BUILDING.md` and `CONTRIBUTING.md`
  around the actual local-first workflow, Windows prerequisites, tests, vendored
  dependency policy, and direct-to-main policy while it remains active.
- Update the README, spec, build plan, smoke procedures, and troubleshooting
  guidance to match the implemented architecture rather than retaining historical
  implementation claims.
- Run a focused Windows lifecycle soak covering cancellation during large-model
  load, repeated start/stop, deliberately stalled translation, application exit
  and restart, default/pinned device changes, sleep/resume, display movement,
  corrupt/missing model recovery, offline launch, and bounded memory. Passing
  actions need no screenshots; record only failures and material timing/resource
  measurements.

### Migration and rollback

Implement the Windows backend adapter around existing capture functions before
changing application selection. Resolve both legacy PID selections and new stable
identities during one compatibility window, then migrate the UI and remove PID
from the shared contract. The resource coordinator initially observes existing
loads before it enforces ownership, allowing measurements to expose false
assumptions without interrupting sessions.

### Acceptance boundary

- Device and application capture still pass their existing Windows smokes through
  the backend contract with no direct Windows call in desktop orchestration.
- A selected application can exit and ordinarily restart under a new PID while
  the same user-facing session waits and resumes; ambiguous matches remain safe.
- Repeated switching and stopping leave no orphan worker, loaded inactive model,
  stale overlay, or unbounded queue/memory growth.
- The Windows soak completes without a crash or unrecoverable state, and its log
  contains session/revision/timing/resource evidence without audio, transcript,
  OCR text, screenshots, or frames.
- `ARCHITECTURE.md`, `BUILDING.md`, and `CONTRIBUTING.md` exist, agree with the
  code and spec, and make adding the later PipeWire backend understandable.
- Local formatting, linting, unit/controller tests, frontend build, Windows
  cross-checks, and the native Windows build/link pass through the documented
  scripts without adding per-push hosted CI.

## Structural program release and completion rules

### Planned published integration points

These are coherent commits or small commit groups, not approval pauses. Continue
from one to the next while the milestone direction remains clear.

| Milestone | Published integration point | Repository remains understandable because |
| --- | --- | --- |
| S1 | Runtime contracts, generated bindings, structured errors, and supervisor tests | The new spine is exercised independently while production still uses its existing adapters |
| S1 | Complete audio-session cutover | Device/application captions use one supervisor and the old audio state owner is removed |
| S1 | Complete visual-session and bootstrap-revision cutover | Both user-visible modes share lifecycle semantics and stale startup state is rejected |
| S2 | Session-scoped scheduler over the existing translation cache | Cancellation, priorities, and deadlines improve immediately without coupling the first change to storage migration |
| S2 | Native translation-store cutover | Every model kind has one inventory and legacy cache data remains deliberately recoverable during transition |
| S2 | Caption and visual presentation-protocol cutover | Each overlay has one versioned authority and old event routes are removed in the same integration |
| S3 | Application store plus separate native/preview bridges | Feature code stops depending on globals and copied production fixtures before the DOM shell moves |
| S3 | Persistent full workspace and configuration migration | The new page structure and durable settings land together with interaction and migration tests |
| S4 | Capture-backend and stable-application-identity cutover | Windows behavior remains complete behind the interface the later PipeWire adapter will implement |
| S4 | Resource enforcement, documentation, and Windows soak | The implemented architecture is documented and the full structural program receives native acceptance |

- This documentation-only planning commit does not change the application
  version. Each later user-visible integrated correction follows
  `docs/VERSIONING.md`: normally a patch increment, with a minor increment only
  when the integration adds a new compatibility or product promise.
- Each published integration updates the changelog, README version statement,
  Cargo/package versions where required, and passes `scripts/check-version.mjs`.
- S1–S4 are complete only at their integrated acceptance boundaries. Individual
  helper types, file moves, test harness setup, or design documents are work
  inside a milestone, not milestone completions.
- Milestones may contain several coherent commits and pushes. Use contract-first,
  vertical cutovers rather than one giant final commit, and do not leave an old
  and new authority active across a published boundary.
- Resume broad Milestone 3, 6, and 7 feature expansion after S2. Complete S3 and
  S4 before claiming the Windows MVP release boundary in Milestone 4.
- Deliberately deferred from this program: simultaneous audio and visual modes,
  new model families, DXGI implementation, the Ubuntu backend itself, GPU
  acceleration, cloud services, and a framework rewrite without evidence.

### Windows smoke checkpoint — 2026-08-10

The repository owner's first native-Windows run confirmed that **Everything I hear** receives audible audio from the selected playback device and produces local captions. It also exposed an unsolicited startup preview, unstable painting while partial captions changed, and Settings actions with no visible result. Those UI paths were corrected in the next integration point and require a focused re-smoke.

A follow-up run exposed an Appearance window trapped behind its always-on-top preview and captions that discarded quiet lead-in audio, finalized too aggressively around short pauses, replaced conversational context immediately, and held the last result too briefly. The correction hides the real overlay while Appearance is open, restores it when live captions continue, retains speech pre-roll and trailing context, and displays recent pause-bounded utterances on separate lines. This does not claim speaker identification.

The next owner re-smoke reported better captions and confirmed that Appearance, Transcript, and Settings all open and close. Short remarks and a slight Southern accent still produced unreliable text. Prollyglot now exposes three local streaming choices—Fast, Balanced, and Enhanced—with verified download/removal, persistent selection, and a broader LibriSpeech-plus-GigaSpeech Enhanced option. Local clean-reference timing confirms that all three are comfortably faster than real time, but representative Windows dialogue remains the accuracy gate.

An optional Nemotron 3.5 Streaming 0.6B integration now provides a higher-resource original-language trial for 28 forced languages plus automatic detection without changing the Fast default. The INT8 560 ms checkpoint downloads 650.6 MiB and uses roughly 950 MiB peak process memory in the current development-host benchmark. One Spanish publisher fixture spot-checked well after band-limited resampling; one English comparison did not beat the English-only choices, and Japanese/automatic detection did not clear an initial fixture check. This retires the integration risk but does not complete Milestone 6 or advertise production multilingual quality.

A subsequent owner test confirmed that Nemotron's selected language conditions recognition: Japanese audio was suppressed under the Spanish setting and produced Japanese text under the Japanese setting. The UI therefore keeps forced language as the accuracy-oriented path and describes Automatic as the mixed-language path. The same feedback requires the live transcript to open on and follow the newest entry while preserving deliberate scrollback, and establishes Japanese/Spanish-to-English as the first optional local translation slice.

That first translation slice integrated Japanese and Spanish direct-to-English q8 models with pinned revisions, per-file size/SHA-256 verification, persistent local cache, removal, and cache-only inference. Original captions render first, translation work is independently bounded, and failures leave the original overlay and transcript usable. Development-WebView runs loaded and translated real Japanese and Spanish text, and exercised removal/reinstallation. Native Windows media latency, sustained memory/CPU, and representative quality remain Milestone 6 gates rather than implied approvals.

Sustained Windows video playback then exposed a transcription-backlog failure: the capture session remained alive, but the inference queue reported that it fell behind and captions stopped advancing. The affected build did not persist that exact condition to its diagnostic log. Backlog handling now absorbs a larger normal Nemotron inference burst, drains stale queued audio as one recovery action, abandons an incomplete hypothesis without an expensive final decode, records drop and recovery counts, and clears the warning after the worker returns near the live edge. Native Windows playback remains the acceptance check for this correction.

The first Windows translation run then exposed two integration defects around that slice: the undecorated control windows had not been granted Tauri's explicit drag/minimize/maximize/close permissions, and a cold translator could still be loading when the normal caption hold expired. The title bars now use permitted native window operations with logged failures. Selecting translated output preloads the chosen translator, a visible pending state distinguishes work in progress from untranslated output, and the overlay defers its clear while that finalized caption is still being translated. The UI now states that installing a translator does not itself enable translated output. Cached real-model development-WebView translation and the pending-to-result overlay transition pass; native Windows title-bar behavior and media latency remain the owner checks.

Long Japanese media then exposed a second translation-latency and layout failure. Translation waited behind both Nemotron's continuous-utterance boundary and older queued captions, while a provisional source row could span the bilingual grid and displace an older result. Translation now begins from a coalesced live partial after a short throttle, finalized work stays higher priority, and Nemotron's pause-light safety boundary is four seconds as a fallback rather than the trigger for translation. A bounded newest-first queue drops stale work before it can keep the live display permanently behind. The overlay keeps each source/translation pair stable, wraps both side-by-side columns without ellipses, and offers zero to three complete smaller fading history pairs. Privacy-safe timing diagnostics report model load, translation inference, queue wait, and stale-skip counts without recording caption text.

The same owner run showed that more installed recognition models made the control window appear slower. The cause was a synchronous full SHA-256 pass over every installed artifact at every launch, including Nemotron's 650.6 MiB files. Catalog inspection now runs after the window opens and writes a verification marker following a successful full pass; unchanged artifacts use manifest, size, and modification metadata on later launches, while any mismatch falls back to full hashing. Existing models receive one background full verification after upgrading. Only the selected recognition model still incurs its actual runtime load when captions start.

The language/translation catalog is now expanded as one integrated pre-release slice. Chinese, French, Korean, and Bengali have smaller dedicated streaming downloads; Nemotron exposes its 28 supported forced languages with ready versus broad-coverage guidance; and the UI offers 29 spoken-language choices in total. Translation has a separate target selector, preferring compact Japanese/Spanish-to-English routes, then a compact multilingual-to-English route, with a larger optional M2M100 model for direct translation among all 29 choices. The rendered-app pass verified changing provisional translation before finalization, three complete wrapped side-by-side history pairs, a real compact French-to-English graph, and a real universal Japanese-to-Spanish graph after full artifact verification. Native Windows media, broader language quality, cold-load timing, and sustained resource use remain the gates.

The control app now has a full desktop workspace with persistent navigation and
a separately selectable compact utility, rather than presenting the growing
feature set as one narrow mobile-style form. Caption and screen-translation
setup use desktop-width grouped panels, while Transcript, Models, Appearance,
and Settings have stable destinations. The model manager puts all installed
packs in one collapsed inventory and uses purpose, language/route, and compatible
model selectors for explicit additions. Progress rerenders preserve selections,
disclosure state, scroll, and keyboard focus.

The next media run exposed an overlay ordering race: a direct raw-caption event
could arrive just before the structured bilingual payload and temporarily turn
the original language into a full-width replacement for its translation. The
overlay now retains the last complete bilingual frame across that gap. Appearance
also offers 6, 10, 15, or 30 seconds of post-speech reading time plus four fade
speeds; the default is 15 seconds and an 800ms fade, and a delayed translation
restarts the reading interval. Native Windows timing remains the owner check.

The first Milestone 7 vertical slice is now integrated behind a separate
**Translate Screen…** action. It enumerates explicit top-level windows and
displays, supports a full-display crop selected with a drag surface, captures
through Windows Graphics Capture, and sends transient BGRA frames through a
capacity-one latest-frame queue. The live source is sampled at up to 12 FPS,
while localized frame-change detection caps expensive OCR opportunities at four
per second and confirms static detections once before going idle. PP-OCRv6 Small
and profile-specific stabilization feed positioned translated labels that reuse
the local translation worker; original text remains visible at its source. The
default prominent-text profile filters low-confidence, tiny, wrong-script, and
common interface-noise results, with an explicit all-detected-text option for
small text. App and overlay surfaces remain available to normal screenshots.
The capture worker suppresses only bounded matches for translations currently
drawn by Prollyglot instead of using display affinity to hide overlay windows.
Stopping hides and clears the visual overlay immediately, then joins capture and
OCR work off the command path so a slow recognition call cannot hold the UI in
an active state.

The 30.4 MiB OCR pack is optional, explicitly downloaded, and fully verified;
actual model initialization and platform-neutral pipeline tests pass on the
development host. Native Windows capture, OCR usefulness on real media,
multi-monitor/DPI movement, resource use, blank-frame recovery, DXGI comparison,
and OBS display parity remain Milestone 7 gates.

The first moving-media owner check found that the initial visual slice did not
meet that gate. Full-frame OCR was inference-bound, a second pass delayed first
output, stacked sign lines became fragments such as `YCORE`, and every fragment
entered a serial translation backlog. Whole-display capture also elevated too
much unrelated browser and taskbar text. The correction bounds live OCR to a
1280-pixel longest side, uses up to four local inference threads, skips upright
text classification, limits detector candidates, groups adjacent and stacked
lines before filtering, and ranks only six prominent regions. Prominent mode now
publishes its first high-confidence pass, preloads translation alongside OCR,
and keeps only newest-snapshot pending work. The overlay reports scanning
immediately and retains newly disappeared labels for up to eight seconds, while
long-lived labels disappear immediately when absent. Full-view Appearance is
also now an embedded workspace page, and Stop visibly enters **Stopping…** on
the first click before native shutdown work begins. Local Rust, TypeScript,
rendered interaction, and Windows cross-target checks pass; the same Spanish
title/subtitle case still requires native owner re-testing before the correction
is accepted.

The follow-up moving-media run found two remaining live-path defects. Capture
status arrived frequently enough to rebuild the active Screen translation page
between pointer-down and pointer-up, so a physical Stop click often lost its
button before the click event could fire. A recognition pass also took roughly
six seconds in the owner's normal `tauri dev` loop because Rust image conversion
and OCR post-processing were still running at development profile optimization,
then published that old result over a newer scene. Status-only updates now patch
the four counters without replacing the Stop node, Stop cooperatively terminates
the active ONNX run, and capture status is throttled to two updates per second.
The worker drains to the newest frame before each pass and rejects results over
three seconds old only after a broad scene change, while retaining a slow result
for text that is still static or changed only in a small region. Targeted
development-profile optimization covers only the CPU-heavy visual OCR path so
`tauri dev` remains a useful performance loop without making ordinary UI crates
expensive to rebuild. Local model, pipeline, TypeScript, rendered one-click
Stop, and Windows cross-target checks pass; the same moving Spanish clip remains
the native acceptance check.

A following Japanese monitor and static-region run showed nonzero OCR passes
and recognized regions but no translated labels, isolating the defect after
capture and recognition. Visual output had been broadcast alongside a raw clear
event that the main and overlay WebViews handled independently; delivery order
could therefore clear a newer rescan result. Native code now caches and emits
the newest visual output directly to the overlay, the main translation
controller exclusively owns clear/rescan state, and status separates OCR
regions from overlay labels. This correction is published as pre-release
`0.1.1`; the same Japanese video and NHK page are the native acceptance cases.

The next NHK-page run confirmed that OCR regions now reach the overlay, then
exposed a separate translation-liveness failure: every visible region rendered
an immediate `Translating…` placeholder even though the local worker processes
one request at a time, and one non-returning inference left the entire snapshot
pending for minutes. Pre-release `0.1.2` ranks and caps the current regions,
shows pending state only for the active request, preloads translation before
capture, and suspends audio-caption translation while visual mode owns the
worker. Compact visual inference has a five-second watchdog and the optional
universal model a twelve-second watchdog; expiry restarts the worker and lets
later regions continue. Output generation is also scaled to source length, and
privacy-safe diagnostics distinguish queue wait, inference duration, remaining
regions, and timeout recovery. Rendered eight-region and forced-stall tests
pass. The real compact Japanese-to-English route loaded in 956 ms and then
translated eight NHK-like labels in 3.1 seconds on the development host, with
the slowest individual label at 605 ms. The Japanese NHK page remains the
native Windows acceptance case.

The first native-Windows start attempt exposed one shared interop defect rather
than three source-specific failures: the TypeScript selection union sent
`sourceId` and `displayId`, while the Rust tagged enum still required snake-case
field names. The native contract now explicitly serializes camel-case fields and
has regression coverage for application, display, and region payloads. The
region document is transparent with a light scrim so its target remains visible,
and control/appearance/selector windows are no longer deliberately hidden from
screenshots. The same owner run found Nemotron-to-English updates appearing to
stall for roughly its old 20-second continuous endpoint. The web translation
scheduler remains latest-text responsive under slow-request simulation; the ASR
adapter now independently enforces the documented four-second Nemotron boundary
and records live-request timing without caption text. A 20.4-second installed-
model Spanish probe completed at 0.26 real-time factor with 30 partial updates;
German media and simultaneous ASR/translator contention remain native checks.

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
- A model catalog and manager that downloads, verifies, selects, loads, unloads, and removes the Fast, Balanced, and Enhanced English streaming choices. The optional Nemotron multilingual trial shares this lifecycle without becoming a Milestone 2 dependency.
- License and provenance records for the runtime and model weights before either is distributed.
- Stable provisional versus committed transcript state with timestamps.
- End-to-end captions from both Windows capture modes to the overlay and transcript store.
- Internal benchmarks comparing all three English choices on accented and unaccented conversation, media, and noisy game/call samples.
- Useful errors for silence, unsupported capture, missing models, corrupt downloads, and insufficient memory.

### Acceptance boundary

Run the end-to-end procedure in [`docs/testing/WINDOWS_MILESTONE_2.md`](docs/testing/WINDOWS_MILESTONE_2.md) on the reference Windows 11 machine and retain the benchmark results.

- A new user can download and select an English model from inside the app and caption real Windows system or application audio without a cloud service.
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
- Overlay controls for font family, font size, weight, line height, text color, outline or shadow, background color and opacity, width, maximum lines, post-speech reading time, fade duration, alignment, screen position, monitor, and click-through.
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

The current integration slices are present for owner evaluation: four dedicated compact streaming models cover Chinese, French, Korean, and Bengali, while Nemotron exposes 28 forced languages and unconstrained automatic detection. Translation can target any other one of the 29 selectable languages through compact-to-English or optional many-to-many models. Original, translated, and bilingual output modes are wired through the overlay and transcript; live provisional translation is throttled independently from ASR, finalized work takes priority, and bilingual history retains complete wrapped pairs. These remain experimental pre-release surfaces. Allowed-language constraints, automatic per-segment language reporting, broader test material, representative translation benchmarks, native Windows latency/resource evidence, and production quality gates are still pending.

### Included outcome

- Downloadable model manifests for selected additional languages, with compact language-specific choices where compatible streaming models and clear licenses exist.
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

## Milestone 7 — Visual text translation

Add visual translation as a separate Windows-first media mode without turning
Prollyglot into a screen recorder or general visual assistant. The first slice
does not run simultaneously with audio captions; the contracts and resource
ownership must leave that end state possible.

The experimental direction is now represented by an integrated first slice:

- capture one explicitly selected top-level window or display through
  `Windows.Graphics.Capture`, cropping a user-drawn region after display capture;
- run pinned PP-OCRv6 Small through a bounded latest-frame queue and change gate
  instead of recognizing every video frame;
- stabilize identical text across frames, track its bounding box, and route new
  text through the existing local translation service;
- group nearby same-line and stacked fragments into one phrase, prioritize a
  bounded live-media result set, and replace stale pending translation work with
  the newest snapshot;
- render translated labels above or below their original source text in a
  separate click-through overlay; and
- keep the app screenshotable while filtering the bounded set of translations
  currently drawn by Prollyglot out of later OCR observations.

The documented DXGI Desktop Duplication backend and equivalent OBS Display
Capture remain compatibility comparisons, not implemented claims. A
`Windows.Media.Ocr` comparison also remains optional: ordinary unpackaged
development and MSI/NSIS installs cannot assume the package identity required
by that API, while PP-OCR provides one shared Windows/Linux direction.

### Included outcome

- A separate **Translate Screen…** action and mutually exclusive audio/visual
  session state.
- Region, selected-application-window, and selected-display visual sources.
- A visible **Switch to Monitor capture** recovery when window capture is blank
  remains pending; blank pixels must not be presented as proof of DRM, and
  monitor capture must not be promised to expose every protected surface.
- Transient GPU/CPU frame handling with no screenshots persisted by default.
- A replaceable OCR contract that returns text, confidence, language/script,
  and capture-space polygons without leaking Windows objects downstream.
- Pinned model provenance, explicit downloads, integrity checking, and no
  automatic OCR or translation download. A packaged `Windows.Media.Ocr`
  comparison remains a follow-up rather than a baseline dependency.
- Position-stable translated overlays that follow crop, DPI, monitor, and
  selected-window geometry changes.
- Privacy-safe timing and backlog diagnostics without captured pixels or OCR
  text.

### Acceptance boundary

- Representative Japanese and Spanish video subtitles, game UI, menus, and
  signs produce useful translated text without saving frames or using a cloud
  service.
- The Windows matrix records WGC window, WGC display, DXGI display, and
  equivalent OBS Display Capture behavior for representative media. OBS-only
  display success is a Prollyglot compatibility defect; an OS-protected blank
  surface is reported without attempting injection or capture-control bypass.
- Static text is not repeatedly recognized or translated, and scene changes
  cannot create an unbounded stale-work queue.
- On the reference Windows machine, a stable changed text region normally
  reaches the overlay within two seconds for a compact-to-English route;
  universal translation is measured separately and may have a higher profile.
- Positioned labels remain attached to their source regions through ordinary
  window movement, resize, DPI, and display changes without covering the source
  text by default.
- Starting visual translation while audio captions run offers to switch modes;
  it does not silently run both. Simultaneous mode remains disabled until a
  later resource and responsiveness gate passes.
- Protected or excluded frames fail clearly and remain within documented OS
  capture behavior; no capture-control bypass is introduced.

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
