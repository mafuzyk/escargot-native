"""Check the native Add Contact form in a cropped screenshot; needs Pillow/Tesseract."""
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
).stdout.decode().casefold()
assert "username or email" in text, "Add Contact must accept an account inside the native client"
assert "open contact list" not in text, "Adding a contact must not require the Escargot website"
print("Add Contact has a native account form")
