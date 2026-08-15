#!/usr/bin/env bash
set -euo pipefail

# embed-resource expects llvm-rc syntax when cross-checking MSVC from Linux.
# WSL commonly has Microsoft's compatible resource compiler on the mounted
# Windows SDK instead. Advertise the expected probe response and remove the one
# LLVM-only argument before forwarding the otherwise compatible invocation.
if [[ "${1:-}" == "-V" ]]; then
  echo "OVERVIEW: LLVM Resource Converter"
  exit 0
fi

: "${PROLLYGLOT_WINDOWS_RC:?set PROLLYGLOT_WINDOWS_RC to the Windows SDK rc.exe path}"
arguments=()
for argument in "$@"; do
  if [[ "$argument" != "--" ]]; then
    arguments+=("$argument")
  fi
done
exec "$PROLLYGLOT_WINDOWS_RC" "${arguments[@]}"
