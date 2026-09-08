# Bundled native inference runtimes

The Ubuntu package places these CPU libraries in `/usr/lib/prollyglot`, outside
system library search paths. Speech models are separate optional downloads.

- **sherpa-onnx 1.13.4**: upstream Linux x64 shared C API from
  <https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.4>, Apache-2.0.
  The Rust `sherpa-onnx-sys` build selects/downloads that versioned archive.
  `sherpa-onnx-LICENSE.txt` is copied from the matching source tag.
- **ONNX Runtime 1.27.0**: supplied by that same sherpa-onnx archive; confirmed
  through its native `GetVersionString` entry point. The MIT license and upstream
  third-party notices are copied from
  <https://github.com/microsoft/onnxruntime/tree/v1.27.0>.
- **PipeWire / pipewire-rs 0.10.1**: the Rust binding is MIT-licensed; the native
  PipeWire runtime comes from Ubuntu packages. Source:
  <https://gitlab.freedesktop.org/pipewire/pipewire-rs>.
- **GTK / WebKitGTK**: native desktop libraries come from Ubuntu packages rather
  than being copied into the application.

The packaged copies of the two inference libraries have only their runtime
search path changed to `$ORIGIN`, so they find each other beside the executable
or in the package's private library directory without relying on the build host.
The upstream library files in Cargo's cache are unchanged.

This initial development package is not a supported public release. A complete
release-wide notice inventory, including statically linked Rust/frontend and
sherpa-onnx dependencies, remains part of distribution hardening for both
platforms. See the neighboring ASR, OCR, and translation provenance records.
