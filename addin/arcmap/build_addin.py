"""Packages addin/arcmap into geokit-arcmap.esriaddin (standard ZIP)."""
import base64
import os
import zipfile

ROOT = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(ROOT, "geokit-arcmap.esriaddin")
IMG_DIR = os.path.join(ROOT, "Images")
IMG_PATH = os.path.join(IMG_DIR, "geokit16.png")

# Minimal valid 16x16 teal PNG icon
PNG_B64 = (
    "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAAI0lEQVR42mP8z8Dw"
    "nwEIGBgYGP5Tqxg1gF4G/Kc5kRoYAABm9wQR9sL1NwAAAABJRU5ErkJggg=="
)

if not os.path.exists(IMG_DIR):
    os.makedirs(IMG_DIR)
with open(IMG_PATH, "wb") as fh:
    fh.write(base64.b64decode(PNG_B64))

with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as zf:
    zf.write(os.path.join(ROOT, "config.xml"), "Config.xml")
    zf.write(
        os.path.join(ROOT, "Install", "geokit_addin.py"),
        "Install/geokit_addin.py",
    )
    zf.write(IMG_PATH, "Images/geokit16.png")

print("Built:", OUT)
