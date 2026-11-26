# -*- coding: utf-8 -*-
"""
Zero-dependency ArcObjects COM Cartography Engine for ArcMap 10.1-10.8 (32-bit Python 2.7).
Runs in-process inside ArcMap.exe via ctypes + verified COM vtable layouts.
Supports:
  - Unlimited-class UniqueValueRenderer (no helper fields, no junk legend entries)
  - Graduated ClassBreaksRenderer (Equal Interval / Quantile, N classes)
  - SimpleRenderer for Polygons (solid or hollow boundary), Polylines (solid/dash/dot), and Points (halo marker)
  - RasterStretchColorRampRenderer with multi-stop RGB color ramps (DEM, Slope, Hazard)
  - Layer transparency (ILayerEffects) & Layout Legend item filtering (ILegend)
"""

import ctypes
from ctypes import (
    POINTER,
    Structure,
    byref,
    c_double,
    c_long,
    c_short,
    c_ulong,
    c_ushort,
    c_void_p,
    c_wchar_p,
)
import uuid

ole32 = ctypes.windll.ole32
oleaut32 = ctypes.windll.oleaut32
oleaut32.SysAllocString.argtypes = [c_wchar_p]
oleaut32.SysAllocString.restype = c_void_p
oleaut32.SysFreeString.argtypes = [c_void_p]

CLSCTX_ALL = 0x17


class GUID(Structure):
    _fields_ = [
        ("Data1", c_ulong),
        ("Data2", c_ushort),
        ("Data3", c_ushort),
        ("Data4", ctypes.c_ubyte * 8),
    ]


def _guid(s):
    g = GUID()
    ctypes.memmove(byref(g), uuid.UUID(s).bytes_le, 16)
    return g


# Verified ArcObjects 10.x CLSIDs & IIDs
CLSID_AppROT = _guid("{FABC30FB-D273-11D2-9F36-00C04F6BC61A}")
IID_IAppROT = _guid("{FABC30F9-D273-11D2-9F36-00C04F6BC61A}")
IID_IMxDocument = _guid("{0522A5F1-487C-11D0-98BD-00805F7CED21}")
IID_ILayer = _guid("{34C20002-4D3C-11D0-92D8-00805F7C28B0}")
IID_IGeoFeatureLayer = _guid("{E156D7E5-22AF-11D3-9F99-00C04F6BC78E}")
IID_ILayerEffects = _guid("{09998199-1FB8-11D3-AB4E-000000000000}")
IID_IRasterLayer = _guid("{D02371C7-35F7-11D2-B1F2-00C04F8EDEFF}")
IID_IRasterRenderer = _guid("{6F5322D1-3790-11D2-8D26-0000F8780535}")
IID_IActiveView = _guid("{DD94D76B-836D-11D0-87EC-00805F7CED21}")
IID_ILegend = _guid("{3141F307-38E2-11D1-8809-080009EC732A}")

CLSID_RgbColor = _guid("{7EE9C496-D123-11D0-8383-080009B996CC}")
IID_IRgbColor = _guid("{20CD40B1-8F3D-11D0-8590-0800091A2A72}")

CLSID_SimpleLineSymbol = _guid("{7914E5F9-C892-11D0-8BB6-080009EE4E41}")
IID_ISimpleLineSymbol = _guid("{A9360291-5828-11D0-98BF-00805F7CED21}")

CLSID_SimpleFillSymbol = _guid("{7914E603-C892-11D0-8BB6-080009EE4E41}")
IID_ISimpleFillSymbol = _guid("{A9360292-5828-11D0-98BF-00805F7CED21}")

CLSID_SimpleMarkerSymbol = _guid("{7914E5FE-C892-11D0-8BB6-080009EE4E41}")
IID_ISimpleMarkerSymbol = _guid("{A9360290-5828-11D0-98BF-00805F7CED21}")

