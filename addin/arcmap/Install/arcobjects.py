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
IID_ILegend3 = _guid("{15ABE7F7-49E8-4012-BE72-D14E5A755ACE}")
IID_ILegendFormat = _guid("{52AA5BC1-CB21-11D0-87F9-080009EC732A}")
IID_ILegendClassFormat = _guid("{52AA5BC2-CB21-11D0-87F9-080009EC732A}")

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
IID_INorthArrow = _guid("{7A3F91F1-B9E3-11D1-8756-0000F8751720}")
IID_ILegendItem = _guid("{52AA5BC0-CB21-11D0-87F9-080009EC732A}")
CLSID_FormattedGridLabel = _guid("{CE41C507-9DF9-11D2-AADE-000000000000}")
IID_IFormattedGridLabel = _guid("{CE41C505-9DF9-11D2-AADE-000000000000}")
IID_IGridLabel = _guid("{CE41C501-9DF9-11D2-AADE-000000000000}")
CLSID_NumericFormat = _guid("{7E4F4719-8E54-11D2-AAD8-000000000000}")
IID_INumericFormat = _guid("{7E4F4710-8E54-11D2-AAD8-000000000000}")
IID_INumberFormat = _guid("{7E4F470D-8E54-11D2-AAD8-000000000000}")
IID_IFont = _guid("{BEF6E002-A874-101A-8BBA-00AA00300CAB}")
CLSID_MapFrame = _guid("{DD94D770-836D-11D0-87EC-080009EC732A}")
CLSID_Map = _guid("{E6BDAA76-4D35-11D0-98BE-00805F7CED21}")
IID_IMap = _guid("{E6BDAA75-4D35-11D0-98BE-00805F7CED21}")
IID_IMaps = _guid("{DFD9FF9F-3080-11D2-94C9-080009EEBECB}")
CLSID_LocatorRectangle = _guid("{83FFCAE2-EDCA-11D0-8683-0000F8751720}")
IID_ILocatorRectangle = _guid("{41D9C095-A403-11D2-87E8-0000F8751720}")
CLSID_SymbolBorder = _guid("{A5D0F017-62DD-11D2-87BE-0000F8751720}")
IID_ISymbolBorder = _guid("{E4240308-DD41-11D2-B868-00600802E603}")
IID_IBorder = _guid("{E4240307-DD41-11D2-B868-00600802E603}")
IID_IFrameProperties = _guid("{D1B1B855-5885-4D7F-A444-A02E3E1078C3}")
CLSID_PngPictureElement = _guid("{85CD6330-D45A-11D3-A414-0004AC1B1D86}")
IID_IPictureElement = _guid("{C0A341C2-CFE2-11D1-9140-0000F87808EE}")
CLSID_Graticule = _guid("{03762C8F-F4D0-11D1-ADE8-080009EC732A}")
CLSID_DMSGridLabel = _guid("{CE41C506-9DF9-11D2-AADE-000000000000}")
IID_IDMSGridLabel = _guid("{CE41C503-9DF9-11D2-AADE-000000000000}")
CLSID_PolygonElement = _guid("{3A9767C7-F253-11D0-83A4-080009B996CC}")
CLSID_Polygon = _guid("{00A5CB42-52DA-11D0-A8F2-00608C85EDE5}")
IID_IPointCollection = _guid("{DE08A214-4FC2-11D1-834E-0000F8775BE9}")

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
    # Official 23-class Indonesian Land Cover palette (KLHK / SNI 7645)
    "klhk": [
        (0, 68, 27),      # 1. Hutan Lahan Kering Primer
        (27, 120, 55),    # 2. Hutan Lahan Kering Sekunder
        (0, 104, 55),     # 3. Hutan Mangrove Primer
        (49, 163, 84),    # 4. Hutan Mangrove Sekunder
        (0, 90, 50),      # 5. Hutan Rawa Primer
        (65, 171, 93),    # 6. Hutan Rawa Sekunder
        (116, 196, 118),  # 7. Hutan Tanaman
        (194, 230, 153),  # 8. Semak Belukar
        (161, 217, 155),  # 9. Belukar Rawa
        (230, 245, 152),  # 10. Savana / Padang Rumput
        (184, 225, 134),  # 11. Perkebunan
        (254, 224, 139),  # 12. Pertanian Lahan Kering
        (253, 174, 97),   # 13. Pertanian Lahan Kering Campur
        (204, 255, 102),  # 14. Sawah
        (107, 174, 214),  # 15. Tambak
        (227, 26, 28),    # 16. Permukiman / Pemukiman
        (245, 222, 179),  # 17. Tanah Terbuka / Lahan Terbuka
        (140, 81, 10),    # 18. Pertambangan
        (99, 99, 99),     # 19. Bandara / Pelabuhan
        (251, 154, 153),  # 20. Transmigrasi
        (127, 205, 187),  # 21. Rawa
        (33, 113, 181),   # 22. Tubuh Air / Badan Air
        (247, 247, 247),  # 23. Awan
    ],
}

KLHK_CLASS_COLORS = [
    (("hutan lahan kering primer", "hutan primer"), (0, 68, 27)),
    (("hutan lahan kering sekunder", "hutan sekunder"), (27, 120, 55)),
    (("mangrove primer",), (0, 104, 55)),
    (("mangrove sekunder", "mangrove", "bakau"), (49, 163, 84)),
    (("hutan rawa primer",), (0, 90, 50)),
    (("hutan rawa sekunder", "hutan rawa"), (65, 171, 93)),
    (("hutan tanaman", "hutan industri", "hti"), (116, 196, 118)),
    (("hutan", "forest"), (27, 120, 55)),
    (("belukar rawa", "semak rawa"), (161, 217, 155)),
    (("semak", "belukar", "shrub"), (194, 230, 153)),
    (("savana", "sabana", "padang rumput", "rumput", "grass"), (230, 245, 152)),
    (("perkebunan", "kebun", "plantation"), (184, 225, 134)),
    (("lahan kering campur", "pertanian campur", "tegalan campur"), (253, 174, 97)),
    (("pertanian lahan kering", "ladang", "tegalan"), (254, 224, 139)),
    (("sawah", "padi", "rice"), (204, 255, 102)),
    (("tambak", "empang", "kolam", "pond"), (107, 174, 214)),
    (("permukiman", "pemukiman", "terbangun", "urban", "built", "perkotaan"), (227, 26, 28)),
    (("terbuka", "kosong", "gundul", "bare"), (245, 222, 179)),
    (("tambang", "pertambangan", "mining"), (140, 81, 10)),
    (("bandara", "pelabuhan", "airport", "port"), (99, 99, 99)),
    (("transmigrasi",), (251, 154, 153)),
    (("rawa", "swamp", "wetland"), (127, 205, 187)),
    (("tubuh air", "badan air", "perairan", "danau", "waduk", "sungai", "water"), (33, 113, 181)),
    (("awan", "cloud"), (247, 247, 247)),
]


