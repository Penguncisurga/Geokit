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
IID_IFeatureRenderer = _guid("{40A9E884-5533-11D0-98BE-00805F7CED21}")
IID_ILayerEffects = _guid("{09998199-1FB8-11D3-AB4E-000000000000}")
IID_IRasterLayer = _guid("{D02371C7-35F7-11D2-B1F2-00C04F8EDEFF}")
IID_IRasterRenderer = _guid("{6F5322D1-3790-11D2-8D26-0000F8780535}")
IID_IActiveView = _guid("{DD94D76B-836D-11D0-87EC-080009EC732A}")
IID_ILegend = _guid("{3141F307-38E2-11D1-8809-080009EC732A}")

CLSID_RgbColor = _guid("{7EE9C496-D123-11D0-8383-080009B996CC}")
IID_IColor = _guid("{20CD40B0-8F3D-11D0-8590-0800091A2A72}")
IID_IRgbColor = _guid("{20CD40B1-8F3D-11D0-8590-0800091A2A72}")
IID_ISymbol = _guid("{F3435802-5779-11D0-98BF-00805F7CED21}")
IID_ILineSymbol = _guid("{E6BDAA7D-4D35-11D0-98BE-00805F7CED21}")
IID_IColorRamp = _guid("{BEB87091-C0B4-11D0-8379-080009B996CC}")

CLSID_SimpleLineSymbol = _guid("{7914E5F9-C892-11D0-8BB6-080009EE4E41}")
IID_ISimpleLineSymbol = _guid("{A9360291-5828-11D0-98BF-00805F7CED21}")

CLSID_CartographicLineSymbol = _guid("{7914E5FB-C892-11D0-8BB6-080009EE4E41}")
IID_ICartographicLineSymbol = _guid("{B04BC359-C36E-11D0-BFA1-0080C7E24280}")
IID_ILineProperties = _guid("{B04BC357-C36E-11D0-BFA1-0080C7E24280}")
CLSID_Template = _guid("{41093A71-CCE1-11D0-BFAA-0080C7E24280}")
IID_ITemplate = _guid("{7914E5F4-C892-11D0-8BB6-080009EE4E41}")

CLSID_TextSymbol = _guid("{B65A3E74-2993-11D1-9A43-0080C7EC5C96}")
IID_ITextSymbol = _guid("{A80B5E91-7F9C-11D0-9410-080009EEBECB}")
IID_IMask = _guid("{6A7EF984-6924-11D2-980D-0080C7E04196}")

IID_IFillSymbol = _guid("{E6BDAA7E-4D35-11D0-98BE-00805F7CED21}")
IID_ILabelEngineLayerProperties = _guid("{01004143-0D1C-11D2-A26F-080009B6F22B}")

IID_IPageLayout = _guid("{DD94D76A-836D-11D0-87EC-080009EC732A}")
IID_IPage = _guid("{2AB4981F-9406-11D0-87EF-080009EC732A}")
IID_IGraphicsContainer = _guid("{D09411D1-4BB4-11D1-B6CC-080009B996CC}")
IID_IElement = _guid("{9B001319-9ED7-11D0-859E-0800091A2A72}")
IID_IElementProperties = _guid("{86F32B61-7667-11D2-A2D3-080009B6F22B}")
IID_ITextElement = _guid("{204034D2-F6EA-11D0-83AD-080009B996CC}")
CLSID_TextElement = _guid("{204034D3-F6EA-11D0-83AD-080009B996CC}")
CLSID_RectangleElement = _guid("{3A9767C2-F253-11D0-83A4-080009B996CC}")
IID_IFillShapeElement = _guid("{3E150C0D-19F5-11D3-9F9E-00C04F6BC8DD}")
IID_IMapFrame = _guid("{9B00131B-9ED7-11D0-859E-0800091A2A72}")
IID_IMapSurroundFrame = _guid("{9B00131C-9ED7-11D0-859E-0800091A2A72}")
IID_IMapGrids = _guid("{08E7D005-9AAE-11D2-AE7B-080009EC732A}")
CLSID_MeasuredGrid = _guid("{03762C90-F4D0-11D1-ADE8-080009EC732A}")
IID_IMeasuredGrid = _guid("{4F69021F-A5C1-11D2-AE82-080009EC732A}")
IID_IMapGrid = _guid("{03762C8B-F4D0-11D1-ADE8-080009EC732A}")

