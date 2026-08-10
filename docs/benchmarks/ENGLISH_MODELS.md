# English streaming model comparison

This record tracks the evidence behind Prollyglot's user-selectable English model catalog. Fast remains the first-run default because it is the smallest option. Balanced and Enhanced can be installed and selected in Settings; a larger profile is an option to test against difficult speech, not a promise of universally better recognition.

## Candidates

| Candidate | Pinned upstream model | Download | Product availability |
| --- | --- | ---: | --- |
| English Streaming Small (Fast) | `sherpa-onnx-streaming-zipformer-en-20M-2023-02-17` | 43.1 MiB | User-facing; current first-run default |
| English Streaming Standard (Balanced) | `sherpa-onnx-streaming-zipformer-en-2023-06-26` | 70.0 MiB | Optional user-facing choice |
| English Streaming Enhanced (Enhanced) | `sherpa-onnx-streaming-zipformer-en-2023-06-21` | 181.4 MiB | Optional user-facing choice |

All three are Apache-2.0 streaming transducers and use the same sherpa-onnx 1.13.4 adapter. Enhanced was trained on LibriSpeech and GigaSpeech; the other two provide lower-download alternatives. Exact revisions, artifact hashes, and provenance are recorded in [`docs/licenses/ASR_MODELS.md`](../licenses/ASR_MODELS.md).

## Reproduce a comparison

The harness accepts a mono WAV at any positive sample rate, band-limits and resamples it to 16 kHz, installs or verifies the candidates in an isolated cache, streams identical 100 ms chunks through each model, and prints a Markdown row containing:

- model preparation and load time;
- inference time and real-time factor (RTF);
- audio consumed before the first partial;
- compute time to the first partial when processing the file as fast as possible;
- number of distinct partial updates;
- word error rate (WER) when reference text is supplied; and
- final transcript.

Run an optimized build so debug overhead does not distort the result:

```bash
cargo run --release --locked -p prollyglot-asr-sherpa --example compare_models -- \
  /path/to/model-cache \
  /path/to/mono-sample.wav \
  "REFERENCE TRANSCRIPT"
```

Use `-` instead of reference text when no trustworthy transcript exists. The first invocation includes model downloads; use at least three cached invocations for timing comparisons. Peak memory and average CPU remain external observations because the cross-platform harness deliberately avoids a platform-specific process monitor.

The three-argument form above preserves the English-only comparison. Add a fourth `en`, `es`, `ja`, or `auto` argument to compare every compatible catalog model; Japanese references are reported as character error rate rather than word error rate. The multilingual results and limitations are recorded in [`MULTILINGUAL_NEMOTRON.md`](MULTILINGUAL_NEMOTRON.md).

## Initial two-model clean-reference baseline — 2026-08-09

This smoke comparison used the model publisher's 6.625-second `test_wavs/0.wav` and its supplied reference transcript. Host: WSL2, AMD Ryzen 7 8745HS (8 cores/16 threads exposed), approximately 11 GiB assigned RAM. The adapter used two inference threads. Values below are medians of three release-mode runs where applicable; the first uncached download was excluded from timing medians.

| Model | Cached prepare | Load | Inference | RTF | First partial audio | First partial compute | Partial updates | WER |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| English Streaming Small | 0.065 s | 0.545 s | 0.183 s | 0.028 | 1,300 ms | 31.3 ms | 17 | 5.6% |
| English Streaming Standard | 0.101 s | 2.016 s | 0.376 s | 0.057 | 1,000 ms | 47.0 ms | 17 | 5.6% |

The lightweight transcript ended with “BROFFEL” and the standard transcript ended with “BROTH,” where the reference says “BROTHELS.” Both therefore had one substitution in 18 words.

### Interpretation

- Both candidates were comfortably faster than real time on this host.
- Standard exposed useful text after 300 ms less audio on this one sample.
- Standard took about twice the inference time and roughly four times the model-load time.
- This clean audiobook sentence showed no WER advantage for standard.

This was runtime and harness validation, not a default-model decision. It did not represent ordinary conversation, accents, browser media, overlapping voices, or noisy game/call audio.

## Three-model catalog verification — 2026-08-10

After an owner report of mangled accented dialogue, the same pinned reference and host were used to verify the complete product catalog. The Enhanced manifest was downloaded from its immutable upstream revision and every artifact passed its size and SHA-256 check before decoding. Timing values are medians of three cached release-mode runs; the first Enhanced download is excluded from cached preparation.

| Model | Cached prepare | Load | Inference | RTF | First partial audio | First partial compute | Partial updates | WER |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| English Streaming Small (Fast) | 0.059 s | 0.485 s | 0.164 s | 0.025 | 1,300 ms | 29.9 ms | 17 | 5.6% |
| English Streaming Standard (Balanced) | 0.088 s | 1.828 s | 0.343 s | 0.052 | 1,000 ms | 45.9 ms | 17 | 5.6% |
| English Streaming Enhanced (Enhanced) | 0.268 s | 1.192 s | 0.308 s | 0.046 | 900 ms | 41.4 ms | 17 | 5.6% |

Fast ended with “BROFFEL,” Balanced with “BROTH,” and Enhanced with “BROTHEL,” where the reference says “BROTHELS.” Each still had one error in the 18-word reference, so this clean sentence does not prove an accuracy advantage for a larger model.

### Catalog interpretation

- All three choices were comfortably faster than real time on this host.
- Enhanced produced the first useful partial after 900 ms of audio on this fixture, 400 ms earlier than Fast, while retaining substantial real-time headroom.
- Enhanced costs about 4.2 times Fast's download size; measured load and inference remained suitable for streaming on this development machine.
- Broader LibriSpeech-plus-GigaSpeech training makes Enhanced a reasonable option to try on varied or accented speech, but only representative Windows listening can show whether it helps the reported case.

Milestone 2 still requires comparisons on the reference Windows machine using ordinary conversation, accents, browser media, overlapping voices, and noisy game/call audio. The current evidence supports exposing a choice while retaining Fast as the default.

## Decoder check — 2026-08-10

After the first native-Windows quality report, Fast and Balanced were rerun on the same pinned reference with four-path `modified_beam_search` instead of greedy decoding. The transcripts and 5.6% WER were unchanged, and inference remained effectively the same (Fast: 0.191 s, 0.029 RTF; Balanced: 0.377 s, 0.057 RTF). Prollyglot therefore retains the simpler greedy decoder rather than claiming an unsupported quality improvement.

This result does not clear the reported conversational-quality concern. The confirmed short-utterance gating losses were corrected separately; remaining difficult dialogue should be compared across the user-facing choices on Windows rather than inferred from another clean audiobook sentence.
