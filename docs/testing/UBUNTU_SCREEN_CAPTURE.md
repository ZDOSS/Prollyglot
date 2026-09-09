# Ubuntu screen-capture backend checks

`crates/visual-pipewire` is an implemented capture subsystem. The desktop app
still exposes screen translation only on Windows. Linux picker/region controls,
presentation and compositor coordinate mapping must be integrated before an
Ubuntu screen-translation build is ready for manual acceptance.

## Run without using the desktop

```bash
bash scripts/check-screen-capture.sh
```

The runner compiles first, then creates a private DBus socket, PipeWire server,
and WirePlumber policy session under a temporary directory. It removes inherited
`DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`, and `PULSE_SERVER`. Tests register a
fake ScreenCast portal only on that private bus and generate synthetic native
video in memory. No windows, media players, real portal picker, microphone,
playback devices, recording, or desktop automation are involved. Processes and
temporary service state are cleaned up afterward. The native tests refuse to
run unless the private bus/runtime guard matches.

Ordinary `cargo test --locked -p prollyglot-visual-pipewire` runs pixel-conversion
unit tests and leaves the private-service tests ignored. `check-local.sh`
includes this crate in the normal tests and Clippy checks.

To also test the installed PP-OCRv6 pack, reuse synthetic subtitle images made by
`scripts/check-visual-browser.mjs` and set both directories:

```bash
PROLLYGLOT_VISUAL_OCR_MODEL_DIR=/path/to/installed/ppocrv6-pack \
PROLLYGLOT_VISUAL_FIXTURE_DIR=/path/to/synthetic/visual-fixtures \
bash scripts/check-screen-capture.sh
```

The OCR fixture uses `zh-1920-24.png`, `zh-3840-24.png`,
`zh-1920-24-region.png`, and the corresponding `es-…` images. The fixture text
comes from [visual-subtitles.html](fixtures/visual-subtitles.html). Model files
are only read; this command does not install or download models. These images
are generated fixtures, not recordings of a user's display.

## Checks and evidence

On 2026-09-08, Ubuntu 26.04/WSL2 with PipeWire 1.6.2 and WirePlumber 0.5.13:

- Four pixel tests passed: BGRx/BGRA/RGBx/RGBA normalization, alpha/padding
  handling, wrapped chunks/crop extraction, every SPA rotation/reflection, and
  invalid dimensions/strides/crops rejected before allocating an output frame.
- Three private-service tests passed. They cover v4 node-ID and v6 serial
  selection, unrelated-source isolation, capacity-one newest-frame delivery,
  monotonic sequence/time, fixed/variable-rate formats, resolution renegotiation, missing/invalid portal
  results, cancellation/rejection, service disappearance, source replacement,
  and revocation. Stop is idempotent and checked against a two-second limit.
  Cancellation is exercised while each of CreateSession, SelectSources, and
  Start stalls its method reply or Response signal. Fast responses deliberately
  arrive before the method reply to catch subscription races.
- A fourth private test passed with the real OCR model: all six Chinese/Spanish
  fixtures were recognized after traversing the portal FD and native PipeWire
  video path. In one development run, first frame arrival was 72–126 ms for
  cropped/1080p fixtures and 409–451 ms for 4K; subsequent first OCR took
  41–176 ms. These are single-run diagnostic observations with preloaded OCR,
  automatic fake-picker acceptance, and synthetic input. They exclude model
  loading, human picker time, translation, overlay delivery, and real video.
- The local suite passed 161 Rust tests (10 opt-in tests ignored) and 58 frontend
  tests, Clippy, formatting, generated contracts, and the frontend production
  build. The existing two private audio-capture tests also passed with the
  isolated runner changes. MSVC cross-compilation checked the desktop and the
  new crate's non-Linux boundary; no native desktop application was launched.

The fake portal passes an FD to the private graph. It verifies the FD transfer
and consumer targeting, not a real compositor's permission implementation.
Fresh GNOME/Wayland, GPU buffer compatibility, physical scaling, and real-media
accuracy/latency remain owner-run acceptance. These tests are not substitutes.

## Capture and integration contract

- `start_capture` returns immediately with frame/event receivers and a session
  handle. The caller supplies Monitor or Window and an exported parent-window
  identifier. Selection always goes through the desktop picker; multiple
  sources and persisted permissions/restore tokens are disabled.
- Capture connects only to the remote FD returned by `OpenPipeWireRemote`.
  Version 6 serials are required and preferred over reusable node IDs. Older
  portals use the supplied node ID; source removal ends capture without fallback
  or reconnection to an unrelated node.
- Pixels use the shared non-serializable `VisualFrame`, with one pending frame.
  Metadata is bounded separately; terminal state is retained if its consumer
  stalls. Empty neutral buffers clear old pixel evidence; corrupted buffers are
  discarded. Crop/rotation are applied before OCR, with raw pixel geometry
  exposed separately. CPU-mappable packed RGB is required; DMA-BUF import,
  negative strides, interlaced and multi-view video remain unsupported.
- `StreamInfo` describes optional compositor **logical** coordinates. A monitor's
  logical size can differ from captured pixel dimensions; a window has no
  portable global position. `FrameGeometry` describes the raw, cropped and
  transformed pixels. Do not pass either directly to the current Windows
  physical overlay coordinates. Linux region selection and anchored presentation
  need their own mapping and compositor verification.
- Start/Stop remain under desktop session supervision when integrated. Every
  terminal path must clear presentation; a source picker must not block Stop.
  Source loss/revocation requires an explicit new selection.

## Implementation references

The backend follows the [XDG ScreenCast contract](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html),
[Request lifecycle](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html),
and [PipeWire stream examples](https://docs.pipewire.org/video-src_8c-example.html).
The direct `zbus` wrapper keeps request handles available for cancellation and
reads the v6 serial metadata. Its dependencies are Linux-only; PipeWire uses the
same Rust/native library line as application audio. Models and image decoding
are test-only dependencies of this capture crate.
