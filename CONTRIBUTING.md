# Contributing to Prollyglot

Prollyglot is an early, local-first desktop project. Windows 11 is the primary
development and first release target. The repository is not yet accepting a
platform promise for Ubuntu, even though shared code is deliberately kept
portable for a later PipeWire backend.

Before changing product behavior, read:

1. [Prollyglot.md](Prollyglot.md) for the product contract;
2. [BUILD_PLAN.md](BUILD_PLAN.md) for delivery order and acceptance evidence;
3. [ARCHITECTURE.md](ARCHITECTURE.md) for runtime ownership and boundaries; and
4. [BUILDING.md](BUILDING.md) for setup, checks, and troubleshooting.

## Product boundaries

Keep contributions centered on a focused subtitle and visible-text translation
utility:

- Capture audio rendered through one selected playback device or one selected
  Windows application using documented operating-system paths.
- Keep capture, recognition, OCR, and translation local by default. Do not add
  an account, cloud dependency, virtual audio driver, injected hook, or
  application plug-in as a normal requirement.
- Treat protected and ordinary sources uniformly. Capture PCM or pixels exposed
  by the documented OS path, but do not weaken or bypass protected-media
  controls.
- Never record raw audio or screen frames by default. Do not write caption, OCR,
  or translation text to diagnostic logs.
- Preserve source text beside a translation when both are requested. Do not
  imply that a model guarantees a perfect translation.
- Keep the primary Start/Stop path minimal, keyboard-operable, scalable, and
  high contrast. Put advanced choices in the persistent desktop workspaces.

Meeting-assistant features, general productivity features, and unrelated AI
features are outside the current scope.

## Temporary repository workflow

The owner is temporarily developing directly on `main`. Do not create a branch
or pull request unless the owner asks for one.

At the start of a change:

```bash
git status -sb
git pull --ff-only origin main
```

Only pull when the worktree is clean. Existing changes belong to their author;
do not discard, rewrite, or sweep unrelated files into a commit. Stage only the
files that implement the current integration point.

Commit substantial, coherent states rather than tiny checkpoints or broken
placeholders. Use short imperative subjects, commonly with `feat:`, `fix:`,
`refactor:`, `test:`, `docs:`, or `build:`. Push each substantial milestone
commit to `origin/main`. Never force-push or rewrite published history.

This policy is intentionally temporary. When outside contribution volume makes
review branches useful, this document and `AGENTS.md` should change together.

## Validation without hosted-CI churn

GitHub Actions minutes are a release resource, not the development loop. Run
the platform-appropriate local gate before publishing:

```powershell
# Native Windows, from the repository root
.\scripts\check-windows.ps1
```

```bash
# Ubuntu or WSL
./scripts/check-local.sh
```

Use the five-minute [Windows smoke test](docs/testing/WINDOWS_SMOKE_TEST.md) for
ordinary changes. Use the focused
[Windows lifecycle soak](docs/testing/WINDOWS_LIFECYCLE_SOAK.md) for changes to
capture recovery, supervision, inference ownership, or shutdown. Passing smoke
actions require no screenshots, recordings, fixtures, or evidence bundle.

Cross-compilation is useful but does not prove physical Windows audio routing,
application isolation, overlay behavior, display geometry, or latency. State
clearly when a native check could not be run.

## Contracts and architecture

Keep platform behavior behind narrow adapters. Shared orchestration must not
depend on a PID, Windows device object, PipeWire node ID, WebView storage
implementation, or a specific inference runtime.

`apps/desktop/src/generated/runtime.ts` is generated from Rust. When changing a
public runtime payload, command, or event:

1. update the Rust contract and round-trip tests;
2. update the native adapter;
3. regenerate the TypeScript bindings;
4. update native and preview bridge implementations plus controller tests; and
5. run the generated-binding check documented in [BUILDING.md](BUILDING.md).

Do not add a second lifecycle authority, unbounded queue, or alternate overlay
event route. Each asynchronous result must retain the session and revision that
created it so stale work can be rejected.

## Dependencies, models, and licenses

Add a dependency only when its value justifies binary size, cold-start cost,
licensing, packaging complexity, and maintenance. Prefer an existing workspace
runtime when it satisfies the requirement, but do not merge inference engines
without measured compatibility evidence.

Every downloadable model must have:

- an explicit, versioned manifest;
- upstream provenance and a pinned revision;
- an exact size and SHA-256 digest;
- a compatible license recorded under `docs/licenses`; and
- bounded download, verification, and atomic publication behavior.

Never commit model weights, partially downloaded artifacts, private media,
transcripts, or captured test data.

## Versioning and documentation

Follow [docs/VERSIONING.md](docs/VERSIONING.md). A substantial user-visible
integration or fix receives the appropriate pre-release patch or minor bump.
Keep `Cargo.toml`, `apps/desktop/package.json`, `CHANGELOG.md`, and the README
version statement synchronized, then run:

```bash
node scripts/check-version.mjs
```

Documentation-only changes and internal checkpoints do not require a version
bump. Update `Prollyglot.md` when an implementation discovery changes product
scope or a platform promise, and update `ARCHITECTURE.md` when ownership or a
major data path changes.

## Reporting a problem

A useful report states what you did, what appeared, what you expected, the
selected source/model/languages, and whether one retry recovered. Exact error
text is helpful. A screenshot, recording, or log is optional and should be
requested only when it helps diagnose that failure.

Prollyglot's rolling log is intended to contain lifecycle, timing, resource, and
recovery metadata—not media content. If a log ever contains spoken, recognized,
or translated text, treat that as a privacy defect.
