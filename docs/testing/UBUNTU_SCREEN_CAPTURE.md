# Ubuntu screen-capture backend checks

Version 0.5.0 supports portal window/monitor capture and monitor-region drawing
in a native still preview. Matching X11/XWayland monitor geometry can anchor
translations; missing or unverified placement falls back to the movable reader.
Native Wayland anchoring, fresh GNOME, and real-media acceptance remain open.

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

## Backend milestone evidence (0.3.1)

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
  physical overlay coordinates. `visual_geometry` maps a region only when the
  portal's complete monitor rectangle matches exactly one GDK logical monitor,
  normalized buffer aspect agrees, and no partial compositor crop is present.
  Negative origins, integer/fractional capture scales, and already-normalized
  rotations are handled separately from desktop positioning.
- Region Start opens a native GTK/Cairo still preview of the authorized monitor.
  Mouse drawing and mnemonic-labeled pixel controls select a crop; Use region
  confirms it, while Cancel/Esc/Stop release the session. Raw preview pixels
  never enter the webview. A latest-frame adapter crops before OCR and rejects
  capture size/crop/transform changes, including changes with the same pixel
  dimensions. Sparse-source terminal events need no new frame to be handled.
- Anchors use GTK logical coordinates on X11/XWayland only. The transparent,
  input-transparent, non-focusable window requests its monitor/region rectangle
  and remains invisible until its actual size/position verifies. A bounded
  periodic check detects layout, scale, and placement changes and restores the
  reader. Shared windows and native Wayland use the reader. Compositor stacking
  and physical fractional scaling still require owner acceptance.
- The native host owns the `anchored` presentation flag. The frontend cannot
  use that field to choose desktop coordinates. Placement fallback updates the
  existing presentation without inventing a translation revision.
- Start/Stop run under desktop session supervision. A blocking startup worker
  polls the cancellation token while the portal waits for selection; dismissal
  returns to stopped. Source loss/revocation hides and clears presentation and
  requires an explicit new selection. Closing the reader invokes Stop.
- Sparse sources may retain one still-valid native frame while OCR finishes a
  regional scan. A separate sample clock advances gating and confirmation;
  capture timestamps and received-frame counts are not fabricated. Stop,
  disconnection, and source loss disable resampling.
- Reader feedback suppression requires a current/recent translation match and
  enough matching opaque-background pixels at the OCR observation. It never
  masks on words or background alone. Color conversion and matching scene
  backgrounds can still defeat this heuristic; prefer sharing only the media
  window or keeping Prollyglot outside a shared monitor.

## Desktop integration fixtures

The browser fixture runs the actual source controls and reader against fake
services. With an existing Playwright installation and a local Vite dev server:

```bash
PROLLYGLOT_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs \
node scripts/check-portal-ui.mjs http://localhost:1420
```

It checks portal window/monitor/region selection without enumerated IDs, an
inherited region preference, Stop while startup is pending, small-window
wrapping/scrolling, native placement changes without new translation content,
and stale presentation rejection. Chromium is always
headless; `PROLLYGLOT_CHROMIUM` can select a local browser executable.

The native fixture additionally needs `Xvfb`, `tauri-driver`, `WebKitWebDriver`,
`libX11`/`libXtst`,
an installed OCR pack, and the synthetic images above:

```bash
PROLLYGLOT_VISUAL_OCR_MODEL_DIR=/path/to/installed/ppocrv6-pack \
PROLLYGLOT_VISUAL_FIXTURE_DIR=/path/to/synthetic/visual-fixtures \
bash scripts/check-portal-desktop.sh
```

Compilation and frontend bundling happen before private services start. The
fixture registers a fake portal on its private bus, produces synthetic video,
and launches the actual GTK app on a high-numbered Xvfb display. It removes
inherited display/playback variables, uses Linux's abstract X socket instead
of changing WSLg's socket directory, and keeps app data/model copies private.
Native processes and their child process groups are cleaned up afterward.
No owner desktop, real sharing picker, hardware routing, or recording is used.

The native cases cover Chinese/window and Spanish/monitor OCR, presentation IPC
and reader output, reader Stop/close and restart, late output after Stop, pending-picker
cancellation, and picker dismissal. Region cases exercise an actual GTK drag and
keyboard coordinate entry, crop-only OCR, Cancel/Esc/Stop during selection, and
monitor/region anchors on the fixture display. A native displacement verifies
reader fallback. `private_x11.py` accepts only the runner's own high-numbered
Xvfb process, uses explicit display handles, and performs no image capture.
Translation text is injected as a fixed
fixture through the normal native presentation contract; this test does not
measure a translation model's quality, runtime, or real-media usefulness.