CLSID_Envelope = _guid("{30707212-52D5-11D0-A8F2-00608C85EDE5}")
IID_IEnvelope = _guid("{1B077914-38C5-11D0-92D2-00805F7C28B0}")
CLSID_Point = _guid("{00A5CB41-52DA-11D0-A8F2-00608C85EDE5}")
IID_IPoint = _guid("{1B077915-38C5-11D0-92D2-00805F7C28B0}")
IID_IGeometry = _guid("{1B07790D-38C5-11D0-92D2-00805F7C28B0}")

CLSID_UID = _guid("{78FF7FA1-FB2F-11D1-94A2-080009EEBECB}")
IID_IUID = _guid("{1714D59B-FB22-11D1-94A2-080009EEBECB}")

IID_IScaleBar = _guid("{6DFAE8E2-ECAA-11D0-8682-0000F8751720}")
IID_ILegendItem = _guid("{52AA5BC0-CB21-11D0-87F9-080009EC732A}")
CLSID_FormattedGridLabel = _guid("{CE41C507-9DF9-11D2-AADE-000000000000}")
IID_IFormattedGridLabel = _guid("{CE41C505-9DF9-11D2-AADE-000000000000}")
IID_IGridLabel = _guid("{CE41C501-9DF9-11D2-AADE-000000000000}")
CLSID_NumericFormat = _guid("{7E4F4719-8E54-11D2-AAD8-000000000000}")
IID_INumericFormat = _guid("{7E4F4710-8E54-11D2-AAD8-000000000000}")
IID_INumberFormat = _guid("{7E4F470D-8E54-11D2-AAD8-000000000000}")
IID_IFont = _guid("{BEF6E002-A874-101A-8BBA-00AA00300CAB}")

SURROUND_LEGEND_CLSID = u"{7A3F91E4-B9E3-11D1-8756-0000F8751720}"
SURROUND_NORTH_CLSID = u"{7A3F91DD-B9E3-11D1-8756-0000F8751720}"
SURROUND_SCALEBAR_CLSID = u"{6589F147-F7F7-11D2-B872-00600802E603}"


class VARIANT(Structure):
    _fields_ = [
        ("vt", c_ushort),
        ("wReserved1", c_ushort),
        ("wReserved2", c_ushort),
        ("wReserved3", c_ushort),
        ("val", c_void_p),
        ("pad", c_void_p),
    ]

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


DASH_PATTERNS = {
    "dash": [(6.0, 3.0)],
    "dot": [(1.5, 2.5)],
    "dashdot": [(8.0, 3.0), (1.5, 3.0)],
    "dashdotdot": [(8.0, 2.5), (1.5, 2.0), (1.5, 2.5)],
    "admin-kab": [(10.0, 3.0), (1.5, 2.5), (1.5, 3.0)],
    "admin-kec": [(8.0, 3.0), (1.5, 3.0)],
    "admin-desa": [(4.5, 2.5)],
}


def make_line_symbol(r, g, b, width=1.2, style="solid", null_color=False):
    st = (style or "solid").lower()
    if (not null_color) and st in DASH_PATTERNS:
        # CartographicLineSymbol renders TRUE thick dashes / BIG admin patterns at ANY line width!
        sym = _co(CLSID_CartographicLineSymbol, IID_ILineSymbol)
        col = make_rgb(r, g, b, null_color=False)
        icol = _qi(col, IID_IColor)
        _vcall(sym, 4, c_void_p)(sym, icol)
        _vcall(sym, 6, c_double)(sym, float(width))
        cls_iface = _qi(sym, IID_ICartographicLineSymbol)
        if cls_iface:
            _vcall(cls_iface, 8, c_long)(cls_iface, 2)   # Cap = Round (2)
            _vcall(cls_iface, 10, c_long)(cls_iface, 2)  # Join = Round (2)
            _release(cls_iface)
        lprop = _qi(sym, IID_ILineProperties)
        if lprop:
            tpl = _co(CLSID_Template, IID_ITemplate)
            _vcall(tpl, 11, c_double)(tpl, 1.0)  # Interval = 1.0 pt
            for mark, gap in DASH_PATTERNS[st]:
                _vcall(tpl, 6, c_double, c_double)(tpl, float(mark), float(gap))
            _vcall(lprop, 14, c_void_p)(lprop, tpl)  # putref_Template
            _release(tpl)
            _release(lprop)
        _release(icol)
        _release(col)
        return sym

    sym = _co(CLSID_SimpleLineSymbol, IID_ISimpleLineSymbol)
    col = make_rgb(r, g, b, null_color=null_color)
    icol = _qi(col, IID_IColor)
    _vcall(sym, 4, c_void_p)(sym, icol)
    _vcall(sym, 6, c_double)(sym, float(width))
    style_code = LINE_STYLES.get(st, 0)
    _vcall(sym, 8, c_long)(sym, style_code)
    _release(icol)
    _release(col)
    return sym


