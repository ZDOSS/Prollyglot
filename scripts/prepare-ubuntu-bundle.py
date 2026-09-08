#!/usr/bin/env python3
"""Stage the linked speech runtime for Tauri's Ubuntu-only Debian bundle."""
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess

root = Path(__file__).resolve().parent.parent
release = platform.freedesktop_os_release()
if release.get("ID") != "ubuntu" or release.get("VERSION_ID") != "26.04" or platform.machine() != "x86_64":
    raise SystemExit("Build the experimental Ubuntu package on Ubuntu 26.04 LTS amd64.")

metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=root))
profile = "debug" if os.environ.get("TAURI_ENV_DEBUG") == "true" else "release"
target = Path(metadata["target_directory"])
# Explicit native --target builds use a target-triple subdirectory.
triple = os.environ.get("TAURI_ENV_TARGET_TRIPLE") or os.environ.get("CARGO_BUILD_TARGET")
if triple and (target / triple / profile / "prollyglot-desktop").is_file():
    target /= triple
binary_dir = target / profile
destination = root / "target" / "ubuntu-runtime"
destination.mkdir(parents=True, exist_ok=True)
for name in ("libsherpa-onnx-c-api.so", "libonnxruntime.so"):
    source = binary_dir / name
    if not source.is_file():
        raise SystemExit(f"Missing native runtime {source}; finish the desktop build first.")
    shutil.copy2(source, destination / name)
    # Keep the private runtime independent of paths on the build machine.
    subprocess.run(["patchelf", "--set-rpath", "$ORIGIN", str(destination / name)], check=True)
print(f"Staged Ubuntu {profile} runtime in {destination}")
