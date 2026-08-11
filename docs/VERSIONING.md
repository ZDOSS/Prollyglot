# Prollyglot versioning

Prollyglot uses Semantic Versioning. Versions below `1.0.0` are pre-release
development builds and may change behavior or local data formats between
minor releases.

The current line uses `0.minor.patch`:

- increment **patch** for a substantial integrated correction or refinement;
- increment **minor** when a new product milestone or compatibility promise is
  introduced; and
- reserve `1.0.0` for a supported release whose documented Windows acceptance
  boundaries have been met.

`[workspace.package].version` in `Cargo.toml` is the native source of truth.
`apps/desktop/package.json` must match it, the Tauri bundle inherits it, and
Vite injects the package version into the interface. The desktop `build` script
runs `scripts/check-version.mjs`, which also requires a matching changelog entry
and current-version statement in the README.

For every published version:

1. choose the SemVer increment appropriate to the integrated change;
2. update `Cargo.toml`, `apps/desktop/package.json`, `CHANGELOG.md`, and the
   current pre-release statement in `README.md`;
3. run the local version check and the checks relevant to the change;
4. commit and push the coherent milestone to `main`; and
5. create a Git tag only for an intentionally distributed build, not for every
   development commit.

Documentation-only corrections and internal checkpoints do not require a
version increment unless they change a published promise or release artifact.
