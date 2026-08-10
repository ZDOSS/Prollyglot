# Prollyglot agent guide

## Source of truth

- Read `Prollyglot.md` before planning or implementing product work.
- Keep the spec aligned with product decisions made during development.
- Use the final product name, **Prollyglot**, in code, documentation, packages, and UI.

## Current product boundaries

- Windows 11 is the primary development and first production target.
- The Windows MVP captures either everything rendered through one selected playback device or one selected application.
- “Everything I hear” means the mixed audio from the selected playback device, not all playback devices combined.
- Application-exclusion modes are exploratory and should not delay the two MVP capture modes.
- Linux work follows a reliable Windows MVP. Initially support one Ubuntu LTS release with PipeWire and a native `.deb`; treat other distributions as community-supported.
- Keep audio and transcription local by default. Do not require accounts, cloud processing, virtual audio devices, or application plugins.
- Use documented operating-system capture paths. Do not weaken or bypass DRM or protected-media controls.
- Protect the focused subtitle utility described by the spec; avoid unrelated meeting-assistant, productivity-suite, or general AI features.

## Repository workflow — temporary direct-to-main policy

Until the repository owner changes this policy:

- Work directly on `main`. Do not create feature branches or pull requests unless the user explicitly requests one.
- At the start of work, inspect `git status -sb` and preserve all existing user changes.
- If the worktree is clean, synchronize with `origin/main` using a fast-forward-only pull before editing.
- Stage only files that belong to the current task. Never sweep unrelated changes into a commit.
- Work continuously through the large milestones in `BUILD_PLAN.md`; do not stop for micro-approvals or treat every internal check as a handoff point when the direction remains clear.
- Commit at substantial, coherent integration points. A large build milestone may contain several commits, but avoid placeholder-only, broken, or trivial checkpoint commits.
- Each published integration point should leave the repository in an understandable state: a documented decision, a compiling vertical slice, a working subsystem, or a verified fix.
- Run the most relevant available checks before committing. Record any check that cannot be run and why.
- Treat GitHub Actions minutes as a scarce release resource, not the development inner loop. Run formatting, linting, unit tests, frontend builds, and cross-compilation locally whenever the host supports them.
- Do not add build-on-push matrices or per-commit hosted validation. If Windows-only hosted evidence or packaging is useful, consolidate it into a manually dispatched workflow used at substantial milestone or release boundaries.
- Use short imperative commit messages with a conventional prefix when useful, such as `docs:`, `feat:`, `fix:`, `test:`, or `build:`.
- Push every substantial milestone commit to `origin/main` immediately after it is created.
- Never force-push, rewrite published history, or discard local changes to resolve a rejected push. Fetch, inspect the divergence, and reconcile it safely.
- Report the commit hash, pushed branch, and validation performed at handoff.

## Implementation priorities

- Resolve the Windows technical risks first: selected-device WASAPI loopback, per-process capture, stable buffering, streaming transcription, and overlay reliability.
- Keep platform capture behind narrow interfaces so Linux can reuse the audio, ASR, transcript, and overlay-independent core later.
- Prefer integrated vertical slices that retire a major product risk. Small experiments may occur inside a milestone but should not become a stream of tiny published deliverables.
- Treat sustained silence, closed applications, device changes, and unavailable protected audio as normal runtime states that require clear recovery behavior.

## Change discipline

- Update the spec when implementation discoveries materially change product scope or platform promises.
- Add dependencies only when their value outweighs footprint, licensing, packaging, and maintenance costs.
- Preserve local-first privacy defaults and avoid persisting raw audio unless the user explicitly enables a future recording feature.
- Keep accessibility central: keyboard operation, scalable readable captions, high contrast, and predictable overlay controls are product requirements.
- Keep the interface minimal by default and customizable by choice. Advanced controls belong outside the primary Start/Stop path.
