# Local translation integration record

This record separates “the model loads and translates through the product path” from a claim that translation quality or latency is ready for production media.

## Current choices

| Direction | Product model | Download | Runtime |
| --- | --- | ---: | --- |
| Japanese → English | `Xenova/opus-mt-ja-en` q8 | 109.4 MiB | CPU, ONNX Runtime Web/WebAssembly |
| Spanish → English | `Xenova/opus-mt-es-en` q8 | 113.8 MiB | CPU, ONNX Runtime Web/WebAssembly |
| Other selectable languages → English | `Xenova/opus-mt-mul-en` q8 | 112.9 MiB | CPU, ONNX Runtime Web/WebAssembly |
| Any selectable language → any other selectable language | `Xenova/m2m100_418M` q8 | 610.3 MiB | CPU, ONNX Runtime Web/WebAssembly |

All models are optional, pinned, integrity-checked, and loaded only for a selected source/target route. Installed direct Japanese/Spanish models are preferred for their English routes, then the compact multilingual-to-English model, then the universal model. At most one translator is loaded at a time. Exact revisions, artifact hashes, licenses, and declared coverage are recorded in [`docs/licenses/TRANSLATION_MODELS.md`](../licenses/TRANSLATION_MODELS.md).

## Development-WebView integration check — 2026-08-10

The real worker path, not the UI mock, completed the following checks:

- downloaded and SHA-256-verified every pinned Japanese artifact;
- reused the installed model from local storage after page/worker restart with remote model loading disabled;
- removed and reinstalled the Japanese translator through the Settings lifecycle;
- loaded the q8 Japanese graph and translated `今日は何をする予定ですか？` to `What are you planning to do today?`;
- downloaded, verified, loaded, and switched to the Spanish translator after Japanese; and
- translated `Las ventanas azules se abren sobre el jardín.` to `The blue windows open over the garden.`.

The overlay development preview also rendered source and English in separate colors using both stacked and side-by-side layouts. The transcript rendered the source immediately, showed a pending state, and replaced that state with English without rewriting the committed source.

The expanded 2026-08-10 browser integration pass additionally:

- downloaded and SHA-256-verified all pinned compact multilingual-to-English artifacts;
- loaded that real q8 graph and translated a provisional French caption before any final ASR segment existed;
- downloaded and SHA-256-verified all 610.3 MiB of the universal model;
- loaded that real q8 M2M100 graph with explicit Japanese and Spanish language codes and translated `今日は何をする予定ですか？` to `¿Qué planeas hacer hoy?`; and
- rendered three complete wrapped side-by-side source/translation pairs without ellipsizing either column.

On the development browser host, a second warm universal provisional request displayed `Mañana voy a Tokio.` about 3.3 seconds after source injection, including the 420 ms live-translation launch throttle. That is useful integration timing, not a Windows latency benchmark; it confirms the universal CPU route can still be noticeably delayed even though it starts before finalization. The deterministic browser translator was also used to drive rapid changing partials and repeatable layout pressure. Together these checks establish install, cache, graph-load, route-code, provisional-update, and display integration; two successful sentences do not establish broad translation quality.

These are deterministic integration checks, not representative accuracy scores. They prove the download, cache-only load, model-switch, inference, and display contracts on the available Chromium/WebAssembly development path.

## Evidence still required

Before production approval, run the feature in the native Windows WebView against familiar media in the languages being evaluated and record only actionable observations:

- whether Nemotron's committed source text is accurate enough to translate;
- cold translator load time, live-partial translation cadence, and finalized-caption correction delay;
- peak process memory and whether translation causes ASR backlog or UI stalls;
- usefulness across multiple source/target routes, conversational, news, accented, noisy, and short-utterance material; and
- behavior through stop/start, model removal, offline restart, and sustained playback.

No screenshots or recordings are required for an ordinary pre-release spot check. Broader quantitative scoring should wait for redistributable, trustworthy source/English reference material.
