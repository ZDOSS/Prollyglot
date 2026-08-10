# Local translation integration record

This record separates “the model loads and translates through the product path” from a claim that translation quality or latency is ready for production media.

## Current choices

| Direction | Product model | Download | Runtime |
| --- | --- | ---: | --- |
| Japanese → English | `Xenova/opus-mt-ja-en` q8 | 109.4 MiB | CPU, ONNX Runtime Web/WebAssembly |
| Spanish → English | `Xenova/opus-mt-es-en` q8 | 113.8 MiB | CPU, ONNX Runtime Web/WebAssembly |

Both models are optional, pinned, integrity-checked, and loaded only for their selected forced source language. Exact revisions, artifact hashes, and licenses are recorded in [`docs/licenses/TRANSLATION_MODELS.md`](../licenses/TRANSLATION_MODELS.md).

## Development-WebView integration check — 2026-08-10

The real worker path, not the UI mock, completed the following checks:

- downloaded and SHA-256-verified every pinned Japanese artifact;
- reused the installed model from local storage after page/worker restart with remote model loading disabled;
- removed and reinstalled the Japanese translator through the Settings lifecycle;
- loaded the q8 Japanese graph and translated `今日は何をする予定ですか？` to `What are you planning to do today?`;
- downloaded, verified, loaded, and switched to the Spanish translator after Japanese; and
- translated `Las ventanas azules se abren sobre el jardín.` to `The blue windows open over the garden.`.

The overlay development preview also rendered source and English in separate colors using both stacked and side-by-side layouts. The transcript rendered the source immediately, showed a pending state, and replaced that state with English without rewriting the committed source.

These are deterministic integration checks, not representative accuracy scores. They prove the download, cache-only load, model-switch, inference, and display contracts on the available Chromium/WebAssembly development path.

## Evidence still required

Before production approval, run the feature in the native Windows WebView against familiar Japanese and Spanish videos and record only actionable observations:

- whether Nemotron's committed source text is accurate enough to translate;
- cold translator load time and typical finalized-caption-to-English delay;
- peak process memory and whether translation causes ASR backlog or UI stalls;
- usefulness across conversational, news, accented, noisy, and short-utterance material; and
- behavior through stop/start, model removal, offline restart, and sustained playback.

No screenshots or recordings are required for an ordinary pre-release spot check. Broader quantitative scoring should wait for redistributable, trustworthy source/English reference material.