def match_klhk_color(label, fallback_idx=0):
    low = (unicode(label) if not isinstance(label, basestring) else label).strip().lower()
    for keys, rgb in KLHK_CLASS_COLORS:
        for k in keys:
            if k in low:
                return rgb
    base = PALETTES["klhk"]
    return base[fallback_idx % len(base)]


def compute_jenks_breaks(sorted_vals, n_classes):
    """Pure-Python Fisher-Jenks Natural Breaks (returns n_classes upper break thresholds)."""
    vals = [float(v) for v in sorted_vals]
    if not vals:
        return []
    if len(vals) <= n_classes:
        return list(vals)
    # Stratified downsample to <= 400 values for fast O(k * n^2) DP on massive feature classes
    if len(vals) > 400:
        step = float(len(vals) - 1) / 399.0
        vals = [vals[int(round(i * step))] for i in range(400)]

    n = len(vals)
    k = max(2, min(int(n_classes), n))
    mat1 = [[0] * (k + 1) for _ in range(n + 1)]
    mat2 = [[0.0] * (k + 1) for _ in range(n + 1)]

    for i in range(1, k + 1):
        mat1[1][i] = 1
        mat2[1][i] = 0.0
        for j in range(2, n + 1):
            mat2[j][i] = float("inf")

    for l in range(2, n + 1):
        s1 = 0.0
        s2 = 0.0
        w = 0.0
        for m in range(1, l + 1):
            i3 = l - m + 1
            val = vals[i3 - 1]
            s2 += val * val
            s1 += val
            w += 1.0
            v = s2 - (s1 * s1) / w
            if v < 0.0:
                v = 0.0
            i4 = i3 - 1
            if i4 != 0:
                for j in range(2, k + 1):
                    cand = v + mat2[i4][j - 1]
                    if mat2[l][j] >= cand:
                        mat1[l][j] = i3
                        mat2[l][j] = cand
        mat1[l][1] = 1
        mat2[l][1] = v

    breaks = [0.0] * k
    breaks[k - 1] = float(sorted_vals[-1])
    idx = n
    for j in range(k, 1, -1):
        brk_idx = int(mat1[idx][j]) - 2
        if brk_idx < 0:
            brk_idx = 0
        breaks[j - 2] = vals[brk_idx]
        idx = int(mat1[idx][j]) - 1
        if idx < 1:
            idx = 1
    return breaks

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


def sample_palette(preset, count, custom_colors=None, class_labels=None):
    if custom_colors:
        stops = [parse_hex_color(c) for c in custom_colors.split(",") if c.strip()]
    else:
        key = (preset or "hazard").lower()
        if key in ("klhk", "lulc-klhk", "tutupan-lahan"):
            if class_labels:
                return [match_klhk_color(lbl, i) for i, lbl in enumerate(class_labels)]
            base = PALETTES["klhk"]
            return [base[i % len(base)] for i in range(max(1, count))]
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

    colors = sample_palette(preset, len(ordered_values), custom_colors=custom_colors, class_labels=ordered_values)
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


def _nice_grid_interval(span_meters, scale=None):
    # Tabel 5 PerKaBIG No. 3 Tahun 2016 + regional scale fallbacks
    if scale and scale > 0:
        if scale <= 3500:
            return 250.0
        if scale <= 7500:
            return 500.0
        if scale <= 15000:
            return 1000.0
        if scale <= 35000:
            return 2000.0
        if scale <= 75000:
            return 5000.0
        if scale <= 150000:
            return 10000.0
        if scale <= 350000:
            return 20000.0
        return 50000.0
    if span_meters <= 0:
        return 10000.0
    raw = span_meters / 3.5
    for step in [250.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0, 25000.0, 50000.0, 100000.0]:
        if raw <= step:
            return step
    return 100000.0


def _add_hline(gc, x1, x2, y, width=0.7, rgb=(30, 30, 30), name="geokit_sep"):
    _add_rect(gc, x1, y - 0.005, x2, y + 0.005, fill_rgb=rgb, outline_rgb=rgb, outline_width=width, hollow=False, name=name)


def _add_picture(gc, xmin, ymin, xmax, ymax, img_path, name="logo_instansi"):
    try:
        pe = _co(CLSID_PngPictureElement, IID_IPictureElement)
        pb = _bstr(img_path)
        hr = _vcall(pe, 10, c_void_p)(pe, pb)  # ImportPictureFromFile
        _free_bstr(pb)
        if hr == 0:
            el = _qi(pe, IID_IElement)
            env = _make_envelope(xmin, ymin, xmax, ymax)
            geom = _qi(env, IID_IGeometry)
            _vcall(el, 4, c_void_p)(el, geom)
            ep = _qi(pe, IID_IElementProperties)
            if ep:
                nb = _bstr(name)
                _vcall(ep, 4, c_void_p)(ep, nb)
                _free_bstr(nb)
                _release(ep)
            _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
            _release(geom)
            _release(env)
            _release(el)
        _release(pe)
    except Exception:
        pass


def _add_polygon_el(gc, pts_xy, fill_rgb=(0, 0, 0), outline_rgb=(0, 0, 0), outline_width=0.7, hollow=False, name="geokit_poly"):
    poly = _co(CLSID_Polygon, IID_IPointCollection)
    for x, y in pts_xy:
        pt = _make_point(x, y)
        _vcall(poly, 5, c_void_p, c_void_p, c_void_p)(poly, pt, None, None)  # AddPoint
        _release(pt)
    pe = _co(CLSID_PolygonElement, IID_IFillShapeElement)
    r, g, b = fill_rgb
    fs = make_fill_symbol(r, g, b, outline_rgb=outline_rgb, outline_width=outline_width, hollow=hollow)
    ifs = _qi(fs, IID_IFillSymbol)
    _vcall(pe, 4, c_void_p)(pe, ifs)
    el = _qi(pe, IID_IElement)
    geom = _qi(poly, IID_IGeometry)
    _vcall(el, 4, c_void_p)(el, geom)
    ep = _qi(pe, IID_IElementProperties)
    if ep:
        nb = _bstr(name)
        _vcall(ep, 4, c_void_p)(ep, nb)
        _free_bstr(nb)
        _release(ep)
    _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
    _release(geom)
    _release(el)
    _release(ifs)
    _release(fs)
    _release(pe)
    _release(poly)


