# Translation runtime and model provenance

Prollyglot keeps translation models outside the application package. A model downloads only after an explicit user action. Every required artifact is checked against its pinned byte size and SHA-256 digest before an installation marker is written or the model becomes loadable. Exact artifact records and route declarations are in `apps/desktop/src/translation-catalog.ts`.

## Japanese to English

- Display name: Japanese to English · Compact
- Architecture: Marian / OPUS-MT, q8 ONNX encoder and merged decoder
- Runtime repository: `Xenova/opus-mt-ja-en`
- Pinned revision: `1a906cfaaf7c8f4193f67f5885c082aa6dbd9d16`
- Original model: `Helsinki-NLP/opus-mt-ja-en`
- Weight license: Apache-2.0, as declared by the upstream records
- Required download: 114,701,000 bytes across configuration, tokenizer, encoder, and decoder artifacts (109.4 MiB)
- Pinned model record: <https://huggingface.co/Xenova/opus-mt-ja-en/tree/1a906cfaaf7c8f4193f67f5885c082aa6dbd9d16>
- Original model record: <https://huggingface.co/Helsinki-NLP/opus-mt-ja-en>

## Spanish to English

- Display name: Spanish to English · Compact
- Architecture: Marian / OPUS-MT, q8 ONNX encoder and merged decoder
- Runtime repository: `Xenova/opus-mt-es-en`
- Pinned revision: `eadfd7c658a9d8929ac3b8e996b68a68e2c7d480`
- Original model: `Helsinki-NLP/opus-mt-es-en`
- Weight license: Apache-2.0, as declared by the upstream records
- Required download: 119,377,236 bytes across configuration, tokenizer, encoder, and decoder artifacts (113.8 MiB)
- Pinned model record: <https://huggingface.co/Xenova/opus-mt-es-en/tree/eadfd7c658a9d8929ac3b8e996b68a68e2c7d480>
- Original model record: <https://huggingface.co/Helsinki-NLP/opus-mt-es-en>

## Multilingual to English

- Display name: Multilingual to English · Compact
- Architecture: Marian / OPUS-MT `mul-en`, q8 ONNX encoder and merged decoder
- Runtime repository: `Xenova/opus-mt-mul-en`
- Pinned revision: `72a05e47cee89c718a9db4dc70d02fef3bc39de8`
- Original model: `Helsinki-NLP/opus-mt-mul-en`
- Product route: every non-English spoken language currently selectable in Prollyglot to English; Japanese and Spanish prefer their direct compact model when it is installed
- Weight license: Apache-2.0, as declared by the original model record
- Required download: 118,351,723 bytes across configuration, tokenizer, encoder, and decoder artifacts (112.9 MiB)
- Pinned model record: <https://huggingface.co/Xenova/opus-mt-mul-en/tree/72a05e47cee89c718a9db4dc70d02fef3bc39de8>
- Original model record and source-language inventory: <https://huggingface.co/Helsinki-NLP/opus-mt-mul-en>

## Universal many-to-many translation

- Display name: Universal 29-language translator
- Architecture: M2M100 418M, q8 ONNX encoder and merged decoder
- Runtime repository: `Xenova/m2m100_418M`
- Pinned revision: `9c374f0b7aca709787cea97b047bfbbd1559d177`
- Original model: `facebook/m2m100_418M`
- Product route: direct source-to-target translation among all 29 selectable spoken languages; Norwegian Bokmål is mapped to M2M100's Norwegian code
- Weight license: MIT, as declared by the original model record
- Required download: 639,976,029 bytes across configuration, tokenizer, encoder, and decoder artifacts (610.3 MiB)
- Pinned model record: <https://huggingface.co/Xenova/m2m100_418M/tree/9c374f0b7aca709787cea97b047bfbbd1559d177>
- Original model and license record: <https://huggingface.co/facebook/m2m100_418M>

The universal route is substantially larger than the compact models and may add more CPU delay. It is optional and is not silently selected or downloaded. “29-language” describes the subset currently exposed by Prollyglot's recognizers, not the full upstream M2M100 language inventory.

## Runtime dependencies

- Transformers.js 4.2.0 — Apache-2.0 — <https://github.com/huggingface/transformers.js/tree/4.2.0>
- ONNX Runtime Web `1.26.0-dev.20260416-b7804b056c` — MIT — <https://github.com/microsoft/onnxruntime>
- noble-hashes 2.0.1 — MIT — <https://github.com/paulmillr/noble-hashes/tree/2.0.1>

The application bundles the JavaScript runtime and ONNX WebAssembly support, not the translation weights. The worker uses CPU/WebAssembly, loads at most one translator at a time, prefers an installed compact route over the universal model, and allows remote access only during the explicit verified install path. Normal inference is cache-only. Release packaging must include the applicable third-party notices for bundled runtime code.
