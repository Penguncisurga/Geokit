# -*- coding: utf-8 -*-
"""
GeoKit Live-UI TCP Bridge for ArcGIS Pro 2.x / 3.x (Python 3).
Speaks the EXACT same JSON-over-TCP protocol (127.0.0.1:9877) as the ArcMap Add-In,
allowing `geokit live ...` and MCP `geokit_live` to control an open ArcGIS Pro project
in real time using `arcpy.mp.ArcGISProject("CURRENT")` and native Pro CIM symbology!

Quick start inside ArcGIS Pro:
  Option A: Double-click "Start GeoKit Live Bridge" inside GeoKit_Live_Bridge.pyt
  Option B: Paste into ArcGIS Pro Python Window:
      exec(open(r"%USERPROFILE%\.geokit\pro\geokit_pro_bridge.py", encoding="utf-8").read())
"""

import json
import os
import socket
import threading
import traceback

import arcpy

HOST = "127.0.0.1"
PORT = int(os.environ.get("GEOKIT_LIVE_PORT", "9877"))

_SERVER = None
_SERVER_THREAD = None

PALETTES = {
    "hazard": [
        [46, 125, 50, 100],
        [129, 199, 132, 100],
        [255, 241, 118, 100],
        [255, 167, 38, 100],
        [211, 47, 47, 100],
    ],
    "hazard5": [
        [27, 94, 32, 100],
        [102, 187, 106, 100],
        [255, 238, 88, 100],
        [251, 140, 0, 100],
        [198, 40, 40, 100],
    ],
    "hazard-pastel": [
        [82, 183, 136, 100],
        [183, 228, 199, 100],
        [255, 230, 109, 100],
        [244, 162, 97, 100],
        [230, 57, 70, 100],
    ],
    "viridis": [
        [68, 1, 84, 100],
        [59, 82, 139, 100],
        [33, 145, 140, 100],
        [94, 201, 98, 100],
        [253, 231, 37, 100],
    ],
    "blue": [
        [227, 242, 253, 100],
        [144, 202, 249, 100],
        [66, 165, 245, 100],
        [25, 118, 210, 100],
        [13, 71, 161, 100],
    ],
    "earth": [
        [46, 125, 50, 100],
        [156, 204, 101, 100],
        [255, 245, 157, 100],
        [215, 168, 110, 100],
        [141, 110, 99, 100],
    ],
    "category": [
        [31, 119, 180, 100],
        [255, 127, 14, 100],
        [44, 160, 44, 100],
        [214, 39, 40, 100],
        [148, 103, 189, 100],
        [140, 86, 75, 100],
        [227, 119, 194, 100],
        [23, 190, 207, 100],
    ],
}

HAZARD_RANK = {
    "sangat rendah": 1,
    "rendah": 2,
    "sedang": 3,
    "tinggi": 4,
    "sangat tinggi": 5,
    "very low": 1,
    "low": 2,
    "medium": 3,
    "moderate": 3,
    "high": 4,
    "very high": 5,
}


def _parse_hex(h):
    s = (h or "").strip().lstrip("#")
    if len(s) == 3:
        s = "".join(c * 2 for c in s)
    if len(s) != 6:
        return [30, 136, 229, 100]
    return [int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16), 100]


def _sample_palette(preset, n, custom_colors=None):
    if custom_colors:
        stops = [_parse_hex(c) for c in custom_colors.split(",") if c.strip()]
    else:
        key = (preset or "hazard-pastel").lower()
        if key == "category":
            base = PALETTES["category"]
            return [base[i % len(base)] for i in range(max(1, n))]
        stops = PALETTES.get(key, PALETTES["hazard-pastel"])
    if n <= 1:
        return [stops[0]]
    if n == len(stops):
        return list(stops)
    out = []
    max_i = float(len(stops) - 1)
    for i in range(n):
        t = (float(i) / float(n - 1)) * max_i
        lo = int(t)
        hi = min(lo + 1, len(stops) - 1)
        f = t - lo
        out.append([
            int(round(stops[lo][0] + (stops[hi][0] - stops[lo][0]) * f)),
            int(round(stops[lo][1] + (stops[hi][1] - stops[lo][1]) * f)),
            int(round(stops[lo][2] + (stops[hi][2] - stops[lo][2]) * f)),
            100,
        ])
    return out