CLSID_SimpleRenderer = _guid("{F3435801-5779-11D0-98BF-00805F7CED21}")
IID_ISimpleRenderer = _guid("{F3435800-5779-11D0-98BF-00805F7CED21}")

CLSID_UniqueValueRenderer = _guid("{C3346D29-B2BC-11D1-8817-080009EC732A}")
IID_IUniqueValueRenderer = _guid("{C3346D2A-B2BC-11D1-8817-080009EC732A}")

CLSID_ClassBreaksRenderer = _guid("{AE5F7EA2-8B48-11D0-8356-080009B996CC}")
IID_IClassBreaksRenderer = _guid("{AE5F7EA1-8B48-11D0-8356-080009B996CC}")

CLSID_AlgorithmicColorRamp = _guid("{BEB8709B-C0B4-11D0-8379-080009B996CC}")
IID_IAlgorithmicColorRamp = _guid("{BEB87096-C0B4-11D0-8379-080009B996CC}")

CLSID_MultiPartColorRamp = _guid("{BEB87099-C0B4-11D0-8379-080009B996CC}")
IID_IMultiPartColorRamp = _guid("{BEB87098-C0B4-11D0-8379-080009B996CC}")

CLSID_RasterStretchColorRampRenderer = _guid("{A301A3B2-74D7-11D2-9F29-00C04F8ED1D7}")
IID_IRasterStretchColorRampRenderer = _guid("{A301A3B1-74D7-11D2-9F29-00C04F8ED1D7}")

# Modern cartographic palettes (ColorBrewer / Carto / Viridis / Hypsometric)
PALETTES = {
    "hazard": [
        (46, 125, 50),    # Dark Forest Green (Sangat Rendah)
        (129, 199, 132),  # Soft Green (Rendah)
        (255, 241, 118),  # Soft Yellow (Sedang)
        (255, 167, 38),   # Warm Orange (Tinggi)
        (211, 47, 47),    # Crisp Hazard Red (Sangat Tinggi)
    ],
    "hazard5": [
        (27, 94, 32),
        (102, 187, 106),
        (255, 238, 88),
        (251, 140, 0),
        (198, 40, 40),
    ],
    "hazard-pastel": [
        (82, 183, 136),
        (183, 228, 199),
        (255, 230, 109),
        (244, 162, 97),
        (230, 57, 70),
    ],
    "viridis": [
        (68, 1, 84),
        (59, 82, 139),
        (33, 145, 140),
        (94, 201, 98),
        (253, 231, 37),
    ],
    "blue": [
        (227, 242, 253),
        (144, 202, 249),
        (66, 165, 245),
        (25, 118, 210),
        (13, 71, 161),
    ],
    "earth": [
        (46, 125, 50),
        (156, 204, 101),
        (255, 245, 157),
        (215, 168, 110),
        (141, 110, 99),
    ],
    "terrain": [
        (46, 125, 50),
        (139, 195, 74),
        (255, 238, 88),
        (245, 124, 0),
        (121, 85, 72),
        (245, 245, 245),
    ],
    "gray": [
        (30, 30, 30),
        (140, 140, 140),
        (245, 245, 245),
    ],
    "category": [
        (31, 119, 180), (255, 127, 14), (44, 160, 44), (214, 39, 40),
        (148, 103, 189), (140, 86, 75), (227, 119, 194), (127, 127, 127),
        (188, 189, 34), (23, 190, 207), (174, 199, 232), (255, 187, 120),
        (152, 223, 138), (255, 152, 150), (197, 176, 213), (196, 156, 148),
        (247, 182, 210), (199, 199, 199), (219, 219, 141), (158, 218, 229),
    ],
}

LINE_STYLES = {
    "solid": 0,
    "dash": 1,
    "dot": 2,
    "dashdot": 3,
    "dashdotdot": 4,
    "null": 5,
}

MARKER_STYLES = {
    "circle": 0,
    "square": 1,
    "cross": 2,
    "x": 3,
    "diamond": 4,
}


