"""The inline Add contact input must be reachable from the keyboard tab order.

Usage: ui_add_contact_focus.py ISOLATED_SWAY_FIXTURE CLIENT_BINARY

Regression: the field is focused when the form opens, but after focus leaves it
the input is skipped by Tab (only Close, Cancel and Send request are stops), so
the address cannot be edited without closing and reopening the form.
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from PIL import Image, ImageStat

assert len(sys.argv) == 3, "Usage: ui_add_contact_focus.py ISOLATED_SWAY_FIXTURE CLIENT_BINARY"
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
    time.sleep(0.3)
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


def field_text(path):
    image = Image.open(path).convert("RGB")
    # Text line inside the add-contact field in the 1280x720 fixture.
    crop = image.crop((8, 338, 160, 370))
    crop = crop.resize((crop.width * 6, crop.height * 6))
    enlarged = path.with_name("add-input-field.png")
    crop.save(enlarged)
    return subprocess.run(
        [
            "tesseract", str(enlarged), "stdout", "--psm", "7",
            "-c", "tessedit_char_whitelist=0123456789",
        ],
        capture_output=True, text=True, check=True,
    ).stdout.casefold()


def field_focused(path):
    image = Image.open(path).convert("RGB")
    # Second border row of the add-contact field in the 1280x720 fixture; it is
    # blue only while the input owns the focus (white when focus is elsewhere).
    row = image.crop((30, 337, 270, 338))
    mean = sum(ImageStat.Stat(row).mean) / 3
    return mean < 140


def keys(*args):
    subprocess.run(["wtype", *args], env=env, check=True)
    time.sleep(0.25)


def wait_field(name, app, pid, focused, timeout=6.0):
    """Capture until the field's focus ring matches; headless repaints can lag."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        path = capture(name, app, pid)
        if field_focused(path) is focused:
            return path
        time.sleep(0.2)
    state = "focused" if focused else "unfocused"
    raise AssertionError(f"The Add contact input never became {state}")


def tab_until(name, app, pid, focused, key, attempts=4):
    """Press Tab/Shift+Tab until the focus ring matches.

    Headless key delivery is occasionally dropped at the start of a session, so
    a bounded retry keeps the regression about focus order, not timing. In the
    broken state the input never leaves the ring, so every attempt is spent and
    the test still fails.
    """
    for _ in range(attempts):
        keys(*key)
        try:
            return wait_field(name, app, pid, focused, timeout=1.4)
        except AssertionError:
            continue
    state = "focused" if focused else "unfocused"
    raise AssertionError(f"The Add contact input never became {state}")


log = (fixture / "ui-add-contact-focus.log").open("w")
process = subprocess.Popen([str(binary), "--demo-settings"], env=app_env, stdout=log, stderr=log)
log.close()
try:
    node = wait("contacts", process.pid)
    subprocess.run(
        ["swaymsg", "-s", socket, f'[pid={node["pid"]}] focus'],
        check=True, stdout=subprocess.DEVNULL,
    )
    time.sleep(0.4)
    keys("-M", "ctrl", "-k", "n", "-m", "ctrl")
    time.sleep(0.5)
    opened = wait_field("add-form-open", "contacts", process.pid, True)
    assert "add a contact" in text(opened), "Ctrl+N must open the inline add contact form"

    # Leave the field with Tab and return with Shift+Tab. Without a tab stop on
    # the input, Tab cannot move focus and Shift+Tab never comes back to it.
    tab_until("add-form-tab-away", "contacts", process.pid, False, ("-k", "Tab"))
    tab_until(
        "add-form-tab-back", "contacts", process.pid, True,
        ("-M", "shift", "-k", "Tab", "-m", "shift"),
    )

    keys("777")
    time.sleep(1.0)
    typed = capture("add-form-tab-typed", "contacts", process.pid)
    # The placeholder has no digits; any digit proves the keystrokes landed in
    # the field. The focus ring above is the primary assertion.
    assert "7" in field_text(typed), (
        "The refocused Add contact input must accept typed text"
    )
    assert process.poll() is None, "Keyboard editing must not close the process"
    print("PASS: Tab reaches the Add contact input and accepts text")
finally:
    process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
    time.sleep(0.3)
