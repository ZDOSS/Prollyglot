# ASR runtime and model provenance

Prollyglot keeps speech models outside the application binary. A model is downloaded only after an explicit user action, and every runtime artifact is verified against the size and SHA-256 digest in its manifest before it becomes loadable.

## Initial lightweight English candidate

- Display name: English Streaming Small
- Architecture: streaming Zipformer transducer (20M training configuration)
- Upstream model: `csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17`
- Pinned revision: `d42f2d9f7ca24806fb667456a18a9f1b60f70d16`
- Weight license: Apache-2.0, as declared by the upstream model card
- Required download: 45,202,074 bytes across encoder, decoder, joiner, and token files
- Manifest: `assets/model-manifests/english-streaming-small.json`
- Upstream model record: <https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/tree/d42f2d9f7ca24806fb667456a18a9f1b60f70d16>

This is an initial lightweight candidate, not yet the declared production default. Milestone 2 requires a measured comparison with a standard-size English model on conversational, media, and noisy game/call samples before the default is selected.

## Runtime

- Runtime: sherpa-onnx 1.13.4
- Runtime license: Apache-2.0
- Upstream source: <https://github.com/k2-fsa/sherpa-onnx/tree/v1.13.4>
- Rust wrapper record: <https://crates.io/crates/sherpa-onnx/1.13.4>

The model and runtime are not included in the repository or the base application package. Release packaging must carry the applicable upstream notices for any runtime binaries it distributes.
