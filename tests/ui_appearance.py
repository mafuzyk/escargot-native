"""Check discoverable, independent native appearance controls in a settings capture."""
import io
import subprocess
import sys
from PIL import Image, ImageOps


def ocr(image, psm="11", scale=3):
    if sum(image.resize((1, 1)).getpixel((0, 0))) < 384:
        image = ImageOps.invert(image)
    image = image.convert("L").resize((image.width * scale, image.height * scale))
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    return subprocess.run(
        ["tesseract", "stdin", "stdout", "--psm", psm],
        input=buffer.getvalue(), capture_output=True, check=True,
    ).stdout.decode().casefold()


image = Image.open(sys.argv[1]).convert("RGB")
if len(sys.argv) > 2 and sys.argv[2] == "picker":
    # The open gallery sits in the lower half; threshold it separately so the
    # small buttons and tile labels survive OCR.
    crop = image.crop((0, int(image.height * 0.45), image.width, image.height))
    if sum(crop.resize((1, 1)).getpixel((0, 0))) < 384:
        crop = ImageOps.invert(crop)
    gray = crop.convert("L")
    texts = []
    for threshold in (130, 165):
        band = gray.point(lambda value: 0 if value < threshold else 255)
        buffer = io.BytesIO()
        band.resize((band.width * 4, band.height * 4)).save(buffer, format="PNG")
        texts.append(
            subprocess.run(
                ["tesseract", "stdin", "stdout", "--psm", "6"],
                input=buffer.getvalue(), capture_output=True, check=True,
            ).stdout.decode().casefold()
        )
    text = "\n".join(texts)
    # The gallery is expanded in this capture; tile labels prove it. The
    # conversation card itself is checked in the full settings capture.
    assert "light:" in text and "night:" in text, "Both bundled wallpaper collections must be available"
    assert "classic" in text, "The classic background must remain available"
else:
    text = ocr(image)
    assert "interface background" in text, "Settings must expose the interface wallpaper"
    assert "conversation background" in text, "Conversation wallpaper must be a separate choice"
    assert "interface size" in text, "Settings must explain interface zoom"
print("Settings expose independent wallpapers, both collections and interface zoom")
