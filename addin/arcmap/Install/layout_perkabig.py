# -*- coding: utf-8 -*-
"""
PerKaBIG No. 3 Tahun 2016 Landscape Layout Builder (A4/A3/A1/A0) for ArcMap 10.1-10.8 (Python 2.7).
Includes verified COM ILegend3 top-anchor + AutoColumns/AutoFit, ScaleBar physical centering,
UTM/DMS/Dual RBI grids, Indonesian 'U' North Arrow, dual insets, and structured non-fatal warnings[].
"""

import ctypes
from ctypes import POINTER, byref, c_double, c_long, c_short, c_void_p

from com_core import (
    CLSID_DMSGridLabel,
    CLSID_Envelope,
    CLSID_FormattedGridLabel,
    CLSID_Graticule,
    CLSID_LocatorRectangle,
    CLSID_Map,
    CLSID_MapFrame,
    CLSID_MeasuredGrid,
    CLSID_NumericFormat,
    CLSID_PngPictureElement,
    CLSID_Polygon,
    CLSID_PolygonElement,
    CLSID_RectangleElement,
    CLSID_SymbolBorder,
    CLSID_TextElement,
    IID_IActiveView,
    IID_IBorder,
    IID_IClassBreaksRenderer,
    IID_IColor,
    IID_IElement,
    IID_IElementProperties,
    IID_IEnvelope,
    IID_IFillShapeElement,
    IID_IFillSymbol,
    IID_IFont,
    IID_IFormattedGridLabel,
    IID_IFrameProperties,
    IID_IGeoFeatureLayer,
    IID_IGeometry,
    IID_IGraphicsContainer,
    IID_IGridLabel,
    IID_ILegend,
    IID_ILegend3,
    IID_ILineSymbol,
    IID_ILocatorRectangle,
    IID_IMap,
    IID_IMapFrame,
    IID_IMapGrid,
    IID_IMapGrids,
    IID_IMeasuredGrid,
    IID_INorthArrow,
    IID_INumberFormat,
    IID_INumericFormat,
    IID_IPictureElement,
    IID_IPointCollection,
    IID_IScaleBar,
    IID_ISymbolBorder,
    IID_ITextElement,
    IID_ITransform2D,
    IID_IUniqueValueRenderer,
    SURROUND_LEGEND_CLSID,
    SURROUND_NORTH_CLSID,
    SURROUND_SCALEBAR_CLSID,
    _bstr,
    _co,
    _free_bstr,
    _make_envelope,
    _make_point,
    _make_uid,
    _qi,
    _release,
    _to_float,
    _vcall,
    refresh_mxdoc,
)
from symbology import (
    apply_single_symbol,
    hide_legend_layers,
    make_fill_symbol,
    make_line_symbol,
    make_rgb,
    make_text_symbol,
)

LAST_WARNINGS = []


def get_last_warnings():
    return list(LAST_WARNINGS)


def _warn(msg):
    LAST_WARNINGS.append(str(msg))


def _add_rect(gc, xmin, ymin, xmax, ymax, fill_rgb=None, outline_rgb=(40, 40, 40), outline_width=0.8, hollow=True, name="geokit_box"):
    re = _co(CLSID_RectangleElement, IID_IFillShapeElement)
    r, g, b = fill_rgb or (255, 255, 255)
    fs = make_fill_symbol(r, g, b, outline_rgb=outline_rgb, outline_width=outline_width, hollow=hollow)
    ifs = _qi(fs, IID_IFillSymbol)
    _vcall(re, 4, c_void_p)(re, ifs)
    el = _qi(re, IID_IElement)
    env = _make_envelope(xmin, ymin, xmax, ymax)
    geom = _qi(env, IID_IGeometry)
    _vcall(el, 4, c_void_p)(el, geom)
    ep = _qi(re, IID_IElementProperties)
    if ep:
        nb = _bstr(name)
        _vcall(ep, 4, c_void_p)(ep, nb)
        _free_bstr(nb)
        _release(ep)
    _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
    _release(geom)
    _release(env)
    _release(el)
    _release(ifs)
    _release(fs)
    _release(re)


