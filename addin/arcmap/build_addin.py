"""Packages addin/arcmap into geokit-arcmap.esriaddin (standard ZIP).

Automatically bundles config.xml, ALL Python modules in Install/, and all assets in Images/
so helper modules like Install/arcobjects.py are never omitted.
"""
import base64
import os
import zipfile

ROOT = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(ROOT, "geokit-arcmap.esriaddin")
IMG_DIR = os.path.join(ROOT, "Images")
INSTALL_DIR = os.path.join(ROOT, "Install")
DEFAULT_IMG = os.path.join(IMG_DIR, "geokit16.png")

# Minimal valid 16x16 teal PNG icon (created only if Images/geokit16.png is missing)
PNG_B64 = (
    "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAAI0lEQVR42mP8z8Dw"
    "nwEIGBgYGP5Tqxg1gF4G/Kc5kRoYAABm9wQR9sL1NwAAAABJRU5ErkJggg=="
)

if not os.path.exists(IMG_DIR):
    os.makedirs(IMG_DIR)
if not os.path.exists(DEFAULT_IMG):
    with open(DEFAULT_IMG, "wb") as fh:
        fh.write(base64.b64decode(PNG_B64))

bundled = []
with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as zf:
    cfg = os.path.join(ROOT, "config.xml")
    zf.write(cfg, "Config.xml")
    bundled.append("Config.xml")

    for fname in sorted(os.listdir(INSTALL_DIR)):
        if fname.endswith(".py"):
            full = os.path.join(INSTALL_DIR, fname)
            if os.path.isfile(full):
                arcname = "Install/" + fname
                zf.write(full, arcname)
                bundled.append(arcname)

    for fname in sorted(os.listdir(IMG_DIR)):
        full = os.path.join(IMG_DIR, fname)
        if os.path.isfile(full):
            arcname = "Images/" + fname
            zf.write(full, arcname)
            bundled.append(arcname)

print("Built: %s (%d files: %s)" % (OUT, len(bundled), ", ".join(bundled)))
