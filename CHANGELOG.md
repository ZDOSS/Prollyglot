# Changelog

Notable changes to Prollyglot are recorded here. The project follows Semantic
Versioning while it is in the `0.x` pre-release line.

## [Unreleased]

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
