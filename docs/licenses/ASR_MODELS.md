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

## Multilingual Nemotron trial

- Display name: Nemotron 3.5 Streaming 0.6B
- Architecture: NVIDIA Nemotron 3.5 ASR Streaming 0.6B, 560 ms latency checkpoint, INT8 sherpa-onnx conversion
- Supported product language settings: English, Spanish, Japanese, and automatic detection
- Converted upstream model: `csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11`
- Pinned revision: `ab43d895f5985b1bbab8b6eac8607fcdc05343f3`
- Original model publisher: NVIDIA
- Weight license: OpenMDW-1.1, as declared by the model records
- Required download: 682,215,356 bytes across encoder, decoder, joiner, and token files
- Manifest: `assets/model-manifests/nemotron-3.5-streaming-multilingual.json`
- Converted model record: <https://huggingface.co/csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11/tree/ab43d895f5985b1bbab8b6eac8607fcdc05343f3>
- Original NVIDIA model card: <https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b>
- License text and interpretation guide: <https://github.com/OpenMDW/openmdw>

`0.6B` describes approximately 600 million parameters; it is not a download-size label. The pinned INT8 artifacts total 650.6 MiB. This model is an optional pre-release trial, is never bundled or downloaded automatically, and currently runs through the CPU sherpa-onnx path. Its presence in the catalog is not a production-quality claim for every listed language.

## Runtime

- Runtime: sherpa-onnx 1.13.4
- Runtime license: Apache-2.0
- Upstream source: <https://github.com/k2-fsa/sherpa-onnx/tree/v1.13.4>
- Rust wrapper record: <https://crates.io/crates/sherpa-onnx/1.13.4>

The model and runtime are not included in the repository or the base application package. Release packaging must carry the applicable upstream notices for any runtime binaries it distributes.

The audio pipeline also uses `rubato` 4.0.0 for band-limited PCM resampling under its `MIT OR Apache-2.0` license. It is application code rather than a speech model, but its notice must be included with other distributed third-party dependencies.
