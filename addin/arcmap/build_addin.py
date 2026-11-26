"""Packages addin/arcmap into geokit-arcmap.esriaddin (standard ZIP)."""
import os
import zipfile

ROOT = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(ROOT, "geokit-arcmap.esriaddin")

with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as zf:
    zf.write(os.path.join(ROOT, "config.xml"), "config.xml")
    zf.write(
        os.path.join(ROOT, "Install", "geokit_addin.py"),
        "Install/geokit_addin.py",
    )

print("Built:", OUT)