def _add_big_north_arrow(gc, cx, y_bot, y_top, s=1.0):
    """Draws the official Indonesian PerKaBIG Gambar D.5 vector North Arrow (bold 'U' + split black/white needle)."""
    h = y_top - y_bot
    _add_text(gc, cx, y_top - 0.02 * s, "U", size=9.0 * s, rgb=(0, 0, 0), halign=1, valign=0, name="arah_utara_u", bold=True)
    tip_y = y_top - 0.36 * s
    base_y = y_bot + 0.06 * s
    notch_y = base_y + (tip_y - base_y) * 0.28
    half_w = min(0.32 * s, h * 0.24)
    # Left half: solid black
    _add_polygon_el(
        gc,
        [(cx, tip_y), (cx - half_w, base_y), (cx, notch_y), (cx, tip_y)],
        fill_rgb=(0, 0, 0),
        outline_rgb=(0, 0, 0),
        outline_width=0.7 * s,
        hollow=False,
        name="arah_utara_left",
    )
    # Right half: crisp white with black outline
    _add_polygon_el(
        gc,
        [(cx, tip_y), (cx, notch_y), (cx + half_w, base_y), (cx, tip_y)],
        fill_rgb=(255, 255, 255),
        outline_rgb=(0, 0, 0),
        outline_width=0.7 * s,
        hollow=False,
        name="arah_utara_right",
    )


def _create_inset_mapframe(gc, main_mf, frame_name, xmin, ymin, xmax, ymax, border_rgb=(0, 0, 0)):
    """Creates a secondary inset MapFrame with a styled border and red LocatorRectangle linked to main_mf."""
    in_map = _co(CLSID_Map, IID_IMap)
    in_nb = _bstr(frame_name)
    _vcall(in_map, 4, c_void_p)(in_map, in_nb)
    _free_bstr(in_nb)
    in_mf = _co(CLSID_MapFrame, IID_IMapFrame)
    _vcall(in_mf, 12, c_void_p)(in_mf, in_map)
    fp = _qi(in_mf, IID_IFrameProperties)
    if fp:
        sbdr = _co(CLSID_SymbolBorder, IID_ISymbolBorder)
        bln = make_line_symbol(border_rgb[0], border_rgb[1], border_rgb[2], width=0.85, style="solid")
        ibln = _qi(bln, IID_ILineSymbol)
        _vcall(sbdr, 11, c_void_p)(sbdr, ibln)
        ibdr = _qi(sbdr, IID_IBorder)
        _vcall(fp, 4, c_void_p)(fp, ibdr)
        _release(ibdr)
        _release(ibln)
        _release(bln)
        _release(sbdr)
        _release(fp)
    lrect = _co(CLSID_LocatorRectangle, IID_ILocatorRectangle)
    _vcall(lrect, 3, c_void_p)(lrect, main_mf)
    rbdr = _co(CLSID_SymbolBorder, IID_ISymbolBorder)
    rln = make_line_symbol(220, 38, 38, width=1.6, style="solid")
    irln = _qi(rln, IID_ILineSymbol)
    _vcall(rbdr, 11, c_void_p)(rbdr, irln)
    irbdr = _qi(rbdr, IID_IBorder)
    _vcall(lrect, 6, c_void_p)(lrect, irbdr)
    _vcall(in_mf, 22, c_void_p)(in_mf, lrect)
    _release(irbdr)
    _release(irln)
    _release(rln)
    _release(rbdr)
    _release(lrect)
    in_el = _qi(in_mf, IID_IElement)
    in_env = _make_envelope(xmin, ymin, xmax, ymax)
    in_geom = _qi(in_env, IID_IGeometry)
    _vcall(in_el, 4, c_void_p)(in_el, in_geom)
    _vcall(gc, 5, c_void_p, c_long)(gc, in_el, 0)
    _release(in_geom)
    _release(in_env)
    _release(in_el)
    _release(in_mf)
    _release(in_map)
    return True


def _nice_dms_interval(scale):
    if not scale or scale <= 0:
        return 10.0 / 60.0
    if scale <= 3500:
        return 5.0 / 3600.0
    if scale <= 7500:
        return 10.0 / 3600.0
    if scale <= 15000:
        return 20.0 / 3600.0
    if scale <= 35000:
        return 1.0 / 60.0
    if scale <= 75000:
        return 2.0 / 60.0
    if scale <= 150000:
        return 5.0 / 60.0
    if scale <= 400000:
        return 10.0 / 60.0
    return 30.0 / 60.0


PAPER_SPECS = {
    "a4": (29.7, 21.0, 1.000),
    "a3": (42.0, 29.7, 1.414),
    "a1": (84.1, 59.4, 2.828),
    "a0": (118.9, 84.1, 4.000),
}


def _select_scalebar_division(scale_val, slot_cm):
    """Selects a clean round metric division (m or km) whose 2-division bar fits ~55% of slot_cm."""
    if not scale_val or scale_val <= 0:
        return False, 5.0, min(slot_cm * 0.65, 3.0)
    use_m = scale_val <= 35000
    if use_m:
        cands = [10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0]
        unit_cm = 100.0
    else:
        cands = [0.5, 1.0, 2.0, 2.5, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0]
        unit_cm = 100000.0
    target_cm = slot_cm * 0.54
    best = None
    for d in cands:
        bar_cm = (2.0 * d * unit_cm) / float(scale_val)
        if bar_cm > slot_cm * 0.78:
            continue
        score = abs(bar_cm - target_cm)
        if best is None or score < best[0]:
            best = (score, d, bar_cm)
    if best is None:
        d = cands[0]
        bar_cm = min(slot_cm * 0.72, (2.0 * d * unit_cm) / float(scale_val))
        return use_m, d, bar_cm
    return use_m, best[1], best[2]