def _vcall(ptr, idx, *argtypes):
    vtbl = ctypes.cast(ptr, POINTER(c_void_p))[0]
    fn = ctypes.cast(vtbl, POINTER(c_void_p))[idx]
    return ctypes.WINFUNCTYPE(c_long, c_void_p, *argtypes)(fn)


def _release(ptr):
    if ptr and getattr(ptr, "value", None):
        try:
            _vcall(ptr, 2)(ptr)
        except Exception:
            pass


def _qi(ptr, iid):
    out = c_void_p()
    hr = _vcall(ptr, 0, POINTER(GUID), POINTER(c_void_p))(ptr, byref(iid), byref(out))
    if hr != 0:
        return None
    return out


def _co(clsid, iid):
    out = c_void_p()
    hr = ole32.CoCreateInstance(byref(clsid), None, CLSCTX_ALL, byref(iid), byref(out))
    if hr != 0:
        raise RuntimeError("ArcObjects CoCreateInstance failed hr=0x%08X" % (hr & 0xFFFFFFFF))
    return out


def _bstr(s):
    return oleaut32.SysAllocString(unicode(s))


def _free_bstr(b):
    if b:
        oleaut32.SysFreeString(b)


def parse_hex_color(hex_str):
    s = hex_str.strip().lstrip("#")
    if len(s) == 3:
        s = "".join(c * 2 for c in s)
    if len(s) != 6:
        raise ValueError("Invalid hex color: %s" % hex_str)
    return (int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16))


def sample_palette(preset, count, custom_colors=None):
    if custom_colors:
        stops = [parse_hex_color(c) for c in custom_colors.split(",") if c.strip()]
    else:
        key = (preset or "hazard").lower()
        if key == "category":
            base = PALETTES["category"]
            return [base[i % len(base)] for i in range(max(1, count))]
        stops = PALETTES.get(key, PALETTES["hazard"])

    if count <= 1:
        return [stops[0]]
    if count == len(stops):
        return list(stops)

    # Smooth linear RGB interpolation across stops for any N classes
    out = []
    max_idx = float(len(stops) - 1)
    for i in range(count):
        t = (float(i) / float(count - 1)) * max_idx
        lo = int(t)
        hi = min(lo + 1, len(stops) - 1)
        frac = t - lo
        r = int(round(stops[lo][0] + (stops[hi][0] - stops[lo][0]) * frac))
        g = int(round(stops[lo][1] + (stops[hi][1] - stops[lo][1]) * frac))
        b = int(round(stops[lo][2] + (stops[hi][2] - stops[lo][2]) * frac))
        out.append((r, g, b))
    return out


def make_rgb(r, g, b, null_color=False):
    col = _co(CLSID_RgbColor, IID_IRgbColor)
    _vcall(col, 15, c_long)(col, int(r))
    _vcall(col, 17, c_long)(col, int(g))
    _vcall(col, 19, c_long)(col, int(b))
    if null_color:
        _vcall(col, 13, c_short)(col, -1)
    return col


def make_line_symbol(r, g, b, width=1.2, style="solid", null_color=False):
    sym = _co(CLSID_SimpleLineSymbol, IID_ISimpleLineSymbol)
    col = make_rgb(r, g, b, null_color=null_color)
    _vcall(sym, 4, c_void_p)(sym, col)
    _vcall(sym, 6, c_double)(sym, float(width))
    style_code = LINE_STYLES.get((style or "solid").lower(), 0)
    _vcall(sym, 8, c_long)(sym, style_code)
    _release(col)
    return sym