def make_fill_symbol(r, g, b, outline_rgb=None, outline_width=0.0, outline_style="solid", hollow=False):
    sym = _co(CLSID_SimpleFillSymbol, IID_ISimpleFillSymbol)
    col = make_rgb(r, g, b, null_color=hollow)
    icol = _qi(col, IID_IColor)
    _vcall(sym, 4, c_void_p)(sym, icol)
    if outline_width and float(outline_width) > 0.0:
        or_, og, ob = outline_rgb or (70, 70, 70)
        outline = make_line_symbol(or_, og, ob, width=float(outline_width), style=outline_style)
    else:
        outline = make_line_symbol(0, 0, 0, width=0.0, style="null", null_color=True)
    iline = _qi(outline, IID_ILineSymbol)
    _vcall(sym, 6, c_void_p)(sym, iline)
    _vcall(sym, 8, c_long)(sym, 1 if hollow else 0)  # 0=Solid, 1=Null/Hollow
    _release(iline)
    _release(outline)
    _release(icol)
    _release(col)
    return sym


def make_marker_symbol(r, g, b, size=7.0, style="circle", outline_rgb=(255, 255, 255), outline_width=1.0):
    sym = _co(CLSID_SimpleMarkerSymbol, IID_ISimpleMarkerSymbol)
    col = make_rgb(r, g, b)
    icol = _qi(col, IID_IColor)
    _vcall(sym, 4, c_double)(sym, float(size))
    _vcall(sym, 6, c_void_p)(sym, icol)
    _vcall(sym, 14, c_long)(sym, MARKER_STYLES.get((style or "circle").lower(), 0))
    if outline_width and float(outline_width) > 0.0:
        or_, og, ob = outline_rgb or (255, 255, 255)
        ocol = make_rgb(or_, og, ob)
        iocol = _qi(ocol, IID_IColor)
        _vcall(sym, 16, c_short)(sym, -1)
        _vcall(sym, 18, c_double)(sym, float(outline_width))
        _vcall(sym, 20, c_void_p)(sym, iocol)
        _release(iocol)
        _release(ocol)
    else:
        _vcall(sym, 16, c_short)(sym, 0)
    _release(icol)
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
        ic1 = _qi(c1, IID_IColor)
        ic2 = _qi(c2, IID_IColor)
        _vcall(seg, 11, c_void_p)(seg, ic1)  # put_FromColor
        _vcall(seg, 13, c_void_p)(seg, ic2)  # put_ToColor
        _vcall(seg, 14, c_long)(seg, 1)      # 1 = esriCIELabAlgorithm
        iramp = _qi(seg, IID_IColorRamp)
        _vcall(mp, 12, c_void_p)(mp, iramp)  # AddRamp
        _release(iramp)
        _release(ic1)
        _release(ic2)
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
            if partial is not None:
                _release(partial[0])
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
        isym = _qi(sym, IID_ISymbol)
        lbl = labels[idx] if (labels and idx < len(labels)) else val
        vb = _bstr(val)
        hb = _bstr("")
        lb = _bstr(lbl)
        _vcall(uvr, 29, c_void_p, c_void_p, c_void_p)(uvr, vb, hb, isym)
        _vcall(uvr, 24, c_void_p, c_void_p)(uvr, vb, lb)
        _free_bstr(vb)
        _free_bstr(hb)
        _free_bstr(lb)
        _release(isym)
        _release(sym)

    frend = _qi(uvr, IID_IFeatureRenderer)
    _vcall(gfl, 33, c_void_p)(gfl, frend)  # putref_Renderer(IFeatureRenderer*)
    set_layer_transparency(lyr, transparency)
    _release(frend)
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
        isym = _qi(sym, IID_ISymbol)
        _vcall(cbr, 16, c_long, c_double)(cbr, idx, float(break_highs[idx]))
        _vcall(cbr, 10, c_long, c_void_p)(cbr, idx, isym)
        lb = _bstr(labels[idx])
        _vcall(cbr, 12, c_long, c_void_p)(cbr, idx, lb)
        _free_bstr(lb)
        _release(isym)
        _release(sym)

    frend = _qi(cbr, IID_IFeatureRenderer)
    _vcall(gfl, 33, c_void_p)(gfl, frend)
    set_layer_transparency(lyr, transparency)
    _release(frend)
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

    isym = _qi(sym, IID_ISymbol)
    sr = _co(CLSID_SimpleRenderer, IID_ISimpleRenderer)
    _vcall(sr, 4, c_void_p)(sr, isym)  # putref_Symbol(ISymbol*)
    frend = _qi(sr, IID_IFeatureRenderer)
    _vcall(gfl, 33, c_void_p)(gfl, frend)
    set_layer_transparency(lyr, transparency)
    _release(frend)
    _release(isym)
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
    iramp = _qi(ramp, IID_IColorRamp)
    sr = _co(CLSID_RasterStretchColorRampRenderer, IID_IRasterStretchColorRampRenderer)
    rr = _qi(sr, IID_IRasterRenderer)
    _vcall(rr, 4, c_void_p)(rr, raster)  # putref_Raster
    _vcall(sr, 8, c_void_p)(sr, iramp)   # put_ColorRamp(IColorRamp*)
    _vcall(rr, 8)(rr)                    # Update()
    _vcall(rl, 27, c_void_p)(rl, rr)     # putref_Renderer(IRasterRenderer*)
    set_layer_transparency(lyr, transparency)
    _release(rr)
    _release(sr)
    _release(iramp)
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


