"""Check a cropped screenshot of the sign-in form using optional Tesseract."""
import io
import subprocess
import sys
from PIL import Image, ImageOps

image = Image.open(sys.argv[1]).convert("RGB")
if sum(image.resize((1, 1)).getpixel((0, 0))) < 384:
    image = ImageOps.invert(image)
image = image.convert("L").resize((image.width * 3, image.height * 3))
buffer = io.BytesIO()
image.save(buffer, format="PNG")
text = subprocess.run(
    ["tesseract", "stdin", "stdout", "--psm", "11"],
    input=buffer.getvalue(), capture_output=True, check=True,
).stdout.decode().casefold().replace(" ", "")
# Small 75% text loses one glyph in Tesseract ("Offine", "oifine"); accept the
# observed variants while still requiring all four choices to be present.
variants = {
    "online": ("online", "onlne", "0nline", "onllne"),
    "away": ("away",),
    "busy": ("busy",),
    "offline": ("offline", "offine", "oifine", "cffine", "0ffine", "ofﬂne"),
}
for status, accepted in variants.items():
    assert any(value in text for value in accepted), (
        f"Sign-in status {status!r} must be an explicit visible choice"
    )
print("Sign-in presence choices are visible")
