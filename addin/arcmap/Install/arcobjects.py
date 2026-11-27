# -*- coding: utf-8 -*-
"""
Backwards-compatible facade for GeoKit's ArcObjects COM Cartography Engine (Python 2.7).
The engine is cleanly modularized into:
  - com_core.py        : COM/ctypes vtable primitives, verified ArcObjects 10.x GUIDs, locale-safe _to_float()
  - symbology.py       : Palettes (incl. KLHK 23-class), Fisher-Jenks Natural Breaks, Renderers & Labels
  - layout_perkabig.py : PerKaBIG No. 3/2016 Landscape Layout Builder (A4/A3/A1/A0) & Inset Styling
"""

from com_core import *  # noqa: F401,F403
from symbology import *  # noqa: F401,F403
from layout_perkabig import *  # noqa: F401,F403
