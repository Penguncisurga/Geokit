# -*- coding: utf-8 -*-
"""
GeoKit ArcMap 10.x Live-UI Python Add-In (Python 2.7).
Runs a local TCP JSON bridge on 127.0.0.1:9877 and dispatches all ArcPy /
MapDocument("CURRENT") calls safely onto ArcMap's main UI thread via user32.SetTimer.
Supports full ArcToolbox discovery/execution, live cartography, and instant UI repaint.
"""

import ctypes
import json
import os
import Queue
import socket
import sys
import tempfile
import threading
import traceback

import arcpy
import pythonaddins

_ADDIN_DIR = os.path.dirname(os.path.abspath(__file__))
if _ADDIN_DIR and _ADDIN_DIR not in sys.path:
    sys.path.insert(0, _ADDIN_DIR)

_AO_ERR = None
try:
    import arcobjects as _ao
except Exception as _exc:
    _ao = None
    _AO_ERR = str(_exc)

HOST = "127.0.0.1"
PORT = int(os.environ.get("GEOKIT_LIVE_PORT", "9877"))
LOG_PATH = os.path.join(tempfile.gettempdir(), "geokit_arcmap_bridge.log")

_CMD_QUEUE = Queue.Queue()
_SERVER_SOCKET = None
_SERVER_THREAD = None
_TIMER_ID = None
_TIMER_PROC_REF = None
_RUNNING = False


def _log(msg):
    try:
        with open(LOG_PATH, "a") as fh:
            fh.write("%s\n" % msg)
    except Exception:
        pass


def _force_refresh():
    try:
        arcpy.RefreshTOC()
    except Exception:
        pass
    try:
        arcpy.RefreshActiveView()
    except Exception:
        pass
    # Trigger immediate Win32 repaint on active ArcMap window so layers appear without pressing F5
    try:
        hwnd = ctypes.windll.user32.GetActiveWindow()
        if not hwnd:
            hwnd = ctypes.windll.user32.GetForegroundWindow()
        if hwnd:
            # RDW_INVALIDATE (0x1) | RDW_UPDATENOW (0x100) | RDW_ALLCHILDREN (0x80)
            ctypes.windll.user32.RedrawWindow(hwnd, None, None, 0x0181)
    except Exception:
        pass


def _auto_checkout_extension(tool_name):
    lower = tool_name.lower()
    ext_map = [
        ("_sa", "Spatial"),
        ("_3d", "3D"),
        ("_na", "Network"),
        ("_ga", "GeoStats"),
        ("_ta", "Tracking"),
    ]
    for suffix, ext_name in ext_map:
        if lower.endswith(suffix):
            try:
                if arcpy.CheckExtension(ext_name) == "Available":
                    arcpy.CheckOutExtension(ext_name)
            except Exception:
                pass
            return


def _find_layer(mxd, df, name):
    target = name.lower()
    for lyr in arcpy.mapping.ListLayers(mxd, "", df):
        if lyr.name.lower() == target or lyr.longName.lower() == target:
            return lyr
    for lyr in arcpy.mapping.ListLayers(mxd, "", df):
        if target in lyr.name.lower():
            return lyr
    return None


