# Prollyglot vendor notes

This directory is the published
[`rapidocr-core` 0.2.2 crate](https://crates.io/crates/rapidocr-core/0.2.2),
licensed Apache-2.0. The registry archive SHA-256 is
`2afdaea55d9e8daf8f547a48a7fb45a43dbe076db3b9489c34386521cbdac294`.
Its `.cargo_vcs_info.json` records upstream repository
<https://github.com/White-NX/rapidocr-rs>, path `crates/rapidocr-core`, at commit
`bc4afd4a3fc5cb65f0358c902241d547e4775274`.

Prollyglot carries the source because the published crate forces the default
`ort` features even when its own default features are disabled. The local
manifest makes only these packaging changes:

- pin ONNX Runtime bindings to `2.0.0-rc.13`;
- use Rustls for the build-time ONNX Runtime download instead of native TLS;
- retain the pinned runtime download/copy behavior needed by desktop bundles;
- disable unused image codec defaults; and
- keep RapidOCR's own model downloader disabled in Prollyglot, because model
  installation goes through Prollyglot's atomic, size- and SHA-256-verified
  model manager.

The Rust source is otherwise unchanged from the upstream crate release.