def make_fill_symbol(r, g, b, outline_rgb=None, outline_width=0.0, outline_style="solid", hollow=False):
    sym = _co(CLSID_SimpleFillSymbol, IID_ISimpleFillSymbol)
    col = make_rgb(r, g, b, null_color=hollow)
    _vcall(sym, 4, c_void_p)(sym, col)
    if outline_width and float(outline_width) > 0.0:
        or_, og, ob = outline_rgb or (70, 70, 70)
        outline = make_line_symbol(or_, og, ob, width=float(outline_width), style=outline_style)
    else:
        outline = make_line_symbol(0, 0, 0, width=0.0, style="null", null_color=True)
    _vcall(sym, 6, c_void_p)(sym, outline)
    _vcall(sym, 8, c_long)(sym, 1 if hollow else 0)  # 0=Solid, 1=Null/Hollow
    _release(col)
    _release(outline)
    return sym


def make_marker_symbol(r, g, b, size=7.0, style="circle", outline_rgb=(255, 255, 255), outline_width=1.0):
    sym = _co(CLSID_SimpleMarkerSymbol, IID_ISimpleMarkerSymbol)
    col = make_rgb(r, g, b)
    _vcall(sym, 4, c_double)(sym, float(size))
    _vcall(sym, 6, c_void_p)(sym, col)
    _vcall(sym, 14, c_long)(sym, MARKER_STYLES.get((style or "circle").lower(), 0))
    if outline_width and float(outline_width) > 0.0:
        or_, og, ob = outline_rgb or (255, 255, 255)
        ocol = make_rgb(or_, og, ob)
        _vcall(sym, 16, c_short)(sym, -1)
        _vcall(sym, 18, c_double)(sym, float(outline_width))
        _vcall(sym, 20, c_void_p)(sym, ocol)
        _release(ocol)
    else:
        _vcall(sym, 16, c_short)(sym, 0)
    _release(col)
    return sym


def make_multipart_ramp(rgb_stops):
    if len(rgb_stops) < 2:
        rgb_stops = [(46, 125, 50), (211, 47, 47)]
    mp = _co(CLSID_MultiPartColorRamp, IID_IMultiPartColorRamp)
    for i in range(len(rgb_stops) - 1):
        seg = _co(CLSID_AlgorithmicColorRamp, IID_IAlgorithmicColorRamp)
        c1 = make_rgb(*rgb_stops[i])
        c2 = make_rgb(*rgb_stops[i + 1])
        _vcall(seg, 11, c_void_p)(seg, c1)  # put_FromColor
        _vcall(seg, 13, c_void_p)(seg, c2)  # put_ToColor
        _vcall(seg, 14, c_long)(seg, 1)     # 1 = esriCIELabAlgorithm (smooth perceptual ramp!)
        _vcall(mp, 12, c_void_p)(mp, seg)   # AddRamp
        _release(c1)
        _release(c2)
        _release(seg)
    _vcall(mp, 5, c_long)(mp, 256)          # put_Size(256)
    ok = c_short(0)
    _vcall(mp, 9, POINTER(c_short))(mp, byref(ok))  # CreateRamp
    return mp


def get_current_mxdoc():
    rot = _co(CLSID_AppROT, IID_IAppROT)
    cnt = c_long(0)
    _vcall(rot, 5, POINTER(c_long))(rot, byref(cnt))
    if cnt.value <= 0:
        _release(rot)
        raise RuntimeError("No running ArcMap instance found in in-process AppROT.")
    app = c_void_p()
    _vcall(rot, 6, c_long, POINTER(c_void_p))(rot, 0, byref(app))
    doc = c_void_p()
    _vcall(app, 8, POINTER(c_void_p))(app, byref(doc))
    mxdoc = _qi(doc, IID_IMxDocument)
    _release(doc)
    _release(app)
    _release(rot)
    if not mxdoc:
        raise RuntimeError("Active ArcGIS document is not an ArcMap MxDocument.")
    return mxdoc


