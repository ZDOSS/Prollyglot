#!/usr/bin/env python3
"""Build a test-only xdg_foreign exporter for private headless Weston 14."""
from pathlib import Path
import shlex
import subprocess

root = Path(__file__).resolve().parent.parent
output = root / "target/wayland-fixture"
output.mkdir(parents=True, exist_ok=True)
protocols = subprocess.check_output(
    ["pkg-config", "--variable=pkgdatadir", "wayland-protocols"], text=True).strip()
protocol = Path(protocols) / "unstable/xdg-foreign/xdg-foreign-unstable-v2.xml"
for kind, filename in [("server-header", "xdg-foreign-server.h"), ("private-code", "xdg-foreign-protocol.c")]:
    subprocess.run(["wayland-scanner", kind, protocol, output / filename], check=True)
flags = shlex.split(subprocess.check_output(
    ["pkg-config", "--cflags", "libweston-14", "wayland-server"], text=True))
libs = shlex.split(subprocess.check_output(["pkg-config", "--libs", "wayland-server"], text=True))
subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC",
                *flags, f"-I{output}", root / "scripts/fixtures/wayland-exporter.c",
                output / "xdg-foreign-protocol.c", *libs,
                "-o", output / "exporter.so"], check=True)
print(output / "exporter.so")