def _handle_command_on_ui_thread(req):
    action = req.get("action", "")
    mxd = arcpy.mapping.MapDocument("CURRENT")
    df = mxd.activeDataFrame

    if action == "ping":
        dfs = [d.name for d in arcpy.mapping.ListDataFrames(mxd)]
        lyrs = arcpy.mapping.ListLayers(mxd, "", df)
        return {
            "ok": True,
            "app": "ArcMap",
            "version": arcpy.GetInstallInfo().get("Version", "10.x"),
            "mxd": mxd.filePath or "(Unsaved Map)",
            "active_data_frame": df.name,
            "data_frames": dfs,
            "layer_count": len(lyrs),
            "port": PORT,
        }

    if action == "list_layers":
        items = []
        for lyr in arcpy.mapping.ListLayers(mxd, "", df):
            ds = ""
            try:
                if lyr.supports("DATASOURCE"):
                    ds = lyr.dataSource
            except Exception:
                ds = ""
            items.append({
                "name": lyr.name,
                "long_name": lyr.longName,
                "visible": bool(lyr.visible),
                "is_broken": bool(lyr.isBroken),
                "is_feature": bool(lyr.isFeatureLayer),
                "is_raster": bool(lyr.isRasterLayer),
                "data_source": ds,
            })
        return {"ok": True, "data_frame": df.name, "layers": items}

    if action == "layer_info":
        name = req.get("layer", "")
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        info = {
            "ok": True,
            "name": lyr.name,
            "long_name": lyr.longName,
            "visible": bool(lyr.visible),
            "is_feature": bool(lyr.isFeatureLayer),
            "is_raster": bool(lyr.isRasterLayer),
            "is_broken": bool(lyr.isBroken),
            "definition_query": lyr.definitionQuery if lyr.supports("DEFINITIONQUERY") else "",
            "show_labels": bool(lyr.showLabels) if lyr.supports("SHOWLABELS") else False,
        }
        try:
            if lyr.supports("DATASOURCE"):
                info["data_source"] = lyr.dataSource
            desc = arcpy.Describe(lyr)
            sr = getattr(desc, "spatialReference", None)
            info["crs"] = sr.name if sr else "Unknown"
            info["epsg"] = sr.factoryCode if (sr and getattr(sr, "factoryCode", 0)) else None
            if lyr.isFeatureLayer:
                info["shape_type"] = getattr(desc, "shapeType", "Unknown")
                info["feature_count"] = int(arcpy.GetCount_management(lyr).getOutput(0))
                fields = [f.name for f in arcpy.ListFields(lyr) if f.type != "Geometry"]
                info["fields"] = fields
                sample = []
                if fields:
                    with arcpy.da.SearchCursor(lyr, fields[:10]) as cur:
                        for idx, row in enumerate(cur):
                            if idx >= 5:
                                break
                            sample.append(dict(zip(fields[:10], [unicode(v) if v is not None else None for v in row])))
                info["sample_rows"] = sample
        except Exception as exc:
            info["describe_warning"] = str(exc)
        return info

    if action == "add_layer":
        path = req.get("path", "")
        if not path or not os.path.exists(path):
            return {"ok": False, "error": "Dataset path not found: %s" % path}
        ext = os.path.splitext(path)[1].lower()
        if ext in (".tif", ".tiff", ".img", ".jp2", ".ecw", ".asc"):
            lyr_name = os.path.splitext(os.path.basename(path))[0]
            prev_add = arcpy.env.addOutputsToMap
            arcpy.env.addOutputsToMap = False
            try:
                res = arcpy.MakeRasterLayer_management(path, lyr_name)
                lyr = res.getOutput(0)
            finally:
                arcpy.env.addOutputsToMap = prev_add
        else:
            lyr = arcpy.mapping.Layer(path)
        pos = (req.get("position") or "TOP").upper()
        arcpy.mapping.AddLayer(df, lyr, pos)
        _force_refresh()
        return {"ok": True, "added": lyr.name, "path": path}

    if action == "remove_layer":
        name = req.get("layer", "")
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        removed_name = lyr.name
        arcpy.mapping.RemoveLayer(df, lyr)
        _force_refresh()
        return {"ok": True, "removed": removed_name}

    if action == "set_visibility":
        name = req.get("layer", "")
        visible = bool(req.get("visible", True))
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        lyr.visible = visible
        _force_refresh()
        return {"ok": True, "layer": lyr.name, "visible": lyr.visible}

    if action == "set_definition_query":
        name = req.get("layer", "")
        where = req.get("where", "")
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        if not lyr.supports("DEFINITIONQUERY"):
            return {"ok": False, "error": "Layer does not support definition queries: %s" % lyr.name}
        lyr.definitionQuery = where
        _force_refresh()
        return {"ok": True, "layer": lyr.name, "definition_query": where}

    if action == "apply_symbology":
        name = req.get("layer", "")
        lyr_file = req.get("lyr_file", "")
        if not lyr_file or not os.path.exists(lyr_file):
            return {"ok": False, "error": "Symbology .lyr file not found: %s" % lyr_file}
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        src_lyr = arcpy.mapping.Layer(lyr_file)
        arcpy.mapping.UpdateLayer(df, lyr, src_lyr, True)
        _force_refresh()
        return {"ok": True, "layer": lyr.name, "applied_symbology": lyr_file}

    if action == "style_layer":
        name = req.get("layer", "")
        field = req.get("field") or ""
        mode = (req.get("mode") or "auto").lower()
        preset = (req.get("preset") or "hazard").lower()
        custom_colors = req.get("colors")
        single_color = req.get("color")
        classes_raw = req.get("classes")
        classes_count = int(req.get("classes_count") or 5)
        break_method = (req.get("break_method") or "quantile").lower()
        outline_color = req.get("outline_color")
        outline_width = float(req.get("outline_width") or 0.0)
        outline_style = (req.get("outline_style") or "solid").lower()
        line_width = float(req.get("width") or 1.4)
        marker_size = float(req.get("size") or 7.0)
        marker_style = (req.get("marker") or "circle").lower()
        hollow = bool(req.get("hollow", False))
        transparency = req.get("transparency")

        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}

        if _ao is None:
            return {"ok": False, "error": "ArcObjects COM module failed to load inside Add-In: %s" % _AO_ERR}

        mxdoc = _ao.get_current_mxdoc()
        com_lyr, com_name = _ao.find_com_layer(mxdoc, lyr.name)
        if not com_lyr:
            _ao._release(mxdoc)
            return {"ok": False, "error": "COM layer not found in FocusMap: %s" % lyr.name}

        try:
            # 1. Raster styling via IRasterStretchColorRampRenderer + MultiPartColorRamp
            if lyr.isRasterLayer or mode == "raster":
                raster_preset = preset if preset not in ("stream", "road", "boundary", "point") else "terrain"
                stops = _ao.apply_raster_stretch(
                    mxdoc, com_lyr, preset=raster_preset,
                    custom_colors=custom_colors, transparency=transparency,
                )
                _force_refresh()
                return {
                    "ok": True,
                    "engine": "ArcObjects COM",
                    "layer": lyr.name,
                    "mode": "raster",
                    "preset": raster_preset,
                    "color_stops": ["#%02X%02X%02X" % c for c in stops],
                    "transparency": transparency,
                }

            if not lyr.isFeatureLayer:
                return {"ok": False, "error": "Layer '%s' is neither a feature layer nor a raster layer." % lyr.name}

            try:
                shape_type = arcpy.Describe(lyr).shapeType
            except Exception:
                shape_type = "Polygon"

            # 2. Single-symbol mode (streams, roads, hollow boundaries, points, or when no field specified)
            if mode == "single" or (not field) or preset in ("stream", "road", "boundary", "point"):
                cols = _ao.apply_single_symbol(
                    mxdoc, com_lyr, shape_type,
                    color_hex=single_color, preset=preset,
                    outline_color=outline_color, outline_width=outline_width,
                    outline_style=outline_style, width=line_width,
                    marker_size=marker_size, marker_style=marker_style,
                    hollow=hollow, transparency=transparency,
                )
                _force_refresh()
                return {
                    "ok": True,
                    "engine": "ArcObjects COM",
                    "layer": lyr.name,
                    "mode": "single",
                    "preset": preset,
                    "shape_type": shape_type,
                    "colors": ["#%02X%02X%02X" % c for c in cols],
                    "transparency": transparency,
                }

            existing = dict((f.name.lower(), f.name) for f in arcpy.ListFields(lyr))
            actual_field = existing.get(field.lower())
            if not actual_field:
                return {"ok": False, "error": "Field '%s' not found on layer '%s'" % (field, lyr.name)}

            # 3. Graduated ClassBreaksRenderer (continuous numeric data, N classes)
            if mode == "graduated":
                nums = []
                with arcpy.da.SearchCursor(lyr, [actual_field]) as cur:
                    for row in cur:
                        if row[0] is not None:
                            try:
                                nums.append(float(row[0]))
                            except (ValueError, TypeError):
                                pass
                if not nums:
                    return {"ok": False, "error": "No numeric values found in field '%s'" % actual_field}
                nums.sort()
                vmin, vmax = nums[0], nums[-1]
                n_cls = max(2, min(classes_count, len(set(nums))))
                if break_method == "equal" or len(nums) <= n_cls:
                    step = (vmax - vmin) / float(n_cls) if vmax > vmin else 1.0
                    highs = [vmin + step * (i + 1) for i in range(n_cls)]
                    highs[-1] = vmax
                elif break_method in ("jenks", "natural", "natural-breaks"):
                    highs = _ao.compute_jenks_breaks(nums, n_cls)
                    highs[-1] = vmax
                else:
                    highs = []
                    for i in range(1, n_cls + 1):
                        q_idx = min(len(nums) - 1, int(round((float(i) / float(n_cls)) * (len(nums) - 1))))
                        highs.append(nums[q_idx])
                    highs[-1] = vmax
                if classes_raw:
                    lbls = [c.strip() for c in classes_raw.split(",") if c.strip()]
                else:
                    lbls = []
                    prev = vmin
                    for hi in highs:
                        lbls.append("%.2f - %.2f" % (prev, hi))
                        prev = hi
                while len(lbls) < len(highs):
                    lbls.append("Class %d" % (len(lbls) + 1))

                cols = _ao.apply_class_breaks(
                    mxdoc, com_lyr, shape_type, actual_field, vmin, highs, lbls[:len(highs)],
                    preset=preset, custom_colors=custom_colors,
                    outline_color=outline_color, outline_width=outline_width,
                    outline_style=outline_style, width=line_width,
                    marker_size=marker_size, marker_style=marker_style,
                    transparency=transparency,
                )
                _force_refresh()
                return {
                    "ok": True,
                    "engine": "ArcObjects COM",
                    "layer": lyr.name,
                    "mode": "graduated",
                    "field": actual_field,
                    "break_method": break_method,
                    "breaks": highs,
                    "labels": lbls[:len(highs)],
                    "colors": ["#%02X%02X%02X" % c for c in cols],
                    "transparency": transparency,
                }

            # 4. Unlimited-class UniqueValueRenderer (exact N values directly on field, zero junk slots!)
            if classes_raw:
                ordered = [c.strip() for c in classes_raw.split(",") if c.strip()]
            else:
                seen = []
                with arcpy.da.SearchCursor(lyr, [actual_field]) as cur:
                    for row in cur:
                        v = row[0]
                        if v is not None:
                            s = unicode(v).strip()
                            if s and s not in seen:
                                seen.append(s)
                severity_rank = {
                    "sangat rendah": 1, "very low": 1, "aman": 1, "tidak rawan": 1, "sangat sesuai": 1,
                    "rendah": 2, "low": 2, "agak rendah": 2, "cukup sesuai": 2,
                    "sedang": 3, "medium": 3, "moderate": 3, "menengah": 3, "sesuai marginal": 3,
                    "tinggi": 4, "high": 4, "agak tinggi": 4, "tidak sesuai": 4,
                    "sangat tinggi": 5, "very high": 5, "ekstrem": 5, "sangat rawan": 5,
                }
                def _sort_key(val):
                    low = val.lower()
                    if low in severity_rank:
                        return (0, severity_rank[low], low)
                    try:
                        return (1, float(val), low)
                    except ValueError:
                        return (2, 0, low)
                ordered = sorted(seen, key=_sort_key)

            if not ordered:
                return {"ok": False, "error": "No non-empty values found in field '%s'" % actual_field}

            cols = _ao.apply_unique_values(
                mxdoc, com_lyr, shape_type, actual_field, ordered,
                preset=preset, custom_colors=custom_colors,
                outline_color=outline_color, outline_width=outline_width,
                outline_style=outline_style, width=line_width,
                marker_size=marker_size, marker_style=marker_style,
                transparency=transparency,
            )
            _force_refresh()
            return {
                "ok": True,
                "engine": "ArcObjects COM",
                "layer": lyr.name,
                "mode": "unique",
                "preset": preset,
                "field": actual_field,
                "class_count": len(ordered),
                "classes": ordered,
                "colors": ["#%02X%02X%02X" % c for c in cols],
                "transparency": transparency,
            }
        finally:
            _ao._release(com_lyr)
            _ao._release(mxdoc)

    if action == "order_layer":
        name = req.get("layer", "")
        position = (req.get("position") or "top").lower()
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        all_lyrs = [l for l in arcpy.mapping.ListLayers(mxd, "", df) if not l.isGroupLayer]
        if len(all_lyrs) >= 2:
            if position == "bottom":
                ref = all_lyrs[-1]
                if ref.longName != lyr.longName:
                    arcpy.mapping.MoveLayer(df, ref, lyr, "AFTER")
            else:
                ref = all_lyrs[0]
                if ref.longName != lyr.longName:
                    arcpy.mapping.MoveLayer(df, ref, lyr, "BEFORE")
        _force_refresh()
        return {"ok": True, "layer": lyr.name, "position": position}

    if action == "layout_update":
        view = (req.get("view") or "").lower()
        template = (req.get("template") or "").lower()
        title = req.get("title")
        subtitle = req.get("subtitle")
        region = req.get("region")
        sheet_number = req.get("sheet_number")
        source = req.get("source")
        author = req.get("author")
        edition = req.get("edition")
        note = req.get("note")
        logo_path = req.get("logo")
        inset_layer = req.get("inset_layer")
        inset_petunjuk = req.get("inset_petunjuk")
        paper = (req.get("paper") or "a4").lower()
        zoom_layer = req.get("zoom_layer")
        hide_legend_raw = req.get("hide_legend")
        add_grid = bool(req.get("grid", True))
        grid_mode = (req.get("grid_mode") or "utm").lower()
        north_style = (req.get("north_style") or "big").lower()

        if template == "perkabig" or view == "layout":
            mxd.activeView = "PAGE_LAYOUT"
        elif view == "data":
            mxd.activeView = df.name

        # Build full PerKaBIG No. 3 Tahun 2016 Landscape layout (A4 or A3) directly via ArcObjects COM!
        if template == "perkabig":
            if _ao is None:
                return {"ok": False, "error": "ArcObjects COM module required for PerKaBIG layout."}
            std_scales = [5000, 10000, 25000, 50000, 75000, 100000, 125000, 150000, 200000, 250000, 300000, 350000, 400000, 500000, 750000, 1000000]
            span_m = 40000.0
            zoom_ext = None
            if zoom_layer:
                zl = _find_layer(mxd, df, zoom_layer)
                if zl:
                    zoom_ext = zl.getExtent(True)
                    dx = (zoom_ext.XMax - zoom_ext.XMin) * 0.08
                    dy = (zoom_ext.YMax - zoom_ext.YMin) * 0.08
                    df.extent = arcpy.Extent(zoom_ext.XMin - dx, zoom_ext.YMin - dy, zoom_ext.XMax + dx, zoom_ext.YMax + dy)
                    span_m = max(zoom_ext.XMax - zoom_ext.XMin, zoom_ext.YMax - zoom_ext.YMin)
                    cur_s = df.scale
                    df.scale = next((s for s in std_scales if s >= cur_s), std_scales[-1])
            sr = getattr(df, "spatialReference", None)
            crs_str = ""
            utm_zone = ""
            if sr and getattr(sr, "name", "Unknown") != "Unknown":
                epsg = getattr(sr, "factoryCode", 0)
                nm_clean = sr.name.replace("_", " ")
                crs_str = nm_clean + ((" (EPSG:%d)" % epsg) if epsg else "")
                if "UTM" in nm_clean.upper() and "ZONE" in nm_clean.upper():
                    parts = nm_clean.upper().split("ZONE")
                    utm_zone = "UTM Zona " + parts[-1].strip()
                else:
                    utm_zone = nm_clean
            hide_names = [n.strip() for n in (hide_legend_raw or "hillshade,Basemap").split(",") if n.strip()]
            has_inset = bool(inset_layer and os.path.exists(inset_layer))
            has_petunjuk = bool(inset_petunjuk and os.path.exists(inset_petunjuk))

            mxdoc = _ao.get_current_mxdoc()
            try:
                hidden, inset_ok, petunjuk_ok = _ao.build_perkabig_layout(
                    mxdoc,
                    title=title or "PETA TEMATIK WILAYAH",
                    subtitle=subtitle or "",
                    region=region or "",
                    sheet_number=sheet_number or "",
                    source=source or "1. Pengolahan Data Spasial GeoKit.",
                    author=author or "Laboratorium SIG / Penyusun",
                    edition=edition or "Edisi 1 - Tahun 2026",
                    note=note or "",
                    logo_path=logo_path if (logo_path and os.path.exists(logo_path)) else "",
                    has_inset=has_inset,
                    has_petunjuk=has_petunjuk,
                    paper=paper,
                    crs_text=crs_str,
                    utm_zone=utm_zone,
                    scale_val=df.scale or 0,
                    span_meters=span_m,
                    hide_legend_names=hide_names,
                    add_grid=add_grid,
                    grid_mode=grid_mode,
                    north_style=north_style,
                )
            finally:
                _ao._release(mxdoc)

            # Re-center extent cleanly inside resized MapFrame and sync numeric scale text
            if zoom_layer:
                zl = _find_layer(mxd, df, zoom_layer)
                if zl:
                    ext = zl.getExtent(True)
                    dx = (ext.XMax - ext.XMin) * 0.08
                    dy = (ext.YMax - ext.YMin) * 0.08
                    df.extent = arcpy.Extent(ext.XMin - dx, ext.YMin - dy, ext.XMax + dx, ext.YMax + dy)
                    cur_s = df.scale
                    df.scale = next((s for s in std_scales if s >= cur_s), std_scales[-1])
                    final_s = int(round(df.scale)) if df.scale else 0
                    if final_s:
                        for el in arcpy.mapping.ListLayoutElements(mxd, "TEXT_ELEMENT", "skala_angka"):
                            el.text = "SKALA 1 : %s" % ("{:,}".format(final_s).replace(",", "."))

            # Populate, project, style & zoom Petunjuk Letak Peta & Diagram Lokasi inset DataFrames!
            def _populate_inset_df(target_df_name, lyr_path, mult_x, mult_y):
                try:
                    for cand_df in arcpy.mapping.ListDataFrames(mxd):
                        if cand_df.name == target_df_name:
                            if sr and getattr(sr, "name", "Unknown") != "Unknown":
                                try:
                                    cand_df.spatialReference = sr
                                except Exception:
                                    pass
                            in_lyr = arcpy.mapping.Layer(lyr_path)
                            arcpy.mapping.AddLayer(cand_df, in_lyr, "TOP")
                            added_in = arcpy.mapping.ListLayers(mxd, "", cand_df)
                            if zoom_ext is not None:
                                w = max(zoom_ext.XMax - zoom_ext.XMin, 1000.0)
                                h = max(zoom_ext.YMax - zoom_ext.YMin, 1000.0)
                                cx_m = (zoom_ext.XMin + zoom_ext.XMax) / 2.0
                                cy_m = (zoom_ext.YMin + zoom_ext.YMax) / 2.0
                                cand_df.extent = arcpy.Extent(cx_m - w * mult_x, cy_m - h * mult_y, cx_m + w * mult_x, cy_m + h * mult_y)
                            elif added_in:
                                cand_df.extent = added_in[0].getExtent(True)
                            try:
                                mx_in = _ao.get_current_mxdoc()
                                try:
                                    _ao.style_inset_layers(mx_in, target_df_name)
                                finally:
                                    _ao._release(mx_in)
                            except Exception:
                                pass
                            return True
                except Exception:
                    pass
                return False

            petunjuk_populated = _populate_inset_df("Petunjuk Letak Peta", inset_petunjuk, 1.5, 1.2) if (has_petunjuk and petunjuk_ok) else False
            inset_populated = _populate_inset_df("Diagram Lokasi", inset_layer, 3.0, 2.0) if (has_inset and inset_ok) else False

            paper_labels = {
                "a4": "A4 Landscape (29.7 x 21.0 cm)",
                "a3": "A3 Landscape (42.0 x 29.7 cm)",
                "a1": "A1 Landscape (84.1 x 59.4 cm)",
                "a0": "A0 Landscape (118.9 x 84.1 cm)",
            }
            _force_refresh()
            return {
                "ok": True,
                "template": "perkabig",
                "paper": paper_labels.get((paper or "a4").lower(), "A4 Landscape (29.7 x 21.0 cm)"),
                "active_view": mxd.activeView,
                "scale": int(round(df.scale)),
                "crs": crs_str,
                "hidden_legend_items": hidden,
                "inset": inset_populated,
                "inset_petunjuk": petunjuk_populated,
                "north_style": north_style,
                "grid": add_grid,
                "grid_mode": grid_mode,
            }

        updated_elements = []
        elems = arcpy.mapping.ListLayoutElements(mxd, "TEXT_ELEMENT")
        for el in elems:
            ename = (el.name or "").lower()
            etext = (el.text or "").lower()
            if title and ("title" in ename or "judul" in ename or "title" in etext or "judul" in etext):
                el.text = title
                updated_elements.append({"element": el.name or "title", "text": title})
            elif subtitle and ("subtitle" in ename or "subjudul" in ename or "subtitle" in etext or "subjudul" in etext):
                el.text = subtitle
                updated_elements.append({"element": el.name or "subtitle", "text": subtitle})
            elif source and ("source" in ename or "sumber" in ename or "source" in etext or "sumber" in etext):
                el.text = source
                updated_elements.append({"element": el.name or "source", "text": source})

        if title and not updated_elements and len(elems) > 0:
            elems[0].text = title
            updated_elements.append({"element": elems[0].name or "text_0", "text": title})

        if zoom_layer:
            zl = _find_layer(mxd, df, zoom_layer)
            if zl:
                ext = zl.getExtent(True)
                dx = (ext.XMax - ext.XMin) * 0.12
                dy = (ext.YMax - ext.YMin) * 0.12
                df.extent = arcpy.Extent(ext.XMin - dx, ext.YMin - dy, ext.XMax + dx, ext.YMax + dy)

        hidden_legend = []
        if hide_legend_raw and _ao is not None:
            try:
                mxdoc = _ao.get_current_mxdoc()
                try:
                    names = [n.strip() for n in hide_legend_raw.split(",") if n.strip()]
                    hidden_legend = _ao.hide_legend_layers(mxdoc, names)
                finally:
                    _ao._release(mxdoc)
            except Exception:
                pass

        _force_refresh()
        return {
            "ok": True,
            "active_view": mxd.activeView,
            "updated_elements": updated_elements,
            "hidden_legend_items": hidden_legend,
            "zoomed_to": zoom_layer,
        }

    if action == "rename_layer":
        name = req.get("layer", "")
        new_name = req.get("new_name", "")
        if not new_name:
            return {"ok": False, "error": "Missing 'new_name'"}
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        old_name = lyr.name
        lyr.name = new_name
        _force_refresh()
        return {"ok": True, "old_name": old_name, "new_name": lyr.name}

    if action == "set_labels":
        name = req.get("layer", "")
        show = bool(req.get("show", True))
        field = req.get("field")
        halo = bool(req.get("halo", True))
        font_size = float(req.get("font_size") or 8.5)
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        if not lyr.supports("SHOWLABELS"):
            return {"ok": False, "error": "Layer does not support labels: %s" % lyr.name}
        if field and lyr.supports("LABELCLASSES") and len(lyr.labelClasses) > 0:
            expr = field if field.startswith("[") else "[%s]" % field
            lyr.labelClasses[0].expression = expr
        lyr.showLabels = show
        halo_applied = False
        if show and halo and _ao is not None:
            try:
                mxdoc = _ao.get_current_mxdoc()
                com_lyr, _ = _ao.find_com_layer(mxdoc, lyr.name)
                if com_lyr:
                    halo_applied = _ao.set_label_halo(mxdoc, com_lyr, font_size=font_size)
                    _ao._release(com_lyr)
                _ao._release(mxdoc)
            except Exception:
                pass
        _force_refresh()
        return {"ok": True, "layer": lyr.name, "show_labels": show, "field": field, "halo": halo_applied}

    if action == "zoom_to_layer":
        name = req.get("layer", "")
        padding = float(req.get("padding") if req.get("padding") is not None else 0.12)
        scale_req = req.get("scale")

        def _apply_extent_with_padding(ext):
            if padding > 0:
                dx = (ext.XMax - ext.XMin) * padding
                dy = (ext.YMax - ext.YMin) * padding
                ext = arcpy.Extent(ext.XMin - dx, ext.YMin - dy, ext.XMax + dx, ext.YMax + dy)
            df.extent = ext
            if scale_req:
                s_str = str(scale_req).lower()
                if s_str == "auto":
                    std_scales = [1000, 2500, 5000, 10000, 25000, 50000, 75000, 100000, 125000, 150000, 200000, 250000, 500000, 1000000, 2500000]
                    cur = df.scale
                    snapped = next((s for s in std_scales if s >= cur), std_scales[-1])
                    df.scale = snapped
                else:
                    try:
                        df.scale = float(s_str)
                    except ValueError:
                        pass

        if not name or name.lower() == "__full__":
            ext = None
            for lyr in arcpy.mapping.ListLayers(mxd, "", df):
                try:
                    e = lyr.getExtent(True)
                    if e:
                        if ext is None:
                            ext = e
                        else:
                            ext.XMin = min(ext.XMin, e.XMin)
                            ext.YMin = min(ext.YMin, e.YMin)
                            ext.XMax = max(ext.XMax, e.XMax)
                            ext.YMax = max(ext.YMax, e.YMax)
                except Exception:
                    pass
            if ext:
                _apply_extent_with_padding(ext)
                _force_refresh()
            return {"ok": True, "zoomed": "full_extent", "scale": round(df.scale, 1)}

        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        ext = lyr.getSelectedExtent(True)
        if not ext:
            ext = lyr.getExtent(True)
        _apply_extent_with_padding(ext)
        _force_refresh()
        return {"ok": True, "zoomed_to": lyr.name, "padding": padding, "scale": round(df.scale, 1)}

    if action == "select":
        name = req.get("layer", "")
        where = req.get("where", "")
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        arcpy.SelectLayerByAttribute_management(lyr, "NEW_SELECTION", where)
        count = int(arcpy.GetCount_management(lyr).getOutput(0))
        _force_refresh()
        return {"ok": True, "layer": lyr.name, "selected_count": count, "where": where}

    if action == "clear_selection":
        name = req.get("layer")
        cleared = []
        targets = [_find_layer(mxd, df, name)] if name else arcpy.mapping.ListLayers(mxd, "", df)
        for lyr in targets:
            if lyr and lyr.isFeatureLayer:
                try:
                    arcpy.SelectLayerByAttribute_management(lyr, "CLEAR_SELECTION")
                    cleared.append(lyr.name)
                except Exception:
                    pass
        _force_refresh()
        return {"ok": True, "cleared": cleared}

    if action == "export_view":
        out_path = req.get("out", "")
        if not out_path:
            return {"ok": False, "error": "Missing 'out' path"}
        parent = os.path.dirname(out_path)
        if parent and not os.path.exists(parent):
            os.makedirs(parent)
        fmt = (req.get("format") or os.path.splitext(out_path)[1].lstrip(".") or "png").lower()
        dpi = int(req.get("dpi") or 300)
        if fmt == "pdf":
            arcpy.mapping.ExportToPDF(mxd, out_path, resolution=dpi)
        elif fmt in ("jpg", "jpeg"):
            arcpy.mapping.ExportToJPEG(mxd, out_path, resolution=dpi)
        else:
            arcpy.mapping.ExportToPNG(mxd, out_path, resolution=dpi)
        return {"ok": True, "exported": out_path, "format": fmt, "dpi": dpi}

    if action == "save_mxd":
        out_path = req.get("out")
        if out_path:
            parent = os.path.dirname(out_path)
            if parent and not os.path.exists(parent):
                os.makedirs(parent)
            mxd.saveACopy(out_path)
            return {"ok": True, "saved": out_path}
        if not mxd.filePath:
            return {"ok": False, "error": "Current map has never been saved. Pass --out <file.mxd>."}
        mxd.save()
        return {"ok": True, "saved": mxd.filePath}

    if action == "list_tools":
        pattern = req.get("pattern") or "*"
        if "*" not in pattern:
            pattern = "*%s*" % pattern
        tools = sorted(arcpy.ListTools(pattern) or [])
        return {"ok": True, "pattern": pattern, "count": len(tools), "tools": tools[:150]}

    if action == "tool_usage":
        tool = req.get("tool", "")
        if not tool:
            return {"ok": False, "error": "Missing 'tool' name"}
        usage = arcpy.Usage(tool)
        return {"ok": True, "tool": tool, "usage": usage}

    if action == "run_gp":
        tool = (req.get("tool") or "").strip()
        args = req.get("args", [])
        kwargs = req.get("kwargs", {}) or {}
        add_output = bool(req.get("add_to_map", True))
        if not tool:
            return {"ok": False, "error": "Missing 'tool' name (e.g. Buffer_analysis)"}
        if tool.startswith("_") or not all(c.isalnum() or c == "_" for c in tool) or not tool[0].isalpha():
            return {"ok": False, "error": "Invalid ArcPy tool name '%s'." % tool}
        _auto_checkout_extension(tool)
        fn = getattr(arcpy, tool, None)
        if fn is None:
            fn = getattr(arcpy.gp, tool, None)
        if fn is None or not callable(fn) or isinstance(fn, type):
            return {
                "ok": False,
                "error": "Unknown ArcPy tool '%s'. Use action 'list_tools' to search exact tool names." % tool,
            }
        prev_ow = arcpy.env.overwriteOutput
        prev_add = arcpy.env.addOutputsToMap
        arcpy.env.overwriteOutput = bool(req.get("overwrite", False))
        arcpy.env.addOutputsToMap = add_output
        try:
            res = fn(*args, **kwargs)
            out_val = res.getOutput(0) if res else ""
        finally:
            arcpy.env.overwriteOutput = prev_ow
            arcpy.env.addOutputsToMap = prev_add
        _force_refresh()
        return {"ok": True, "tool": tool, "output": out_val}

    return {"ok": False, "error": "Unknown live action: %s" % action}