def find_com_layer(mxdoc, layer_name):
    fmap = c_void_p()
    _vcall(mxdoc, 9, POINTER(c_void_p))(mxdoc, byref(fmap))
    cnt = c_long(0)
    _vcall(fmap, 8, POINTER(c_long))(fmap, byref(cnt))
    target = layer_name.lower()
    partial = None
    for i in range(cnt.value):
        lyr = c_void_p()
        _vcall(fmap, 9, c_long, POINTER(c_void_p))(fmap, i, byref(lyr))
        bn = c_void_p()
        _vcall(lyr, 3, POINTER(c_void_p))(lyr, byref(bn))
        nm = ctypes.wstring_at(bn) if bn.value else u""
        _free_bstr(bn)
        if nm.lower() == target:
            _release(partial)
            _release(fmap)
            return lyr, nm
        if target in nm.lower() and partial is None:
            partial = (lyr, nm)
        else:
            _release(lyr)
    _release(fmap)
    if partial is not None:
        return partial
    return None, None


def refresh_mxdoc(mxdoc):
    try:
        _vcall(mxdoc, 16)(mxdoc)  # UpdateContents()
    except Exception:
        pass
    try:
        av = c_void_p()
        _vcall(mxdoc, 5, POINTER(c_void_p))(mxdoc, byref(av))
        if av.value:
            _vcall(av, 31)(av)  # Refresh()
            _release(av)
    except Exception:
        pass


def set_layer_transparency(lyr, transparency):
    if transparency is None:
        return
    eff = _qi(lyr, IID_ILayerEffects)
    if eff:
        _vcall(eff, 9, c_short)(eff, int(transparency))
        _release(eff)


def apply_unique_values(mxdoc, lyr, shape_type, field_name, ordered_values, preset="hazard",
                        custom_colors=None, outline_color=None, outline_width=0.0,
                        outline_style="solid", width=1.4, marker_size=7.0,
                        marker_style="circle", transparency=None, labels=None):
    gfl = _qi(lyr, IID_IGeoFeatureLayer)
    if not gfl:
        raise RuntimeError("Target layer does not support feature rendering.")

    colors = sample_palette(preset, len(ordered_values), custom_colors=custom_colors)
    out_rgb = parse_hex_color(outline_color) if outline_color else None

    uvr = _co(CLSID_UniqueValueRenderer, IID_IUniqueValueRenderer)
    _vcall(uvr, 9, c_long)(uvr, 1)  # FieldCount = 1
    fb = _bstr(field_name)
    _vcall(uvr, 4, c_long, c_void_p)(uvr, 0, fb)  # Field(0)
    _free_bstr(fb)
    _vcall(uvr, 15, c_short)(uvr, 0)  # UseDefaultSymbol = False (clean legend!)

    st = (shape_type or "Polygon").lower()
    for idx, val in enumerate(ordered_values):
        r, g, b = colors[idx]
        if "line" in st:
            sym = make_line_symbol(r, g, b, width=width, style=outline_style)
        elif "point" in st or "multipoint" in st:
            sym = make_marker_symbol(r, g, b, size=marker_size, style=marker_style,
                                     outline_rgb=out_rgb or (255, 255, 255),
                                     outline_width=outline_width if outline_width else 0.8)
        else:
            sym = make_fill_symbol(r, g, b, outline_rgb=out_rgb,
                                   outline_width=outline_width, outline_style=outline_style)
        lbl = labels[idx] if (labels and idx < len(labels)) else val
        vb = _bstr(val)
        hb = _bstr("")
        lb = _bstr(lbl)
        _vcall(uvr, 29, c_void_p, c_void_p, c_void_p)(uvr, vb, hb, sym)
        _vcall(uvr, 24, c_void_p, c_void_p)(uvr, vb, lb)
        _free_bstr(vb)
        _free_bstr(hb)
        _free_bstr(lb)
        _release(sym)

    _vcall(gfl, 33, c_void_p)(gfl, uvr)  # putref_Renderer
    set_layer_transparency(lyr, transparency)
    _release(uvr)
    _release(gfl)
    refresh_mxdoc(mxdoc)
    return colors


