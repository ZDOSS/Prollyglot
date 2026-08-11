# ASR runtime and model provenance

Prollyglot keeps speech models outside the application binary. A model is downloaded only after an explicit user action, and every runtime artifact is verified against the size and SHA-256 digest in its manifest before it becomes loadable.

## Fast English model

- Display name: English Streaming Small
- Architecture: streaming Zipformer transducer (20M training configuration)
- Upstream model: `csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17`
- Pinned revision: `d42f2d9f7ca24806fb667456a18a9f1b60f70d16`
- Weight license: Apache-2.0, as declared by the upstream model card
- Required download: 45,202,074 bytes across encoder, decoder, joiner, and token files
- Manifest: `assets/model-manifests/english-streaming-small.json`
- Upstream model record: <https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/tree/d42f2d9f7ca24806fb667456a18a9f1b60f70d16>

This is the current first-run default because it has the smallest download and resource cost. It remains a user-selectable model rather than being silently replaced when other models are installed.

## Balanced English model

- Display name: English Streaming Standard
- Architecture: streaming Zipformer transducer, chunk 16 with 128 frames of left context
- Upstream model: `csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-26`
- Pinned revision: `672fbf1b30579d6585301139bb363f42a0ad4a24`
- Weight license: Apache-2.0, as declared by the upstream model card
- Required download: 73,440,167 bytes across encoder, decoder, joiner, and token files
- Manifest: `assets/model-manifests/english-streaming-standard.json`
- Upstream model record: <https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-26/tree/672fbf1b30579d6585301139bb363f42a0ad4a24>

This is an optional user-facing model. It streams through the same local runtime and can be installed, selected, and removed independently.

## Enhanced English model

- Display name: English Streaming Enhanced
- Architecture: streaming Zipformer transducer trained on LibriSpeech and GigaSpeech
- Upstream model: `csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-21`
- Pinned revision: `9a65b6ea94c311ca770c2bf895b30f456a22d703`
- Weight license: Apache-2.0, as declared by the upstream model card
- Required download: 190,180,941 bytes across encoder, decoder, joiner, and token files
- Manifest: `assets/model-manifests/english-streaming-enhanced.json`
- Upstream model record: <https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-21/tree/9a65b6ea94c311ca770c2bf895b30f456a22d703>

This is the broadest initial user-facing English option. The broader training data makes it a useful candidate for varied speech, but the product does not claim a universal accent or accuracy improvement without representative benchmark evidence.

## Compact language-specific streaming models

These optional transducer models use the same local sherpa-onnx streaming path as the English choices. They provide lower-download alternatives to Nemotron for four languages where suitable pinned online models are available. Their presence is an integration choice, not a claim that each is more accurate than Nemotron on real Windows media.