def _drain_queue_callback(hwnd, msg, id_event, dw_time):
    while True:
        try:
            req, holder, done_evt = _CMD_QUEUE.get_nowait()
        except Queue.Empty:
            break
        try:
            holder["response"] = _handle_command_on_ui_thread(req)
        except Exception as exc:
            _log(traceback.format_exc())
            holder["response"] = {"ok": False, "error": str(exc)}
        finally:
            done_evt.set()


def _verify_local_token(req_token):
    expected = os.environ.get("GEOKIT_AUTH_TOKEN", "").strip()
    if not expected:
        home = os.environ.get("USERPROFILE") or os.path.expanduser("~")
        tok_path = os.path.join(home, ".geokit", "token")
        if os.path.isfile(tok_path):
            try:
                with open(tok_path, "r") as fh:
                    expected = fh.read().strip()
            except Exception:
                expected = ""
    if not expected:
        return True
    if not isinstance(req_token, basestring):
        return False
    a = str(req_token)
    b = str(expected)
    if len(a) != len(b):
        return False
    diff = 0
    for x, y in zip(a, b):
        diff |= ord(x) ^ ord(y)
    return diff == 0


def _serve_client(conn):
    max_bytes = 10 * 1024 * 1024  # 10 MB safety cap
    try:
        conn.settimeout(30.0)
        data = b""
        while b"\n" not in data:
            chunk = conn.recv(65536)
            if not chunk:
                break
            data += chunk
            if len(data) > max_bytes:
                err = (json.dumps({"ok": False, "error": "Request exceeds 10 MB limit"}) + "\n").encode("utf-8")
                conn.sendall(err)
                return
        if not data:
            return
        req = json.loads(data.decode("utf-8").strip())
        if not _verify_local_token(req.get("token")):
            err = (json.dumps({"ok": False, "error": "Unauthorized: invalid or missing GeoKit local auth token"}) + "\n").encode("utf-8")
            conn.sendall(err)
            return
        holder = {}
        done_evt = threading.Event()
        _CMD_QUEUE.put((req, holder, done_evt))
        if not done_evt.wait(300.0):
            resp = {"ok": False, "error": "Timed out waiting for ArcMap UI thread"}
        else:
            resp = holder.get("response", {"ok": False, "error": "Empty response"})
        payload = (json.dumps(resp) + "\n").encode("utf-8")
        conn.sendall(payload)
    except Exception as exc:
        _log(traceback.format_exc())
        try:
            err = (json.dumps({"ok": False, "error": str(exc)}) + "\n").encode("utf-8")
            conn.sendall(err)
        except Exception:
            pass
    finally:
        try:
            conn.close()
        except Exception:
            pass


