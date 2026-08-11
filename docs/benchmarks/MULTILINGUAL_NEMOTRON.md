# Nemotron multilingual streaming trial

This record explains why Prollyglot exposes NVIDIA Nemotron 3.5 ASR Streaming 0.6B as an optional pre-release choice rather than replacing the English default. `0.6B` means approximately 600 million parameters. The pinned 560 ms INT8 sherpa-onnx conversion downloads 650.6 MiB.

## Integration profile

- Model: `nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11`
- Product language settings: 15 transcription-ready languages, 13 broad-coverage languages, and unconstrained automatic detection (28 unique forced-language choices)
- Runtime: sherpa-onnx 1.13.4, two CPU inference threads
- Model license: OpenMDW-1.1
- Current acceleration: CPU only
- Streaming input: mono 16 kHz PCM in 100 ms chunks, with 500 ms left padding and 800 ms final decoder context
- Resampling: band-limited sinc conversion before inference

Exact provenance, revision, artifact hashes, and source links are in [`docs/licenses/ASR_MODELS.md`](../licenses/ASR_MODELS.md).

## Development-host spot check — 2026-08-10

Host: WSL2, AMD Ryzen 7 8745HS, approximately 11 GiB assigned RAM. These are fixture-level integration checks, not a representative language-quality evaluation.

| Language/mode | Audio | Load | Inference | RTF | First partial audio | Partial updates | Result |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| English, forced | 7.15 s | 2.085 s | 1.735 s | 0.243 | 1,900 ms | 9 | 33.3% WER on the supplied English reference; worse than the three English-only choices on this fixture |
| Spanish, forced | 5.32 s | 2.166 s | 1.261 s | 0.237 | 800 ms | 9 | Manual spot check matched the known sentence after band-limited resampling |
| Japanese, forced | 8.16 s | 2.166 s | 1.967 s | 0.241 | 800 ms | Failed the manual spot check; not quality-cleared |

Peak resident process memory during the development-host run was 971,696 KiB, about 949 MiB. Cached manifest verification was under one second in these runs. CPU use exceeded one full core while decoding, so the current build must not describe this as a lightweight model merely because it is quantized.

Automatic detection also remains provisional. It delayed the first English partial to about 1.9 seconds on the available fixture, and the Japanese spot check mixed scripts/languages and was not usable. The official sherpa-onnx command-line runtime also produced a poor result on the same Japanese publisher fixture, which points to a checkpoint-quality limitation rather than evidence that Prollyglot's adapter alone is at fault.

## Resampling finding

The earlier pipeline used linear interpolation when converting common 44.1/48 kHz Windows audio to the model's 16 kHz input. That conversion did not reject frequencies above the output Nyquist limit and could fold high-frequency energy back into the speech band. Replacing it with packet-stable band-limited sinc resampling changed the Spanish result from a sentence with omitted/misplaced words to the expected sentence on this fixture. This is useful evidence for the input pipeline, not proof of broad Spanish accuracy.

## Product decision

- Keep Fast as the first-run English default.
- Expose Nemotron only through an explicit 650.6 MiB download.
- Use the 560 ms checkpoint rather than the 1120 ms variant while caption delay is a known concern.
- Let the owner compare Nemotron against Enhanced on actual Windows media and accents.
- Treat all catalog coverage as pre-release until representative Windows spot checks say otherwise; label NVIDIA's broad-coverage tier and automatic detection as the less certain paths.
- This ASR comparison records original-language recognition only; translation quality is evaluated separately after Nemotron commits source text.

The catalog now exposes the complete transcription-ready set—Arabic, Dutch, English, French, German, Hindi, Italian, Japanese, Korean, Portuguese, Russian, Spanish, Turkish, Ukrainian, and Vietnamese—and broad coverage for Bulgarian, Chinese, Croatian, Czech, Danish, Estonian, Finnish, Hungarian, Norwegian Bokmål, Polish, Romanian, Slovak, and Swedish. Bengali is exposed through a separate compact streaming model. The upstream adaptation-ready languages are not listed because they require fine-tuning rather than merely selecting a language code.

## Reproduce

Use `-` when no trustworthy reference transcript exists:

```bash
cargo run --release --locked -p prollyglot-asr-sherpa --example compare_models -- \
  /path/to/model-cache \
  /path/to/mono-sample.wav \
  "REFERENCE TRANSCRIPT OR -" \
  es
```

The fourth argument may be any language code declared by an installed product manifest or `auto`. The harness includes every compatible catalog model, so languages with a dedicated compact model can be compared against Nemotron while the remaining forced languages run Nemotron. Japanese and other no-space scripts should use a suitable character-level reference metric; ordinary space-delimited references use word error rate.
