# -*- coding: utf-8 -*-
"""
ArcObjects COM Symbology, Palettes (including KLHK 23-class), Fisher-Jenks Natural Breaks,
Legend Filtering, and Label Halo helpers for ArcMap 10.1-10.8 (Python 2.7).
"""

import ctypes
from ctypes import POINTER, byref, c_double, c_long, c_short, c_void_p

from com_core import (
    CLSID_AlgorithmicColorRamp,
    CLSID_CartographicLineSymbol,
    CLSID_ClassBreaksRenderer,
    CLSID_MultiPartColorRamp,
    CLSID_RasterStretchColorRampRenderer,
    CLSID_RgbColor,
    CLSID_SimpleFillSymbol,
    CLSID_SimpleLineSymbol,
    CLSID_SimpleMarkerSymbol,
    CLSID_SimpleRenderer,
    CLSID_Template,
    CLSID_TextSymbol,
    CLSID_UniqueValueRenderer,
    IID_IAlgorithmicColorRamp,
    IID_ICartographicLineSymbol,
    IID_IClassBreaksRenderer,
    IID_IColor,
    IID_IColorRamp,
    IID_IFeatureRenderer,
    IID_IFillSymbol,
    IID_IFont,
    IID_IGeoFeatureLayer,
    IID_ILabelEngineLayerProperties,
    IID_ILegend,
    IID_ILineProperties,
    IID_ILineSymbol,
    IID_IMask,
    IID_IMultiPartColorRamp,
    IID_IRasterLayer,
    IID_IRasterRenderer,
    IID_IRasterStretchColorRampRenderer,
    IID_IRgbColor,
    IID_ISimpleFillSymbol,
    IID_ISimpleLineSymbol,
    IID_ISimpleMarkerSymbol,
    IID_ISimpleRenderer,
    IID_ISymbol,
    IID_ITemplate,
    IID_ITextSymbol,
    IID_IUniqueValueRenderer,
    _bstr,
    _co,
    _free_bstr,
    _qi,
    _release,
    _to_float,
    _vcall,
    refresh_mxdoc,
    set_layer_transparency,
)

# Modern cartographic palettes (ColorBrewer / Carto / Viridis / Hypsometric / KLHK)
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
    vals = [_to_float(v) for v in sorted_vals]
    if not vals:
        return []
    if len(vals) <= n_classes:
        return list(vals)
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
    breaks[k - 1] = _to_float(sorted_vals[-1])
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

DASH_PATTERNS = {
    "dash": [(6.0, 3.0)],
    "dot": [(1.5, 2.5)],
    "dashdot": [(8.0, 3.0), (1.5, 3.0)],
    "dashdotdot": [(8.0, 2.5), (1.5, 2.0), (1.5, 2.5)],
    "admin-kab": [(10.0, 3.0), (1.5, 2.5), (1.5, 3.0)],
    "admin-kec": [(8.0, 3.0), (1.5, 3.0)],
    "admin-desa": [(4.5, 2.5)],
}


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
    st = (style or "solid").lower()
    if (not null_color) and st in DASH_PATTERNS:
        sym = _co(CLSID_CartographicLineSymbol, IID_ILineSymbol)
        col = make_rgb(r, g, b, null_color=False)
        icol = _qi(col, IID_IColor)
        _vcall(sym, 4, c_void_p)(sym, icol)
        _vcall(sym, 6, c_double)(sym, _to_float(width, 1.2))
        cls_iface = _qi(sym, IID_ICartographicLineSymbol)
        if cls_iface:
            _vcall(cls_iface, 8, c_long)(cls_iface, 2)   # Cap = Round (2)
            _vcall(cls_iface, 10, c_long)(cls_iface, 2)  # Join = Round (2)
            _release(cls_iface)
        lprop = _qi(sym, IID_ILineProperties)
        if lprop:
            tpl = _co(CLSID_Template, IID_ITemplate)
            _vcall(tpl, 11, c_double)(tpl, 1.0)
            for mark, gap in DASH_PATTERNS[st]:
                _vcall(tpl, 6, c_double, c_double)(tpl, float(mark), float(gap))
            _vcall(lprop, 14, c_void_p)(lprop, tpl)
            _release(tpl)
            _release(lprop)
        _release(icol)
        _release(col)
        return sym

    sym = _co(CLSID_SimpleLineSymbol, IID_ISimpleLineSymbol)
    col = make_rgb(r, g, b, null_color=null_color)
    icol = _qi(col, IID_IColor)
    _vcall(sym, 4, c_void_p)(sym, icol)
    _vcall(sym, 6, c_double)(sym, _to_float(width, 1.2))
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
    ow = _to_float(outline_width, 0.0)
    if ow > 0.0:
        or_, og, ob = outline_rgb or (70, 70, 70)
        outline = make_line_symbol(or_, og, ob, width=ow, style=outline_style)
    else:
        outline = make_line_symbol(0, 0, 0, width=0.0, style="null", null_color=True)
    iline = _qi(outline, IID_ILineSymbol)
    _vcall(sym, 6, c_void_p)(sym, iline)
    _vcall(sym, 8, c_long)(sym, 1 if hollow else 0)
    _release(iline)
    _release(outline)
    _release(icol)
    _release(col)
    return sym


