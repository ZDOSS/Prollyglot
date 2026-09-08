# Visual reliability evaluation

Version 0.1.14 addresses the small full-display text missed by 0.1.13. The
adapter discovers contrast regions at native resolution, divides large busy
areas into overlapping tiles, and schedules bounded passes on current frames.
Unchanged recognition is reused only after pixel comparison, and changed text
is invalidated before a deferred rescan. The model and CPU provider are unchanged.

The 0.1.13 fixes remain covered: cold-load deadlines, retry starvation, queue
priority, change detection, short-text filtering, reading order, overlay echoes,
late translations, and the audio session clock through WASAPI recovery.

## Repeatable local checks

Run `bash scripts/check-local.sh` for shared tests, lint, Windows cross-check,
generated contracts, version consistency, TypeScript tests, and frontend build.
`scripts/check-windows.ps1` also executes native Windows tests and a real MSVC
build/link of the desktop application.

With the Vite development server running, reuse an existing Playwright installation:

```sh
node scripts/check-visual-browser.mjs http://localhost:1420 target/visual-evaluation
```

Set `PROLLYGLOT_PLAYWRIGHT_MODULE` to its `index.mjs` and `PROLLYGLOT_CHROMIUM` to
the browser executable when they are installed elsewhere. The Browser plugin
was unavailable in this environment, so verification used regular Playwright
and the existing Chromium installation. No browser dependency was added.

This checks controls at 1280, 420, and 400 pixels, source-language choices,
console errors, and overlay clamping at all edges down to 240 pixels. It writes
only explicitly generated synthetic fixtures: 1080p/4K Chinese and Spanish at
24/48 pixels, corresponding 980 × 180 crops, text eight pixels from display
edges, tile seams, short words, and a fully textured 4K background. `manifest.json`
records expected source text and geometry from the rendered DOM.

Point the local OCR evaluator at the already installed, verified model directory:

```sh
cargo run --locked -p prollyglot-visual-ocr-rapid --example evaluate -- --focused MODEL_DIRECTORY zh target/visual-evaluation/zh-3840-24.png
cargo run --locked -p prollyglot-visual-ocr-rapid --example evaluate -- MODEL_DIRECTORY es target/visual-evaluation/es-3840-24-textured.png
```

Omit `--focused` for All detected text. Both use the production adapter. When
fixture metadata is present, the evaluator fails on incorrect text or bounding
box intersection-over-union below 0.3. Each PNG receives three actual inference
runs, with the scan cache reset between runs. A separate unchanged-frame check
verifies cache equality and that new overlay filters still apply; a blank frame
must remove the previous text. These are model-backed assertions, not mock OCR.

JSON lines distinguish first scan, first correct text, maximum individual scan,
complete coverage, and cache reuse. Stage timings sum every scan in a run. The
first model use is cold inference; later runs are warm. Coverage scans here run
back-to-back on a static PNG: they exclude capture cadence, stabilization,
translation, and overlay delivery. Complete coverage can therefore take longer
in the live pipeline. The explicit fixture tool prints synthetic source text;
the application's diagnostics continue to contain only counts and timings.

## Native Windows capture verification

Run the following in native Windows PowerShell with the OCR pack installed:

```powershell
.\scripts\check-visual-windows.ps1 -ModelDirectory 'PATH\TO\OCR\MODEL\DIRECTORY'
```

The script builds `capture_check`, opens its own synthetic fullscreen fixture on
a secondary monitor when available, and closes that process after each language.
It does not change display settings, install models, or save captured pixels.
Escape closes the fixture; it also expires automatically after 90 seconds.

For each language it exercises selected-window, selected-display, and selected-
region WGC capture three times. Every session checks at least three frames with
increasing sequence/timestamps, expected recognized text, and capture shutdown
within two seconds. It uses the default Prominent text profile. The expected
text match ignores punctuation/case; this is source detection, not translation
accuracy. Timing starts before capture startup, after the OCR model is loaded.