def _get_aprx_and_map():
    aprx = arcpy.mp.ArcGISProject("CURRENT")
    m = aprx.activeMap
    if m is None:
        maps = aprx.listMaps()
        if not maps:
            raise RuntimeError("No open Map found in the current ArcGIS Pro project.")
        m = maps[0]
    return aprx, m


def _find_layer(m, name):
    target = (name or "").lower()
    exact = None
    partial = None
    for lyr in m.listLayers():
        ln = (lyr.name or "").lower()
        if ln == target:
            exact = lyr
            break
        if target in ln and partial is None:
            partial = lyr
    return exact or partial


def _get_cim(lyr):
    for ver in ("V3", "V2"):
        try:
            return lyr.getDefinition(ver), ver
        except Exception:
            continue
    return None, None


def _handle_request(req):
    action = req.get("action", "")
    aprx, m = _get_aprx_and_map()

    if action == "ping":
        return {
            "ok": True,
            "app": "ArcGIS Pro",
            "version": arcpy.GetInstallInfo().get("Version", "3.x"),
            "aprx": aprx.filePath or "(Unsaved Project)",
            "active_map": m.name,
            "maps": [mp.name for mp in aprx.listMaps()],
            "layer_count": len(m.listLayers()),
            "port": PORT,
        }

    if action == "list_layers":
        items = []
        for idx, lyr in enumerate(m.listLayers()):
            items.append({
                "index": idx,
                "name": lyr.name,
                "visible": bool(lyr.visible),
                "is_feature": bool(lyr.isFeatureLayer),
                "is_raster": bool(lyr.isRasterLayer),
                "broken": bool(lyr.isBroken),
                "data_source": lyr.dataSource if lyr.supports("DATASOURCE") else None,
                "transparency": getattr(lyr, "transparency", None),
            })
        return {"ok": True, "map": m.name, "layers": items}

    if action == "layer_info":
        lyr = _find_layer(m, req.get("layer", ""))
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        info = {
            "ok": True,
            "name": lyr.name,
            "visible": bool(lyr.visible),
            "data_source": lyr.dataSource if lyr.supports("DATASOURCE") else None,
        }
        if lyr.isFeatureLayer:
            desc = arcpy.Describe(lyr)
            info["shape_type"] = getattr(desc, "shapeType", "Unknown")
            fields = [
                {"name": f.name, "type": f.type}
                for f in arcpy.ListFields(lyr)
                if f.type not in ("Geometry", "OID", "Blob", "Raster")
            ]
            info["fields"] = fields
            info["feature_count"] = int(arcpy.management.GetCount(lyr)[0])
        return info

    if action == "add_layer":
        path = req.get("path", "")
        if not path:
            return {"ok": False, "error": "Missing 'path'"}
        if path.lower().endswith(".lyrx"):
            lyr = m.addLayer(arcpy.mp.LayerFile(path), "TOP")[0]
        else:
            lyr = m.addDataFromPath(path)
        return {"ok": True, "added": lyr.name, "path": path}

    if action == "remove_layer":
        lyr = _find_layer(m, req.get("layer", ""))
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        nm = lyr.name
        m.removeLayer(lyr)
        return {"ok": True, "removed": nm}

    if action == "rename_layer":
        lyr = _find_layer(m, req.get("layer", ""))
        new_name = req.get("new_name", "")
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        old_name = lyr.name
        lyr.name = new_name
        return {"ok": True, "old_name": old_name, "new_name": lyr.name}

    if action == "set_visibility":
        lyr = _find_layer(m, req.get("layer", ""))
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        lyr.visible = bool(req.get("show", True))
        return {"ok": True, "layer": lyr.name, "visible": lyr.visible}

    if action == "definition_query":
        lyr = _find_layer(m, req.get("layer", ""))
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        lyr.definitionQuery = req.get("where") or ""
        return {"ok": True, "layer": lyr.name, "definition_query": lyr.definitionQuery}

    if action == "apply_symbology":
        lyr = _find_layer(m, req.get("layer", ""))
        lyr_file = req.get("lyr", "")
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        arcpy.management.ApplySymbologyFromLayer(lyr, lyr_file)
        return {"ok": True, "layer": lyr.name, "applied": lyr_file}

    if action == "zoom_to_layer":
        lyr = _find_layer(m, req.get("layer", ""))
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        mv = aprx.activeView
        if mv and hasattr(mv, "camera"):
            ext = mv.getLayerExtent(lyr, False, True)
            pad = float(req.get("padding") if req.get("padding") is not None else 0.12)
            dx = (ext.XMax - ext.XMin) * pad
            dy = (ext.YMax - ext.YMin) * pad
            mv.camera.setExtent(arcpy.Extent(ext.XMin - dx, ext.YMin - dy, ext.XMax + dx, ext.YMax + dy))
            sc = req.get("scale")
            if sc:
                if str(sc).lower() == "auto":
                    stds = [5000, 10000, 25000, 50000, 75000, 100000, 150000, 200000, 250000, 300000, 350000, 500000, 1000000]
                    mv.camera.scale = next((s for s in stds if s >= mv.camera.scale), stds[-1])
                else:
                    mv.camera.scale = float(sc)
            return {"ok": True, "zoomed_to": lyr.name, "scale": int(round(mv.camera.scale))}
        return {"ok": False, "error": "Active view does not support camera zoom."}

    if action == "order_layer":
        lyr = _find_layer(m, req.get("layer", ""))
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        pos = (req.get("position") or "top").lower()
        all_lyrs = m.listLayers()
        if len(all_lyrs) > 1:
            ref = all_lyrs[0] if pos == "top" else all_lyrs[-1]
            if ref.name != lyr.name:
                m.moveLayer(ref, lyr, "BEFORE" if pos == "top" else "AFTER")
        return {"ok": True, "layer": lyr.name, "position": pos}

    if action == "style_layer":
        lyr = _find_layer(m, req.get("layer", ""))
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        field = req.get("field")
        mode = (req.get("mode") or "auto").lower()
        preset = (req.get("preset") or "hazard-pastel").lower()
        custom_colors = req.get("colors")
        color_hex = req.get("color")
        transparency = req.get("transparency")
        hollow = bool(req.get("hollow", False)) or preset == "boundary"

        sym = lyr.symbology
        if field and lyr.isFeatureLayer:
            if mode == "graduated":
                sym.updateRenderer("GraduatedColorsRenderer")
                sym.renderer.classificationField = field
                sym.renderer.breakCount = int(req.get("classes_count") or 5)
                cols = _sample_palette(preset, len(sym.renderer.classBreaks), custom_colors)
                for i, brk in enumerate(sym.renderer.classBreaks):
                    brk.symbol.color = {"RGB": cols[i]}
                    brk.symbol.outlineWidth = float(req.get("outline_width") or 0.0)
            else:
                sym.updateRenderer("UniqueValueRenderer")
                sym.renderer.fields = [field]
                items = []
                for grp in sym.renderer.groups:
                    items.extend(grp.items)
                items.sort(key=lambda it: (HAZARD_RANK.get(str(it.label).lower(), 999), str(it.label)))
                cols = _sample_palette(preset, len(items), custom_colors)
                for i, it in enumerate(items):
                    it.symbol.color = {"RGB": cols[i]}
                    it.symbol.outlineWidth = float(req.get("outline_width") or 0.0)
            lyr.symbology = sym
        elif lyr.isFeatureLayer:
            sym.updateRenderer("SimpleRenderer")
            rgb = _parse_hex(color_hex) if color_hex else (
                [2, 136, 209, 100] if preset == "stream" else ([216, 67, 21, 100] if preset == "road" else [30, 136, 229, 100])
            )
            if hollow:
                sym.renderer.symbol.color = {"RGB": [0, 0, 0, 0]}
                sym.renderer.symbol.outlineColor = {"RGB": _parse_hex(req.get("outline_color") or "#263238")}
                sym.renderer.symbol.outlineWidth = float(req.get("outline_width") or 1.5)
            else:
                sym.renderer.symbol.color = {"RGB": rgb}
                if req.get("width"):
                    sym.renderer.symbol.size = float(req["width"])
                elif req.get("size"):
                    sym.renderer.symbol.size = float(req["size"])
            lyr.symbology = sym

        if transparency is not None:
            lyr.transparency = int(transparency)
        return {"ok": True, "engine": "ArcGIS Pro CIM", "layer": lyr.name, "field": field, "preset": preset}

    if action == "set_labels":
        lyr = _find_layer(m, req.get("layer", ""))
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        lyr.showLabels = bool(req.get("show", True))
        field = req.get("field")
        if field and lyr.listLabelClasses():
            lc = lyr.listLabelClasses()[0]
            lc.expression = f"$feature.{field}"
        return {"ok": True, "layer": lyr.name, "show_labels": lyr.showLabels, "field": field}

    if action == "select":
        lyr = _find_layer(m, req.get("layer", ""))
        if not lyr:
            return {"ok": False, "error": f"Layer not found: {req.get('layer')}"}
        arcpy.management.SelectLayerByAttribute(lyr, "NEW_SELECTION", req.get("where", ""))
        cnt = int(arcpy.management.GetCount(lyr)[0])
        return {"ok": True, "layer": lyr.name, "selected_count": cnt}

    if action == "clear_selection":
        lyr = _find_layer(m, req.get("layer", ""))
        if lyr:
            arcpy.management.SelectLayerByAttribute(lyr, "CLEAR_SELECTION")
            return {"ok": True, "cleared": lyr.name}
        for cand in m.listLayers():
            if cand.isFeatureLayer:
                arcpy.management.SelectLayerByAttribute(cand, "CLEAR_SELECTION")
        return {"ok": True, "cleared": "all"}

    if action == "export_view":
        out_path = req.get("out", "")
        dpi = int(req.get("dpi") or 200)
        parent = os.path.dirname(out_path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        layouts = aprx.listLayouts()
        av = aprx.activeView
        if hasattr(av, "exportToPNG"):
            if out_path.lower().endswith(".pdf"):
                av.exportToPDF(out_path, resolution=dpi)
            else:
                av.exportToPNG(out_path, resolution=dpi)
        elif layouts:
            if out_path.lower().endswith(".pdf"):
                layouts[0].exportToPDF(out_path, resolution=dpi)
            else:
                layouts[0].exportToPNG(out_path, resolution=dpi)
        else:
            return {"ok": False, "error": "No active Layout or exportable MapView found."}
        return {"ok": True, "exported": out_path, "dpi": dpi}

    if action == "save_mxd":
        out_path = req.get("out")
        if out_path:
            aprx.saveACopy(out_path)
            return {"ok": True, "saved": out_path}
        aprx.save()
        return {"ok": True, "saved": aprx.filePath}

    if action == "list_tools":
        pat = (req.get("pattern") or "*").strip()
        if "*" not in pat and "?" not in pat:
            pat = f"*{pat}*"
        tools = sorted(arcpy.ListTools(pat))
        return {"ok": True, "count": len(tools), "tools": tools[:200]}

    if action == "tool_usage":
        tool = req.get("tool", "")
        return {"ok": True, "tool": tool, "usage": arcpy.Usage(tool)}

    if action == "run_gp":
        tool = req.get("tool", "")
        args = req.get("args") or []
        for ext in ("Spatial", "3D", "Network"):
            try:
                if arcpy.CheckExtension(ext) == "Available":
                    arcpy.CheckOutExtension(ext)
            except Exception:
                pass
        fn = getattr(arcpy, tool, None) or getattr(arcpy.gp, tool, None)
        if fn is None:
            return {"ok": False, "error": f"Unknown ArcPy tool: {tool}"}
        res = fn(*args)
        return {"ok": True, "tool": tool, "output": str(res)}

    return {"ok": False, "error": f"Unsupported action in ArcGIS Pro Live Bridge: {action}"}


def _client_worker(conn):
    try:
        buf = b""
        while True:
            chunk = conn.recv(65536)
            if not chunk:
                break
            buf += chunk
            if b"\n" in buf:
                break
        line = buf.decode("utf-8", errors="replace").strip()
        if not line:
            return
        req = json.loads(line)
        try:
            resp = _handle_request(req)
        except Exception as exc:
            resp = {"ok": False, "error": str(exc), "trace": traceback.format_exc()}
        conn.sendall((json.dumps(resp) + "\n").encode("utf-8"))
    finally:
        conn.close()


def _serve_loop(srv):
    while True:
        try:
            conn, _ = srv.accept()
            threading.Thread(target=_client_worker, args=(conn,), daemon=True).start()
        except Exception:
            break


def start_bridge(port=PORT):
    global _SERVER, _SERVER_THREAD
    if _SERVER is not None:
        print(f"[GeoKit Pro Bridge] Already listening on {HOST}:{port}")
        return
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((HOST, port))
    srv.listen(8)
    _SERVER = srv
    _SERVER_THREAD = threading.Thread(target=_serve_loop, args=(srv,), daemon=True)
    _SERVER_THREAD.start()
    print(f"[GeoKit Pro Bridge] Listening on {HOST}:{port} — ready for `geokit live` & MCP!")


if __name__ == "__main__":
    start_bridge(PORT)
