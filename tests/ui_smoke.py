"""Check a demo chat screenshot captured on an isolated desktop.

Usage: python3 tests/ui_smoke.py /absolute/path/to/chat.png
Requires the optional system tool tesseract; no real account is used.
"""
import subprocess
import sys
from pathlib import Path

screenshot = Path(sys.argv[1])
assert screenshot.is_file(), f"Screenshot missing: {screenshot}"
text = subprocess.run(
    ["tesseract", str(screenshot), "stdout", "--psm", "11"],
    check=True, capture_output=True, text=True,
).stdout.casefold()
assert "friends" in text, "Contacts must stay visible alongside the active conversation"
assert "alex" in text, "The active conversation must identify the recipient"
assert "web" in text, "Web search must remain discoverable alongside contact search"
assert "appear off" not in text, "Offline contacts must be labelled Offline, not as your own presence setting"
print("Demo chat: contacts, recipient and web search are visible")
