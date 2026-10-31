"""Keyboard-only contact opening on an isolated Sway fixture.

Usage: ui_keyboard_open.py ISOLATED_SWAY_FIXTURE CLIENT_BINARY

Regression: the contact search focused with Ctrl+F, but Enter did nothing, so a
keyboard user could not open a conversation. Typing a unique name and pressing
Enter must open that contact using only the keyboard.
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from PIL import Image

assert len(sys.argv) == 3, "Usage: ui_keyboard_open.py ISOLATED_SWAY_FIXTURE CLIENT_BINARY"
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
(config / "config.toml").write_text("night = false\nsingle_window = true\nui_zoom = 100\n")
app_env = dict(
    env,
    XDG_CONFIG_HOME=str(fixture / "config"),
    XDG_DATA_HOME=str(fixture / "data"),
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


def wait(app, pid, timeout=30):
    for _ in range(timeout * 10):
        found = [
            n
            for n in windows()
            if n["app_id"] == "escargot-native-" + app and n["pid"] == pid
        ]
        if found:
            return found[0]
        time.sleep(0.1)
    raise AssertionError(f"native window missing: {app} (pid {pid})")


def capture(name, app, pid):
    node = wait(app, pid)
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


def wait_text(name, app, pid, needles, timeout=6.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        path = capture(name, app, pid)
        shown = text(path)
        if any(needle in shown for needle in needles):
            return path, shown
        time.sleep(0.2)
    return None, None


log = (fixture / "ui-keyboard-open.log").open("w")
process = subprocess.Popen([str(binary), "--demo-settings"], env=app_env, stdout=log, stderr=log)
log.close()
try:
    node = wait("contacts", process.pid)
    subprocess.run(
        ["swaymsg", "-s", socket, f'[pid={node["pid"]}] focus'],
        check=True, stdout=subprocess.DEVNULL,
    )
    time.sleep(0.4)
    keys("-M", "ctrl", "-k", "f", "-m", "ctrl")
    keys("alex")
    # Give the filtered list a frame to settle, then open it with Enter. Key
    # delivery on the headless fixture occasionally drops the first press, so
    # retry a bounded number of times; the assertion is still that Enter opens.
    time.sleep(1.0)
    before = text(capture("keyboard-before-enter", "contacts", process.pid))
    assert "write a message" not in before, "The demo must start without an open conversation"
    shown = None
    for _ in range(3):
        keys("-k", "Return")
        _, shown = wait_text(
            "keyboard-open", "contacts", process.pid,
            ["hey! you're actually running this on linux now?", "write a message"],
            timeout=2.0,
        )
        if shown:
            break
    assert shown and (
        "hey! you're actually running this on linux now?" in shown or "write a message" in shown
    ), "Pressing Enter after searching for a contact must open its conversation"
    assert process.poll() is None, "Opening a conversation must keep the process alive"
    print("PASS: Enter opens the filtered contact without a pointer")
finally:
    process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
    time.sleep(0.3)
