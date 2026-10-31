"""Separate-window smoke test on an isolated Sway fixture.

Usage: ui_second_window.py ISOLATED_SWAY_FIXTURE CLIENT_BINARY

Regression: opening the demo conversation during the contacts frame aborted
GPUI on Wayland. Both windows must render, and the chat composer must accept a
message without closing either window.
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from PIL import Image

assert len(sys.argv) == 3, "Usage: ui_second_window.py ISOLATED_SWAY_FIXTURE CLIENT_BINARY"
fixture = Path(sys.argv[1]).resolve()
binary = Path(sys.argv[2]).resolve()
assert (fixture / "runtime").is_dir(), "Use a separate headless Sway runtime"
socket = str(next((fixture / "runtime").glob("sway-ipc*.sock")))
env = dict(
    os.environ,
    XDG_RUNTIME_DIR=str(fixture / "runtime"),
    WAYLAND_DISPLAY="wayland-1",
    TMPDIR=str(fixture / "tmp"),
    ESCARGOT_NATIVE_ASSETS=os.environ.get(
        "ESCARGOT_NATIVE_ASSETS", str(Path.home() / ".local/share/escargot-native")
    ),
)
config = fixture / "config/escargot-native"
config.mkdir(parents=True, exist_ok=True)
(config / "config.toml").write_text("night = false\nsingle_window = false\nui_zoom = 100\n")
app_env = dict(
    env,
    XDG_CONFIG_HOME=str(fixture / "config"),
    XDG_DATA_HOME=str(fixture / "data"),
    ESCARGOT_DEMO_CHAT="alex@escargot.chat",
)


def walk(node):
    yield node
    for child in node.get("nodes", []) + node.get("floating_nodes", []):
        yield from walk(child)


def windows():
    tree = json.loads(
        subprocess.check_output(["swaymsg", "-s", socket, "-t", "get_tree", "-r"], text=True)
    )
    return [n for n in walk(tree) if n.get("app_id", "").startswith("escargot-native-")]


def wait(app, timeout=30):
    for _ in range(timeout * 10):
        found = [n for n in windows() if n["app_id"] == "escargot-native-" + app]
        if found:
            return found[0]
        time.sleep(0.1)
    raise AssertionError(f"native window missing: {app}")


def capture(name, app):
    node = wait(app)
    time.sleep(0.4)
    subprocess.run(["grim", str(fixture / "screen.png")], env=env, check=True)
    rect = node["rect"]
    image = Image.open(fixture / "screen.png").crop(
        (rect["x"], rect["y"], rect["x"] + rect["width"], rect["y"] + rect["height"])
    )
    path = fixture / f"{name}.png"
    image.save(path)
    return path


def text(path):
    return subprocess.run(
        ["tesseract", str(path), "stdout", "--psm", "11"],
        capture_output=True, text=True, check=True,
    ).stdout.casefold()


def keys(*args):
    subprocess.run(["wtype", *args], env=env, check=True)
    time.sleep(0.25)


log = (fixture / "ui-second-window.log").open("w")
process = subprocess.Popen([str(binary), "--demo-settings"], env=app_env, stdout=log, stderr=log)
log.close()
try:
    wait("contacts")
    wait("chat")
    assert process.poll() is None, "Opening the second window must not abort the process"
    subprocess.run(
        ["swaymsg", "-s", socket, '[app_id="escargot-native-chat"] focus'],
        check=True, stdout=subprocess.DEVNULL,
    )
    capture("second-window-chat", "chat")
    keys("second window probe")
    keys("-k", "Return")
    time.sleep(0.5)
    assert process.poll() is None, "Sending in the separate chat window must keep the process alive"
    shown = capture("second-window-sent", "chat")
    assert "second window probe" in text(shown), "The separate chat window must show the sent line"
    assert any(n["app_id"] == "escargot-native-contacts" for n in windows()), (
        "The contacts window must stay open beside the conversation"
    )
    print("PASS: separate windows open and message without aborting")
finally:
    process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
    time.sleep(0.3)