def _accept_loop():
    global _RUNNING
    while _RUNNING and _SERVER_SOCKET is not None:
        try:
            conn, _ = _SERVER_SOCKET.accept()
            t = threading.Thread(target=_serve_client, args=(conn,))
            t.daemon = True
            t.start()
        except socket.timeout:
            continue
        except Exception:
            if _RUNNING:
                _log(traceback.format_exc())
            break


def start_bridge():
    global _SERVER_SOCKET, _SERVER_THREAD, _TIMER_ID, _TIMER_PROC_REF, _RUNNING
    if _RUNNING:
        return True
    try:
        srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        srv.settimeout(1.0)
        srv.bind((HOST, PORT))
        srv.listen(5)
        _SERVER_SOCKET = srv
        _RUNNING = True

        # Register 100ms Win32 timer on ArcMap's main UI thread
        TIMERPROC = ctypes.WINFUNCTYPE(
            None, ctypes.c_void_p, ctypes.c_uint, ctypes.c_size_t, ctypes.c_uint
        )
        _TIMER_PROC_REF = TIMERPROC(_drain_queue_callback)
        _TIMER_ID = ctypes.windll.user32.SetTimer(0, 0, 100, _TIMER_PROC_REF)

        _SERVER_THREAD = threading.Thread(target=_accept_loop)
        _SERVER_THREAD.daemon = True
        _SERVER_THREAD.start()
        _log("GeoKit ArcMap Live Bridge started on %s:%d (timer=%s)" % (HOST, PORT, _TIMER_ID))
        return True
    except Exception:
        _log(traceback.format_exc())
        _RUNNING = False
        return False