def apply_class_breaks(mxdoc, lyr, shape_type, field_name, min_val, break_highs, labels,
                       preset="hazard", custom_colors=None, outline_color=None,
                       outline_width=0.0, outline_style="solid", width=1.4,
                       marker_size=7.0, marker_style="circle", transparency=None):
    gfl = _qi(lyr, IID_IGeoFeatureLayer)
    if not gfl:
        raise RuntimeError("Target layer does not support feature rendering.")

    n = len(break_highs)
    colors = sample_palette(preset, n, custom_colors=custom_colors)
    out_rgb = parse_hex_color(outline_color) if outline_color else None

    cbr = _co(CLSID_ClassBreaksRenderer, IID_IClassBreaksRenderer)
    fb = _bstr(field_name)
    _vcall(cbr, 6, c_void_p)(cbr, fb)  # put_Field
    _free_bstr(fb)
    _vcall(cbr, 4, c_long)(cbr, n)     # put_BreakCount
    _vcall(cbr, 18, c_double)(cbr, float(min_val))  # put_MinimumBreak

    st = (shape_type or "Polygon").lower()
    for idx in range(n):
        r, g, b = colors[idx]
        if "line" in st:
            sym = make_line_symbol(r, g, b, width=width, style=outline_style)
        elif "point" in st or "multipoint" in st:
            sym = make_marker_symbol(r, g, b, size=marker_size, style=marker_style,
                                     outline_rgb=out_rgb or (255, 255, 255),
                                     outline_width=outline_width if outline_width else 0.8)
        else:
            sym = make_fill_symbol(r, g, b, outline_rgb=out_rgb,
                                   outline_width=outline_width, outline_style=outline_style)
        _vcall(cbr, 16, c_long, c_double)(cbr, idx, float(break_highs[idx]))
        _vcall(cbr, 10, c_long, c_void_p)(cbr, idx, sym)
        lb = _bstr(labels[idx])
        _vcall(cbr, 12, c_long, c_void_p)(cbr, idx, lb)
        _free_bstr(lb)
        _release(sym)

    _vcall(gfl, 33, c_void_p)(gfl, cbr)
    set_layer_transparency(lyr, transparency)
    _release(cbr)
    _release(gfl)
    refresh_mxdoc(mxdoc)
    return colors


def apply_single_symbol(mxdoc, lyr, shape_type, color_hex=None, preset=None,
                        outline_color=None, outline_width=1.0, outline_style="solid",
                        width=1.4, marker_size=7.0, marker_style="circle",
                        hollow=False, transparency=None):
    gfl = _qi(lyr, IID_IGeoFeatureLayer)
    if not gfl:
        raise RuntimeError("Target layer does not support feature rendering.")

    p = (preset or "").lower()
    if p == "stream":
        rgb = parse_hex_color(color_hex) if color_hex else (2, 136, 209)  # Cartographic River Blue
        width = width if width else 1.4
    elif p == "road":
        rgb = parse_hex_color(color_hex) if color_hex else (216, 67, 21)  # Warm Road Red-Orange
        width = width if width else 1.5
    elif p == "boundary":
        hollow = True
        rgb = (0, 0, 0)
        outline_rgb = parse_hex_color(outline_color or color_hex) if (outline_color or color_hex) else (38, 50, 56)
        outline_width = outline_width if outline_width else 1.4
    elif p == "point":
        rgb = parse_hex_color(color_hex) if color_hex else (211, 47, 47)
    else:
        rgb = parse_hex_color(color_hex) if color_hex else (30, 136, 229)

    if p != "boundary":
        outline_rgb = parse_hex_color(outline_color) if outline_color else (55, 71, 79)

    st = (shape_type or "Polygon").lower()
    if "line" in st or p in ("stream", "road"):
        sym = make_line_symbol(rgb[0], rgb[1], rgb[2], width=width, style=outline_style)
    elif "point" in st or "multipoint" in st or p == "point":
        sym = make_marker_symbol(rgb[0], rgb[1], rgb[2], size=marker_size, style=marker_style,
                                 outline_rgb=parse_hex_color(outline_color) if outline_color else (255, 255, 255),
                                 outline_width=outline_width if outline_width else 1.0)
    else:
        sym = make_fill_symbol(rgb[0], rgb[1], rgb[2], outline_rgb=outline_rgb,
                               outline_width=outline_width, outline_style=outline_style, hollow=hollow)

    sr = _co(CLSID_SimpleRenderer, IID_ISimpleRenderer)
    _vcall(sr, 4, c_void_p)(sr, sym)  # putref_Symbol
    _vcall(gfl, 33, c_void_p)(gfl, sr)
    set_layer_transparency(lyr, transparency)
    _release(sym)
    _release(sr)
    _release(gfl)
    refresh_mxdoc(mxdoc)
    return [rgb]


