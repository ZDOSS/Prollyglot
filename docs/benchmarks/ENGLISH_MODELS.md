# English streaming model comparison

This record tracks the evidence used to choose Prollyglot's first English model. The lightweight candidate remains the app's first-run model until a broader comparison shows that another default provides enough caption-quality benefit to justify its resource cost.

## Candidates

| Candidate | Pinned upstream model | Download | Product availability |
| --- | --- | ---: | --- |
| English Streaming Small | `sherpa-onnx-streaming-zipformer-en-20M-2023-02-17` | 43.1 MiB | Current first-run candidate |
| English Streaming Standard | `sherpa-onnx-streaming-zipformer-en-2023-06-26` | 70.0 MiB | Benchmark only |

Both are Apache-2.0 streaming transducers and use the same sherpa-onnx 1.13.4 adapter. Exact revisions, artifact hashes, and provenance are recorded in [`docs/licenses/ASR_MODELS.md`](../licenses/ASR_MODELS.md).

## Reproduce a comparison

The harness accepts a mono WAV at any positive sample rate, resamples it to 16 kHz, installs or verifies both candidates in an isolated cache, streams identical 100 ms chunks through each model, and prints a Markdown row containing:

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

## Initial clean-reference baseline — 2026-08-09

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

This is runtime and harness validation, not a default-model decision. It does not represent ordinary conversation, accents, browser media, overlapping voices, or noisy game/call audio. Milestone 2 still requires the Windows procedure's broader sample set and measurements on the reference Windows machine.

## Decoder check — 2026-08-10

After the first native-Windows quality report, both candidates were rerun on the same pinned reference with four-path `modified_beam_search` instead of greedy decoding. The transcripts and 5.6% WER were unchanged, and inference remained effectively the same (small: 0.191 s, 0.029 RTF; standard: 0.377 s, 0.057 RTF). Prollyglot therefore retains the simpler greedy decoder rather than claiming an unsupported quality improvement.

This result does not clear the reported conversational-quality concern. The confirmed short-utterance gating losses are corrected first; if ordinary dialogue remains mangled, the next model decision must use representative conversational Windows audio rather than another clean audiobook sentence.
