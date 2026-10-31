"""Check the pending-request actions in a cropped contacts screenshot."""
import io
import subprocess
import sys
from PIL import Image, ImageOps

image = Image.open(sys.argv[1]).convert("RGB")
if len(sys.argv) > 2 and sys.argv[2] == "removal":
    if sum(image.resize((1, 1)).getpixel((0, 0))) < 384:
        image = ImageOps.invert(image)
    image = image.convert("L").resize((image.width * 3, image.height * 3))
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    text = subprocess.run(
        ["tesseract", "stdin", "stdout", "--psm", "11"],
        input=buffer.getvalue(), capture_output=True, check=True,
    ).stdout.decode().casefold()
    assert "from your contacts?" in text, "Removal must ask which contact is being removed"
    assert "history is kept" in text, "Removal must explain that conversation history is preserved"
    assert "does not block them" in text, "Removal must explain that it does not block the person"
    assert "remove" in text, "Removal must offer a confirmation action"
    # The cancel path itself is exercised by the keyboard flow that follows
    # (Escape and the Keep/Cancel button must leave the contact in the list).
    print("Contact removal asks for confirmation and preserves conversation history")
else:
    # The request card sits in the left pane. Small aqua buttons disappear in a
    # whole-window OCR pass, so threshold just that column.
    crop = image.crop((0, 130, min(image.width, 360), min(image.height, 520)))
    if sum(crop.resize((1, 1)).getpixel((0, 0))) < 384:
        crop = ImageOps.invert(crop)
    gray = crop.convert("L")
    # Anti-aliasing makes "t" look like "f" at one threshold and drops the
    # helper line at another; read both binarizations and require every phrase
    # somewhere in the combined text.
    texts = []
    for threshold in (130, 165):
        brought = gray.point(lambda value: 0 if value < threshold else 255)
        brought = brought.resize((brought.width * 5, brought.height * 5))
        buffer = io.BytesIO()
        brought.save(buffer, format="PNG")
        texts.append(
            subprocess.run(
                ["tesseract", "stdin", "stdout", "--psm", "6"],
                input=buffer.getvalue(), capture_output=True, check=True,
            ).stdout.decode().casefold()
        )
    text = "\n".join(texts)
    assert "contact requests" in text, "Pending contact requests must be visible in the native contacts window"
    assert "taylor" in text and "wants to add you" in text, "A request must identify the sender"
    assert "accept" in text and "not now" in text, "Requests must offer explicit acceptance and deferral"
    print("Pending contact requests identify the sender and offer acceptance or deferral")