def _add_text(gc, x, y, text, size=10.0, rgb=(25, 25, 25), halign=1, valign=1, name="geokit_text", bold=False):
    te = _co(CLSID_TextElement, IID_ITextElement)
    ts = make_text_symbol(size=size, rgb=rgb, halign=halign, valign=valign, halo=False, bold=bold)
    _vcall(te, 4, c_void_p)(te, ts)
    tb = _bstr(text)
    _vcall(te, 6, c_void_p)(te, tb)
    _free_bstr(tb)
    el = _qi(te, IID_IElement)
    pt = _make_point(x, y)
    geom = _qi(pt, IID_IGeometry)
    _vcall(el, 4, c_void_p)(el, geom)
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
        hr = _vcall(pe, 10, c_void_p)(pe, pb)
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
        else:
            _warn("Logo import failed (hr=0x%08X): %s" % (hr & 0xFFFFFFFF, img_path))
        _release(pe)
    except Exception as exc:
        _warn("Logo picture element skipped: %s" % exc)


def _add_polygon_el(gc, pts_xy, fill_rgb=(0, 0, 0), outline_rgb=(0, 0, 0), outline_width=0.7, hollow=False, name="geokit_poly"):
    poly = _co(CLSID_Polygon, IID_IPointCollection)
    for x, y in pts_xy:
        pt = _make_point(x, y)
        _vcall(poly, 5, c_void_p, c_void_p, c_void_p)(poly, pt, None, None)
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
    _add_polygon_el(
        gc,
        [(cx, tip_y), (cx - half_w, base_y), (cx, notch_y), (cx, tip_y)],
        fill_rgb=(0, 0, 0),
        outline_rgb=(0, 0, 0),
        outline_width=0.7 * s,
        hollow=False,
        name="arah_utara_left",
    )
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


def _query_element_bounds(pl, el):
    """Queries the exact rendered page-unit bounding box (xmin, ymin, xmax, ymax) of an IElement."""
    av = _qi(pl, IID_IActiveView)
    if not av:
        return None
    sd = c_void_p()
    try:
        if _vcall(av, 7, POINTER(c_void_p))(av, byref(sd)) != 0 or not sd.value:
            return None
        benv = _co(CLSID_Envelope, IID_IEnvelope)
        try:
            if _vcall(el, 5, c_void_p, c_void_p)(el, sd, benv) != 0:
                return None
            x1, y1, x2, y2 = c_double(0.0), c_double(0.0), c_double(0.0), c_double(0.0)
            _vcall(benv, 58, POINTER(c_double), POINTER(c_double), POINTER(c_double), POINTER(c_double))(
                benv, byref(x1), byref(y1), byref(x2), byref(y2)
            )
            return (x1.value, y1.value, x2.value, y2.value)
        finally:
            _release(benv)
    finally:
        if sd.value:
            _release(sd)
        _release(av)


def _move_element(gc, el, dx, dy):
    """Translates an IElement by (dx, dy) page units via verified ITransform2D.Move."""
    if abs(dx) < 1e-4 and abs(dy) < 1e-4:
        return False
    tr = _qi(el, IID_ITransform2D)
    if not tr:
        return False
    try:
        _vcall(tr, 3, c_double, c_double)(tr, float(dx), float(dy))
        _vcall(gc, 14, c_void_p)(gc, el)
        return True
    finally:
        _release(tr)