def apply_raster_stretch(mxdoc, lyr, preset="terrain", custom_colors=None, transparency=None):
    rl = _qi(lyr, IID_IRasterLayer)
    if not rl:
        raise RuntimeError("Target layer is not a raster layer.")
    raster = c_void_p()
    _vcall(rl, 22, POINTER(c_void_p))(rl, byref(raster))
    if not raster.value:
        _release(rl)
        raise RuntimeError("Could not retrieve raster object from layer.")

    stops = sample_palette(preset, 5, custom_colors=custom_colors)
    ramp = make_multipart_ramp(stops)
    sr = _co(CLSID_RasterStretchColorRampRenderer, IID_IRasterStretchColorRampRenderer)
    rr = _qi(sr, IID_IRasterRenderer)
    _vcall(rr, 4, c_void_p)(rr, raster)  # putref_Raster
    _vcall(sr, 8, c_void_p)(sr, ramp)    # put_ColorRamp
    _vcall(rr, 8)(rr)                    # Update()
    _vcall(rl, 27, c_void_p)(rl, rr)     # putref_Renderer
    set_layer_transparency(lyr, transparency)
    _release(rr)
    _release(sr)
    _release(ramp)
    _release(raster)
    _release(rl)
    refresh_mxdoc(mxdoc)
    return stops


def hide_legend_layers(mxdoc, layer_names_to_hide):
    """Removes specified layer names (e.g. Hillshade, Basemap) from all Layout Legends."""
    targets = [n.strip().lower() for n in layer_names_to_hide if n.strip()]
    if not targets:
        return []
    fmap = c_void_p()
    _vcall(mxdoc, 9, POINTER(c_void_p))(mxdoc, byref(fmap))
    ms_cnt = c_long(0)
    _vcall(fmap, 11, POINTER(c_long))(fmap, byref(ms_cnt))
    removed = []
    for i in range(ms_cnt.value):
        ms = c_void_p()
        _vcall(fmap, 12, c_long, POINTER(c_void_p))(fmap, i, byref(ms))
        leg = _qi(ms, IID_ILegend)
        if leg:
            icnt = c_long(0)
            _vcall(leg, 25, POINTER(c_long))(leg, byref(icnt))
            for idx in range(icnt.value - 1, -1, -1):
                item = c_void_p()
                _vcall(leg, 26, c_long, POINTER(c_void_p))(leg, idx, byref(item))
                if item.value:
                    item_lyr = c_void_p()
                    _vcall(item, 5, POINTER(c_void_p))(item, byref(item_lyr))
                    if item_lyr.value:
                        bn = c_void_p()
                        _vcall(item_lyr, 3, POINTER(c_void_p))(item_lyr, byref(bn))
                        nm = ctypes.wstring_at(bn) if bn.value else u""
                        _free_bstr(bn)
                        if any(t in nm.lower() for t in targets):
                            _vcall(leg, 29, c_long)(leg, idx)  # RemoveItem(idx)
                            removed.append(nm)
                        _release(item_lyr)
                    _release(item)
            _release(leg)
        _release(ms)
    _release(fmap)
    refresh_mxdoc(mxdoc)
    return removed
