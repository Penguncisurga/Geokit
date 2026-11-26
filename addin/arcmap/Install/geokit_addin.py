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
import tempfile
import threading
import traceback

import arcpy
import pythonaddins

try:
    import arcobjects as _ao
except Exception:
    _ao = None

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
            return {"ok": False, "error": "ArcObjects COM module failed to load inside Add-In."}

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
        title = req.get("title")
        subtitle = req.get("subtitle")
        source = req.get("source")
        zoom_layer = req.get("zoom_layer")

        if view == "layout":
            mxd.activeView = "PAGE_LAYOUT"
        elif view == "data":
            mxd.activeView = df.name

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
                df.extent = zl.getExtent(True)

        hidden_legend = []
        hide_legend_raw = req.get("hide_legend")
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

    if action == "set_labels":
        name = req.get("layer", "")
        show = bool(req.get("show", True))
        field = req.get("field")
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        if not lyr.supports("SHOWLABELS"):
            return {"ok": False, "error": "Layer does not support labels: %s" % lyr.name}
        if field and lyr.supports("LABELCLASSES") and len(lyr.labelClasses) > 0:
            expr = field if field.startswith("[") else "[%s]" % field
            lyr.labelClasses[0].expression = expr
        lyr.showLabels = show
        _force_refresh()
        return {"ok": True, "layer": lyr.name, "show_labels": show, "field": field}

    if action == "zoom_to_layer":
        name = req.get("layer", "")
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
                df.extent = ext
                _force_refresh()
            return {"ok": True, "zoomed": "full_extent"}

        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        ext = lyr.getSelectedExtent(True)
        if not ext:
            ext = lyr.getExtent(True)
        df.extent = ext
        _force_refresh()
        return {"ok": True, "zoomed_to": lyr.name}

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
        dpi = int(req.get("dpi") or 200)
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
        tool = req.get("tool", "")
        args = req.get("args", [])
        kwargs = req.get("kwargs", {}) or {}
        add_output = bool(req.get("add_to_map", True))
        if not tool:
            return {"ok": False, "error": "Missing 'tool' name (e.g. Buffer_analysis)"}
        _auto_checkout_extension(tool)
        fn = getattr(arcpy, tool, None)
        if fn is None:
            fn = getattr(arcpy.gp, tool, None)
        if fn is None:
            return {
                "ok": False,
                "error": "Unknown arcpy tool '%s'. Use action 'list_tools' to search exact tool names." % tool,
            }
        arcpy.env.overwriteOutput = True
        prev_add = arcpy.env.addOutputsToMap
        arcpy.env.addOutputsToMap = add_output
        try:
            res = fn(*args, **kwargs)
            out_val = res.getOutput(0) if res else ""
        finally:
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


def _serve_client(conn):
    try:
        data = b""
        while b"\n" not in data:
            chunk = conn.recv(65536)
            if not chunk:
                break
            data += chunk
        if not data:
            return
        req = json.loads(data.decode("utf-8").strip())
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
