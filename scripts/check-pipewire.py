#!/usr/bin/env python3
"""Run native capture tests on a private PipeWire graph with no hardware nodes."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def main():
    # Compilation is outside the private session so a slow first build does not
    # leave unnecessary audio/session-manager processes running.
    subprocess.run(["cargo", "test", "--locked", "-p", "prollyglot-audio-pipewire",
                    "--test", "pipewire", "--no-run"], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="prollyglot-pipewire-") as directory:
        root = Path(directory)
        env = os.environ.copy()
        env.update(PIPEWIRE_RUNTIME_DIR=directory, XDG_RUNTIME_DIR=directory,
                   PIPEWIRE_REMOTE="pipewire-0", PROLLYGLOT_PRIVATE_PIPEWIRE=directory,
                   XDG_CONFIG_HOME=str(root / "config"), XDG_STATE_HOME=str(root / "state"),
                   XDG_CACHE_HOME=str(root / "cache"))
        processes, logs = [], []
        try:
            dbus = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"],
                                    stdout=subprocess.PIPE, text=True, env=env)
            processes.append(dbus)
            env["DBUS_SESSION_BUS_ADDRESS"] = dbus.stdout.readline().strip()
            for name, args in [
                ("pipewire", ["pipewire", "-P", "{ module.rt = false }"]),
                # The policy profile links our synthetic outputs and supplies
                # default metadata. It never loads audio/video hardware monitors.
                ("wireplumber", ["wireplumber", "--profile", "policy"]),
            ]:
                log = open(root / f"{name}.log", "w+")
                logs.append(log)
                processes.append(subprocess.Popen(args, env=env, stdout=log, stderr=log))
                time.sleep(0.5)
                if processes[-1].poll() is not None:
                    raise RuntimeError(f"private {name} exited during startup")
            test_command = sys.argv[1:] or ["cargo", "test", "--locked", "-p", "prollyglot-audio-pipewire",
                                           "--test", "pipewire", "--", "--ignored", "--nocapture", "--test-threads=1"]
            subprocess.run(test_command, cwd=ROOT, env=env, check=True, timeout=180)
        except BaseException:
            for log in logs:
                log.seek(0)
                print(log.read()[-8000:])
            raise
        finally:
            for process in reversed(processes):
                process.terminate()
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            for log in logs:
                log.close()


if __name__ == "__main__":
    main()