def build_perkabig_layout(mxdoc, title="PETA TEMATIK WILAYAH", subtitle="", region="",
                          sheet_number="", source="Sumber: Analisis Spasial GeoKit",
                          author="Laboratorium SIG / Penyusun", edition="Edisi 1 - Tahun 2026",
                          note="", logo_path="", has_inset=False, has_petunjuk=False, paper="a4",
                          crs_text="", utm_zone="", scale_val=0, span_meters=40000.0,
                          hide_legend_names=None, add_grid=True, grid_mode="utm", north_style="big"):
    """
    Constructs a strict PerKaBIG No. 3 Tahun 2016 Landscape layout (A4, A3, A1, or A0):
      - Outer neatline + Map neatline + Main MapFrame with crisp UTM / DMS / Dual RBI grids
      - 7-section Right Info Panel with uniform black typography, centered North+Scale,
        explicit divider lines for Edisi and Catatan, and top-anchored KETERANGAN legend.
    """
    pl = c_void_p()
    _vcall(mxdoc, 4, POINTER(c_void_p))(mxdoc, byref(pl))  # get_PageLayout
    if not pl.value:
        raise RuntimeError("Could not retrieve PageLayout from MxDocument.")

    paper_key = (paper or "a4").lower()
    pw, ph, s = PAPER_SPECS.get(paper_key, PAPER_SPECS["a4"])
    gmode = (grid_mode or "utm").lower()
    if not add_grid:
        gmode = "none"

    # 1. Configure A4 / A3 / A1 / A0 Landscape in Centimeters (8 = esriCentimeters)
    page = c_void_p()
    _vcall(pl, 3, POINTER(c_void_p))(pl, byref(page))  # get_Page
    if page.value:
        _vcall(page, 8, c_long)(page, 8)               # put_Units(8 = esriCentimeters)
        _vcall(page, 6, c_short)(page, 2)              # put_Orientation(2 = Landscape)
        _vcall(page, 25, c_double, c_double)(page, pw, ph)
        _release(page)

    gc = _qi(pl, IID_IGraphicsContainer)
    fmap = c_void_p()
    _vcall(mxdoc, 9, POINTER(c_void_p))(mxdoc, byref(fmap))

    # Locate the main FocusMap MapFrame & delete all other elements (including old inset frames!)
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
            frm_map = c_void_p()
            _vcall(is_mf, 11, POINTER(c_void_p))(is_mf, byref(frm_map))  # get_Map
            is_focus = (frm_map.value == fmap.value) if frm_map.value else (mf is None)
            _release(frm_map)
            if is_focus and mf is None:
                mf = is_mf
                mf_el = el
            else:
                _release(is_mf)
                to_delete.append(el)
        else:
            to_delete.append(el)
    for el in to_delete:
        _vcall(gc, 7, c_void_p)(gc, el)  # DeleteElement
        _release(el)

    # Clear old MapSurrounds on FocusMap so no stale duplicate legends/scalebars remain
    _vcall(fmap, 35)(fmap)  # ClearMapSurrounds()

    # Exact PerKaBIG proportions scaled smoothly across A4 / A3 / A1 / A0
    m_out = 0.50 * s
    m_in = 0.72 * s
    gap_p = 0.30 * s
    ox, oy, ow, oh = m_out, m_out, pw - 2.0 * m_out, ph - 2.0 * m_out
    pw_p = 8.00 * s
    px, py, ph_p = pw - m_in - pw_p, m_in, ph - 2.0 * m_in
    nx, ny, nw, nh = m_in, m_in, px - gap_p - m_in, ph - 2.0 * m_in
    # Generous coordinate border margin so larger UTM/DMS coordinate labels never clip
    m_grid = (1.35 if gmode == "dual" else 1.08) * s
    mx, my, mw, mh = nx + m_grid, ny + m_grid, nw - 2.0 * m_grid, nh - 2.0 * m_grid
    pad = 0.25 * s
    ix = px + pad
    cx = px + pw_p / 2.0

    # 2. Position Main MapFrame + Crisp Border + UTM / DMS / Dual RBI Grids
    if mf and mf_el:
        mf_env = _make_envelope(mx, my, mx + mw, my + mh)
        mf_geom = _qi(mf_env, IID_IGeometry)
        _vcall(mf_el, 4, c_void_p)(mf_el, mf_geom)  # put_Geometry
        _vcall(gc, 14, c_void_p)(gc, mf_el)         # UpdateElement
        _release(mf_geom)
        _release(mf_env)

        mgrids = _qi(mf, IID_IMapGrids)
        if mgrids:
            _vcall(mgrids, 8)(mgrids)  # ClearMapGrids()

            # 2a. Black UTM MeasuredGrid (utm or dual)
            if gmode in ("utm", "dual"):
                mg_obj = _co(CLSID_MeasuredGrid, IID_IMeasuredGrid)
                mgrid = _qi(mg_obj, IID_IMapGrid)
                if mgrid:
                    nb = _bstr("Grid UTM")
                    _vcall(mgrid, 4, c_void_p)(mgrid, nb)
                    _free_bstr(nb)
                    _vcall(mgrid, 31, c_void_p)(mgrid, mf)  # SetDefaults(mf)
                    interval = _nice_grid_interval(span_meters, scale=scale_val)
                    _vcall(mg_obj, 14, c_short)(mg_obj, -1)         # FixedOrigin = True
                    _vcall(mg_obj, 4, c_double)(mg_obj, 0.0)        # XOrigin = 0.0
                    _vcall(mg_obj, 8, c_double)(mg_obj, 0.0)        # YOrigin = 0.0
                    _vcall(mg_obj, 12, c_long)(mg_obj, 9)           # Units = 9 (esriMeters)
                    _vcall(mg_obj, 6, c_double)(mg_obj, interval)   # XIntervalSize
                    _vcall(mg_obj, 10, c_double)(mg_obj, interval)  # YIntervalSize
                    try:
                        fgl = _co(CLSID_FormattedGridLabel, IID_IFormattedGridLabel)
                        nf = _co(CLSID_NumericFormat, IID_INumericFormat)
                        _vcall(nf, 3, c_long)(nf, 0)    # esriRoundNumberOfDecimals
                        _vcall(nf, 5, c_long)(nf, 0)    # 0 decimal places
                        _vcall(nf, 11, c_short)(nf, 0)  # UseSeparator = False
                        _vcall(nf, 13, c_short)(nf, 0)  # ZeroPad = False
                        inf = _qi(nf, IID_INumberFormat)
                        _vcall(fgl, 4, c_void_p)(fgl, inf)
                        igl = _qi(fgl, IID_IGridLabel)
                        gfnt = c_void_p()
                        if _vcall(igl, 7, POINTER(c_void_p))(igl, byref(gfnt)) == 0 and gfnt.value:
                            gifnt = _qi(gfnt, IID_IFont)
                            if gifnt:
                                g_pt = (8.0 if gmode == "dual" else 8.6) * s
                                _vcall(gifnt, 6, ctypes.c_longlong)(gifnt, int(g_pt * 10000))
                                _release(gifnt)
                            _vcall(igl, 8, c_void_p)(igl, gfnt)
                            _release(gfnt)
                        _vcall(igl, 12, c_double)(igl, 3.0 * s)       # LabelOffset
                        _vcall(igl, 14, c_long, c_short)(igl, 1, -1)  # Top = Horizontal
                        _vcall(igl, 14, c_long, c_short)(igl, 2, -1)  # Bottom = Horizontal
                        _vcall(igl, 14, c_long, c_short)(igl, 3, 0)   # Left = VERTICAL
                        _vcall(igl, 14, c_long, c_short)(igl, 4, 0)   # Right = VERTICAL
                        _vcall(mgrid, 10, c_void_p)(mgrid, igl)
                        _release(igl)
                        _release(inf)
                        _release(nf)
                        _release(fgl)
                    except Exception:
                        pass
                    grid_line = make_line_symbol(140, 150, 160, width=0.28 * s, style="solid")
                    igline = _qi(grid_line, IID_ILineSymbol)
                    _vcall(mgrid, 6, c_void_p)(mgrid, igline)
                    _vcall(mgrid, 14, c_double)(mgrid, 3.2 * s)
                    _vcall(mgrid, 24, c_short)(mgrid, -1)
                    _vcall(mgrids, 6, c_void_p)(mgrids, mgrid)
                    _release(igline)
                    _release(grid_line)
                    _release(mgrid)
                _release(mg_obj)

            # 2b. Blue BIG Geographic WGS84 DMS Graticule (dms or dual — official RBI Cetak!)
            if gmode in ("dms", "dual"):
                try:
                    gr_obj = _co(CLSID_Graticule, IID_IMeasuredGrid)
                    gr_grid = _qi(gr_obj, IID_IMapGrid)
                    if gr_grid:
                        nb = _bstr("Gratikul Geografi")
                        _vcall(gr_grid, 4, c_void_p)(gr_grid, nb)
                        _free_bstr(nb)
                        _vcall(gr_grid, 31, c_void_p)(gr_grid, mf)  # SetDefaults(mf)
                        dms_int = _nice_dms_interval(scale_val)
                        _vcall(gr_obj, 14, c_short)(gr_obj, -1)         # FixedOrigin = True
                        _vcall(gr_obj, 4, c_double)(gr_obj, 0.0)        # XOrigin = 0.0
                        _vcall(gr_obj, 8, c_double)(gr_obj, 0.0)        # YOrigin = 0.0
                        _vcall(gr_obj, 6, c_double)(gr_obj, dms_int)    # XIntervalSize (deg)
                        _vcall(gr_obj, 10, c_double)(gr_obj, dms_int)   # YIntervalSize (deg)
                        dms_lbl = _co(CLSID_DMSGridLabel, IID_IGridLabel)
                        # BIG Blue (#0070C0) DMS coordinate text
                        bcol = make_rgb(0, 112, 192)
                        ibcol = _qi(bcol, IID_IColor)
                        _vcall(dms_lbl, 10, c_void_p)(dms_lbl, ibcol)  # put_Color
                        _release(ibcol)
                        _release(bcol)
                        dfnt = c_void_p()
                        if _vcall(dms_lbl, 7, POINTER(c_void_p))(dms_lbl, byref(dfnt)) == 0 and dfnt.value:
                            difnt = _qi(dfnt, IID_IFont)
                            if difnt:
                                d_pt = (7.2 if gmode == "dual" else 7.6) * s
                                _vcall(difnt, 6, ctypes.c_longlong)(difnt, int(d_pt * 10000))
                                _release(difnt)
                            _vcall(dms_lbl, 8, c_void_p)(dms_lbl, dfnt)
                            _release(dfnt)
                        # Offset cleanly outside UTM labels in dual mode so black UTM and blue DMS never collide
                        _vcall(dms_lbl, 12, c_double)(dms_lbl, (13.8 * s) if gmode == "dual" else (3.2 * s))
                        _vcall(dms_lbl, 14, c_long, c_short)(dms_lbl, 1, -1)  # Top Horizontal
                        _vcall(dms_lbl, 14, c_long, c_short)(dms_lbl, 2, -1)  # Bottom Horizontal
                        _vcall(dms_lbl, 14, c_long, c_short)(dms_lbl, 3, 0)   # Left VERTICAL
                        _vcall(dms_lbl, 14, c_long, c_short)(dms_lbl, 4, 0)   # Right VERTICAL
                        _vcall(gr_grid, 10, c_void_p)(gr_grid, dms_lbl)
                        _release(dms_lbl)
                        if gmode == "dual":
                            bline = make_line_symbol(0, 112, 192, width=0.0, style="null", null_color=True)
                        else:
                            bline = make_line_symbol(0, 112, 192, width=0.22 * s, style="dash")
                        ibline = _qi(bline, IID_ILineSymbol)
                        _vcall(gr_grid, 6, c_void_p)(gr_grid, ibline)
                        btick = make_line_symbol(0, 112, 192, width=0.70 * s, style="solid")
                        ibtick = _qi(btick, IID_ILineSymbol)
                        _vcall(gr_grid, 8, c_void_p)(gr_grid, ibtick)
                        _vcall(gr_grid, 14, c_double)(gr_grid, 4.2 * s)
                        _vcall(gr_grid, 24, c_short)(gr_grid, -1)
                        _vcall(mgrids, 6, c_void_p)(mgrids, gr_grid)
                        _release(ibtick)
                        _release(btick)
                        _release(ibline)
                        _release(bline)
                        _release(gr_grid)
                    _release(gr_obj)
                except Exception:
                    pass
            _release(mgrids)

    # 3. Draw Outer Frame, Map Neatline, and Right Info Panel Frame (PerKaBIG 1:1!)
    _add_rect(gc, ox, oy, ox + ow, oy + oh, outline_rgb=(0, 0, 0), outline_width=1.4 * s, hollow=True, name="bingkai_luar")
    _add_rect(gc, nx, ny, nx + nw, ny + nh, outline_rgb=(0, 0, 0), outline_width=0.7 * s, hollow=True, name="bingkai_muka_peta")
    _add_rect(gc, px, py, px + pw_p, py + ph_p, outline_rgb=(0, 0, 0), outline_width=1.0 * s, hollow=True, name="bingkai_panel_informasi")

    top_y = py + ph_p
    cur_y = top_y

    # === SECTION 1: JUDUL PETA, SUBJUDUL/NOMOR LEMBAR & NAMA WILAYAH (HITAM SERAGAM, BESAR & SEIMBANG) ===
    h_hdr = 2.45 * s
    y_hdr_bot = cur_y - h_hdr
    y_hdr_mid = (cur_y + y_hdr_bot) / 2.0
    mid_txt = sheet_number or subtitle
    if region:
        if mid_txt:
            _add_text(gc, cx, cur_y - 0.18 * s, _wrap_text(title.upper(), max_chars=26), size=11.0 * s, rgb=(0, 0, 0), halign=1, valign=0, name="lbl_judul", bold=True)
            _add_text(gc, cx, y_hdr_mid - 0.08 * s, _wrap_text(mid_txt, max_chars=48), size=7.2 * s, rgb=(0, 0, 0), halign=1, valign=1, name="lbl_subjudul", bold=True)
            _add_text(gc, cx, y_hdr_bot + 0.18 * s, region.upper(), size=12.6 * s, rgb=(0, 0, 0), halign=1, valign=2, name="lbl_nama_wilayah", bold=True)
        else:
            _add_text(gc, cx, cur_y - 0.34 * s, _wrap_text(title.upper(), max_chars=24), size=11.8 * s, rgb=(0, 0, 0), halign=1, valign=0, name="lbl_judul", bold=True)
            _add_text(gc, cx, y_hdr_bot + 0.34 * s, region.upper(), size=13.2 * s, rgb=(0, 0, 0), halign=1, valign=2, name="lbl_nama_wilayah", bold=True)
    else:
        if mid_txt:
            _add_text(gc, cx, cur_y - 0.32 * s, _wrap_text(title.upper(), max_chars=24), size=12.2 * s, rgb=(0, 0, 0), halign=1, valign=0, name="lbl_judul", bold=True)
            _add_text(gc, cx, y_hdr_bot + 0.28 * s, _wrap_text(mid_txt, max_chars=46), size=7.6 * s, rgb=(0, 0, 0), halign=1, valign=2, name="lbl_subjudul", bold=True)
        else:
            _add_text(gc, cx, y_hdr_mid, _wrap_text(title.upper(), max_chars=22), size=12.8 * s, rgb=(0, 0, 0), halign=1, valign=1, name="lbl_judul", bold=True)
    cur_y = y_hdr_bot
    _add_hline(gc, px, px + pw_p, cur_y, width=0.7 * s, name="garis_header")

    # === SECTION 2: ARAH UTARA + SKALA ANGKA & SKALA BATANG PRESISI TENGAH ===
    h_sc = 1.95 * s
    y_sc_bot = cur_y - h_sc
    if mf:
        if (north_style or "big").lower() == "classic":
            uid_na = _make_uid(SURROUND_NORTH_CLSID)
            na_sf = c_void_p()
            if _vcall(mf, 21, c_void_p, c_void_p, POINTER(c_void_p))(mf, uid_na, None, byref(na_sf)) == 0 and na_sf.value:
                na_ms = c_void_p()
                _vcall(na_sf, 11, POINTER(c_void_p))(na_sf, byref(na_ms))
                if na_ms.value:
                    ina = _qi(na_ms, IID_INorthArrow)
                    if ina:
                        _vcall(ina, 16, c_double)(ina, 40.0 * s)
                        _release(ina)
                    _release(na_ms)
                el = _qi(na_sf, IID_IElement)
                env = _make_envelope(ix + 0.04 * s, y_sc_bot + 0.16 * s, ix + 1.44 * s, cur_y - 0.16 * s)
                geom = _qi(env, IID_IGeometry)
                _vcall(el, 4, c_void_p)(el, geom)
                _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
                _release(geom)
                _release(env)
                _release(el)
                _release(na_sf)
            _release(uid_na)
        else:
            _add_big_north_arrow(gc, ix + 0.68 * s, y_sc_bot + 0.16 * s, cur_y - 0.16 * s, s=s)

        # Right side of Section 2: SKALA 1 : N + physically centered Alternating Scale Bar
        sb_x = ix + 1.50 * s
        sb_right = px + pw_p - pad
        sb_slot_w = sb_right - sb_x
        sb_cx = (sb_x + sb_right) / 2.0
        scale_txt = ("SKALA 1 : %s" % ("{:,}".format(int(round(scale_val))).replace(",", "."))) if scale_val else "SKALA 1 : -"
        _add_text(gc, sb_cx, cur_y - 0.24 * s, scale_txt, size=9.6 * s, rgb=(0, 0, 0), halign=1, valign=0, name="skala_angka", bold=True)

        use_m, div_val, bar_cm = _select_scalebar_division(scale_val, sb_slot_w)
        est_total_w = min(sb_slot_w, bar_cm + 0.55 * s)
        sb_env_x1 = max(sb_x, sb_cx - est_total_w / 2.0)
        sb_env_x2 = min(sb_right, sb_env_x1 + est_total_w)

        uid_sb = _make_uid(SURROUND_SCALEBAR_CLSID)
        sb_sf = c_void_p()
        if _vcall(mf, 21, c_void_p, c_void_p, POINTER(c_void_p))(mf, uid_sb, None, byref(sb_sf)) == 0 and sb_sf.value:
            sb_ms = c_void_p()
            _vcall(sb_sf, 11, POINTER(c_void_p))(sb_sf, byref(sb_ms))
            if sb_ms.value:
                sb = _qi(sb_ms, IID_IScaleBar)
                if sb:
                    _vcall(sb, 22, c_long)(sb, 9 if use_m else 10)  # 9=Meters, 10=Kilometers
                    ul = _bstr("m" if use_m else "km")
                    _vcall(sb, 24, c_void_p)(sb, ul)
                    _free_bstr(ul)
                    _vcall(sb, 16, c_short)(sb, 2)  # Divisions = 2
                    _vcall(sb, 20, c_short)(sb, 2)  # Subdivisions = 2
                    _vcall(sb, 14, c_double)(sb, div_val)
                    _vcall(sb, 46, c_long)(sb, 0)          # ResizeHint = 0 (esriScaleBarFixed)
                    _vcall(sb, 44, c_double)(sb, 5.4 * s)  # BarHeight
                    sb_ts = make_text_symbol(size=7.4 * s, rgb=(0, 0, 0), halign=1, valign=2)
                    _vcall(sb, 40, c_void_p)(sb, sb_ts)    # put_LabelSymbol
                    _vcall(sb, 30, c_void_p)(sb, sb_ts)    # put_UnitLabelSymbol
                    _release(sb_ts)
                    _release(sb)
                _release(sb_ms)
            el = _qi(sb_sf, IID_IElement)
            env = _make_envelope(sb_env_x1, y_sc_bot + 0.24 * s, sb_env_x2, y_sc_bot + 1.08 * s)
            geom = _qi(env, IID_IGeometry)
            _vcall(el, 4, c_void_p)(el, geom)
            _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
            _release(geom)
            _release(env)
            _release(el)
            _release(sb_sf)
        _release(uid_sb)
    cur_y = y_sc_bot
    _add_hline(gc, px, px + pw_p, cur_y, width=0.7 * s, name="garis_skala")

    # === SECTION 3: PETUNJUK LETAK PETA (KIRI) & DIAGRAM LOKASI (KANAN) — HEADER HITAM SERAGAM ===
    inset_created = False
    petunjuk_created = False
    if (has_inset or has_petunjuk) and mf:
        h_ins = 2.85 * s
        y_ins_bot = cur_y - h_ins
        if has_petunjuk and has_inset:
            gap = 0.22 * s
            half_w = (pw_p - 2.0 * pad - gap) / 2.0
            x1_l = ix
            x1_r = ix + half_w
            x2_l = x1_r + gap
            x2_r = px + pw_p - pad
            _add_text(gc, (x1_l + x1_r) / 2.0, cur_y - 0.11 * s, "PETUNJUK LETAK PETA", size=6.3 * s, rgb=(0, 0, 0), halign=1, valign=0, name="lbl_petunjuk_letak", bold=True)
            _add_text(gc, (x2_l + x2_r) / 2.0, cur_y - 0.11 * s, "DIAGRAM LOKASI", size=6.3 * s, rgb=(0, 0, 0), halign=1, valign=0, name="lbl_diagram_lokasi", bold=True)
            try:
                petunjuk_created = _create_inset_mapframe(gc, mf, "Petunjuk Letak Peta", x1_l, y_ins_bot + 0.14 * s, x1_r, cur_y - 0.46 * s, border_rgb=(0, 0, 0))
            except Exception:
                pass
            try:
                inset_created = _create_inset_mapframe(gc, mf, "Diagram Lokasi", x2_l, y_ins_bot + 0.14 * s, x2_r, cur_y - 0.46 * s, border_rgb=(0, 0, 0))
            except Exception:
                pass
        else:
            lbl_t = "PETUNJUK LETAK PETA" if has_petunjuk else "DIAGRAM LOKASI"
            frm_nm = "Petunjuk Letak Peta" if has_petunjuk else "Diagram Lokasi"
            _add_text(gc, cx, cur_y - 0.11 * s, lbl_t, size=7.2 * s, rgb=(0, 0, 0), halign=1, valign=0, name="lbl_diagram_lokasi", bold=True)
            try:
                ok = _create_inset_mapframe(gc, mf, frm_nm, ix + 0.15 * s, y_ins_bot + 0.14 * s, px + pw_p - pad - 0.15 * s, cur_y - 0.48 * s, border_rgb=(0, 0, 0))
                if has_petunjuk:
                    petunjuk_created = ok
                else:
                    inset_created = ok
            except Exception:
                pass
        cur_y = y_ins_bot
        _add_hline(gc, px, px + pw_p, cur_y, width=0.7 * s, name="garis_inset")

    # === SECTION 4: SISTEM REFERENSI KOORDINAT (VERTIKAL TERPUSAT & LEBIH JELAS) ===
    h_proj = 1.48 * s
    y_proj_bot = cur_y - h_proj
    y_proj_mid = (cur_y + y_proj_bot) / 2.0
    zone_txt = utm_zone or (crs_text if crs_text else "UTM")
    grid_desc = "Grid UTM & Gratikul Geografi" if gmode == "dual" else ("Gratikul Geografi (DMS)" if gmode == "dms" else "Grid UTM (Meter)")
    proj_lines = (
        "Proyeksi         : %s\n"
        "Sistem Grid      : %s\n"
        "Datum Horizontal : SRGI 2013 / WGS 1984"
    ) % (zone_txt, grid_desc)
    _add_text(gc, ix, y_proj_mid, proj_lines, size=7.2 * s, rgb=(0, 0, 0), halign=0, valign=1, name="lbl_proyeksi")
    cur_y = y_proj_bot
    _add_hline(gc, px, px + pw_p, cur_y, width=0.7 * s, name="garis_proyeksi")

    # === SECTION 5: PEMBUAT / INSTANSI (+ OPTIONAL LOGO) | GARIS PEMISAH | EDISI & HAK CIPTA ===
    h_org = 1.90 * s
    y_org_bot = cur_y - h_org
    y_ed_sep = y_org_bot + 0.44 * s
    y_org_upper_mid = (cur_y + y_ed_sep) / 2.0
    text_x = ix
    max_ch = 34
    if logo_path:
        lsz = 1.06 * s
        _add_picture(gc, ix, y_org_upper_mid - lsz / 2.0, ix + lsz, y_org_upper_mid + lsz / 2.0, logo_path, name="logo_instansi")
        text_x = ix + lsz + 0.22 * s
        max_ch = 25
    _add_text(gc, text_x, cur_y - 0.18 * s, "DIBUAT DAN DITERBITKAN OLEH:", size=6.8 * s, rgb=(0, 0, 0), halign=0, valign=0, name="lbl_instansi_hdr", bold=True)
    _add_text(gc, text_x, cur_y - 0.56 * s, _wrap_text(author or "Laboratorium SIG / Penyusun", max_chars=max_ch), size=6.8 * s, rgb=(20, 25, 30), halign=0, valign=0, name="lbl_instansi")
    _add_hline(gc, px, px + pw_p, y_ed_sep, width=0.55 * s, name="garis_edisi")
    hc_txt = "%s  |  Hak cipta dilindungi UU" % (edition or "Edisi 1 - Tahun 2026")
    _add_text(gc, cx, (y_org_bot + y_ed_sep) / 2.0, hc_txt, size=5.9 * s, rgb=(35, 40, 45), halign=1, valign=1, name="lbl_hak_cipta")
    cur_y = y_org_bot
    _add_hline(gc, px, px + pw_p, cur_y, width=0.7 * s, name="garis_instansi")

    # === SECTION 7 (ANCHORED AT BOTTOM): SUMBER DATA | GARIS PEMISAH | CATATAN ===
    h_foot = 2.65 * s
    y_foot_top = py + h_foot
    y_note_sep = py + 1.02 * s
    _add_hline(gc, px, px + pw_p, y_foot_top, width=0.7 * s, name="garis_legenda")
    _add_text(gc, ix, y_foot_top - 0.14 * s, "SUMBER DATA DAN RIWAYAT PETA:", size=6.5 * s, rgb=(0, 0, 0), halign=0, valign=0, name="lbl_sumber_hdr", bold=True)
    _add_text(gc, ix, y_foot_top - 0.48 * s, _wrap_text(source or "1. Pengolahan Data Spasial GeoKit.", max_chars=42), size=6.1 * s, rgb=(20, 25, 30), halign=0, valign=0, name="lbl_sumber_riwayat")
    _add_hline(gc, px, px + pw_p, y_note_sep, width=0.55 * s, name="garis_catatan")
    note_txt = note or "CATATAN: Batas wilayah administrasi pada peta ini bukan referensi resmi penegasan batas."
    _add_text(gc, ix, y_note_sep - 0.12 * s, _wrap_text(note_txt, max_chars=48), size=5.4 * s, rgb=(40, 45, 50), halign=0, valign=0, name="lbl_catatan")

    # === SECTION 6 (FIXED-FRAME, TOP-ANCHORED, AUTO-FIT & AUTO-COLUMNS): KETERANGAN ===
    hidden = []
    if mf:
        uid_leg = _make_uid(SURROUND_LEGEND_CLSID)
        leg_sf = c_void_p()
        if _vcall(mf, 21, c_void_p, c_void_p, POINTER(c_void_p))(mf, uid_leg, None, byref(leg_sf)) == 0 and leg_sf.value:
            ms = c_void_p()
            _vcall(leg_sf, 11, POINTER(c_void_p))(leg_sf, byref(ms))
            if ms.value:
                leg = _qi(ms, IID_ILegend)
                if leg:
                    tb = _bstr("KETERANGAN")
                    _vcall(leg, 16, c_void_p)(leg, tb)
                    _free_bstr(tb)
                    # Disable AutoAdd so layers added later cannot explode the legend box
                    _vcall(leg, 18, c_short)(leg, 0)  # AutoAdd = False

                    # Apply compact PerKaBIG typography & patch sizing via ILegendFormat
                    lfmt = c_void_p()
                    if _vcall(leg, 13, POINTER(c_void_p))(leg, byref(lfmt)) == 0 and lfmt.value:
                        try:
                            t_sym = make_text_symbol(size=8.6 * s, rgb=(0, 0, 0), halign=0, valign=0, bold=True)
                            _vcall(lfmt, 6, c_void_p)(lfmt, t_sym)         # put_TitleSymbol
                            _release(t_sym)
                            _vcall(lfmt, 10, c_double)(lfmt, 3.5 * s)      # put_TitleGap
                            _vcall(lfmt, 12, c_double)(lfmt, 4.0 * s)      # put_VerticalItemGap
                            _vcall(lfmt, 16, c_double)(lfmt, 2.5 * s)      # put_LayerNameGap
                            _vcall(lfmt, 24, c_double)(lfmt, 2.5 * s)      # put_VerticalPatchGap
                            _vcall(lfmt, 28, c_double)(lfmt, 15.0 * s)     # put_DefaultPatchWidth
                            _vcall(lfmt, 30, c_double)(lfmt, 8.5 * s)      # put_DefaultPatchHeight
                            _vcall(leg, 14, c_void_p)(leg, lfmt)           # putref_Format
                        except Exception:
                            pass
                        _release(lfmt)

                    # Style every legend item cleanly (compact 7.5pt layer name, 7.0pt class label)
                    icnt = c_long(0)
                    _vcall(leg, 25, POINTER(c_long))(leg, byref(icnt))
                    for li in range(icnt.value):
                        item = c_void_p()
                        _vcall(leg, 26, c_long, POINTER(c_void_p))(leg, li, byref(item))
                        if item.value:
                            _vcall(item, 20, c_short)(item, 0)  # ShowHeading = False
                            try:
                                ln_sym = make_text_symbol(size=7.4 * s, rgb=(0, 0, 0), halign=0, valign=1, bold=True)
                                _vcall(item, 16, c_void_p)(item, ln_sym)  # put_LayerNameSymbol
                                _release(ln_sym)
                                lcf = c_void_p()
                                if _vcall(item, 23, POINTER(c_void_p))(item, byref(lcf)) == 0 and lcf.value:
                                    lbl_sym = make_text_symbol(size=7.0 * s, rgb=(20, 20, 20), halign=0, valign=1, bold=False)
                                    _vcall(lcf, 4, c_void_p)(lcf, lbl_sym)        # put_LabelSymbol
                                    _vcall(lcf, 8, c_double)(lcf, 15.0 * s)       # put_PatchWidth
                                    _vcall(lcf, 10, c_double)(lcf, 8.5 * s)       # put_PatchHeight
                                    _vcall(item, 24, c_void_p)(item, lcf)         # put_LegendClassFormat
                                    _release(lbl_sym)
                                    _release(lcf)
                            except Exception:
                                pass
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
                                            _vcall(item, 14, c_short)(item, -1)
                                        _release(is_uv)
                                        _release(is_cb)
                                        _release(rend)
                                    _release(gfl)
                                _release(item_lyr)
                            _release(item)
                    _release(leg)

                # Lock legend strictly inside Section 6 via ILegend3 (FixedFrame + AutoColumns + AutoFit!)
                leg3 = _qi(ms, IID_ILegend3)
                if leg3:
                    try:
                        _vcall(leg3, 36, c_short)(leg3, -1)          # put_FixedFrame = True (top-anchored, NEVER overflows!)
                        _vcall(leg3, 38, c_short)(leg3, -1)          # put_AutoColumns = True (auto multi-column for 10-23 classes)
                        _vcall(leg3, 40, c_short)(leg3, -1)          # put_AutoFit = True (shrinks font cleanly if needed)
                        _vcall(leg3, 42, c_double)(leg3, 5.0 * s)    # put_MinFontSize = 5pt * s
                    except Exception:
                        pass
                    _release(leg3)
                _release(ms)

            el = _qi(leg_sf, IID_IElement)
            env = _make_envelope(ix, y_foot_top + 0.14 * s, px + pw_p - pad, cur_y - 0.14 * s)
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

    # Zoom layout to full page so user sees the entire PerKaBIG sheet cleanly
    _vcall(pl, 12)(pl)  # ZoomToWhole()

    _release(mf)
    _release(mf_el)
    _release(fmap)
    _release(gc)
    _release(pl)
    refresh_mxdoc(mxdoc)
    return hidden, inset_created, petunjuk_created


