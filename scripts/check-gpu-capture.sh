#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test --locked -p prollyglot-visual-pipewire --lib --no-run
# Never inherit an owner display or use a hardware renderer for this fixture.
env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET -u XAUTHORITY \
  LIBGL_ALWAYS_SOFTWARE=1 PROLLYGLOT_PRIVATE_GPU_TEST=1 \
  cargo test --locked -p prollyglot-visual-pipewire --lib software_egl_readback \
  -- --ignored --nocapture --test-threads=1
if [[ -n "${PROLLYGLOT_DMABUF_RENDER_NODE:-}" ]]; then
  env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET -u XAUTHORITY -u LIBGL_ALWAYS_SOFTWARE \
    cargo test --locked -p prollyglot-visual-pipewire --lib real_dmabuf_import \
    -- --ignored --nocapture --test-threads=1
else
  echo "Real DMA-BUF import not tested: supply PROLLYGLOT_DMABUF_RENDER_NODE=/dev/dri/renderD… on a GPU host."
fi
