"""Exercise zoom and independent wallpapers on the isolated Sway contact-flow fixture."""
import os
from pathlib import Path
import subprocess
import time
import tomllib
import ui_contact_flow as ui

settings = ui.p / "config/escargot-native/config.toml"
settings.write_text("night = false\nsingle_window = true\nui_zoom = 100\n")
subprocess.run(["swaymsg", "-s", ui.socket, "output HEADLESS-1 mode 1400x1200@60Hz"], check=True, stdout=subprocess.DEVNULL)

def profile():
    return tomllib.loads(settings.read_text())

def expect_text(phrase, name):
    for _ in range(12):
        shot = ui.capture(name, "contacts")
        if phrase in ui.text(shot):
            return shot
        time.sleep(.1)
    raise AssertionError(f"The native window did not show {phrase!r}")

def tabs(count):
    args = []
    for _ in range(count):
        args.extend(["-k", "Tab", "-s", "80"])
    ui.keys(*args, "-k", "Return")

def shift_tabs(count):
    args = []
    for _ in range(count):
        args.extend(["-M", "shift", "-k", "Tab", "-s", "80", "-m", "shift"])
    ui.keys(*args, "-k", "Return")

def press_tabs(count):
    args = []
    for _ in range(count):
        args.extend(["-k", "Tab", "-s", "80"])
    ui.keys(*args)

def start(chat):
    env = dict(ui.env, XDG_CONFIG_HOME=str(ui.p / "config"), XDG_DATA_HOME=str(ui.p / "data"))
    for key in ["DISPLAY", "ESCARGOT_SETTINGS", "ESCARGOT_DEMO_REQUESTS", "ESCARGOT_DEMO_CHAT"]:
        env.pop(key, None)
    if chat:
        env["ESCARGOT_DEMO_CHAT"] = "alex@escargot.chat"
    log = (ui.p / "ui-appearance.log").open("a")
    process = subprocess.Popen([str(ui.binary), "--demo-settings"], env=env, stdout=log, stderr=log)
    log.close()
    for _ in range(300):
        assert process.poll() is None, "The native appearance fixture exited before rendering"
        if any(node["app_id"] == "escargot-native-contacts" for node in ui.windows()):
            return process
        time.sleep(.1)
    process.terminate()
    process.wait(timeout=10)
    raise AssertionError("The native appearance fixture did not render")

process = start(True)
try:
    ui.capture("zoom-baseline", "contacts")
    ui.ctrl("Tab")
    ui.ctrl("Tab")
    ui.keys("Zoom test kept this draft")
    ui.ctrl("plus")
    assert profile()["ui_zoom"] == 125, "Ctrl+plus must enlarge the interface while composing"
    ui.capture("zoom-larger", "contacts")
    for _ in range(3):
        ui.ctrl("plus")
    assert profile()["ui_zoom"] == 200
    ui.capture("zoom-maximum", "contacts")
    for _ in range(5):
        ui.ctrl("minus")
    assert profile()["ui_zoom"] == 75, "Ctrl+minus must reduce the interface"
    ui.capture("zoom-smaller", "contacts")
    ui.ctrl("0")
    assert profile()["ui_zoom"] == 100, "Ctrl+0 must restore normal size"
    ui.keys("-k", "Return")
    assert "zoom test kept this draft" in ui.text(ui.capture("zoom-draft-preserved", "contacts")), "Zoom must preserve the composed message"
    print("PASS: keyboard zoom and draft preservation", flush=True)
    ui.ctrl("w")
    expect_text("good to see you", "closed-zoom-chat")
    ui.ctrl("comma")
    expect_text("choose how new chats open", "opened-appearance")
    subprocess.run(["swaymsg", "-s", ui.socket, '[app_id="escargot-native-contacts"] resize set width 1000 px height 1100 px'], check=True, stdout=subprocess.DEVNULL)
    shot = ui.capture("independent-appearance", "contacts")
    subprocess.run(["python3", "tests/ui_appearance.py", str(shot)], check=True)
    tabs(8)
    shot = ui.capture("interface-wallpaper-choices", "contacts")
    subprocess.run(["python3", "tests/ui_appearance.py", str(shot), "picker"], check=True)
    tabs(4)
    assert profile().get("interface_wallpaper") == "blue-water" and profile().get("wallpaper") is None, "Interface selection must not change the conversation background"
    tabs(2)
    tabs(8)
    assert profile().get("interface_wallpaper") == "blue-water" and profile().get("wallpaper") == "night-forest", "Each surface must keep its own background"
    shift_tabs(6)
    assert profile()["night"] and profile()["interface_wallpaper"] == "blue-water" and profile()["wallpaper"] == "night-forest", "Switching palettes must preserve both choices"
    ui.capture("independent-backgrounds-night", "contacts")
    ui.ctrl("plus")
    assert profile()["ui_zoom"] == 125, "The chosen zoom must be persisted for the next launch"
    print("PASS: independent Light/Night wallpaper selections", flush=True)
finally:
    process.terminate()
    process.wait(timeout=10)
    time.sleep(.3)

process = start(False)
try:
    ui.capture("interface-wallpaper-restored", "contacts")
    ui.ctrl("comma")
    subprocess.run(["swaymsg", "-s", ui.socket, '[app_id="escargot-native-contacts"] resize set width 1000 px height 1100 px'], check=True, stdout=subprocess.DEVNULL)
    # Keyboard focus scrolls the settings panel to each restored choice, so the
    # captures do not depend on everything fitting in one viewport.
    press_tabs(8)
    restored = ui.text(ui.capture("appearance-restored-interface", "contacts"))
    assert "blue water" in restored, "The interface selection must survive a restart"
    assert "125%" in restored, "The chosen interface size must be restored after restarting the app"
    press_tabs(2)
    restored_chat = ui.text(ui.capture("appearance-restored-conversation", "contacts"))
    assert "forest" in restored_chat, "The conversation selection must survive a restart"
    print("PASS: independent appearance preferences survive restart", flush=True)
finally:
    process.terminate()
    process.wait(timeout=10)