def make_text_symbol(size=10.0, rgb=(30, 30, 30), halign=1, valign=2, halo=False, halo_size=1.5, bold=False):
    """Creates an ITextSymbol (halign: 0=left,1=center,2=right; valign: 0=top,1=center,2=baseline,3=bottom)."""
    ts = _co(CLSID_TextSymbol, IID_ITextSymbol)
    col = make_rgb(*rgb)
    icol = _qi(col, IID_IColor)
    if bold:
        fnt = c_void_p()
        if _vcall(ts, 3, POINTER(c_void_p))(ts, byref(fnt)) == 0 and fnt.value:
            ifnt = _qi(fnt, IID_IFont)
            if ifnt:
                _vcall(ifnt, 8, c_short)(ifnt, -1)  # put_Bold(True)
                _release(ifnt)
            _vcall(ts, 4, c_void_p)(ts, fnt)  # put_Font
            _release(fnt)
    _vcall(ts, 6, c_double)(ts, float(size))
    _vcall(ts, 8, c_void_p)(ts, icol)
    _vcall(ts, 12, c_long)(ts, int(halign))
    _vcall(ts, 10, c_long)(ts, int(valign))
    if halo:
        mask = _qi(ts, IID_IMask)
        if mask:
            wfill = make_fill_symbol(255, 255, 255, outline_width=0.0)
            ifill = _qi(wfill, IID_IFillSymbol)
            _vcall(mask, 4, c_long)(mask, 1)  # 1 = esriMSHalo
            _vcall(mask, 6, c_double)(mask, float(halo_size))
            _vcall(mask, 8, c_void_p)(mask, ifill)
            _release(ifill)
            _release(wfill)
            _release(mask)
    _release(icol)
    _release(col)
    return ts


def set_label_halo(mxdoc, lyr, font_size=8.5, color_rgb=(33, 33, 33), halo_size=1.5):
    """Applies a clean white halo + readable font size to a feature layer's label class."""
    gfl = _qi(lyr, IID_IGeoFeatureLayer)
    if not gfl:
        return False
    ap_col = c_void_p()
    _vcall(gfl, 40, POINTER(c_void_p))(gfl, byref(ap_col))  # get_AnnotationProperties
    if not ap_col.value:
        _release(gfl)
        return False
    cnt = c_long(0)
    _vcall(ap_col, 4, POINTER(c_long))(ap_col, byref(cnt))  # Count
    applied = False
    if cnt.value > 0:
        item = c_void_p()
        placed = c_void_p()
        unplaced = c_void_p()
        _vcall(ap_col, 5, c_long, POINTER(c_void_p), POINTER(c_void_p), POINTER(c_void_p))(
            ap_col, 0, byref(item), byref(placed), byref(unplaced)
        )
        _release(placed)
        _release(unplaced)
        if item.value:
            lelp = _qi(item, IID_ILabelEngineLayerProperties)
            if lelp:
                ts = make_text_symbol(size=font_size, rgb=color_rgb, halign=1, valign=1, halo=True, halo_size=halo_size)
                _vcall(lelp, 3, c_void_p)(lelp, ts)  # putref_Symbol
                _release(ts)
                _release(lelp)
                applied = True
            _release(item)
    _release(ap_col)
    _release(gfl)
    refresh_mxdoc(mxdoc)
    return applied


