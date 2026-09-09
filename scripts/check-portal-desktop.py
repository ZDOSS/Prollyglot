#!/usr/bin/env python3
"""Child of the ignored Rust desktop fixture; never use the owner's display.

Exercises the actual GTK app, OCR, presentation IPC and reader. Translation is
a deterministic fixture here, not a model quality/latency acceptance result.
"""
import json
import os
from pathlib import Path
import shutil
import signal
import secrets
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
sys.dont_write_bytecode = True
from private_x11 import PrivateX11

private = Path(os.environ["PROLLYGLOT_PRIVATE_PIPEWIRE"])
assert private.name.startswith("prollyglot-pipewire-")
assert os.environ["XDG_RUNTIME_DIR"] == str(private)
assert os.environ["PIPEWIRE_RUNTIME_DIR"] == str(private)
assert os.environ["DBUS_SESSION_BUS_ADDRESS"].split(",guid=")[0] == f"unix:path={private}/bus"
assert not any(os.environ.get(name) for name in ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET"))
language, mode, backend = sys.argv[1:]
assert backend in ("x11", "wayland")
wayland = backend == "wayland"
scale = 2 if mode.endswith("2x") else 1
root = Path(os.environ["PROLLYGLOT_DESKTOP_FIXTURE_STATE"])
root.mkdir(parents=True)
assert root.parent == private
env = os.environ.copy()
env.update(GDK_BACKEND=backend, GDK_SCALE=str(scale), NO_AT_BRIDGE="1", XDG_DATA_HOME=str(root / "data"),
           XDG_CONFIG_HOME=str(root / "config"), XDG_CACHE_HOME=str(root / "cache"),
           XDG_STATE_HOME=str(root / "state"))
# Copy, not a symlink: model verification stamps and cleanup must never write
# into the developer's installed model directory.
model = root / "data/com.prollyglot.desktop/models/visual/ppocrv6-small-multilingual/v3.9.0"
shutil.copytree(os.environ["PROLLYGLOT_VISUAL_OCR_MODEL_DIR"], model)
processes, logs = [], []
session = None
x11 = None


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


port, native_port = free_port(), free_port()


def raw(method, path, body=None):
    request = urllib.request.Request(f"http://127.0.0.1:{port}/{path}",
                                     data=None if body is None else json.dumps(body).encode(),
                                     method=method, headers={"Content-Type": "application/json"})
    try:
        return json.load(urllib.request.urlopen(request, timeout=20))["value"]
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error


def request(method, path, body=None):
    return raw(method, f"session/{session}/{path}", body)


def js(script, *args):
    return request("POST", "execute/sync", {"script": script, "args": list(args)})


def invoke(command, args=None):
    result = request("POST", "execute/async", {"script": "const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(value=>done({ok:true,value}),error=>done({ok:false,error}));", "args": [command, args or {}]})
    assert result["ok"], result
    return result["value"]


def wait(predicate, seconds=15):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(.08)
    raise AssertionError("Private native desktop fixture timed out")


try:
    if wayland:
        # Explicit headless/Pixman backends cannot connect to a desktop,
        # acquire a DRM device, or create a nested WSLg window.
        env.update(WAYLAND_DISPLAY=f"prollyglot-{secrets.token_hex(8)}",
                   XDG_SESSION_TYPE="wayland", LIBGL_ALWAYS_SOFTWARE="1",
                   WEBKIT_DISABLE_DMABUF_RENDERER="1")
        log = open(root / "weston.log", "w+")
        logs.append(log)
        modules = []
        if mode != "parentUnavailable":
            module = Path(__file__).resolve().parent.parent / "target/wayland-fixture/exporter.so"
            assert module.is_file(), "Run scripts/build-wayland-fixture.py first"
            env["PROLLYGLOT_WAYLAND_EXPORT_MODE"] = "stall" if mode in ("parentTimeout", "cancelParent") else "normal"
            modules = [f"--modules={module}"]
        display = subprocess.Popen(["weston", "--backend=headless", "--renderer=pixman",
                                    "--shell=kiosk-shell.so", "--no-config", "--idle-time=0",
                                    "--width=1280", "--height=900", f"--socket={env['WAYLAND_DISPLAY']}", *modules],
                                   env=env, stdout=log, stderr=log, start_new_session=True)
        processes.append(display)
        wait(lambda: (private / env["WAYLAND_DISPLAY"]).is_socket())
        assert display.poll() is None
    else:
        log = open(root / "xvfb.log", "w+")
        logs.append(log)
        # Use a high private display, never :0. WSLg owns a read-only filesystem
        # socket directory; Linux's abstract transport needs no changes there.
        display_number = 100 + secrets.randbelow(900)
        while Path(f"/tmp/.X11-unix/X{display_number}").exists():
            display_number = 100 + secrets.randbelow(900)
        display = subprocess.Popen(["Xvfb", f":{display_number}", "-displayfd", "1", "-screen", "0", f"{1280 * scale}x{900 * scale}x24", "-nolisten", "tcp", "-nolisten", "unix"],
                                   env=env, stdout=subprocess.PIPE, stderr=log, text=True, start_new_session=True)
        processes.append(display)
        display_id = display.stdout.readline().strip()
        assert display_id == str(display_number) and display.poll() is None
        env["DISPLAY"] = f":{display_id}"
        x11 = PrivateX11(display, env["DISPLAY"])
    log = open(root / "driver.log", "w+")
    logs.append(log)
    driver = subprocess.Popen(["tauri-driver", "--port", str(port), "--native-port", str(native_port)], env=env, stdout=log, stderr=log, start_new_session=True)
    processes.append(driver)

    def ready():
        try:
            return raw("GET", "status")
        except (urllib.error.URLError, ConnectionError):
            return False

    wait(ready)
    session = raw("POST", "session", {"capabilities": {"alwaysMatch": {"tauri:options": {"application": os.environ["PROLLYGLOT_DESKTOP_TEST_BINARY"]}}}})["sessionId"]
    request("POST", "timeouts", {"script": 15000})
    wait(lambda: js("return !!window.__TAURI_INTERNALS__"))
    main_handle = request("GET", "window")
    caps = invoke("visual_capabilities")
    assert caps["portalScreenCast"] and not caps["windowsGraphicsCapture"]
    assert invoke("visual_source_snapshot") == {"windows": [], "displays": []}
    wait(lambda: invoke("visual_model_status")["models"][0]["phase"] == "ready")
    js("window.fixtureText=[];window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'visual-text-update',target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(event=>window.fixtureText.push(event.payload))});")
    kind = mode if mode.startswith("portal") else "portalRegion" if "Region" in mode else "portalDisplay" if mode == "anchorDisplay" else "portalWindow"
    args = {"selection": {"kind": kind}, "sourceLanguage": language, "targetLanguage": "en", "detectionMode": "focused"}
    js("window.fixtureStart=undefined;window.__TAURI_INTERNALS__.invoke('start_visual_translation',arguments[0]).then(()=>window.fixtureStart={ok:true},error=>window.fixtureStart={error});", args)
    if kind == "portalRegion":
        if wayland:
            wait(lambda: "region preview" in (invoke("visual_status").get("message") or ""))
        else:
            selector = wait(lambda: x11.find("Choose screen region — Prollyglot"))
        assert js("return window.fixtureText.length") == 0, "OCR must wait for an explicit region"
        assert "region preview" in invoke("visual_status")["message"]
        if mode == "dismissRegion":
            x11.focus(selector)
            x11.tap("Escape")
        elif mode.startswith("anchorRegion"):
            x11.focus(selector)
            # Exercise a real drag, then use accessible coordinate controls to
            # make the final crop independent of GTK theme spacing.
            sx, sy, sw, sh = x11.geometry(selector)
            x11.drag((sx + sw // 3, sy + sh // 3), (sx + sw * 2 // 3, sy + sh * 2 // 3))
            for mnemonic, value in [("x", 140), ("y", 590), ("w", 1000), ("h", 200)]:
                x11.coordinate(mnemonic, value, commit=mnemonic != "h")
            x11.key("Alt_L", True)
            x11.tap("u")
            x11.key("Alt_L", False)
            wait(lambda: not x11.find("Choose screen region — Prollyglot"))
    if mode in ("cancel", "cancelRegion", "cancelParent"):
        if mode == "cancelParent":
            wait(lambda: "export" in (root / "exports.log").read_text())
        else:
            wait(lambda: ("preview" if mode == "cancelRegion" else "picker") in (invoke("visual_status").get("message") or ""))
        started = time.monotonic()
        invoke("stop_visual_translation")
        wait(lambda: invoke("visual_status")["state"] == "stopped", 3)
        assert time.monotonic() - started < 2
        assert wait(lambda: js("return window.fixtureStart"))["error"]["code"] == "startupCancelled"
    elif mode in ("dismiss", "dismissRegion"):
        assert wait(lambda: js("return window.fixtureStart"))["error"]["code"] == "startupCancelled"
        wait(lambda: invoke("visual_status")["state"] == "stopped")
    else:
        result = wait(lambda: js("return window.fixtureStart"))
        assert result == {"ok": True}, result
        if wayland and mode not in ("parentUnavailable", "parentTimeout"):
            assert (root / "exports.log").read_text().splitlines() == ["export"], "Picker parent must remain exported while sharing"
        update = wait(lambda: js("return window.fixtureText.find(update=>update.visible.length)"))
        original = " ".join(region["text"] for region in update["visible"])
        assert ("你好" in original) if language == "zh" else ("Buenos" in original), original
        if mode.startswith("anchorRegion"):
            assert "Buenos" not in original, "Text outside the chosen region reached OCR"
            assert (update["source"]["width"], update["source"]["height"]) == (1000, 200)
        # Verify the same presentation contract used by the translation
        # controller, with fixed fixture output independent of installed MT.
        current = invoke("visual_presentation")
        frame = {**current, "runtimeRevision": max(current["runtimeRevision"], update["runtimeRevision"]), "presentationRevision": current["presentationRevision"] + 1000,
                 "anchored": not mode.startswith("anchor"),  # Host must ignore caller placement.
                 "sourceWidth": update["source"]["width"], "sourceHeight": update["source"]["height"], "scanning": False,
                 "regions": [{"trackId": region["trackId"], "textRevision": region["textRevision"], "original": region["text"],
                              "translation": "Fixture translation: hello, world.", "translationPending": False, "retained": False, "bounds": region["bounds"]} for region in update["visible"]]}
        assert invoke("update_visual_presentation", {"frame": frame})
        assert invoke("visual_presentation")["anchored"] == mode.startswith("anchor")
        for handle in request("GET", "window/handles"):
            request("POST", "window", {"handle": handle})
            if request("GET", "url").endswith("/visual-overlay.html"):
                break
        wait(lambda: js("return document.body.innerText.includes('Fixture translation')"))
        if mode.startswith("anchor"):
            assert not js("return document.body.classList.contains('visual-reader-body')")
            assert js("return getComputedStyle(document.querySelector('.visual-translation-label')).position") == "absolute"
            overlay_window = x11.find("Screen translations — Prollyglot")
            expected = (140, 590, 1000, 200) if mode.startswith("anchorRegion") else (0, 0, 1280, 900)
            assert x11.geometry(overlay_window) == tuple(value * scale for value in expected)
            assert x11.input_shape(overlay_window) == 0, "Anchor must let input reach the source"
            assert not x11.accepts_focus(overlay_window), "Anchor must not steal keyboard focus"
            time.sleep(.55)  # Allow native placement verification to finish.
            assert invoke("visual_presentation")["anchored"]
            # An actual native displacement must trigger the reader fallback.
            x11.move(overlay_window, 35, 45)
            wait(lambda: js("return document.body.classList.contains('visual-reader-body')"))
            assert not invoke("visual_presentation")["anchored"]
            assert x11.input_shape(overlay_window) > 0
            assert x11.accepts_focus(overlay_window)
        else:
            assert js("return document.body.classList.contains('visual-reader-body')")
        assert js("return getComputedStyle(document.querySelector('.visual-translation-label')).position") == "static"
        started = time.monotonic()
        if mode == "portalDisplay":
            invoke("plugin:window|close", {"label": "visual-overlay"})
        else:
            js("document.querySelector('.visual-reader-header button').click()")
        request("POST", "window", {"handle": main_handle})
        wait(lambda: invoke("visual_status")["state"] == "stopped", 3)
        assert time.monotonic() - started < 2
        assert invoke("visual_presentation")["regions"] == []
        assert not invoke("update_visual_presentation", {"frame": {**frame, "presentationRevision": frame["presentationRevision"] + 100}})
        if mode in ("portalDisplay", "anchorDisplay"):
            # Close must hide and Stop, not destroy the reusable reader.
            invoke("start_visual_translation", args)
            assert invoke("visual_presentation")["sessionId"] != frame["sessionId"]
            if mode == "anchorDisplay":
                time.sleep(.55)
                assert invoke("visual_presentation")["anchored"], "Restart must restore an eligible anchor after reader fallback"
                assert x11.geometry(overlay_window) == (0, 0, 1280, 900)
            invoke("stop_visual_translation")
            wait(lambda: invoke("visual_status")["state"] == "stopped", 3)
    if wayland and mode != "parentUnavailable":
        def exports_released():
            events = (root / "exports.log").read_text().splitlines()
            return events.count("export") > 0 and events.count("export") == events.count("release")
        # Check before terminating the app/compositor: Stop must release the
        # handle; process teardown cannot hide a per-session export leak.
        wait(exports_released, 2)
    print(f"Private native {backend} {language}/{mode}: passed", flush=True)
    if x11:
        assert not x11.find("Choose screen region — Prollyglot")
except BaseException:
    if x11: print("Private X window titles:", getattr(x11, "titles", []), flush=True)
    for log in logs:
        log.flush()
        log.seek(0)
        print(log.read()[-6000:])
    for log in (root / "data/com.prollyglot.desktop/logs").glob("*"):
        print(log.read_text()[-6000:])
    raise
finally:
    if x11:
        x11.close()
    if session:
        try:
            raw("DELETE", f"session/{session}")
        except (OSError, RuntimeError):
            pass
    for process in reversed(processes):
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
    for log in logs:
        log.close()