def _select_scalebar_division(scale_val, slot_cm):
    """Selects a clean round metric division (m or km) whose 2-division bar fits ~55% of slot_cm."""
    scale_f = _to_float(scale_val, 0.0)
    slot_f = _to_float(slot_cm, 5.0)
    if scale_f <= 0:
        return False, 5.0, min(slot_f * 0.65, 3.0)
    use_m = scale_f <= 35000
    if use_m:
        cands = [10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0]
        unit_cm = 100.0
    else:
        cands = [0.5, 1.0, 2.0, 2.5, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0]
        unit_cm = 100000.0
    target_cm = slot_f * 0.54
    best = None
    for d in cands:
        bar_cm = (2.0 * d * unit_cm) / scale_f
        if bar_cm > slot_f * 0.78:
            continue
        score = abs(bar_cm - target_cm)
        if best is None or score < best[0]:
            best = (score, d, bar_cm)
    if best is None:
        d = cands[0]
        bar_cm = min(slot_f * 0.72, (2.0 * d * unit_cm) / scale_f)
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
        explicit divider lines for Edisi and Catatan, and top-anchored / auto-fit KETERANGAN legend.
    """
    del LAST_WARNINGS[:]
    scale_val = _to_float(scale_val, 0.0)
    span_meters = _to_float(span_meters, 40000.0)

    pl = c_void_p()
    _vcall(mxdoc, 4, POINTER(c_void_p))(mxdoc, byref(pl))
    if not pl.value:
        raise RuntimeError("Could not retrieve PageLayout from MxDocument.")

    paper_key = (paper or "a4").lower()
    pw, ph, s = PAPER_SPECS.get(paper_key, PAPER_SPECS["a4"])
    gmode = (grid_mode or "utm").lower()
    if not add_grid:
        gmode = "none"

    page = c_void_p()
    _vcall(pl, 3, POINTER(c_void_p))(pl, byref(page))
    if page.value:
        _vcall(page, 8, c_long)(page, 8)
        _vcall(page, 6, c_short)(page, 2)
        _vcall(page, 25, c_double, c_double)(page, pw, ph)
        _release(page)

    gc = _qi(pl, IID_IGraphicsContainer)
    fmap = c_void_p()
    _vcall(mxdoc, 9, POINTER(c_void_p))(mxdoc, byref(fmap))

    mf = None
    mf_el = None
    to_delete = []
    _vcall(gc, 4)(gc)
    while True:
        el = c_void_p()
        hr = _vcall(gc, 3, POINTER(c_void_p))(gc, byref(el))
        if hr != 0 or not el.value:
            break
        is_mf = _qi(el, IID_IMapFrame)
        if is_mf:
            frm_map = c_void_p()
            _vcall(is_mf, 11, POINTER(c_void_p))(is_mf, byref(frm_map))
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
        _vcall(gc, 7, c_void_p)(gc, el)
        _release(el)

    _vcall(fmap, 35)(fmap)

    m_out = 0.50 * s
    m_in = 0.72 * s
    gap_p = 0.30 * s
    ox, oy, ow, oh = m_out, m_out, pw - 2.0 * m_out, ph - 2.0 * m_out
    pw_p = 8.00 * s
    px, py, ph_p = pw - m_in - pw_p, m_in, ph - 2.0 * m_in
    nx, ny, nw, nh = m_in, m_in, px - gap_p - m_in, ph - 2.0 * m_in
    m_grid = (1.35 if gmode == "dual" else 1.08) * s
    mx, my, mw, mh = nx + m_grid, ny + m_grid, nw - 2.0 * m_grid, nh - 2.0 * m_grid
    pad = 0.25 * s
    ix = px + pad
    cx = px + pw_p / 2.0

    # 2. Position Main MapFrame + UTM / DMS / Dual RBI Grids
    if mf and mf_el:
        mf_env = _make_envelope(mx, my, mx + mw, my + mh)
        mf_geom = _qi(mf_env, IID_IGeometry)
        _vcall(mf_el, 4, c_void_p)(mf_el, mf_geom)
        _vcall(gc, 14, c_void_p)(gc, mf_el)
        _release(mf_geom)
        _release(mf_env)

        mgrids = _qi(mf, IID_IMapGrids)
        if mgrids:
            _vcall(mgrids, 8)(mgrids)

            if gmode in ("utm", "dual"):
                mg_obj = _co(CLSID_MeasuredGrid, IID_IMeasuredGrid)
                mgrid = _qi(mg_obj, IID_IMapGrid)
                if mgrid:
                    nb = _bstr("Grid UTM")
                    _vcall(mgrid, 4, c_void_p)(mgrid, nb)
                    _free_bstr(nb)
                    _vcall(mgrid, 31, c_void_p)(mgrid, mf)
                    interval = _nice_grid_interval(span_meters, scale=scale_val)
                    _vcall(mg_obj, 14, c_short)(mg_obj, -1)
                    _vcall(mg_obj, 4, c_double)(mg_obj, 0.0)
                    _vcall(mg_obj, 8, c_double)(mg_obj, 0.0)
                    _vcall(mg_obj, 12, c_long)(mg_obj, 9)
                    _vcall(mg_obj, 6, c_double)(mg_obj, interval)
                    _vcall(mg_obj, 10, c_double)(mg_obj, interval)
                    try:
                        fgl = _co(CLSID_FormattedGridLabel, IID_IFormattedGridLabel)
                        nf = _co(CLSID_NumericFormat, IID_INumericFormat)
                        _vcall(nf, 3, c_long)(nf, 0)
                        _vcall(nf, 5, c_long)(nf, 0)
                        _vcall(nf, 11, c_short)(nf, 0)
                        _vcall(nf, 13, c_short)(nf, 0)
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
                        _vcall(igl, 12, c_double)(igl, 3.0 * s)
                        _vcall(igl, 14, c_long, c_short)(igl, 1, -1)
                        _vcall(igl, 14, c_long, c_short)(igl, 2, -1)
                        _vcall(igl, 14, c_long, c_short)(igl, 3, 0)
                        _vcall(igl, 14, c_long, c_short)(igl, 4, 0)
                        _vcall(mgrid, 10, c_void_p)(mgrid, igl)
                        _release(igl)
                        _release(inf)
                        _release(nf)
                        _release(fgl)
                    except Exception as exc:
                        _warn("UTM grid label formatting fallback: %s" % exc)
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

            if gmode in ("dms", "dual"):
                try:
                    gr_obj = _co(CLSID_Graticule, IID_IMeasuredGrid)
                    gr_grid = _qi(gr_obj, IID_IMapGrid)
                    if gr_grid:
                        nb = _bstr("Gratikul Geografi")
                        _vcall(gr_grid, 4, c_void_p)(gr_grid, nb)
                        _free_bstr(nb)
                        _vcall(gr_grid, 31, c_void_p)(gr_grid, mf)
                        dms_int = _nice_dms_interval(scale_val)
                        _vcall(gr_obj, 14, c_short)(gr_obj, -1)
                        _vcall(gr_obj, 4, c_double)(gr_obj, 0.0)
                        _vcall(gr_obj, 8, c_double)(gr_obj, 0.0)
                        _vcall(gr_obj, 6, c_double)(gr_obj, dms_int)
                        _vcall(gr_obj, 10, c_double)(gr_obj, dms_int)
                        dms_lbl = _co(CLSID_DMSGridLabel, IID_IGridLabel)
                        bcol = make_rgb(0, 112, 192)
                        ibcol = _qi(bcol, IID_IColor)
                        _vcall(dms_lbl, 10, c_void_p)(dms_lbl, ibcol)
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
                        _vcall(dms_lbl, 12, c_double)(dms_lbl, (13.8 * s) if gmode == "dual" else (3.2 * s))
                        _vcall(dms_lbl, 14, c_long, c_short)(dms_lbl, 1, -1)
                        _vcall(dms_lbl, 14, c_long, c_short)(dms_lbl, 2, -1)
                        _vcall(dms_lbl, 14, c_long, c_short)(dms_lbl, 3, 0)
                        _vcall(dms_lbl, 14, c_long, c_short)(dms_lbl, 4, 0)
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
                except Exception as exc:
                    _warn("DMS graticule skipped: %s" % exc)
            _release(mgrids)

    # 3. Neatlines
    _add_rect(gc, ox, oy, ox + ow, oy + oh, outline_rgb=(0, 0, 0), outline_width=1.4 * s, hollow=True, name="bingkai_luar")
    _add_rect(gc, nx, ny, nx + nw, ny + nh, outline_rgb=(0, 0, 0), outline_width=0.7 * s, hollow=True, name="bingkai_muka_peta")
    _add_rect(gc, px, py, px + pw_p, py + ph_p, outline_rgb=(0, 0, 0), outline_width=1.0 * s, hollow=True, name="bingkai_panel_informasi")

    top_y = py + ph_p
    cur_y = top_y

    # SECTION 1: JUDUL PETA
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

    # SECTION 2: ARAH UTARA + SKALA
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
                    _vcall(sb, 22, c_long)(sb, 9 if use_m else 10)
                    ul = _bstr("m" if use_m else "km")
                    _vcall(sb, 24, c_void_p)(sb, ul)
                    _free_bstr(ul)
                    _vcall(sb, 16, c_short)(sb, 2)
                    _vcall(sb, 20, c_short)(sb, 2)
                    _vcall(sb, 14, c_double)(sb, div_val)
                    _vcall(sb, 46, c_long)(sb, 0)
                    _vcall(sb, 44, c_double)(sb, 5.4 * s)
                    sb_ts = make_text_symbol(size=7.4 * s, rgb=(0, 0, 0), halign=1, valign=2)
                    _vcall(sb, 40, c_void_p)(sb, sb_ts)
                    _vcall(sb, 30, c_void_p)(sb, sb_ts)
                    _release(sb_ts)
                    _release(sb)
                _release(sb_ms)
            el = _qi(sb_sf, IID_IElement)
            env = _make_envelope(sb_env_x1, y_sc_bot + 0.24 * s, sb_env_x2, y_sc_bot + 1.08 * s)
            geom = _qi(env, IID_IGeometry)
            _vcall(el, 4, c_void_p)(el, geom)
            _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
            try:
                sb_b = _query_element_bounds(pl, el)
                if sb_b and (sb_b[2] > sb_b[0]):
                    actual_cx = (sb_b[0] + sb_b[2]) / 2.0
                    dx = sb_cx - actual_cx
                    if sb_b[0] + dx >= sb_x - 0.05 * s and sb_b[2] + dx <= sb_right + 0.05 * s:
                        _move_element(gc, el, dx, 0.0)
            except Exception as exc:
                _warn("ScaleBar centering fallback: %s" % exc)
            _release(geom)
            _release(env)
            _release(el)
            _release(sb_sf)
        _release(uid_sb)
    cur_y = y_sc_bot
    _add_hline(gc, px, px + pw_p, cur_y, width=0.7 * s, name="garis_skala")

    # SECTION 3: INSETS
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
            except Exception as exc:
                _warn("Petunjuk Letak Peta inset failed: %s" % exc)
            try:
                inset_created = _create_inset_mapframe(gc, mf, "Diagram Lokasi", x2_l, y_ins_bot + 0.14 * s, x2_r, cur_y - 0.46 * s, border_rgb=(0, 0, 0))
            except Exception as exc:
                _warn("Diagram Lokasi inset failed: %s" % exc)
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
            except Exception as exc:
                _warn("Inset MapFrame failed: %s" % exc)
        cur_y = y_ins_bot
        _add_hline(gc, px, px + pw_p, cur_y, width=0.7 * s, name="garis_inset")

    # SECTION 4: PROYEKSI
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

    # SECTION 5: INSTANSI + EDISI
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

    # SECTION 7 (BOTTOM): SUMBER DATA + CATATAN
    h_foot = 2.65 * s
    y_foot_top = py + h_foot
    y_note_sep = py + 1.02 * s
    _add_hline(gc, px, px + pw_p, y_foot_top, width=0.7 * s, name="garis_legenda")
    _add_text(gc, ix, y_foot_top - 0.14 * s, "SUMBER DATA DAN RIWAYAT PETA:", size=6.5 * s, rgb=(0, 0, 0), halign=0, valign=0, name="lbl_sumber_hdr", bold=True)
    _add_text(gc, ix, y_foot_top - 0.48 * s, _wrap_text(source or "1. Pengolahan Data Spasial GeoKit.", max_chars=42), size=6.1 * s, rgb=(20, 25, 30), halign=0, valign=0, name="lbl_sumber_riwayat")
    _add_hline(gc, px, px + pw_p, y_note_sep, width=0.55 * s, name="garis_catatan")
    note_txt = note or "CATATAN: Batas wilayah administrasi pada peta ini bukan referensi resmi penegasan batas."
    _add_text(gc, ix, y_note_sep - 0.12 * s, _wrap_text(note_txt, max_chars=48), size=5.4 * s, rgb=(40, 45, 50), halign=0, valign=0, name="lbl_catatan")

    # SECTION 6: KETERANGAN (2-case top-anchor vs FixedFrame+AutoColumns+AutoFit)
    hidden = []
    if mf:
        uid_leg = _make_uid(SURROUND_LEGEND_CLSID)
        leg_sf = c_void_p()
        if _vcall(mf, 21, c_void_p, c_void_p, POINTER(c_void_p))(mf, uid_leg, None, byref(leg_sf)) == 0 and leg_sf.value:
            ms = c_void_p()
            _vcall(leg_sf, 11, POINTER(c_void_p))(leg_sf, byref(ms))
            leg3 = None
            if ms.value:
                leg = _qi(ms, IID_ILegend)
                if leg:
                    tb = _bstr("KETERANGAN")
                    _vcall(leg, 16, c_void_p)(leg, tb)
                    _free_bstr(tb)
                    _vcall(leg, 18, c_short)(leg, 0)  # AutoAdd = False

                    lfmt = c_void_p()
                    if _vcall(leg, 13, POINTER(c_void_p))(leg, byref(lfmt)) == 0 and lfmt.value:
                        try:
                            t_sym = make_text_symbol(size=8.6 * s, rgb=(0, 0, 0), halign=0, valign=0, bold=True)
                            _vcall(lfmt, 6, c_void_p)(lfmt, t_sym)
                            _release(t_sym)
                            _vcall(lfmt, 10, c_double)(lfmt, 3.5 * s)
                            _vcall(lfmt, 12, c_double)(lfmt, 4.0 * s)
                            _vcall(lfmt, 16, c_double)(lfmt, 2.5 * s)
                            _vcall(lfmt, 24, c_double)(lfmt, 2.5 * s)
                            _vcall(lfmt, 28, c_double)(lfmt, 15.0 * s)
                            _vcall(lfmt, 30, c_double)(lfmt, 8.5 * s)
                            _vcall(leg, 14, c_void_p)(leg, lfmt)
                        except Exception as exc:
                            _warn("Legend format tuning fallback: %s" % exc)
                        _release(lfmt)

                    icnt = c_long(0)
                    _vcall(leg, 25, POINTER(c_long))(leg, byref(icnt))
                    for li in range(icnt.value):
                        item = c_void_p()
                        _vcall(leg, 26, c_long, POINTER(c_void_p))(leg, li, byref(item))
                        if item.value:
                            _vcall(item, 20, c_short)(item, 0)
                            try:
                                ln_sym = make_text_symbol(size=7.4 * s, rgb=(0, 0, 0), halign=0, valign=1, bold=True)
                                _vcall(item, 16, c_void_p)(item, ln_sym)
                                _release(ln_sym)
                                lcf = c_void_p()
                                if _vcall(item, 23, POINTER(c_void_p))(item, byref(lcf)) == 0 and lcf.value:
                                    lbl_sym = make_text_symbol(size=7.0 * s, rgb=(20, 20, 20), halign=0, valign=1, bold=False)
                                    _vcall(lcf, 4, c_void_p)(lcf, lbl_sym)
                                    _vcall(lcf, 8, c_double)(lcf, 15.0 * s)
                                    _vcall(lcf, 10, c_double)(lcf, 8.5 * s)
                                    _vcall(item, 24, c_void_p)(item, lcf)
                                    _release(lbl_sym)
                                    _release(lcf)
                            except Exception as exc:
                                _warn("Legend item typography fallback: %s" % exc)
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

                leg3 = _qi(ms, IID_ILegend3)
                if leg3:
                    try:
                        _vcall(leg3, 36, c_short)(leg3, 0)
                    except Exception:
                        pass
                _release(ms)

            el = _qi(leg_sf, IID_IElement)
            leg_top_limit = cur_y - 0.18 * s
            leg_bot_limit = y_foot_top + 0.16 * s
            avail_h = max(1.0 * s, leg_top_limit - leg_bot_limit)
            env = _make_envelope(ix, leg_bot_limit, px + pw_p - pad, leg_top_limit)
            geom = _qi(env, IID_IGeometry)
            _vcall(el, 4, c_void_p)(el, geom)
            _vcall(gc, 5, c_void_p, c_long)(gc, el, 0)
            _release(geom)
            _release(env)
            if hide_legend_names:
                hidden = hide_legend_layers(mxdoc, hide_legend_names)
            try:
                _vcall(gc, 14, c_void_p)(gc, el)
                lb = _query_element_bounds(pl, el)
                if lb and (lb[3] > lb[1]):
                    actual_h = lb[3] - lb[1]
                    if actual_h <= avail_h:
                        dy = leg_top_limit - lb[3]
                        _move_element(gc, el, 0.0, dy)
                    else:
                        if leg3:
                            _vcall(leg3, 36, c_short)(leg3, -1)
                            _vcall(leg3, 38, c_short)(leg3, -1)
                            _vcall(leg3, 40, c_short)(leg3, -1)
                            _vcall(leg3, 42, c_double)(leg3, 5.0 * s)
                        env2 = _make_envelope(ix, leg_bot_limit, px + pw_p - pad, leg_top_limit)
                        geom2 = _qi(env2, IID_IGeometry)
                        _vcall(el, 4, c_void_p)(el, geom2)
                        _vcall(gc, 14, c_void_p)(gc, el)
                        _release(geom2)
                        _release(env2)
            except Exception as exc:
                _warn("Legend bounds positioning fallback: %s" % exc)
                if leg3:
                    try:
                        _vcall(leg3, 36, c_short)(leg3, -1)
                        _vcall(leg3, 38, c_short)(leg3, -1)
                        _vcall(leg3, 40, c_short)(leg3, -1)
                    except Exception:
                        pass
            if leg3:
                _release(leg3)
            _release(el)
            _release(leg_sf)
        _release(uid_leg)
    elif hide_legend_names:
        hidden = hide_legend_layers(mxdoc, hide_legend_names)

    _vcall(pl, 12)(pl)

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
    _vcall(gc, 4)(gc)
    while True:
        el = c_void_p()
        hr = _vcall(gc, 3, POINTER(c_void_p))(gc, byref(el))
        if hr != 0 or not el.value:
            break
        imf = _qi(el, IID_IMapFrame)
        if imf:
            imap = c_void_p()
            _vcall(imf, 11, POINTER(c_void_p))(imf, byref(imap))
            if imap.value:
                bn = c_void_p()
                _vcall(imap, 3, POINTER(c_void_p))(imap, byref(bn))
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
                            except Exception as exc:
                                _warn("Inset layer styling skipped: %s" % exc)
                            _release(lyr)
                _release(imap)
            _release(imf)
        _release(el)
    _release(gc)
    _release(pl)
    if styled:
        refresh_mxdoc(mxdoc)
    return styled