| Language | Display name | Upstream model and pinned revision | Download | Manifest |
| --- | --- | --- | ---: | --- |
| Chinese | Chinese Streaming Small | [`csukuangfj/sherpa-onnx-streaming-zipformer-zh-14M-2023-02-23`](https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-zh-14M-2023-02-23/tree/204ad334e2e683fd295359930cc16fc0432a23ac) at `204ad334e2e683fd295359930cc16fc0432a23ac` | 30,975,688 bytes (29.5 MiB) | `assets/model-manifests/chinese-streaming-small.json` |
| French | French Streaming Compact | [`shaojieli/sherpa-onnx-streaming-zipformer-fr-2023-04-14`](https://huggingface.co/shaojieli/sherpa-onnx-streaming-zipformer-fr-2023-04-14/tree/3db9565d9633758d6b87b9a7b3dc09ebfb6b2c73) at `3db9565d9633758d6b87b9a7b3dc09ebfb6b2c73` | 129,012,566 bytes (123.0 MiB) | `assets/model-manifests/french-streaming-compact.json` |
| Korean | Korean Streaming Compact | [`k2-fsa/sherpa-onnx-streaming-zipformer-korean-2024-06-16`](https://huggingface.co/k2-fsa/sherpa-onnx-streaming-zipformer-korean-2024-06-16/tree/ba6078bca4daf3f0dd37f79d0ab505af71df14a6) at `ba6078bca4daf3f0dd37f79d0ab505af71df14a6` | 140,919,603 bytes (134.4 MiB) | `assets/model-manifests/korean-streaming-compact.json` |
| Bengali | Bengali Streaming Compact | [`csukuangfj2/sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09`](https://huggingface.co/csukuangfj2/sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09/tree/a7c3c1547450a7c546c876be9ca8a6ab54464423) at `a7c3c1547450a7c546c876be9ca8a6ab54464423` | 94,119,939 bytes (89.8 MiB) | `assets/model-manifests/bengali-streaming-compact.json` |

The Chinese and French conversion records declare Apache-2.0. The Korean conversion originates from the Apache-2.0 [`johnBamma/icefall-asr-ksponspeech-pruned-transducer-stateless7-streaming-2024-06-12`](https://huggingface.co/johnBamma/icefall-asr-ksponspeech-pruned-transducer-stateless7-streaming-2024-06-12) model. The Bengali conversion originates from the Apache-2.0 [`alphacep/vosk-model-small-streaming-bn`](https://huggingface.co/alphacep/vosk-model-small-streaming-bn) model. Each Prollyglot manifest records Apache-2.0 and pins the converted encoder, decoder, joiner, and token artifacts by byte size and SHA-256.

## Multilingual Nemotron trial

- Display name: Nemotron 3.5 Streaming 0.6B
- Architecture: NVIDIA Nemotron 3.5 ASR Streaming 0.6B, 560 ms latency checkpoint, INT8 sherpa-onnx conversion
- Transcription-ready product settings: Arabic, Dutch, English, French, German, Hindi, Italian, Japanese, Korean, Portuguese, Russian, Spanish, Turkish, Ukrainian, and Vietnamese
- Broad-coverage product settings: Bulgarian, Chinese, Croatian, Czech, Danish, Estonian, Finnish, Hungarian, Norwegian Bokmål, Polish, Romanian, Slovak, and Swedish
- Additional mode: automatic detection
- Converted upstream model: `csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11`
- Pinned revision: `ab43d895f5985b1bbab8b6eac8607fcdc05343f3`
- Original model publisher: NVIDIA
- Weight license: OpenMDW-1.1, as declared by the model records
- Required download: 682,215,356 bytes across encoder, decoder, joiner, and token files
- Manifest: `assets/model-manifests/nemotron-3.5-streaming-multilingual.json`
- Converted model record: <https://huggingface.co/csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11/tree/ab43d895f5985b1bbab8b6eac8607fcdc05343f3>
- Original NVIDIA model card: <https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b>
- License text and interpretation guide: <https://github.com/OpenMDW/openmdw>

`0.6B` describes approximately 600 million parameters; it is not a download-size label. The pinned INT8 artifacts total 650.6 MiB. This model is an optional pre-release trial, is never bundled or downloaded automatically, and currently runs through the CPU sherpa-onnx path. Prollyglot exposes NVIDIA's 15 transcription-ready languages and 13 broad-coverage languages, for 28 unique languages, but does not expose the model card's adaptation-ready languages because those require fine-tuning. Broad coverage and automatic detection are labeled as less certain, and catalog presence is not a production-quality claim.

## Runtime

- Runtime: sherpa-onnx 1.13.4
- Runtime license: Apache-2.0
- Upstream source: <https://github.com/k2-fsa/sherpa-onnx/tree/v1.13.4>
- Rust wrapper record: <https://crates.io/crates/sherpa-onnx/1.13.4>

The model and runtime are not included in the repository or the base application package. Release packaging must carry the applicable upstream notices for any runtime binaries it distributes.

The audio pipeline also uses `rubato` 4.0.0 for band-limited PCM resampling under its `MIT OR Apache-2.0` license. It is application code rather than a speech model, but its notice must be included with other distributed third-party dependencies.
