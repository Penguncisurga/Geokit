# -*- coding: utf-8 -*-
"""
GeoKit ArcMap 10.x Live-UI Python Add-In (Python 2.7).
Runs a local TCP JSON bridge on 127.0.0.1:9877 and dispatches all ArcPy /
MapDocument("CURRENT") calls safely onto ArcMap's main UI thread via user32.SetTimer.
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
        arcpy.RefreshActiveView()
        arcpy.RefreshTOC()
        return {"ok": True, "added": lyr.name, "path": path}

    if action == "remove_layer":
        name = req.get("layer", "")
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        removed_name = lyr.name
        arcpy.mapping.RemoveLayer(df, lyr)
        arcpy.RefreshActiveView()
        arcpy.RefreshTOC()
        return {"ok": True, "removed": removed_name}

    if action == "set_visibility":
        name = req.get("layer", "")
        visible = bool(req.get("visible", True))
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        lyr.visible = visible
        arcpy.RefreshActiveView()
        arcpy.RefreshTOC()
        return {"ok": True, "layer": lyr.name, "visible": lyr.visible}

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
                arcpy.RefreshActiveView()
            return {"ok": True, "zoomed": "full_extent"}

        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        ext = lyr.getSelectedExtent(True)
        if not ext:
            ext = lyr.getExtent(True)
        df.extent = ext
        arcpy.RefreshActiveView()
        return {"ok": True, "zoomed_to": lyr.name}

    if action == "select":
        name = req.get("layer", "")
        where = req.get("where", "")
        lyr = _find_layer(mxd, df, name)
        if not lyr:
            return {"ok": False, "error": "Layer not found: %s" % name}
        arcpy.SelectLayerByAttribute_management(lyr, "NEW_SELECTION", where)
        count = int(arcpy.GetCount_management(lyr).getOutput(0))
        arcpy.RefreshActiveView()
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
        arcpy.RefreshActiveView()
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

    if action == "run_gp":
        tool = req.get("tool", "")
        args = req.get("args", [])
        add_output = bool(req.get("add_to_map", True))
        if not tool:
            return {"ok": False, "error": "Missing 'tool' name (e.g. Buffer_analysis)"}
        fn = getattr(arcpy, tool, None)
        if fn is None:
            return {"ok": False, "error": "Unknown arcpy tool: %s" % tool}
        arcpy.env.overwriteOutput = True
        prev_add = arcpy.env.addOutputsToMap
        arcpy.env.addOutputsToMap = add_output
        try:
            res = fn(*args)
            out_val = res.getOutput(0) if res else ""
        finally:
            arcpy.env.addOutputsToMap = prev_add
        arcpy.RefreshActiveView()
        arcpy.RefreshTOC()
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
        if not done_evt.wait(120.0):
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
            msg = "GeoKit Live Bridge is RUNNING on %s:%d.\nReady for live commands from geokit CLI / MCP!" % (
                HOST,
                PORT,
            )
        pythonaddins.MessageBox(msg, "GeoKit Live Bridge", 0)
