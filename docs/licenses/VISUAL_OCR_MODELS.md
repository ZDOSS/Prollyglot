# Visual OCR runtime and model record

Prollyglot's experimental visual-translation mode uses one optional local OCR
pack. It is not included in the source tree or downloaded automatically. The
user must choose **Download** in Settings or in the Translate Screen setup, and
the model manager verifies every file's declared size and SHA-256 before making
the pack available.

## PP-OCRv6 Small · Multilingual

- Prollyglot model ID: `ppocrv6-small-multilingual`
- Manifest version: `v3.9.0`
- Upstream model source: [RapidAI/RapidOCR on ModelScope](https://www.modelscope.cn/models/RapidAI/RapidOCR)
- Declared license: Apache-2.0
- Installed size: 31,824,456 bytes (30.4 MiB)
- Prollyglot manifest: [`assets/model-manifests/visual-ocr-ppocrv6-small.json`](../../assets/model-manifests/visual-ocr-ppocrv6-small.json)

| Role | Artifact | Source revision | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| Detection | `PP-OCRv6_det_small.onnx` | RapidOCR `v3.9.0` | 9,929,594 | `090f04abcd9d9a7498bc4ebf677e4cb9bdce1fe4197ddb7e529f1ef44e1ff94f` |
| Classification | `ch_ppocr_mobile_v2.0_cls_mobile.onnx` | RapidOCR `v3.9.0` | 585,532 | `e47acedf663230f8863ff1ab0e64dd2d82b838fceb5957146dab185a89d6215c` |
| Recognition | `PP-OCRv6_rec_small.onnx` | RapidOCR `v3.9.0` | 21,234,383 | `6f327246b50388f3c176ae304bd95767ea6dc0c9ae92153ef8cbe210b3c14884` |
| Dictionary | `ppocrv6_dict.txt` | RapidOCR `master`, content pinned by hash | 74,947 | `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d` |

The unified recognizer is exposed for the 29 language choices shared with the
current translation UI. That catalog is a product routing surface, not a claim
that every language has already passed representative accuracy testing.

## Runtime

OCR inference uses
[`rapidocr-core` 0.2.2](https://crates.io/crates/rapidocr-core/0.2.2), licensed
Apache-2.0. The published package records upstream commit
`bc4afd4a3fc5cb65f0358c902241d547e4775274`; its registry archive SHA-256 is
`2afdaea55d9e8daf8f547a48a7fb45a43dbe076db3b9489c34386521cbdac294`.
That exact package source is vendored under `vendor/rapidocr-core` because the
published manifest forces ONNX Runtime default features that conflict with the
desktop bundle. The Rust implementation is unchanged. Packaging-only manifest
changes pin ONNX Runtime rc.13, use Rustls for its build-time download, copy the
required runtime library, and disable unused image codecs and RapidOCR's own
downloader. Full details live in
[`vendor/rapidocr-core/PROLLYGLOT-VENDOR-NOTES.md`](../../vendor/rapidocr-core/PROLLYGLOT-VENDOR-NOTES.md).

Windows capture uses
[`windows-capture` 2.0.1](https://crates.io/crates/windows-capture/2.0.1),
licensed MIT. Captured frames are transient inputs: Prollyglot does not save
them, include pixels in logs, or send them to a network service.

## Distribution gate

Before a public binary release, recheck the upstream license files and model
host redistribution terms against the exact pinned artifacts, and include all
required notices in the packaged application. Changing any artifact, URL,
runtime revision, or hash requires updating both the manifest and this record.
