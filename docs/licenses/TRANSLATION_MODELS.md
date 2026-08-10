# Translation runtime and model provenance

Prollyglot keeps translation models outside the application package. A language pair downloads only after an explicit user action. Every required artifact is checked against its pinned byte size and SHA-256 digest before an installation marker is written or the model becomes loadable. Exact artifact records are in `apps/desktop/src/translation-catalog.ts`.

## Japanese to English

- Display name: Japanese to English
- Architecture: Marian / OPUS-MT, q8 ONNX encoder and merged decoder
- Runtime repository: `Xenova/opus-mt-ja-en`
- Pinned revision: `1a906cfaaf7c8f4193f67f5885c082aa6dbd9d16`
- Original model: `Helsinki-NLP/opus-mt-ja-en`
- Weight license: Apache-2.0, as declared by the upstream records
- Required download: 114,701,000 bytes across configuration, tokenizer, encoder, and decoder artifacts (109.4 MiB)
- Pinned model record: <https://huggingface.co/Xenova/opus-mt-ja-en/tree/1a906cfaaf7c8f4193f67f5885c082aa6dbd9d16>
- Original model record: <https://huggingface.co/Helsinki-NLP/opus-mt-ja-en>

## Spanish to English

- Display name: Spanish to English
- Architecture: Marian / OPUS-MT, q8 ONNX encoder and merged decoder
- Runtime repository: `Xenova/opus-mt-es-en`
- Pinned revision: `eadfd7c658a9d8929ac3b8e996b68a68e2c7d480`
- Original model: `Helsinki-NLP/opus-mt-es-en`
- Weight license: Apache-2.0, as declared by the upstream records
- Required download: 119,377,236 bytes across configuration, tokenizer, encoder, and decoder artifacts (113.8 MiB)
- Pinned model record: <https://huggingface.co/Xenova/opus-mt-es-en/tree/eadfd7c658a9d8929ac3b8e996b68a68e2c7d480>
- Original model record: <https://huggingface.co/Helsinki-NLP/opus-mt-es-en>

## Runtime dependencies

- Transformers.js 4.2.0 — Apache-2.0 — <https://github.com/huggingface/transformers.js/tree/4.2.0>
- ONNX Runtime Web `1.26.0-dev.20260416-b7804b056c` — MIT — <https://github.com/microsoft/onnxruntime>
- noble-hashes 2.0.1 — MIT — <https://github.com/paulmillr/noble-hashes/tree/2.0.1>

The application bundles the JavaScript runtime and ONNX WebAssembly support, not the translation weights. The worker uses CPU/WebAssembly, loads at most one translator at a time, and allows remote access only during the explicit verified install path. Normal inference is cache-only. Release packaging must include the applicable third-party notices for bundled runtime code.
