"""Check a cropped narrow-window background picker capture with Tesseract."""
import subprocess
import sys

text = subprocess.run(
    ["tesseract", sys.argv[1], "stdout", "--psm", "11"],
    capture_output=True, text=True, check=True,
).stdout.casefold()
assert "conversation background" in text, "The background picker title must remain inside a narrow window"
print("Narrow-window background picker keeps its title visible")