## Region and anchor validation (0.5.0)

On 2026-09-08, on the Ubuntu 26.04/WSL2 development host:

- 169 Rust tests and 58 frontend tests passed; 11 opt-in tests remain ignored
  in the ordinary suite. Formatting, Clippy, generated bindings, version checks,
  frontend production bundling, and Windows desktop MSVC cross-compilation passed.
- All four private portal/backend tests passed, including real OCR of the six
  synthetic Chinese/Spanish images. Headless browser checks passed for portal
  region selection, placement fallback without new translation content, stale
  revisions, and existing Windows visual controls/overlay placement.
- All nine native scenarios passed against the **extracted final `.deb`**:
  Chinese window and Spanish display readers; Chinese region anchors at 1× and
  simulated 2× GTK scaling; Spanish display anchors; Stop/Esc during the region
  preview; and pending-picker cancellation/dismissal. The region fixture uses
  real pointer dragging and keyboard coordinates, including confirming an entry
  before leaving its field. Text outside the crop does not reach OCR.
- Native X input-shape and focus hints verify that anchors pass input through
  and do not accept keyboard focus. This caught and fixed the toolkit's initial
  draw restoring focus. Actual native displacement switches to the reader;
  reader input/focus behavior and a subsequent anchored restart also pass.
- Built `target/release/bundle/deb/Prollyglot_0.5.0_amd64.deb` (33,469,924 bytes).
  All extracted native library dependencies resolved. No install, upgrade,
  owner desktop manipulation, or recording was performed.

These use a fake portal and synthetic video, with deterministic presentation
text. They do not establish real translation latency/quality, physical mixed-DPI
behavior, compositor transparency/fullscreen stacking, or fresh GNOME/Wayland
acceptance. Native Wayland anchoring remains unimplemented; the reader is the
fallback. Follow the short [owner check](UBUNTU_SMOKE_TEST.md#owner-screen-translation-check).

## Integration validation (0.4.0)

On the same Ubuntu 26.04/WSL2 development host:

- 166 Rust tests and 58 frontend tests passed, with 11 opt-in Rust tests ignored
  in the ordinary suite. Formatting, Clippy, generated bindings, Windows desktop
  cross-compilation, and frontend bundling passed. A new sparse-image regression
  confirms OCR completes pending work without inventing capture evidence.
- All four private portal/backend tests passed, including real OCR of the six
  synthetic Chinese/Spanish images. Their timings remain diagnostic observations,
  not translation or real-media acceptance.
- The headless portal UI fixture passed for source selection, cancellable pending
  startup, reader wrapping/scrolling, and stale presentation rejection. The
  existing headless Windows source-selection regression also passed.
- The native virtual-display fixture passed Chinese/window and Spanish/monitor
  capture through OCR and reader presentation, reader Stop/close, restart after
  close, cancellation and dismissal. Translation output is deterministic
  fixture text, as described above.
- Built `target/release/bundle/deb/Prollyglot_0.4.0_amd64.deb` (33,418,940 bytes).
  All extracted native libraries resolved, and that extracted package binary
  passed the four native desktop cases above. This was extraction and private
  execution, not a fresh GNOME install/upgrade/remove acceptance check.

Fresh GNOME permission dialogs, actual compositor video formats, fractional
scaling, fullscreen behavior, and real Chinese/Spanish translation usefulness
remain owner acceptance. See the short [manual check](UBUNTU_SMOKE_TEST.md#owner-screen-translation-check).

## Implementation references

Geometry and placement follow [GDK logical monitor geometry](https://docs.gtk.org/gdk3/method.Monitor.get_geometry.html)
and [GTK window positioning](https://docs.gtk.org/gtk3/method.Window.move.html).
Window-manager requests are verified, not treated as guaranteed placement.

The backend follows the [XDG ScreenCast contract](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html),
[Request lifecycle](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html),
and [PipeWire stream examples](https://docs.pipewire.org/video-src_8c-example.html).
The direct `zbus` wrapper keeps request handles available for cancellation and
reads the v6 serial metadata. Its dependencies are Linux-only; PipeWire uses the
same Rust/native library line as application audio. Models and image decoding
are test-only dependencies of this capture crate.