The 2026-09-08 Windows 11 build 26200 run passed all 18 sessions on a 1920 × 1080
display, plus 980 × 180 region capture. First matching text arrived in
193–243 ms (median 214 ms), and Stop took 13–20 ms. The fixture's small animated
marker exercised frame delivery while its sentence remained static. This
verifies real WGC capture, OCR, and repeated capture shutdown; it does not prove
Tauri overlay delivery, moving-media accuracy, or a complete application soak.

## Evidence and remaining acceptance

All **192 inference passes** (32 fixtures × two profiles × three runs) passed
exact source text and geometry checks on 2026-09-08. Separate cache, overlay
filter, and disappearance assertions also passed. This includes the four
previously failing 4K cases, short “No”, “Sí”, “OK”, and “猫”, actual display edges,
and a textured background that forces all 16 detection tiles.

With no build checks competing for CPU, warm runs measured:

| Fixture group | First correct OCR result | Complete scan |
| --- | --- | --- |
| Sparse 1080p/4K full frames, corners, short words | 27–75 ms | Same pass |
| 980 × 180 crops | 49–64 ms | Same pass |
| Fully textured 4K, subtitle near lower center | 185–222 ms | 3.86–3.96 s |

These are ranges across a small synthetic set, not latency guarantees. The
largest individual warm scan was 305 ms; static full-frame cache checks took
about 0.5–5.8 ms. The textured case demonstrates early useful output while
coverage continues, and does not imply every text position receives that timing.

The local check script passed shared Rust tests, 58 TypeScript tests, Clippy,
formatting, generated contracts, Windows cross-checks, and the frontend build.
The native Windows gate passed its Rust/desktop tests, the same frontend tests,
workspace Clippy, and an actual MSVC desktop build/link. Four opt-in download/model
tests remain excluded from those default suites; the OCR model itself was
exercised by the explicit evaluations above.

[The 0.1.14 results](results/visual-ocr-0.1.14.json) record the synthetic corpus
and native capture runs. The earlier [0.1.13 baseline](results/visual-ocr-0.1.13.json)
missed all four 4K full-frame cases while recognizing all eight crops and the
four full-frame 1080p cases. Its serial warm OCR medians were 79–119 ms for crops,
367–427 ms for successful 1080p frames, and 530–605 ms for unsuccessful 4K frames.
Both runs used the manifest's size- and SHA-256-verified PP-OCRv6 artifacts on
the same Ryzen 7 8745HS development host with four inference threads.

Progressive detection improves the first useful result but does not make a dense
4K scan instantaneous. Text outside the first areas can still be late, especially
on constantly changing backgrounds. Native physical 4K capture, mixed-DPI monitor
moves, and the following real-media checks remain acceptance work:

| Check | What still needs observation |
| --- | --- |
| Chinese → English and Spanish → English video/game text | Misses over at least 20 sentences; OCR correctness separately from human-rated translation faithfulness |
| Cold translator and warm compact route | Source appearance to first readable translated label; warm labels should normally begin within two seconds |
| Disappearance during OCR/translation | No newly appearing label after absence is confirmed; existing-label retention follows the documented policy |
| Moving window, mixed DPI, fullscreen media | Overlay alignment, clipping, focus/click-through, feedback, and WGC/OBS parity if capture fails |
| Audio and full application lifecycle | Device/application reconnect, Nemotron clock continuity, Start/Stop during loading, and comparable post-stop resources |

Use [the timed Chinese/Spanish fixture](fixtures/visual-subtitles.html) for a
controlled live sequence (three seconds visible, one second blank), then actual
media. Follow the [visual smoke](WINDOWS_VISUAL_SMOKE_TEST.md) and
[lifecycle soak](WINDOWS_LIFECYCLE_SOAK.md) for the remaining application-level
checks. Automated capture and OCR passes do not close these acceptance items or
establish Ubuntu readiness by themselves.