def style_inset_layers(mxdoc, df_name="Diagram Lokasi", fill_hex="#F3F4F6", outline_hex="#64748B", outline_width=0.55):
    """Applies a clean neutral light-gray + slate-outline fill to all layers inside the inset MapFrame."""
    pl = c_void_p()
    _vcall(mxdoc, 4, POINTER(c_void_p))(mxdoc, byref(pl))
    if not pl.value:
        return 0
    gc = _qi(pl, IID_IGraphicsContainer)
    styled = 0
    target = (df_name or "").lower()
    _vcall(gc, 4)(gc)  # Reset
    while True:
        el = c_void_p()
        hr = _vcall(gc, 3, POINTER(c_void_p))(gc, byref(el))  # Next
        if hr != 0 or not el.value:
            break
        imf = _qi(el, IID_IMapFrame)
        if imf:
            imap = c_void_p()
            _vcall(imf, 11, POINTER(c_void_p))(imf, byref(imap))  # get_Map
            if imap.value:
                bn = c_void_p()
                _vcall(imap, 3, POINTER(c_void_p))(imap, byref(bn))  # IMap.get_Name
                mname = ctypes.wstring_at(bn) if bn.value else u""
                _free_bstr(bn)
                if mname.lower() == target:
                    lcnt = c_long(0)
                    _vcall(imap, 8, POINTER(c_long))(imap, byref(lcnt))
                    for i in range(lcnt.value):
                        lyr = c_void_p()
                        _vcall(imap, 9, c_long, POINTER(c_void_p))(imap, i, byref(lyr))
                        if lyr.value:
                            try:
                                apply_single_symbol(
                                    mxdoc,
                                    lyr,
                                    "Polygon",
                                    color_hex=fill_hex,
                                    outline_color=outline_hex,
                                    outline_width=outline_width,
                                )
                                styled += 1
                            except Exception:
                                pass
                            _release(lyr)
                _release(imap)
            _release(imf)
        _release(el)
    _release(gc)
    _release(pl)
    if styled:
        refresh_mxdoc(mxdoc)
    return styled

