# Windows Milestone 2 validation

This is the manual acceptance run for Prollyglot's first complete English-caption vertical slice. It must run on a real Windows 11 machine: cross-compilation proves the code shape, but cannot prove physical audio routing, caption latency, overlay behavior, or sustained CPU and memory use.

> [!NOTE]
> Use the [complete Windows 11 test plan](WINDOWS_TEST_PLAN.md) for literal setup commands, generated routing fixtures, expected results, evidence tables, OBS parity steps, and the final report template. This file remains the shorter Milestone 2 acceptance summary.

This is a substantial milestone gate, not a per-commit CI workflow.

## Prepare the machine

Record the Windows version, CPU, installed RAM, GPU if any, playback device, display scale, and whether the machine is on battery or AC power.

From PowerShell at the repository root:

```powershell
git pull --ff-only origin main
pnpm --dir apps/desktop install --frozen-lockfile
./scripts/check-windows.ps1
pnpm --dir apps/desktop tauri dev
```

The local check must pass without using GitHub Actions minutes. Leave Task Manager open to Prollyglot's CPU and memory columns.

## First-run model flow

1. If the English model is already installed, remove it from Settings and restart Prollyglot.
2. Confirm the model card is visible immediately, its size is shown, and Start Captions is unavailable until the model is ready.
3. Start the download. Confirm progress changes and that the UI remains responsive.
4. Close Prollyglot partway through one download, reopen it, and start the download again. It must recover without treating a partial file as an installed model.
5. Complete the download. The card should disappear, Start Captions should become available, and Settings should report the model as installed locally.
6. Disconnect the network, restart Prollyglot, and start captions. An installed model must work offline with no account or network request.
7. Stop captions, remove the model, and confirm Start Captions becomes unavailable again. Reinstall it before continuing.

Expected download size is approximately 43.1 MiB (45,202,074 bytes). Every installed artifact is checked against the pinned size and SHA-256 digest before it becomes ready.

## Reference recognition sanity check

The repository includes an ignored test that downloads the pinned model and sends the model publisher's reference speech through Prollyglot's actual streaming adapter. It checks incremental output and recognizable final text, including the beginning of the phrase.

Download the pinned reference WAV, then run:

```powershell
$ReferenceWav = Join-Path $env:TEMP "prollyglot-reference-0.wav"
Invoke-WebRequest "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/d42f2d9f7ca24806fb667456a18a9f1b60f70d16/test_wavs/0.wav" -OutFile $ReferenceWav
$env:PROLLYGLOT_TEST_WAV = $ReferenceWav
cargo test --release --locked -p prollyglot-asr-sherpa transcribes_the_pinned_models_reference_speech -- --ignored --nocapture
```

This is a runtime sanity check, not the product-quality benchmark. The model weights and reference file come from the same pinned Apache-2.0 model revision recorded in `docs/licenses/ASR_MODELS.md`.

## Everything I hear

Use ordinary English speech with known wording, such as a locally stored video or a spoken test passage.

1. Select Everything I hear and Follow system default, then start captions.
2. Confirm the overlay appears without taking keyboard focus.
3. Confirm partial text appears while the speaker is still talking and final text remains stable rather than rewriting older segments.
4. Pay special attention to the first words after each pause. Phrase openings should not disappear.
5. Pause for at least five seconds. The capture state may say Waiting, inference should stay quiet, and the final overlay caption should clear after its short hold.
6. Resume speech and confirm captions recover without restarting.
7. Open Transcript. Confirm finalized captions appear once, live text is visually provisional, timestamps increase, and Clear removes the current session transcript.
8. Stop and start captions ten times. No session should remain stuck, duplicate a final, or leave an orphan overlay.
9. Repeat with a pinned non-default playback endpoint.

## Only this application

Use two applications that can play speech independently, such as Firefox and VLC.

1. Select the first application and start captions.
2. Play only the unrelated application. Prollyglot should not produce meaningful text from it.
3. Play the selected application. Prollyglot should produce partial and final captions.
4. Play both applications. The transcript should follow only the selected process tree.
5. Close the selected application while it is talking. Prollyglot must report the source lifecycle failure clearly and permit a new session after refresh/reselection.
6. Repeat with the other application selected.

If current OBS Application Audio Capture receives the selected process under equivalent routing and Prollyglot does not, record a Prollyglot defect. Protected-media sources are evaluated by the same parity rule; Prollyglot does not add its own DRM classification or refusal layer.

## Latency and resource run

Use at least ten minutes of mixed material containing conversational speech, a media segment, and speech over game/call background noise. For each category, perform at least ten phrase starts.

Record:

| Measure | Conversation | Media | Noisy game/call |
| --- | ---: | ---: | ---: |
| Phrase starts sampled |  |  |  |
| Median time to useful partial |  |  |  |
| 95th-percentile time to useful partial |  |  |  |
| Obvious missed phrase openings |  |  |  |
| Material final-text errors |  |  |  |

Measure latency from an audible word to the first useful matching partial in the overlay. A phone slow-motion recording of the source playback and overlay in the same frame is acceptable for this gate. The lightweight model must be at least real-time and median partial latency must remain below two seconds.

Then run captions continuously for 30 minutes and record:

- CPU after the first minute and near the end;
- memory after the first minute and near the end;
- total dropped-packet count or backpressure warnings;
- whether overlay latency grows over time; and
- whether stop completes normally.

Memory and delay must not grow continuously. If inference falls behind, Prollyglot should discard old buffered audio, expose a warning, and recover near live playback instead of accumulating an ever-longer delay.

## Model comparison gate

The initial 20M English Zipformer is a lightweight candidate, not automatically the permanent default. Prollyglot's comparison harness downloads the pinned benchmark-only standard candidate into a separate cache and processes the same local WAV through both models. It does not add the standard model to the app or change the user's installed model.

Prepare a mono WAV with a trustworthy transcript for each conversation, media, and noisy category, then run at least three cached release-mode comparisons per sample:

```powershell
$BenchmarkModels = Join-Path $env:LOCALAPPDATA "Prollyglot\benchmark-models"
cargo run --release --locked -p prollyglot-asr-sherpa --example compare_models -- $BenchmarkModels "C:\path\to\sample.wav" "REFERENCE TRANSCRIPT"
```

The harness accepts different WAV sample rates and resamples internally, but the file must be mono. The additional standard candidate is approximately 70.0 MiB. Retain:

- model/download size;
- peak memory;
- average CPU;
- real-time factor;
- median and 95th-percentile partial latency; and
- a transcript comparison or word-error measurement against known text.

Choose the default only after that evidence. If neither model is clearly preferable, keep the lightweight model as the first-run option and defer automatic hardware-based selection; do not hide the unresolved quality tradeoff.

The reproducible harness, candidate details, and initial clean-reference smoke result are documented in [`docs/benchmarks/ENGLISH_MODELS.md`](../benchmarks/ENGLISH_MODELS.md).

## Report back

Please return the machine details, first-run/offline results, both source-mode results, restart and source-exit behavior, transcript/overlay observations, latency table, 30-minute resource figures, backpressure warnings, model-comparison table, and nearby Prollyglot log lines for any failure.

Milestone 2 is accepted only after the end-to-end behavior passes and the small-versus-standard model decision is supported by measured evidence.
