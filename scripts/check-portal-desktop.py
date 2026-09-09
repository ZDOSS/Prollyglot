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

private = Path(os.environ["PROLLYGLOT_PRIVATE_PIPEWIRE"])
assert private.name.startswith("prollyglot-pipewire-")
assert os.environ["XDG_RUNTIME_DIR"] == str(private)
assert os.environ["PIPEWIRE_RUNTIME_DIR"] == str(private)
assert os.environ["DBUS_SESSION_BUS_ADDRESS"].split(",guid=")[0] == f"unix:path={private}/bus"
assert not os.environ.get("DISPLAY") and not os.environ.get("WAYLAND_DISPLAY")
language, mode = sys.argv[1:]
root = Path(os.environ["PROLLYGLOT_DESKTOP_FIXTURE_STATE"])
root.mkdir(parents=True)
assert root.parent == private
env = os.environ.copy()
env.update(GDK_BACKEND="x11", NO_AT_BRIDGE="1", XDG_DATA_HOME=str(root / "data"),
           XDG_CONFIG_HOME=str(root / "config"), XDG_CACHE_HOME=str(root / "cache"),
           XDG_STATE_HOME=str(root / "state"))
# Copy, not a symlink: model verification stamps and cleanup must never write
# into the developer's installed model directory.
model = root / "data/com.prollyglot.desktop/models/visual/ppocrv6-small-multilingual/v3.9.0"
shutil.copytree(os.environ["PROLLYGLOT_VISUAL_OCR_MODEL_DIR"], model)
processes, logs = [], []
session = None


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
    log = open(root / "xvfb.log", "w+")
    logs.append(log)
    # Use a high private display, never :0. WSLg owns a read-only filesystem
    # socket directory; Linux's abstract transport needs no changes there.
    display_number = 100 + secrets.randbelow(900)
    while Path(f"/tmp/.X11-unix/X{display_number}").exists():
        display_number = 100 + secrets.randbelow(900)
    display = subprocess.Popen(["Xvfb", f":{display_number}", "-displayfd", "1", "-screen", "0", "1280x900x24", "-nolisten", "tcp", "-nolisten", "unix"],
                               env=env, stdout=subprocess.PIPE, stderr=log, text=True, start_new_session=True)
    processes.append(display)
    display_id = display.stdout.readline().strip()
    assert display_id == str(display_number) and display.poll() is None
    env["DISPLAY"] = f":{display_id}"
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
    args = {"selection": {"kind": mode if mode.startswith("portal") else "portalWindow"}, "sourceLanguage": language, "targetLanguage": "en", "detectionMode": "focused"}
    js("window.fixtureStart=undefined;window.__TAURI_INTERNALS__.invoke('start_visual_translation',arguments[0]).then(()=>window.fixtureStart={ok:true},error=>window.fixtureStart={error});", args)
    if mode == "cancel":
        wait(lambda: "picker" in (invoke("visual_status").get("message") or ""))
        started = time.monotonic()
        invoke("stop_visual_translation")
        wait(lambda: invoke("visual_status")["state"] == "stopped", 3)
        assert time.monotonic() - started < 2
        assert wait(lambda: js("return window.fixtureStart"))["error"]["code"] == "startupCancelled"
    elif mode == "dismiss":
        assert wait(lambda: js("return window.fixtureStart"))["error"]["code"] == "startupCancelled"
        wait(lambda: invoke("visual_status")["state"] == "stopped")
    else:
        assert wait(lambda: js("return window.fixtureStart")) == {"ok": True}
        update = wait(lambda: js("return window.fixtureText.find(update=>update.visible.length)"))
        original = " ".join(region["text"] for region in update["visible"])
        assert ("你好" in original) if language == "zh" else ("Buenos" in original), original
        # Verify the same presentation contract used by the translation
        # controller, with fixed fixture output independent of installed MT.
        current = invoke("visual_presentation")
        frame = {**current, "runtimeRevision": max(current["runtimeRevision"], update["runtimeRevision"]), "presentationRevision": current["presentationRevision"] + 1000,
                 "sourceWidth": update["source"]["width"], "sourceHeight": update["source"]["height"], "scanning": False,
                 "regions": [{"trackId": region["trackId"], "textRevision": region["textRevision"], "original": region["text"],
                              "translation": "Fixture translation: hello, world.", "translationPending": False, "retained": False, "bounds": region["bounds"]} for region in update["visible"]]}
        assert invoke("update_visual_presentation", {"frame": frame})
        for handle in request("GET", "window/handles"):
            request("POST", "window", {"handle": handle})
            if request("GET", "url").endswith("/visual-overlay.html"):
                break
        wait(lambda: js("return document.body.classList.contains('visual-reader-body') && document.body.innerText.includes('Fixture translation')"))
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
        if mode == "portalDisplay":
            # Close must hide and Stop, not destroy the reusable reader.
            invoke("start_visual_translation", args)
            assert invoke("visual_presentation")["sessionId"] != frame["sessionId"]
            invoke("stop_visual_translation")
            wait(lambda: invoke("visual_status")["state"] == "stopped", 3)
    print(f"Private native {language}/{mode}: passed", flush=True)
except BaseException:
    for log in logs:
        log.flush()
        log.seek(0)
        print(log.read()[-6000:])
    for log in (root / "data/com.prollyglot.desktop/logs").glob("*"):
        print(log.read_text()[-6000:])
    raise
finally:
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