def _make_envelope(xmin, ymin, xmax, ymax):
    env = _co(CLSID_Envelope, IID_IEnvelope)
    _vcall(env, 57, c_double, c_double, c_double, c_double)(
        env, float(xmin), float(ymin), float(xmax), float(ymax)
    )
    return env


def _make_point(x, y):
    pt = _co(CLSID_Point, IID_IPoint)
    _vcall(pt, 16, c_double, c_double)(pt, float(x), float(y))
    return pt


def _make_uid(guid_str):
    uid = _co(CLSID_UID, IID_IUID)
    bs = _bstr(guid_str)
    v = VARIANT()
    v.vt = 8  # VT_BSTR
    v.val = bs
    _vcall(uid, 8, VARIANT)(uid, v)  # put_Value
    _free_bstr(bs)
    return uid


def _add_rect(gc, xmin, ymin, xmax, ymax, fill_rgb=None, outline_rgb=(40, 40, 40), outline_width=0.8, hollow=True, name="geokit_box"):
    re = _co(CLSID_RectangleElement, IID_IFillShapeElement)
    r, g, b = fill_rgb or (255, 255, 255)
    fs = make_fill_symbol(r, g, b, outline_rgb=outline_rgb, outline_width=outline_width, hollow=hollow)
    ifs = _qi(fs, IID_IFillSymbol)
    _vcall(re, 4, c_void_p)(re, ifs)  # put_Symbol
    el = _qi(re, IID_IElement)
    env = _make_envelope(xmin, ymin, xmax, ymax)
    geom = _qi(env, IID_IGeometry)
    _vcall(el, 4, c_void_p)(el, geom)  # put_Geometry
    ep = _qi(re, IID_IElementProperties)
    if ep:
        nb = _bstr(name)
        _vcall(ep, 4, c_void_p)(ep, nb)
        _free_bstr(nb)
        _release(ep)
    _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)  # AddElement
    _release(geom)
    _release(env)
    _release(el)
    _release(ifs)
    _release(fs)
    _release(re)


def _add_text(gc, x, y, text, size=10.0, rgb=(25, 25, 25), halign=1, valign=1, name="geokit_text", bold=False):
    te = _co(CLSID_TextElement, IID_ITextElement)
    ts = make_text_symbol(size=size, rgb=rgb, halign=halign, valign=valign, halo=False, bold=bold)
    _vcall(te, 4, c_void_p)(te, ts)  # put_Symbol
    tb = _bstr(text)
    _vcall(te, 6, c_void_p)(te, tb)  # put_Text
    _free_bstr(tb)
    el = _qi(te, IID_IElement)
    pt = _make_point(x, y)
    geom = _qi(pt, IID_IGeometry)
    _vcall(el, 4, c_void_p)(el, geom)  # put_Geometry
    ep = _qi(te, IID_IElementProperties)
    if ep:
        nb = _bstr(name)
        _vcall(ep, 4, c_void_p)(ep, nb)
        _free_bstr(nb)
        _release(ep)
    _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
    _release(geom)
    _release(pt)
    _release(el)
    _release(ts)
    _release(te)


def _wrap_text(text, max_chars=26):
    words = text.split()
    lines = []
    cur = ""
    for w in words:
        if not cur:
            cur = w
        elif len(cur) + 1 + len(w) <= max_chars:
            cur = cur + " " + w
        else:
            lines.append(cur)
            cur = w
    if cur:
        lines.append(cur)
    return "\n".join(lines)


def _nice_grid_interval(span_meters):
    if span_meters <= 0:
        return 10000.0
    raw = span_meters / 4.0
    for step in [100.0, 250.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0, 25000.0, 50000.0, 100000.0]:
        if raw <= step:
            return step
    return 100000.0


