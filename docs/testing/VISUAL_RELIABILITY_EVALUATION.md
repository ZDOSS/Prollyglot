# Visual reliability evaluation

Version 0.1.13 fixes independent sources of missed or late labels: cold-load
deadlines, retry starvation, reversed queue priority, isolated change-detection
samples, short-text filtering, OCR reading order, global overlay echo matching,
and late completion after disappearance. It also preserves the audio session
clock through WASAPI recovery. These are covered by deterministic regressions.

Small text on a whole display remains a detector limitation. Sharper recognition
crops cannot recover a box the detector never found. The current model and CPU
provider remain unchanged; a larger model is not a demonstrated remedy.

## Repeatable local checks

Run `bash scripts/check-local.sh` for the shared tests, lint, Windows cross-check,
generated contracts, TypeScript tests, and frontend build. The Windows script in
`scripts/` runs the corresponding native checks on the supported platform.

With the Vite development server running, use an existing Playwright installation:

```sh
node scripts/check-visual-browser.mjs http://localhost:1420 target/visual-evaluation
```

If Playwright or Chromium is installed elsewhere, set
`PROLLYGLOT_PLAYWRIGHT_MODULE` to its `index.mjs` and `PROLLYGLOT_CHROMIUM` to the
browser executable. This checks controls at 1280, 420, and 400 pixels, accessible
navigation, the OCR source choices, and label geometry at all four edges. It
also explicitly writes synthetic Chinese/Spanish PNG fixtures at 1080p and 4K,
with 24- and 48-pixel text, plus 980 × 180 crops of the same source pixels.
Nothing is downloaded by this script.

Point the local OCR evaluator at the verified, already installed model directory:

```sh
cargo run --locked -p prollyglot-visual-ocr-rapid --example evaluate -- MODEL_DIRECTORY zh target/visual-evaluation/zh-1920-24.png target/visual-evaluation/zh-1920-24-region.png
cargo run --locked -p prollyglot-visual-ocr-rapid --example evaluate -- MODEL_DIRECTORY es target/visual-evaluation/es-3840-24.png target/visual-evaluation/es-3840-24-region.png
```

The evaluator uses the production All detected text OCR adapter. JSON lines
include model-load time, three passes per PNG, detector/recognizer/preprocessing
timings, and the actual recognized text. Pass 0 is first-use inference; passes 1
and 2 are warm. This explicit fixture tool prints text, unlike the application's
media-free timing diagnostics. It does not measure OS capture, translation, or
overlay delivery. Keep those stages separate when interpreting results.

The vendor crop regression is run separately:

```sh
cargo test --manifest-path vendor/rapidocr-core/Cargo.toml --no-default-features --lib prollyglot_source_crops
```

## Native Windows acceptance

Open [the timed fixture](fixtures/visual-subtitles.html) in a browser. Select
Chinese or Spanish, then **Play sequence**. Sentences remain for three seconds,
followed by a one-second blank. **Hold first sentence** tests static text and
cold model loading. The sequence includes “No”, “Sí”, “OK”, and a single Han
character. The expected translations include “Good morning. How are you?” and
“Hello world. Welcome back.”; exact wording is not the translation accuracy test.

Compare each source at the same resolution and font size:

| Case | Record |
| --- | --- |
| Whole display, selected window, selected region | Missed sentences / total; source width, height, and Windows display scaling |
| Chinese → English, Spanish → English | OCR correctness separately from human-rated translation faithfulness |
| Cold route, warm compact route, optional universal route | Time from source appearance to first readable translation; median and worst case over at least 20 sentences |
| Text disappears during OCR or translation | Number of newly appearing late labels; target zero after absence is confirmed |
| Static sign, short words, multi-line text, moving video | Missing text, reordered words, flicker, overlay feedback, and clipping |
| Audio device/application recovery with Nemotron | Transcript timestamps remain increasing; short recovered audio does not finalize repeatedly at the four-second boundary |

Warm compact translation should normally become readable within two seconds.
Record startup separately. Preserve source text for accuracy comparison, and
compare WGC display capture with equivalent OBS Display Capture when capture
itself fails. The existing Windows visual smoke test remains the capture/DPI
and overlay acceptance guide.

## Local evidence and remaining work

The 2026-09-07 WSL CPU run (Ryzen 7 8745HS, four OCR inference threads) used
the exact size- and SHA-256-verified PP-OCRv6
manifest artifacts and Chromium-rendered synthetic fixtures. All eight cropped
cases reproduced the expected Chinese or Spanish text exactly in all three
passes. The four 1080p full-frame cases also recognized the expected text; all
four 4K full-frame cases missed it entirely. These sparse, clean fixtures are
a reason to retain varied media cases and avoid claiming full-display reliability
from one successful screenshot. [Recorded per-fixture results](results/visual-ocr-0.1.13.json)
include the warm timings and exact-text pass counts.

In a serial run without the build checks competing for CPU, the per-fixture
median of the two warm passes was 79–119 ms for crops, 367–427 ms for the
successful 1080p full frames, and 530–605 ms for the unsuccessful 4K full frames.
These are OCR-only observations from a small synthetic set, not statistical
latency targets or before/after speedup claims.

These small synthetic runs establish a scale-sensitive detector problem, not a
Windows performance or accuracy certification. Model initialization and actual
OCR inference were exercised; native WASAPI/WGC, translation model accuracy,
multi-monitor DPI, and live overlay timing still require the Windows matrix.

The next performance work should compare detection on regions at sufficient
resolution, with current text regions prioritized and unchanged recognition
reused. Benchmark its whole-display recall and total latency before enabling it
by default. GPU execution or a different OCR model should follow that comparison
if CPU inference still exceeds the latency target. Tiling every frame without
a work budget could improve recall while recreating the delayed-label problem.