def stop_bridge():
    global _SERVER_SOCKET, _TIMER_ID, _RUNNING
    _RUNNING = False
    if _TIMER_ID:
        try:
            ctypes.windll.user32.KillTimer(0, _TIMER_ID)
        except Exception:
            pass
        _TIMER_ID = None
    if _SERVER_SOCKET:
        try:
            _SERVER_SOCKET.close()
        except Exception:
            pass
        _SERVER_SOCKET = None


class GeoKitLiveExtension(object):
    """ArcMap Extension that auto-starts the GeoKit Live-UI Bridge."""

    def __init__(self):
        self.enabled = True

    def startup(self):
        start_bridge()

    def newDocument(self):
        start_bridge()

    def openDocument(self):
        start_bridge()


class GeoKitStatusButton(object):
    """Toolbar button to inspect or start the GeoKit Live-UI Bridge."""

    def __init__(self):
        self.enabled = True
        self.checked = False

    def onClick(self):
        if not _RUNNING:
            ok = start_bridge()
            msg = (
                "GeoKit Live Bridge STARTED on %s:%d" % (HOST, PORT)
                if ok
                else "Failed to start GeoKit Live Bridge. Check log: %s" % LOG_PATH
            )
        else:
            msg = (
                "GeoKit Live Bridge is RUNNING on %s:%d.\n"
                "All ArcToolbox tools and live map controls are ready for geokit CLI / MCP!"
                % (HOST, PORT)
            )
        pythonaddins.MessageBox(msg, "GeoKit Live Bridge", 0)