def build_perkabig_layout(mxdoc, title="PETA TEMATIK WILAYAH", subtitle="", source="Sumber: Analisis GeoKit GIS",
                          crs_text="", scale_text="", span_meters=40000.0, hide_legend_names=None, add_grid=True):
    """
    Constructs a clean A4 Landscape (29.7 x 21.0 cm) Indonesian PerKaBIG-style map layout:
      - Left 70% MapFrame with UTM MeasuredGrid + crisp border
      - Right 28% Info Panel: Title/Subtitle, North Arrow + ScaleBar + Scale Ratio, Clean Legend, CRS & Source
    """
    pl = c_void_p()
    _vcall(mxdoc, 4, POINTER(c_void_p))(mxdoc, byref(pl))  # get_PageLayout
    if not pl.value:
        raise RuntimeError("Could not retrieve PageLayout from MxDocument.")

    # 1. Configure A4 Landscape in Centimeters (29.7 x 21.0 cm)
    page = c_void_p()
    _vcall(pl, 3, POINTER(c_void_p))(pl, byref(page))  # get_Page
    if page.value:
        _vcall(page, 8, c_long)(page, 8)               # put_Units(8 = esriCentimeters)
        _vcall(page, 6, c_short)(page, 2)              # put_Orientation(2 = Landscape)
        _vcall(page, 25, c_double, c_double)(page, 29.7, 21.0)  # PutCustomSize(29.7, 21.0)
        _release(page)

    gc = _qi(pl, IID_IGraphicsContainer)
    fmap = c_void_p()
    _vcall(mxdoc, 9, POINTER(c_void_p))(mxdoc, byref(fmap))

    # Locate the main MapFrame & clear all non-MapFrame layout elements cleanly
    mf = None
    mf_el = None
    to_delete = []
    _vcall(gc, 4)(gc)  # Reset
    while True:
        el = c_void_p()
        hr = _vcall(gc, 3, POINTER(c_void_p))(gc, byref(el))  # Next
        if hr != 0 or not el.value:
            break
        is_mf = _qi(el, IID_IMapFrame)
        if is_mf:
            if mf is None:
                mf = is_mf
                mf_el = el
            else:
                _release(is_mf)
                _release(el)
        else:
            to_delete.append(el)
    for el in to_delete:
        _vcall(gc, 7, c_void_p)(gc, el)  # DeleteElement
        _release(el)

    # Clear old MapSurrounds on FocusMap so no stale duplicate legends remain
    _vcall(fmap, 35)(fmap)  # ClearMapSurrounds()

    # 2. Position Left MapFrame (1.7, 1.6) -> (19.9, 19.4) cm (leaves clean 1.2 cm gutter before right panel!)
    if mf and mf_el:
        mf_env = _make_envelope(1.7, 1.6, 19.9, 19.4)
        mf_geom = _qi(mf_env, IID_IGeometry)
        _vcall(mf_el, 4, c_void_p)(mf_el, mf_geom)  # put_Geometry
        _vcall(gc, 14, c_void_p)(gc, mf_el)         # UpdateElement
        _release(mf_geom)
        _release(mf_env)

        # Attach PerKaBIG-style UTM MeasuredGrid (clean round coords, 0 decimals, vertical L/R labels)
        if add_grid:
            mgrids = _qi(mf, IID_IMapGrids)
            if mgrids:
                _vcall(mgrids, 8)(mgrids)  # ClearMapGrids()
                mg_obj = _co(CLSID_MeasuredGrid, IID_IMeasuredGrid)
                mgrid = _qi(mg_obj, IID_IMapGrid)
                if mgrid:
                    nb = _bstr("Grid UTM")
                    _vcall(mgrid, 4, c_void_p)(mgrid, nb)
                    _free_bstr(nb)
                    _vcall(mgrid, 31, c_void_p)(mgrid, mf)  # SetDefaults(mf)
                    interval = _nice_grid_interval(span_meters)
                    _vcall(mg_obj, 14, c_short)(mg_obj, -1)         # FixedOrigin = True (round UTM!)
                    _vcall(mg_obj, 4, c_double)(mg_obj, 0.0)        # XOrigin = 0.0
                    _vcall(mg_obj, 8, c_double)(mg_obj, 0.0)        # YOrigin = 0.0
                    _vcall(mg_obj, 12, c_long)(mg_obj, 9)           # Units = 9 (esriMeters)
                    _vcall(mg_obj, 6, c_double)(mg_obj, interval)   # XIntervalSize
                    _vcall(mg_obj, 10, c_double)(mg_obj, interval)  # YIntervalSize
                    # Clean integer coordinate labels (0 decimals, vertical on Left & Right axes)
                    try:
                        fgl = _co(CLSID_FormattedGridLabel, IID_IFormattedGridLabel)
                        nf = _co(CLSID_NumericFormat, IID_INumericFormat)
                        _vcall(nf, 3, c_long)(nf, 0)    # esriRoundNumberOfDecimals
                        _vcall(nf, 5, c_long)(nf, 0)    # 0 decimal places
                        _vcall(nf, 11, c_short)(nf, 0)  # UseSeparator = False
                        _vcall(nf, 13, c_short)(nf, 0)  # ZeroPad = False
                        inf = _qi(nf, IID_INumberFormat)
                        _vcall(fgl, 4, c_void_p)(fgl, inf)  # put_Format
                        igl = _qi(fgl, IID_IGridLabel)
                        _vcall(igl, 14, c_long, c_short)(igl, 3, -1)  # Left axis vertical
                        _vcall(igl, 14, c_long, c_short)(igl, 4, -1)  # Right axis vertical
                        _vcall(mgrid, 10, c_void_p)(mgrid, igl)       # put_LabelFormat
                        _release(igl)
                        _release(inf)
                        _release(nf)
                        _release(fgl)
                    except Exception:
                        pass
                    grid_line = make_line_symbol(176, 190, 197, width=0.3, style="solid")
                    igline = _qi(grid_line, IID_ILineSymbol)
                    _vcall(mgrid, 6, c_void_p)(mgrid, igline)  # put_LineSymbol
                    _vcall(mgrid, 24, c_short)(mgrid, -1)      # Visible = True
                    _vcall(mgrids, 6, c_void_p)(mgrids, mgrid) # AddMapGrid
                    _release(igline)
                    _release(grid_line)
                    _release(mgrid)
                _release(mg_obj)
                _release(mgrids)

    # 3. Draw Outer Page Border & Proportional Right Info Panel Boxes (21.1 -> 29.1 cm, center x=25.1)
    _add_rect(gc, 0.6, 0.6, 29.1, 20.4, outline_rgb=(30, 30, 30), outline_width=1.4, hollow=True, name="outer_frame")
    _add_rect(gc, 21.1, 17.2, 29.1, 20.4, outline_rgb=(50, 50, 50), outline_width=0.9, hollow=True, name="box_title")
    _add_rect(gc, 21.1, 13.4, 29.1, 17.2, outline_rgb=(50, 50, 50), outline_width=0.9, hollow=True, name="box_scale")
    _add_rect(gc, 21.1, 4.2, 29.1, 13.4, outline_rgb=(50, 50, 50), outline_width=0.9, hollow=True, name="box_legend")
    _add_rect(gc, 21.1, 0.6, 29.1, 4.2, outline_rgb=(50, 50, 50), outline_width=0.9, hollow=True, name="box_info")

    # 4. Box 1: Bold Title & Subtitle (centered cleanly inside 21.1..29.1, y=17.2..20.4)
    wrapped_title = _wrap_text(title.upper(), max_chars=24)
    _add_text(gc, 25.1, 19.3, wrapped_title, size=11.5, rgb=(18, 24, 34), halign=1, valign=1, name="judul_peta", bold=True)
    if subtitle:
        _add_text(gc, 25.1, 17.9, _wrap_text(subtitle, max_chars=34), size=8.0, rgb=(70, 80, 92), halign=1, valign=1, name="subjudul_peta")

    # 5. Box 2: North Arrow + Scale Ratio + Metric Kilometers Scale Bar (y=13.4..17.2)
    if mf:
        # North Arrow (cleanly padded below top divider y=17.2)
        uid_na = _make_uid(SURROUND_NORTH_CLSID)
        na_sf = c_void_p()
        if _vcall(mf, 21, c_void_p, c_void_p, POINTER(c_void_p))(mf, uid_na, None, byref(na_sf)) == 0 and na_sf.value:
            el = _qi(na_sf, IID_IElement)
            env = _make_envelope(24.4, 15.5, 25.8, 16.85)
            geom = _qi(env, IID_IGeometry)
            _vcall(el, 4, c_void_p)(el, geom)
            _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
            _release(geom)
            _release(env)
            _release(el)
            _release(na_sf)
        _release(uid_na)

        # Alternating Scale Bar in Kilometers (km)
        uid_sb = _make_uid(SURROUND_SCALEBAR_CLSID)
        sb_sf = c_void_p()
        if _vcall(mf, 21, c_void_p, c_void_p, POINTER(c_void_p))(mf, uid_sb, None, byref(sb_sf)) == 0 and sb_sf.value:
            sb_ms = c_void_p()
            _vcall(sb_sf, 11, POINTER(c_void_p))(sb_sf, byref(sb_ms))  # get_MapSurround
            if sb_ms.value:
                sb = _qi(sb_ms, IID_IScaleBar)
                if sb:
                    _vcall(sb, 22, c_long)(sb, 10)   # Units = 10 (esriKilometers)
                    ul = _bstr("km")
                    _vcall(sb, 24, c_void_p)(sb, ul) # UnitLabel = "km"
                    _free_bstr(ul)
                    _vcall(sb, 16, c_short)(sb, 2)   # Divisions = 2
                    _vcall(sb, 20, c_short)(sb, 2)   # Subdivisions = 2
                    _release(sb)
                _release(sb_ms)
            el = _qi(sb_sf, IID_IElement)
            env = _make_envelope(21.7, 13.75, 28.5, 14.55)
            geom = _qi(env, IID_IGeometry)
            _vcall(el, 4, c_void_p)(el, geom)
            _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
            _release(geom)
            _release(env)
            _release(el)
            _release(sb_sf)
        _release(uid_sb)

    if scale_text:
        _add_text(gc, 25.1, 15.05, scale_text, size=8.5, rgb=(25, 25, 25), halign=1, valign=1, name="skala_angka", bold=True)

    # 6. Box 3: Clean Legend with Multi-Class Layer Headings & No Raw Field Names (y=4.2..13.4)
    hidden = []
    if mf:
        uid_leg = _make_uid(SURROUND_LEGEND_CLSID)
        leg_sf = c_void_p()
        if _vcall(mf, 21, c_void_p, c_void_p, POINTER(c_void_p))(mf, uid_leg, None, byref(leg_sf)) == 0 and leg_sf.value:
            ms = c_void_p()
            _vcall(leg_sf, 11, POINTER(c_void_p))(leg_sf, byref(ms))  # get_MapSurround
            if ms.value:
                leg = _qi(ms, IID_ILegend)
                if leg:
                    tb = _bstr("LEGENDA")
                    _vcall(leg, 16, c_void_p)(leg, tb)  # put_Title("LEGENDA")
                    _free_bstr(tb)
                    # Clean legend item formatting: hide raw field headings, show multi-class layer names
                    icnt = c_long(0)
                    _vcall(leg, 25, POINTER(c_long))(leg, byref(icnt))
                    for li in range(icnt.value):
                        item = c_void_p()
                        _vcall(leg, 26, c_long, POINTER(c_void_p))(leg, li, byref(item))
                        if item.value:
                            _vcall(item, 20, c_short)(item, 0)  # ShowHeading = False (no raw column name!)
                            item_lyr = c_void_p()
                            _vcall(item, 5, POINTER(c_void_p))(item, byref(item_lyr))
                            if item_lyr.value:
                                gfl = _qi(item_lyr, IID_IGeoFeatureLayer)
                                if gfl:
                                    rend = c_void_p()
                                    if _vcall(gfl, 32, POINTER(c_void_p))(gfl, byref(rend)) == 0 and rend.value:
                                        is_uv = _qi(rend, IID_IUniqueValueRenderer)
                                        is_cb = _qi(rend, IID_IClassBreaksRenderer)
                                        if is_uv or is_cb:
                                            _vcall(item, 14, c_short)(item, -1)  # ShowLayerName = True for multi-class!
                                        _release(is_uv)
                                        _release(is_cb)
                                        _release(rend)
                                    _release(gfl)
                                _release(item_lyr)
                            _release(item)
                    _release(leg)
                _release(ms)
            el = _qi(leg_sf, IID_IElement)
            env = _make_envelope(21.6, 4.6, 28.6, 13.0)
            geom = _qi(env, IID_IGeometry)
            _vcall(el, 4, c_void_p)(el, geom)
            _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
            _release(geom)
            _release(env)
            _release(el)
            _release(leg_sf)
        _release(uid_leg)

    if hide_legend_names:
        hidden = hide_legend_layers(mxdoc, hide_legend_names)

    # 7. Box 4: Projection & Source Info (y=0.6..4.2)
    info_lines = []
    if crs_text:
        info_lines.append("Sistem Proyeksi:\n" + _wrap_text(crs_text, max_chars=34))
    if source:
        info_lines.append(_wrap_text(source, max_chars=34))
    if info_lines:
        _add_text(gc, 21.6, 3.85, "\n\n".join(info_lines), size=7.5, rgb=(50, 55, 60), halign=0, valign=0, name="sumber_data")

    # Zoom layout to full page so user sees the entire A4 PerKaBIG sheet cleanly
    _vcall(pl, 12)(pl)  # ZoomToWhole()

    _release(mf)
    _release(mf_el)
    _release(fmap)
    _release(gc)
    _release(pl)
    refresh_mxdoc(mxdoc)
    return hidden