def make_marker_symbol(r, g, b, size=7.0, style="circle", outline_rgb=(255, 255, 255), outline_width=1.0):
    sym = _co(CLSID_SimpleMarkerSymbol, IID_ISimpleMarkerSymbol)
    col = make_rgb(r, g, b)
    icol = _qi(col, IID_IColor)
    _vcall(sym, 4, c_double)(sym, _to_float(size, 7.0))
    _vcall(sym, 6, c_void_p)(sym, icol)
    _vcall(sym, 14, c_long)(sym, MARKER_STYLES.get((style or "circle").lower(), 0))
    ow = _to_float(outline_width, 0.0)
    if ow > 0.0:
        or_, og, ob = outline_rgb or (255, 255, 255)
        ocol = make_rgb(or_, og, ob)
        iocol = _qi(ocol, IID_IColor)
        _vcall(sym, 16, c_short)(sym, -1)
        _vcall(sym, 18, c_double)(sym, ow)
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
        _vcall(seg, 11, c_void_p)(seg, ic1)
        _vcall(seg, 13, c_void_p)(seg, ic2)
        _vcall(seg, 14, c_long)(seg, 1)
        iramp = _qi(seg, IID_IColorRamp)
        _vcall(mp, 12, c_void_p)(mp, iramp)
        _release(iramp)
        _release(ic1)
        _release(ic2)
        _release(c1)
        _release(c2)
        _release(seg)
    _vcall(mp, 5, c_long)(mp, 256)
    ok = c_short(0)
    _vcall(mp, 9, POINTER(c_short))(mp, byref(ok))
    return mp


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
                _vcall(ifnt, 8, c_short)(ifnt, -1)
                _release(ifnt)
            _vcall(ts, 4, c_void_p)(ts, fnt)
            _release(fnt)
    _vcall(ts, 6, c_double)(ts, _to_float(size, 10.0))
    _vcall(ts, 8, c_void_p)(ts, icol)
    _vcall(ts, 12, c_long)(ts, int(halign))
    _vcall(ts, 10, c_long)(ts, int(valign))
    if halo:
        mask = _qi(ts, IID_IMask)
        if mask:
            wfill = make_fill_symbol(255, 255, 255, outline_width=0.0)
            ifill = _qi(wfill, IID_IFillSymbol)
            _vcall(mask, 4, c_long)(mask, 1)
            _vcall(mask, 6, c_double)(mask, _to_float(halo_size, 1.5))
            _vcall(mask, 8, c_void_p)(mask, ifill)
            _release(ifill)
            _release(wfill)
            _release(mask)
    _release(icol)
    _release(col)
    return ts


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
    _vcall(uvr, 9, c_long)(uvr, 1)
    fb = _bstr(field_name)
    _vcall(uvr, 4, c_long, c_void_p)(uvr, 0, fb)
    _free_bstr(fb)
    _vcall(uvr, 15, c_short)(uvr, 0)

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
    _vcall(gfl, 33, c_void_p)(gfl, frend)
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
    _vcall(cbr, 6, c_void_p)(cbr, fb)
    _free_bstr(fb)
    _vcall(cbr, 4, c_long)(cbr, n)
    _vcall(cbr, 18, c_double)(cbr, _to_float(min_val))

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
        _vcall(cbr, 16, c_long, c_double)(cbr, idx, _to_float(break_highs[idx]))
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
        rgb = parse_hex_color(color_hex) if color_hex else (2, 136, 209)
        width = width if width else 1.4
    elif p == "road":
        rgb = parse_hex_color(color_hex) if color_hex else (216, 67, 21)
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
    _vcall(sr, 4, c_void_p)(sr, isym)
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
    _vcall(rr, 4, c_void_p)(rr, raster)
    _vcall(sr, 8, c_void_p)(sr, iramp)
    _vcall(rr, 8)(rr)
    _vcall(rl, 27, c_void_p)(rl, rr)
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
                            _vcall(leg, 29, c_long)(leg, idx)
                            removed.append(nm)
                        _release(item_lyr)
                    _release(item)
            _release(leg)
        _release(ms)
    _release(fmap)
    refresh_mxdoc(mxdoc)
    return removed


def set_label_halo(mxdoc, lyr, font_size=8.5, color_rgb=(33, 33, 33), halo_size=1.5):
    """Applies a clean white halo + readable font size to a feature layer's label class."""
    gfl = _qi(lyr, IID_IGeoFeatureLayer)
    if not gfl:
        return False
    ap_col = c_void_p()
    _vcall(gfl, 40, POINTER(c_void_p))(gfl, byref(ap_col))
    if not ap_col.value:
        _release(gfl)
        return False
    cnt = c_long(0)
    _vcall(ap_col, 4, POINTER(c_long))(ap_col, byref(cnt))
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
                _vcall(lelp, 3, c_void_p)(lelp, ts)
                _release(ts)
                _release(lelp)
                applied = True
            _release(item)
    _release(ap_col)
    _release(gfl)
    refresh_mxdoc(mxdoc)
    return applied
