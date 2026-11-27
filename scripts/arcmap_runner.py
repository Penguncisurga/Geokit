# -*- coding: utf-8 -*-
"""
geokit embedded ArcMap 10.x (Python 2.7) batch runner.
Supports ArcGIS Desktop 10.3 - 10.8 geoprocessing and .mxd mapping operations.
"""

from __future__ import print_function
import json
import os
import re
import sys

try:
    import arcpy
except ImportError as exc:
    sys.stderr.write("[geokit] Failed to import ArcMap arcpy: %s\n" % exc)
    sys.exit(2)

arcpy.env.overwriteOutput = False

VECTOR_EXTS = set([".shp"])
RASTER_EXTS = set([".tif", ".tiff", ".img", ".jp2", ".ecw", ".asc", ".hgt"])
ALLOWED_INTERP_METHODS = set(["idw", "spline", "kriging"])
ALLOWED_INDEX_TYPES = set(["ndvi", "ndwi", "ndbi", "savi", "nbr"])


def _norm_path(p):
    if not p:
        return ""
    try:
        return os.path.normcase(os.path.abspath(str(p)))
    except Exception:
        return str(p).lower()


def _is_overwrite(payload):
    return bool(payload.get("overwrite")) or os.environ.get("GEOKIT_OVERWRITE") in ("1", "true", "TRUE")


def _check_safe_output(out_path, input_paths, overwrite):
    """Validates that output never collides with any input and respects Safe Mode (no overwrite by default)."""
    if not out_path:
        return None
    norm_out = _norm_path(out_path)
    for inp in input_paths:
        if inp and _norm_path(inp) == norm_out:
            return "Refusing to overwrite input/reference dataset: %s" % out_path
    if not overwrite and (os.path.exists(out_path) or arcpy.Exists(out_path)):
        return "Output already exists: %s. Pass --overwrite to replace it." % out_path
    return None


def _compile_safe_math(expr):
    import ast
    tree = ast.parse(expr, mode="eval")
    allowed_nodes = (
        ast.Expression, ast.Num, ast.Str, ast.Name, ast.Load,
        ast.UnaryOp, ast.UAdd, ast.USub, ast.Not,
        ast.BinOp, ast.Add, ast.Sub, ast.Mult, ast.Div, ast.FloorDiv, ast.Mod, ast.Pow,
        ast.BoolOp, ast.And, ast.Or,
        ast.Compare, ast.Eq, ast.NotEq, ast.Lt, ast.LtE, ast.Gt, ast.GtE,
        ast.IfExp, ast.Call,
    )
    if hasattr(ast, "Constant"):
        allowed_nodes = allowed_nodes + (ast.Constant,)
    for node in ast.walk(tree):
        if not isinstance(node, allowed_nodes):
            raise ValueError("Disallowed expression syntax: %s" % type(node).__name__)
        if isinstance(node, ast.Name) and node.id.startswith("_"):
            raise ValueError("Disallowed identifier in expression: %s" % node.id)
        if isinstance(node, ast.Call):
            if not isinstance(node.func, ast.Name) or node.keywords or getattr(node, "starargs", None) or getattr(node, "kwargs", None):
                raise ValueError("Only direct calls to safe math functions are allowed.")
    return tree


def _eval_safe_math_node(node, env, funcs):
    import ast
    import operator
    if isinstance(node, ast.Expression):
        return _eval_safe_math_node(node.body, env, funcs)
    if hasattr(ast, "Constant") and isinstance(node, ast.Constant):
        return node.value
    if isinstance(node, ast.Num):
        return node.n
    if isinstance(node, ast.Str):
        return node.s
    if isinstance(node, ast.Name):
        if node.id in env:
            return env[node.id]
        raise ValueError("Unknown field or variable '%s' in expression." % node.id)
    if isinstance(node, ast.UnaryOp):
        v = _eval_safe_math_node(node.operand, env, funcs)
        if isinstance(node.op, ast.UAdd):
            return +v
        if isinstance(node.op, ast.USub):
            return -v
        if isinstance(node.op, ast.Not):
            return not v
    if isinstance(node, ast.BinOp):
        l = _eval_safe_math_node(node.left, env, funcs)
        r = _eval_safe_math_node(node.right, env, funcs)
        ops = {
            ast.Add: operator.add, ast.Sub: operator.sub,
            ast.Mult: operator.mul, ast.Div: operator.truediv,
            ast.FloorDiv: operator.floordiv, ast.Mod: operator.mod,
            ast.Pow: operator.pow,
        }
        return ops[type(node.op)](l, r)
    if isinstance(node, ast.BoolOp):
        if isinstance(node.op, ast.And):
            res = True
            for v in node.values:
                res = _eval_safe_math_node(v, env, funcs)
                if not res:
                    return res
            return res
        res = False
        for v in node.values:
            res = _eval_safe_math_node(v, env, funcs)
            if res:
                return res
        return res
    if isinstance(node, ast.Compare):
        left = _eval_safe_math_node(node.left, env, funcs)
        cmps = {
            ast.Eq: operator.eq, ast.NotEq: operator.ne,
            ast.Lt: operator.lt, ast.LtE: operator.le,
            ast.Gt: operator.gt, ast.GtE: operator.ge,
        }
        for op, comp in zip(node.ops, node.comparators):
            right = _eval_safe_math_node(comp, env, funcs)
            if not cmps[type(op)](left, right):
                return False
            left = right
        return True
    if isinstance(node, ast.IfExp):
        return _eval_safe_math_node(node.body, env, funcs) if _eval_safe_math_node(node.test, env, funcs) else _eval_safe_math_node(node.orelse, env, funcs)
    if isinstance(node, ast.Call):
        fn = funcs.get(node.func.id)
        if fn is None:
            raise ValueError("Disallowed function '%s' in expression." % node.func.id)
        args = [_eval_safe_math_node(a, env, funcs) for a in node.args]
        return fn(*args)
    raise ValueError("Unsupported expression node: %s" % type(node).__name__)


def sanitize_gdb_name(name):
    stem = os.path.splitext(os.path.basename(name))[0]
    clean = re.sub(r"[^a-zA-Z0-9_]+", "_", stem).strip("_").lower()
    if not clean:
        clean = "layer"
    if clean[0].isdigit():
        clean = "l_" + clean
    return clean


def is_gdb(path):
    return path.lower().endswith(".gdb") and os.path.isdir(path)


def collect_datasets(input_path, include_rasters=True):
    items = []
    if os.path.isfile(input_path):
        ext = os.path.splitext(input_path)[1].lower()
        if ext in RASTER_EXTS:
            if include_rasters:
                items.append((input_path, "raster"))
        else:
            items.append((input_path, "vector"))
        return items

    if is_gdb(input_path):
        arcpy.env.workspace = input_path
        for fc in arcpy.ListFeatureClasses() or []:
            items.append((os.path.join(input_path, fc), "vector"))
        for ds in arcpy.ListDatasets("", "Feature") or []:
            for fc in arcpy.ListFeatureClasses(feature_dataset=ds) or []:
                items.append((os.path.join(input_path, ds, fc), "vector"))
        if include_rasters:
            for r in arcpy.ListRasters() or []:
                items.append((os.path.join(input_path, r), "raster"))
        return items

    if os.path.isdir(input_path):
        for root, dirs, files in os.walk(input_path):
            dirs[:] = [d for d in dirs if not d.lower().endswith(".gdb")]
            for fname in sorted(files):
                ext = os.path.splitext(fname)[1].lower()
                full = os.path.join(root, fname)
                if ext in VECTOR_EXTS:
                    items.append((full, "vector"))
                elif include_rasters and ext in RASTER_EXTS:
                    items.append((full, "raster"))
        return items

    return items


def ensure_output_container(out_path, is_batch):
    if out_path.lower().endswith(".gdb"):
        if not os.path.exists(out_path):
            parent = os.path.dirname(out_path) or "."
            if not os.path.exists(parent):
                os.makedirs(parent)
            arcpy.CreateFileGDB_management(parent, os.path.basename(out_path))
        return "gdb"

    if is_batch or os.path.isdir(out_path) or not os.path.splitext(out_path)[1]:
        if not os.path.exists(out_path):
            os.makedirs(out_path)
        return "dir"

    parent = os.path.dirname(out_path)
    if parent and not os.path.exists(parent):
        os.makedirs(parent)
    return "file"


def resolve_out_target(src_path, kind, out_path, container_type):
    if container_type == "file":
        return out_path
    if container_type == "gdb":
        return os.path.join(out_path, sanitize_gdb_name(src_path))
    stem = os.path.splitext(os.path.basename(src_path))[0]
    ext = os.path.splitext(src_path)[1]
    if not ext:
        ext = ".tif" if kind == "raster" else ".shp"
    return os.path.join(out_path, stem + ext)


def collect_mxd_files(input_path):
    if os.path.isfile(input_path):
        return [input_path] if input_path.lower().endswith(".mxd") else []
    mxd_files = []
    for root, dirs, files in os.walk(input_path):
        dirs[:] = [d for d in dirs if not d.lower().endswith(".gdb")]
        for fname in sorted(files):
            if fname.lower().endswith(".mxd"):
                mxd_files.append(os.path.join(root, fname))
    return mxd_files


def cmd_info(payload):
    items = collect_datasets(payload["input"], include_rasters=True)
    results = []
    for ds_path, kind in items:
        desc = arcpy.Describe(ds_path)
        sr = getattr(desc, "spatialReference", None)
        entry = {
            "name": os.path.basename(ds_path),
            "path": ds_path,
            "kind": kind,
            "crs": sr.name if sr else "Unknown",
            "epsg": sr.factoryCode if (sr and getattr(sr, "factoryCode", 0)) else None,
        }
        if kind == "vector":
            entry["geometry_type"] = getattr(desc, "shapeType", "Unknown")
            entry["feature_count"] = int(arcpy.GetCount_management(ds_path).getOutput(0))
            entry["fields"] = [f.name for f in arcpy.ListFields(ds_path)]
        else:
            entry["band_count"] = getattr(desc, "bandCount", None)
            entry["pixel_type"] = getattr(desc, "pixelType", None)
        results.append(entry)

    if payload.get("json"):
        print(json.dumps(results, indent=2))
        return 0
    if not results:
        print("No GIS datasets found.")
        return 0
    for item in results:
        epsg_str = ("EPSG:%s" % item["epsg"]) if item["epsg"] else "No EPSG"
        if item["kind"] == "vector":
            print(
                "- %s [%s] | %s features | %s (%s)"
                % (item["name"], item["geometry_type"], item["feature_count"], item["crs"], epsg_str)
            )
            print("    Fields: %s" % ", ".join(item["fields"]))
        else:
            print(
                "- %s [Raster, %s band(s)] | %s (%s)"
                % (item["name"], item["band_count"], item["crs"], epsg_str)
            )
    return 0


def cmd_project(payload):
    items = collect_datasets(payload["input"], include_rasters=True)
    if not items:
        print("No datasets found to project.")
        return 0
    sr = arcpy.SpatialReference(int(payload["epsg"]))
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, (src, kind) in enumerate(items, 1):
        dst = resolve_out_target(src, kind, payload["out"], ctype)
        try:
            if kind == "vector":
                arcpy.Project_management(src, dst, sr)
            else:
                arcpy.ProjectRaster_management(src, dst, sr)
            print("[%d/%d] OK   %s -> %s" % (idx, len(items), os.path.basename(src), dst))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    print("Summary: %d succeeded, %d failed." % (len(items) - failures, failures))
    return 1 if failures else 0


def cmd_clip(payload):
    items = collect_datasets(payload["input"], include_rasters=True)
    if not items:
        print("No datasets found to clip.")
        return 0
    aoi = payload["aoi"]
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, (src, kind) in enumerate(items, 1):
        dst = resolve_out_target(src, kind, payload["out"], ctype)
        try:
            if kind == "vector":
                arcpy.Clip_analysis(src, aoi, dst)
            else:
                arcpy.Clip_management(src, "#", dst, aoi, "#", "ClippingGeometry", "NO_MAINTAIN_EXTENT")
            print("[%d/%d] OK   %s -> %s" % (idx, len(items), os.path.basename(src), dst))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    print("Summary: %d succeeded, %d failed." % (len(items) - failures, failures))
    return 1 if failures else 0


def cmd_merge(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if len(items) < 2:
        sys.stderr.write("Need at least 2 vector datasets to merge.\n")
        return 1
    ensure_output_container(payload["out"], is_batch=False)
    arcpy.Merge_management(items, payload["out"])
    print("OK Merged %d dataset(s) -> %s" % (len(items), payload["out"]))
    return 0


def cmd_pack(payload):
    include_rasters = bool(payload.get("include_rasters", False))
    items = collect_datasets(payload["input"], include_rasters=include_rasters)
    if not items:
        print("No datasets found to pack.")
        return 0
    out_gdb = payload["out"]
    if not out_gdb.lower().endswith(".gdb"):
        out_gdb = out_gdb + ".gdb"
    ensure_output_container(out_gdb, is_batch=True)
    failures = 0
    for idx, (src, kind) in enumerate(items, 1):
        name = sanitize_gdb_name(src)
        try:
            if kind == "vector":
                arcpy.FeatureClassToFeatureClass_conversion(src, out_gdb, name)
            else:
                arcpy.RasterToGeodatabase_conversion(src, out_gdb)
            print("[%d/%d] OK   %s -> %s\\%s" % (idx, len(items), os.path.basename(src), out_gdb, name))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    print("Summary: %d packed, %d failed." % (len(items) - failures, failures))
    return 1 if failures else 0


def cmd_unpack(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No feature classes found to unpack.")
        return 0
    out_dir = payload["out"]
    if not os.path.exists(out_dir):
        os.makedirs(out_dir)
    failures = 0
    for idx, src in enumerate(items, 1):
        name = os.path.basename(src)
        try:
            arcpy.FeatureClassToShapefile_conversion(src, out_dir)
            print("[%d/%d] OK   %s -> %s\\%s.shp" % (idx, len(items), name, out_dir, name))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), name, exc))
    print("Summary: %d unpacked, %d failed." % (len(items) - failures, failures))
    return 1 if failures else 0


def cmd_repair(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    failures = 0
    for idx, src in enumerate(items, 1):
        try:
            arcpy.RepairGeometry_management(src, "DELETE_NULL")
            print("[%d/%d] OK   Repaired %s" % (idx, len(items), os.path.basename(src)))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    print("Summary: %d repaired, %d failed." % (len(items) - failures, failures))
    return 1 if failures else 0


def cmd_buffer(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    dissolve_opt = "ALL" if payload.get("dissolve") else "NONE"
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.Buffer_analysis(src, dst, payload["dist"], dissolve_option=dissolve_opt)
            print("[%d/%d] OK   %s -> %s" % (idx, len(items), os.path.basename(src), dst))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    return 1 if failures else 0


def cmd_dissolve(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    fields = [f.strip() for f in payload["field"].split(",") if f.strip()]
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.Dissolve_management(src, dst, fields)
            print("[%d/%d] OK   %s -> %s" % (idx, len(items), os.path.basename(src), dst))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    return 1 if failures else 0


def _safe_shape_type(src):
    try:
        return arcpy.Describe(src).shapeType
    except Exception:
        tmp = "geokit_desc_tmp_lyr"
        if arcpy.Exists(tmp):
            arcpy.Delete_management(tmp)
        arcpy.MakeFeatureLayer_management(src, tmp)
        try:
            return arcpy.Describe(tmp).shapeType
        finally:
            if arcpy.Exists(tmp):
                arcpy.Delete_management(tmp)


def cmd_calc_geom(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    unit_req = (payload.get("unit") or "").lower()
    custom_field = payload.get("field")
    failures = 0
    for idx, src in enumerate(items, 1):
        try:
            shape_type = _safe_shape_type(src)
            if shape_type == "Polygon":
                unit = unit_req or "ha"
                area_map = {
                    "ha": ("!shape.geodesicArea@hectares!", "AREA_HA"),
                    "m2": ("!shape.geodesicArea@squaremeters!", "AREA_M2"),
                    "km2": ("!shape.geodesicArea@squarekilometers!", "AREA_KM2"),
                }
                expr, default_field = area_map[unit]
                field_name = custom_field or default_field
            elif shape_type == "Polyline":
                unit = unit_req or "km"
                len_map = {
                    "km": ("!shape.geodesicLength@kilometers!", "LEN_KM"),
                    "m": ("!shape.geodesicLength@meters!", "LEN_M"),
                }
                expr, default_field = len_map[unit]
                field_name = custom_field or default_field
            else:
                print("[%d/%d] SKIP %s (%s)" % (idx, len(items), os.path.basename(src), shape_type))
                continue
            existing = [f.name.upper() for f in arcpy.ListFields(src)]
            if field_name.upper() not in existing:
                arcpy.AddField_management(src, field_name, "DOUBLE")
            arcpy.CalculateField_management(src, field_name, expr, "PYTHON_9.3")
            print("[%d/%d] OK   %s -> calculated %s" % (idx, len(items), os.path.basename(src), field_name))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    return 1 if failures else 0


def cmd_intersect(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    overlay = payload["overlay"]
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.Intersect_analysis([src, overlay], dst)
            print("[%d/%d] OK   %s -> %s" % (idx, len(items), os.path.basename(src), dst))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    return 1 if failures else 0


def cmd_erase(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    mask = payload["overlay"]
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.Erase_analysis(src, mask, dst)
            print("[%d/%d] OK   %s -> %s" % (idx, len(items), os.path.basename(src), dst))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    return 1 if failures else 0


def cmd_spatial_join(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    join_fc = payload["overlay"]
    match_opt = (payload.get("match_option") or "intersect").upper()
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.SpatialJoin_analysis(src, join_fc, dst, match_option=match_opt)
            print("[%d/%d] OK   %s -> %s" % (idx, len(items), os.path.basename(src), dst))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    return 1 if failures else 0


def cmd_layout_list(payload):
    mxd_files = collect_mxd_files(payload["input"])
    results = []
    for mxd_path in mxd_files:
        mxd = arcpy.mapping.MapDocument(mxd_path)
        results.append({
            "project": os.path.basename(mxd_path),
            "project_path": mxd_path,
            "layout": mxd.title or os.path.splitext(os.path.basename(mxd_path))[0],
            "page_width": mxd.pageSize.width,
            "page_height": mxd.pageSize.height,
        })
        del mxd
    if payload.get("json"):
        print(json.dumps(results, indent=2))
        return 0
    for item in results:
        print("- [%s] %s (%s x %s)" % (item["project"], item["layout"], item["page_width"], item["page_height"]))
    return 0


def cmd_layout_export(payload):
    mxd_files = collect_mxd_files(payload["input"])
    out_dir = payload["out"]
    if not os.path.exists(out_dir):
        os.makedirs(out_dir)
    fmt = (payload.get("format") or "pdf").lower()
    dpi = int(payload.get("dpi") or 300)
    ext = "jpg" if fmt == "jpeg" else fmt
    exported = 0
    failures = 0
    for mxd_path in mxd_files:
        stem = sanitize_gdb_name(mxd_path)
        out_file = os.path.join(out_dir, "%s.%s" % (stem, ext))
        try:
            mxd = arcpy.mapping.MapDocument(mxd_path)
            if ext == "pdf":
                arcpy.mapping.ExportToPDF(mxd, out_file, resolution=dpi)
            elif ext == "png":
                arcpy.mapping.ExportToPNG(mxd, out_file, resolution=dpi)
            elif ext == "jpg":
                arcpy.mapping.ExportToJPEG(mxd, out_file, resolution=dpi)
            del mxd
            exported += 1
            print("[OK]  %s -> %s" % (os.path.basename(mxd_path), out_file))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[ERR] %s: %s\n" % (os.path.basename(mxd_path), exc))
    print("Summary: %d MXD(s) exported (%d DPI, %s), %d failed." % (exported, dpi, fmt.upper(), failures))
    return 1 if failures else 0


def cmd_map_broken_links(payload):
    mxd_files = collect_mxd_files(payload["input"])
    broken = []
    for mxd_path in mxd_files:
        mxd = arcpy.mapping.MapDocument(mxd_path)
        for lyr in arcpy.mapping.ListBrokenDataSources(mxd):
            ds = ""
            try:
                ds = lyr.dataSource
            except Exception:
                ds = "(unavailable)"
            broken.append({
                "project": os.path.basename(mxd_path),
                "project_path": mxd_path,
                "layer": lyr.longName,
                "data_source": ds,
            })
        del mxd
    if payload.get("json"):
        print(json.dumps(broken, indent=2))
        return 0
    if not broken:
        print("All MXD layers have valid data sources! No broken links found.")
        return 0
    for item in broken:
        print("[BROKEN] %s | Layer: %s | Source: %s" % (item["project"], item["layer"], item["data_source"]))
    return 0


def cmd_map_fix_links(payload):
    mxd_path = payload["input"]
    mxd = arcpy.mapping.MapDocument(mxd_path)
    mxd.findAndReplaceWorkspacePaths(payload["find"], payload["replace"])
    out_path = payload.get("out")
    if out_path:
        parent = os.path.dirname(out_path)
        if parent and not os.path.exists(parent):
            os.makedirs(parent)
        mxd.saveACopy(out_path)
        print("OK Updated MXD workspace paths and saved copy -> %s" % out_path)
    else:
        mxd.save()
        print("OK Updated MXD workspace paths and saved -> %s" % mxd_path)
    del mxd
    return 0


def checkout_spatial_analyst():
    if arcpy.CheckExtension("Spatial") != "Available":
        raise RuntimeError(
            "ArcMap Spatial Analyst extension is not available/licensed. "
            "Tip: For slope/aspect/hillshade or mosaic without Spatial Analyst, use `--engine gdal`."
        )
    arcpy.CheckOutExtension("Spatial")


def cmd_raster_mosaic(payload):
    rasters = [p for p, kind in collect_datasets(payload["input"], include_rasters=True) if kind == "raster"]
    if not rasters:
        sys.stderr.write("No raster datasets found to mosaic.\n")
        return 1
    out_path = payload["out"]
    out_dir = os.path.dirname(out_path) or "."
    if not os.path.exists(out_dir):
        os.makedirs(out_dir)
    out_name = os.path.basename(out_path)
    desc = arcpy.Describe(rasters[0])
    bands = getattr(desc, "bandCount", 1) or 1
    arcpy.MosaicToNewRaster_management(
        ";".join(rasters), out_dir, out_name, "", "32_BIT_FLOAT", "", bands, "LAST", "FIRST"
    )
    print("OK Mosaicked %d raster(s) -> %s" % (len(rasters), out_path))
    return 0


def cmd_raster_terrain(payload):
    checkout_spatial_analyst()
    from arcpy import sa

    dem = payload["input"]
    out_dir = payload["out"]
    if not os.path.exists(out_dir):
        os.makedirs(out_dir)
    stem = sanitize_gdb_name(dem)

    if payload.get("slope"):
        dst = os.path.join(out_dir, "%s_slope.tif" % stem)
        sa.Slope(dem, "DEGREE").save(dst)
        print("[OK] Slope     -> %s" % dst)
    if payload.get("aspect"):
        dst = os.path.join(out_dir, "%s_aspect.tif" % stem)
        sa.Aspect(dem).save(dst)
        print("[OK] Aspect    -> %s" % dst)
    if payload.get("hillshade"):
        dst = os.path.join(out_dir, "%s_hillshade.tif" % stem)
        sa.Hillshade(dem).save(dst)
        print("[OK] Hillshade -> %s" % dst)
    return 0


def cmd_raster_zonal_stats(payload):
    checkout_spatial_analyst()
    from arcpy import sa

    raster = payload["input"]
    zones = payload["zones"]
    zone_field = payload["zone_field"]
    out_path = payload["out"]
    parent = os.path.dirname(out_path) or "."
    if not os.path.exists(parent):
        os.makedirs(parent)

    if out_path.lower().endswith(".csv"):
        tmp_tbl = os.path.join(arcpy.env.scratchGDB, "geokit_zonal_tmp")
        sa.ZonalStatisticsAsTable(zones, zone_field, raster, tmp_tbl, "DATA", "ALL")
        arcpy.TableToTable_conversion(tmp_tbl, parent, os.path.basename(out_path))
        arcpy.Delete_management(tmp_tbl)
    else:
        sa.ZonalStatisticsAsTable(zones, zone_field, raster, out_path, "DATA", "ALL")
    print("OK Zonal statistics saved -> %s" % out_path)
    return 0


def cmd_hydro_delineate(payload):
    checkout_spatial_analyst()
    from arcpy import sa

    dem = payload["dem"]
    out_dir = payload["out"]
    threshold = int(payload.get("threshold") or 1000)
    pour_point = payload.get("pour_point")
    snap_dist = float(payload.get("snap_dist") or 100.0)
    if not os.path.exists(out_dir):
        os.makedirs(out_dir)

    print("[1/5] Filling DEM sinks...")
    filled = sa.Fill(dem)
    filled.save(os.path.join(out_dir, "01_dem_filled.tif"))

    print("[2/5] Computing D8 Flow Direction...")
    fdir = sa.FlowDirection(filled, "NORMAL")
    fdir.save(os.path.join(out_dir, "02_flow_dir.tif"))

    print("[3/5] Computing Flow Accumulation...")
    facc = sa.FlowAccumulation(fdir)
    facc.save(os.path.join(out_dir, "03_flow_acc.tif"))

    print("[4/5] Extracting Stream Network (threshold >= %d cells)..." % threshold)
    streams_shp = os.path.join(out_dir, "streams.shp")
    stream_ras = sa.Con(facc >= threshold, 1)
    streams_ok = False
    try:
        sa.StreamToFeature(stream_ras, fdir, streams_shp, "SIMPLIFY")
        if arcpy.Exists(streams_shp) and int(arcpy.GetCount_management(streams_shp).getOutput(0)) > 0:
            streams_ok = True
            print("      -> %s" % streams_shp)
        else:
            print("      [WARN] Stream feature class is empty at threshold %d (try a lower --threshold)." % threshold)
    except Exception as exc:
        print("      [WARN] No streams extracted at threshold %d (%s)" % (threshold, exc))

    if pour_point:
        print("[5/5] Snapping pour point (%s map units) & delineating watershed..." % snap_dist)
        snapped = sa.SnapPourPoint(pour_point, facc, snap_dist)
        ws_ras = sa.Watershed(fdir, snapped)
        ws_ras.save(os.path.join(out_dir, "watershed.tif"))
        ws_shp = os.path.join(out_dir, "watershed.shp")
        arcpy.RasterToPolygon_conversion(ws_ras, ws_shp, "SIMPLIFY", "VALUE")
        print("      -> %s" % ws_shp)
    else:
        print("[5/5] No --pour-point provided; skipping single-outlet watershed polygon.")

    if streams_ok:
        print("OK Hydrology delineation complete -> %s" % out_dir)
    else:
        print("PARTIAL Hydrology outputs written to %s, but stream network was NOT extracted (lower --threshold)." % out_dir)
    return 0


def cmd_hydro_basin(payload):
    checkout_spatial_analyst()
    from arcpy import sa

    dem = payload["dem"]
    out_dir = payload["out"]
    if not os.path.exists(out_dir):
        os.makedirs(out_dir)

    print("[1/4] Filling DEM sinks...")
    filled = sa.Fill(dem)
    filled.save(os.path.join(out_dir, "dem_filled.tif"))

    print("[2/4] Computing D8 Flow Direction...")
    fdir = sa.FlowDirection(filled, "NORMAL")
    fdir.save(os.path.join(out_dir, "flow_dir.tif"))

    print("[3/4] Delineating all drainage basins...")
    basin_ras = sa.Basin(fdir)
    basin_ras.save(os.path.join(out_dir, "basins.tif"))

    print("[4/4] Converting basins to polygon shapefile...")
    basins_shp = os.path.join(out_dir, "basins.shp")
    arcpy.RasterToPolygon_conversion(basin_ras, basins_shp, "SIMPLIFY", "VALUE")
    print("OK Drainage basins saved -> %s" % basins_shp)
    return 0


def auto_checkout_extension(tool_name):
    lower = tool_name.lower()
    for suffix, ext_name in [("_sa", "Spatial"), ("_3d", "3D"), ("_na", "Network"), ("_ga", "GeoStats")]:
        if lower.endswith(suffix):
            try:
                if arcpy.CheckExtension(ext_name) == "Available":
                    arcpy.CheckOutExtension(ext_name)
            except Exception:
                pass
            return


def cmd_gp_tools(payload):
    pattern = payload.get("input") or "*"
    if "*" not in pattern:
        pattern = "*%s*" % pattern
    tools = sorted(arcpy.ListTools(pattern) or [])
    if payload.get("json"):
        print(json.dumps({"pattern": pattern, "count": len(tools), "tools": tools}, indent=2))
        return 0
    print("Found %d ArcMap tool(s) matching '%s':" % (len(tools), pattern))
    for t in tools[:150]:
        print("  - %s" % t)
    return 0


def cmd_gp_tool_usage(payload):
    print(arcpy.Usage(payload.get("tool") or ""))
    return 0


def cmd_gp_run(payload):
    tool = (payload.get("tool") or "").strip()
    args = payload.get("args") or []
    if not tool or tool.startswith("_") or not tool[0].isalpha() or not all(c.isalnum() or c == "_" for c in tool):
        sys.stderr.write("Invalid ArcPy tool name '%s'.\n" % tool)
        return 1
    auto_checkout_extension(tool)
    fn = getattr(arcpy, tool, None)
    if fn is None:
        fn = getattr(arcpy.gp, tool, None)
    if fn is None or not callable(fn) or isinstance(fn, type):
        sys.stderr.write("Unknown ArcPy tool '%s'. Run `geokit gp tools <keyword>` to search.\n" % tool)
        return 1
    # Normalize relative filesystem paths so ArcMap 10.x tools (e.g. CopyRaster) never reject them
    norm_args = []
    for a in args:
        s = str(a)
        if ("\\" in s or "/" in s) and not os.path.isabs(s) and not s.startswith(("http://", "https://")):
            norm_args.append(os.path.abspath(s))
        else:
            norm_args.append(a)
    res = fn(*norm_args)
    out_val = res.getOutput(0) if res else ""
    print("OK %s completed%s" % (tool, (" -> " + out_val) if out_val else ""))
    return 0


def cmd_xy2shp(payload):
    in_csv = payload["input"]
    out_path = payload["out"]
    x_field = payload.get("x_field") or "lon"
    y_field = payload.get("y_field") or "lat"
    epsg = int(payload.get("epsg") or 4326)
    ensure_output_container(out_path, is_batch=False)
    sr = arcpy.SpatialReference(epsg)
    tmp_lyr = "geokit_xy_event_lyr"
    arcpy.MakeXYEventLayer_management(in_csv, x_field, y_field, tmp_lyr, sr)
    arcpy.CopyFeatures_management(tmp_lyr, out_path)
    print("OK Created point dataset from CSV (%s, %s, EPSG:%d) -> %s" % (x_field, y_field, epsg, out_path))
    return 0


def cmd_convert(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    fmt = (payload.get("target_format") or "shp").lower()
    out_path = payload["out"]
    is_batch = len(items) > 1 or os.path.isdir(out_path) or not os.path.splitext(out_path)[1]
    if is_batch and not os.path.exists(out_path):
        os.makedirs(out_path)
    failures = 0
    for idx, src in enumerate(items, 1):
        stem = sanitize_gdb_name(src)
        dst = os.path.join(out_path, "%s.%s" % (stem, fmt)) if is_batch else out_path
        try:
            if fmt == "shp":
                arcpy.CopyFeatures_management(src, dst)
            elif fmt == "geojson":
                arcpy.FeaturesToJSON_conversion(src, dst, geoJSON="GEOJSON")
            else:
                raise ValueError("Format '%s' via ArcMap requires `--engine gdal`." % fmt)
            print("[%d/%d] OK   %s -> %s" % (idx, len(items), os.path.basename(src), dst))
        except Exception as exc:
            failures += 1
            sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    return 1 if failures else 0


def cmd_qa_check(payload):
    items = collect_datasets(payload["input"], include_rasters=True)
    expected_epsg = payload.get("expected_epsg")
    check_nulls = [f.strip() for f in (payload.get("check_nulls") or "").split(",") if f.strip()]
    check_overlaps = bool(payload.get("check_overlaps", False))

    report = []
    crs_set = set()
    total_issues = 0

    for ds_path, kind in items:
        desc = arcpy.Describe(ds_path)
        sr = getattr(desc, "spatialReference", None)
        epsg = sr.factoryCode if (sr and getattr(sr, "factoryCode", 0)) else None
        crs_name = sr.name if sr else "Unknown"
        crs_set.add("%s (%s)" % (crs_name, epsg))
        issues = []

        if not sr or crs_name == "Unknown":
            issues.append("Missing/Unknown CRS")
        elif expected_epsg and epsg != int(expected_epsg):
            issues.append("CRS mismatch: got EPSG:%s, expected EPSG:%s" % (epsg, expected_epsg))

        if kind == "vector":
            try:
                chk_tbl = os.path.join(arcpy.env.scratchGDB, "geokit_qa_geom")
                if arcpy.Exists(chk_tbl):
                    arcpy.Delete_management(chk_tbl)
                arcpy.CheckGeometry_management(ds_path, chk_tbl)
                bad_count = int(arcpy.GetCount_management(chk_tbl).getOutput(0))
                if bad_count > 0:
                    issues.append("%d invalid geometry problem(s)" % bad_count)
                arcpy.Delete_management(chk_tbl)
            except Exception as exc:
                issues.append("CheckGeometry skipped: %s" % exc)

            if check_nulls:
                existing_fields = dict((f.name.lower(), f.name) for f in arcpy.ListFields(ds_path))
                for req_f in check_nulls:
                    actual = existing_fields.get(req_f.lower())
                    if not actual:
                        issues.append("Missing required field '%s'" % req_f)
                    else:
                        null_cnt = 0
                        with arcpy.da.SearchCursor(ds_path, [actual]) as cur:
                            for row in cur:
                                val = row[0]
                                if val is None or (isinstance(val, basestring) and not val.strip()):
                                    null_cnt += 1
                        if null_cnt > 0:
                            issues.append("Field '%s' has %d NULL/empty value(s)" % (actual, null_cnt))

            if check_overlaps and getattr(desc, "shapeType", "") == "Polygon":
                try:
                    polys = []
                    with arcpy.da.SearchCursor(ds_path, ["OID@", "SHAPE@"]) as cur:
                        for oid, geom in cur:
                            if geom is not None and geom.area > 0:
                                ext = geom.extent
                                polys.append((oid, ext.XMin, ext.YMin, ext.XMax, ext.YMax, geom))
                    ov_count = 0
                    n_p = len(polys)
                    for i in range(n_p):
                        _, ax1, ay1, ax2, ay2, ga = polys[i]
                        for j in range(i + 1, n_p):
                            _, bx1, by1, bx2, by2, gb = polys[j]
                            if ax2 <= bx1 or bx2 <= ax1 or ay2 <= by1 or by2 <= ay1:
                                continue
                            if ga.overlaps(gb) or ga.contains(gb) or gb.contains(ga) or ga.equals(gb):
                                ov_count += 1
                    if ov_count > 0:
                        issues.append("%d overlapping polygon pair(s)" % ov_count)
                except Exception as exc:
                    issues.append("Overlap check skipped: %s" % exc)

        total_issues += len(issues)
        report.append({
            "name": os.path.basename(ds_path),
            "path": ds_path,
            "kind": kind,
            "crs": crs_name,
            "epsg": epsg,
            "status": "PASS" if not issues else "WARN",
            "issues": issues,
        })

    mixed_crs = len(crs_set) > 1
    strict = bool(payload.get("strict", False))
    exit_code = 1 if (strict and (total_issues > 0 or mixed_crs)) else 0

    if payload.get("json"):
        print(json.dumps({
            "scope": "technical_dataset_qa",
            "datasets_checked": len(report),
            "unique_crs_count": len(crs_set),
            "mixed_crs_warning": mixed_crs,
            "crs_list": sorted(crs_set),
            "total_issues": total_issues,
            "strict_failed": exit_code != 0,
            "results": report,
        }, indent=2))
        return exit_code

    print("Spatial Technical QA Report (%d dataset(s) checked — CRS/geometry/NULL/overlaps):" % len(report))
    if mixed_crs:
        print("  [WARN] Multiple CRS detected across workspace: %s" % ", ".join(sorted(crs_set)))
    for item in report:
        tag = "[PASS]" if item["status"] == "PASS" else "[WARN]"
        epsg_s = ("EPSG:%s" % item["epsg"]) if item["epsg"] else "No EPSG"
        print("  %s %s (%s / %s)" % (tag, item["name"], item["crs"], epsg_s))
        for iss in item["issues"]:
            print("         - %s" % iss)
    print("Summary: %d checked, %d issue(s) found." % (len(report), total_issues))
    return exit_code


def _cast_value(raw_str, field_type):
    if raw_str is None:
        return None
    ft = field_type.upper()
    if ft in ("LONG", "SHORT"):
        return int(float(raw_str))
    if ft in ("DOUBLE", "FLOAT"):
        return float(raw_str)
    return str(raw_str)


def _parse_classify_rules(rules_str, field_type=None):
    raw_rules = [r.strip() for r in rules_str.split(";") if r.strip()]
    if not raw_rules:
        raise ValueError("No valid rules found. Example: 'Hutan=1;Sawah=3' or '<=8:1;8-15:2;>15:3'")

    parsed = []
    out_samples = []
    for r in raw_rules:
        if "=" in r and not r.startswith("<=") and not r.startswith(">="):
            lhs, rhs = r.split("=", 1)
        elif ":" in r:
            lhs, rhs = r.split(":", 1)
        else:
            raise ValueError("Invalid rule syntax '%s'. Use 'key=val' or 'range:val'." % r)
        lhs = lhs.strip()
        rhs = rhs.strip()
        out_samples.append(rhs)

        if lhs.startswith("<="):
            parsed.append(("le", float(lhs[2:].strip()), None, rhs))
        elif lhs.startswith(">="):
            parsed.append(("ge", float(lhs[2:].strip()), None, rhs))
        elif lhs.startswith("<"):
            parsed.append(("lt", float(lhs[1:].strip()), None, rhs))
        elif lhs.startswith(">"):
            parsed.append(("gt", float(lhs[1:].strip()), None, rhs))
        else:
            range_match = None
            for idx in range(1, len(lhs)):
                if lhs[idx] == "-":
                    try:
                        lo = float(lhs[:idx].strip())
                        hi = float(lhs[idx + 1:].strip())
                        range_match = (lo, hi)
                        break
                    except ValueError:
                        pass
            if range_match is not None:
                parsed.append(("range", range_match[0], range_match[1], rhs))
            else:
                parsed.append(("eq", lhs.lower(), None, rhs))

    if not field_type:
        all_int = True
        all_float = True
        for s in out_samples:
            try:
                int(s)
            except ValueError:
                all_int = False
            try:
                float(s)
            except ValueError:
                all_float = False
        if all_int:
            field_type = "LONG"
        elif all_float:
            field_type = "DOUBLE"
        else:
            field_type = "TEXT"
    else:
        field_type = field_type.upper()

    typed_rules = [(op, a, b, _cast_value(rhs, field_type)) for op, a, b, rhs in parsed]
    return typed_rules, field_type


def _match_classify_rule(val, rules):
    if val is None:
        return None, False
    for op, a, b, out_val in rules:
        if op == "eq":
            if unicode(val).strip().lower() == a:
                return out_val, True
            try:
                if float(val) == float(a):
                    return out_val, True
            except (ValueError, TypeError):
                pass
        else:
            try:
                num = float(val)
            except (ValueError, TypeError):
                continue
            if op == "le" and num <= a:
                return out_val, True
            if op == "lt" and num < a:
                return out_val, True
            if op == "ge" and num >= a:
                return out_val, True
            if op == "gt" and num > a:
                return out_val, True
            if op == "range" and a <= num <= b:
                return out_val, True
    return None, False


def cmd_values(payload):
    import math
    from collections import Counter

    items = collect_datasets(payload["input"], include_rasters=True)
    target_field = payload.get("field")
    limit = int(payload.get("limit") or 50)
    results = []

    for ds_path, kind in items:
        entry = {"name": os.path.basename(ds_path), "path": ds_path, "kind": kind}
        if kind == "raster":
            try:
                arcpy.CalculateStatistics_management(ds_path)
            except Exception:
                pass
            stats = {}
            for prop in ("MINIMUM", "MAXIMUM", "MEAN", "STD"):
                try:
                    val = arcpy.GetRasterProperties_management(ds_path, prop).getOutput(0)
                    stats[prop.lower()] = float(val.replace(",", "."))
                except Exception:
                    stats[prop.lower()] = None
            entry["raster_stats"] = stats
        else:
            fields = [f for f in arcpy.ListFields(ds_path) if f.type not in ("Geometry", "OID", "Blob", "Raster")]
            if target_field:
                fields = [f for f in fields if f.name.lower() == target_field.lower()]
                if not fields:
                    sys.stderr.write("Field '%s' not found in %s\n" % (target_field, ds_path))
                    return 1
            field_names = [f.name for f in fields]
            numeric_types = set(["Integer", "SmallInteger", "Single", "Double"])
            counters = dict((f.name, Counter()) for f in fields)
            num_vals = dict((f.name, []) for f in fields if f.type in numeric_types)
            null_counts = dict((f.name, 0) for f in fields)
            total_rows = 0

            if field_names:
                with arcpy.da.SearchCursor(ds_path, field_names) as cur:
                    for row in cur:
                        total_rows += 1
                        for idx, f in enumerate(fields):
                            v = row[idx]
                            if v is None or (isinstance(v, basestring) and not v.strip()):
                                null_counts[f.name] += 1
                            else:
                                if f.type in numeric_types:
                                    num_vals[f.name].append(float(v))
                                counters[f.name][v] += 1

            field_summaries = []
            for f in fields:
                c = counters[f.name]
                fs = {
                    "field": f.name,
                    "type": f.type,
                    "total_rows": total_rows,
                    "null_count": null_counts[f.name],
                    "distinct_count": len(c),
                }
                if f.type in numeric_types and num_vals[f.name]:
                    vals = num_vals[f.name]
                    mean = sum(vals) / float(len(vals))
                    var = sum((x - mean) ** 2 for x in vals) / float(len(vals))
                    fs["min"] = min(vals)
                    fs["max"] = max(vals)
                    fs["mean"] = round(mean, 4)
                    fs["std"] = round(math.sqrt(var), 4)
                if len(c) <= limit or f.type not in numeric_types:
                    fs["top_values"] = [{"value": unicode(k), "count": cnt} for k, cnt in c.most_common(limit)]
                field_summaries.append(fs)
            entry["fields"] = field_summaries
        results.append(entry)

    if payload.get("json"):
        print(json.dumps(results, indent=2))
        return 0

    for item in results:
        print("Dataset: %s (%s)" % (item["name"], item["kind"]))
        if item["kind"] == "raster":
            st = item["raster_stats"]
            print("  Raster stats: min=%s  max=%s  mean=%s  std=%s" % (st["minimum"], st["maximum"], st["mean"], st["std"]))
        else:
            for fs in item["fields"]:
                head = "  - %s (%s): %d unique, %d null" % (fs["field"], fs["type"], fs["distinct_count"], fs["null_count"])
                if "min" in fs:
                    head += " | min=%s, max=%s, mean=%s, std=%s" % (fs["min"], fs["max"], fs["mean"], fs["std"])
                print(head)
                if fs.get("top_values"):
                    preview = ", ".join("%s (%d)" % (tv["value"], tv["count"]) for tv in fs["top_values"][:12])
                    more = (" ... (+%d more)" % (len(fs["top_values"]) - 12)) if len(fs["top_values"]) > 12 else ""
                    print("      Values: %s%s" % (preview, more))
    return 0


def cmd_classify(payload):
    ds_path = payload["input"]
    src_field = payload["field"]
    out_field = payload["out_field"]
    rules_str = payload["rules"]
    req_type = payload.get("field_type")
    default_raw = payload.get("default_value")

    rules, field_type = _parse_classify_rules(rules_str, req_type)
    default_val = _cast_value(default_raw, field_type) if default_raw is not None else None

    existing = dict((f.name.lower(), f.name) for f in arcpy.ListFields(ds_path))
    actual_src = existing.get(src_field.lower())
    if not actual_src:
        sys.stderr.write("Source field '%s' not found in %s\n" % (src_field, ds_path))
        return 1

    actual_out = existing.get(out_field.lower())
    if not actual_out:
        if field_type == "TEXT":
            arcpy.AddField_management(ds_path, out_field, "TEXT", field_length=64)
        else:
            arcpy.AddField_management(ds_path, out_field, field_type)
        actual_out = out_field

    matched = 0
    unmatched = 0
    with arcpy.da.UpdateCursor(ds_path, [actual_src, actual_out]) as cur:
        for row in cur:
            val, hit = _match_classify_rule(row[0], rules)
            if hit:
                row[1] = val
                matched += 1
            else:
                if default_val is not None:
                    row[1] = default_val
                unmatched += 1
            cur.updateRow(row)

    print("OK Classified '%s' -> '%s' (%s): %d matched, %d unmatched/defaulted." % (actual_src, actual_out, field_type, matched, unmatched))
    return 0


def cmd_calc_field(payload):
    import math

    ds_path = payload["input"]
    out_field = payload["field"]
    expr = payload["expr"]
    field_type = (payload.get("field_type") or "DOUBLE").upper()

    existing = dict((f.name.lower(), f.name) for f in arcpy.ListFields(ds_path))
    actual_out = existing.get(out_field.lower())
    if not actual_out:
        if field_type == "TEXT":
            arcpy.AddField_management(ds_path, out_field, "TEXT", field_length=128)
        else:
            arcpy.AddField_management(ds_path, out_field, field_type)
        actual_out = out_field

    all_fields = [f.name for f in arcpy.ListFields(ds_path) if f.type not in ("Geometry", "Blob", "Raster")]
    out_idx = all_fields.index(actual_out)
    safe_funcs = {
        "abs": abs, "min": min, "max": max, "round": round,
        "int": int, "float": float, "str": str, "len": len,
        "sqrt": math.sqrt, "log": math.log, "pow": math.pow,
    }
    try:
        expr_tree = _compile_safe_math(expr)
    except Exception as exc:
        sys.stderr.write("Invalid/unsafe calc-field expression '%s': %s\n" % (expr, exc))
        return 1

    updated = 0
    with arcpy.da.UpdateCursor(ds_path, all_fields) as cur:
        for row in cur:
            env = {}
            for idx, fname in enumerate(all_fields):
                v = row[idx]
                env[fname] = v
                env.setdefault(fname.upper(), v)
                env.setdefault(fname.lower(), v)
            val = _eval_safe_math_node(expr_tree, env, safe_funcs)
            row[out_idx] = _cast_value(val, field_type) if val is not None else None
            cur.updateRow(row)
            updated += 1

    print("OK Calculated field '%s' (%s) = %s on %d feature(s)." % (actual_out, field_type, expr, updated))
    return 0


def cmd_union(payload):
    raw_inputs = payload.get("inputs") or [payload["input"]]
    out_path = payload["out"]
    layers = []
    for item in raw_inputs:
        if os.path.isdir(item):
            for ds_p, kind in collect_datasets(item, include_rasters=False):
                if kind == "vector":
                    layers.append(ds_p)
        else:
            layers.append(item)

    if len(layers) < 2:
        sys.stderr.write("Union requires at least 2 vector layers (found %d).\n" % len(layers))
        return 1

    ensure_output_container(out_path, is_batch=False)
    arcpy.Union_analysis(layers, out_path, "ALL")
    cnt = int(arcpy.GetCount_management(out_path).getOutput(0))
    print("OK Unioned %d layer(s) -> %s (%d feature(s))" % (len(layers), out_path, cnt))
    return 0


def cmd_layout_update(payload):
    mxd_path = payload["input"]
    mxd = arcpy.mapping.MapDocument(mxd_path)
    title = payload.get("title")
    subtitle = payload.get("subtitle")
    source = payload.get("source")
    zoom_layer = payload.get("zoom_layer")

    updated = 0
    for el in arcpy.mapping.ListLayoutElements(mxd, "TEXT_ELEMENT"):
        ename = (el.name or "").lower()
        etext = (el.text or "").lower()
        if title and ("title" in ename or "judul" in ename or "title" in etext or "judul" in etext):
            el.text = title
            updated += 1
        elif subtitle and ("subtitle" in ename or "subjudul" in ename or "subtitle" in etext or "subjudul" in etext):
            el.text = subtitle
            updated += 1
        elif source and ("source" in ename or "sumber" in ename or "source" in etext or "sumber" in etext):
            el.text = source
            updated += 1

    if zoom_layer:
        dfs = arcpy.mapping.ListDataFrames(mxd)
        if dfs:
            df = dfs[0]
            target = zoom_layer.lower()
            for cand in arcpy.mapping.ListLayers(mxd, "", df):
                if cand.name.lower() == target or target in cand.name.lower():
                    df.extent = cand.getExtent(True)
                    break

    out_path = payload.get("out")
    if out_path:
        parent = os.path.dirname(out_path)
        if parent and not os.path.exists(parent):
            os.makedirs(parent)
        mxd.saveACopy(out_path)
        print("OK Updated MXD layout (%d text element(s)) -> %s" % (updated, out_path))
    else:
        mxd.save()
        print("OK Updated MXD layout (%d text element(s)) -> %s" % (updated, mxd_path))
    return 0


def _parse_remap_ranges(rules_str):
    import re
    ranges = []
    for raw in rules_str.split(";"):
        seg = raw.strip()
        if not seg:
            continue
        if ":" in seg:
            cond, val_s = seg.rsplit(":", 1)
            cond = cond.strip()
            cls_v = int(float(val_s.strip()))
            if cond.startswith("<="):
                ranges.append([-1e9, float(cond[2:].strip()), cls_v])
            elif cond.startswith("<"):
                ranges.append([-1e9, float(cond[1:].strip()), cls_v])
            elif cond.startswith(">="):
                ranges.append([float(cond[2:].strip()), 1e9, cls_v])
            elif cond.startswith(">"):
                ranges.append([float(cond[1:].strip()), 1e9, cls_v])
            else:
                m = re.match(r"^\s*([+-]?\d+(?:\.\d+)?)\s*(?:-|to|\.\.)\s*([+-]?\d+(?:\.\d+)?)\s*$", cond, re.I)
                if m:
                    ranges.append([float(m.group(1)), float(m.group(2)), cls_v])
                else:
                    v = float(cond)
                    ranges.append([v, v, cls_v])
        else:
            parts = seg.split()
            if len(parts) == 3:
                ranges.append([float(parts[0]), float(parts[1]), int(float(parts[2]))])
            elif len(parts) == 2:
                v = float(parts[0])
                ranges.append([v, v, int(float(parts[1]))])
            else:
                return None
    return ranges if ranges else None


def cmd_raster_reclass(payload):
    in_r = payload["input"]
    rules = payload["rules"]
    out_r = payload["out"]
    smooth = bool(payload.get("smooth", False))
    overwrite = _is_overwrite(payload)

    # Validate rules and output safety BEFORE touching any existing output file!
    ranges = _parse_remap_ranges(rules)
    if not rules or not str(rules).strip():
        sys.stderr.write("Reclassify rules cannot be empty.\n")
        return 1
    err = _check_safe_output(out_r, [in_r], overwrite)
    if err:
        sys.stderr.write(err + "\n")
        return 1

    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import Reclassify, RemapRange, MajorityFilter

    parent = os.path.dirname(out_r)
    if parent and not os.path.exists(parent):
        os.makedirs(parent)
    if overwrite and arcpy.Exists(out_r):
        arcpy.Delete_management(out_r)
    if ranges:
        rc = Reclassify(in_r, "Value", RemapRange(ranges), "NODATA")
        if smooth:
            MajorityFilter(rc, "EIGHT", "MAJORITY").save(out_r)
        else:
            rc.save(out_r)
    elif smooth:
        tmp_r = os.path.join(arcpy.env.scratchFolder, "geokit_reclass_raw.tif")
        arcpy.gp.Reclassify_sa(in_r, "Value", rules, tmp_r, "NODATA")
        arcpy.gp.MajorityFilter_sa(tmp_r, out_r, "EIGHT", "MAJORITY")
    else:
        arcpy.gp.Reclassify_sa(in_r, "Value", rules, out_r, "NODATA")
    print("OK Reclassified raster -> %s%s" % (out_r, " (smoothed)" if smooth else ""))
    return 0


def cmd_raster_weighted_sum(payload):
    import math

    pairs = payload.get("weighted_inputs") or []
    out_r = payload["out"]
    n_classes = payload.get("classes")
    poly_out = payload.get("polygon_out")
    smooth = bool(payload.get("smooth", False))
    overwrite = _is_overwrite(payload)

    if not pairs:
        sys.stderr.write("weighted-sum requires at least one raster:weight pair.\n")
        return 1
    in_paths = []
    w_sum = 0.0
    for r_path, weight in pairs:
        wf = float(weight)
        if math.isnan(wf) or math.isinf(wf) or wf < 0.0:
            sys.stderr.write("Invalid weight '%s' for %s (must be finite and >= 0).\n" % (weight, r_path))
            return 1
        w_sum += wf
        in_paths.append(r_path)
    if abs(w_sum - 1.0) > 0.01:
        sys.stderr.write("Note: Sum of weights is %.4f (expected 1.0 for normalized weighted overlay).\n" % w_sum)

    err = _check_safe_output(out_r, in_paths, overwrite)
    if err:
        sys.stderr.write(err + "\n")
        return 1
    if poly_out:
        err_p = _check_safe_output(poly_out, in_paths + [out_r], overwrite)
        if err_p:
            sys.stderr.write(err_p + "\n")
            return 1

    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import Raster, Reclassify, RemapRange, MajorityFilter

    parent = os.path.dirname(out_r)
    if parent and not os.path.exists(parent):
        os.makedirs(parent)
    if overwrite and arcpy.Exists(out_r):
        arcpy.Delete_management(out_r)

    total = None
    for r_path, weight in pairs:
        term = Raster(r_path) * float(weight)
        total = term if total is None else (total + term)
    total.save(out_r)
    print("OK Weighted sum (%d rasters) -> %s" % (len(pairs), out_r))

    if n_classes or poly_out:
        n_cls = int(n_classes or 5)
        try:
            arcpy.CalculateStatistics_management(out_r)
        except Exception:
            pass
        rmin = float(arcpy.GetRasterProperties_management(out_r, "MINIMUM").getOutput(0).replace(",", "."))
        rmax = float(arcpy.GetRasterProperties_management(out_r, "MAXIMUM").getOutput(0).replace(",", "."))
        if rmax <= rmin:
            ranges = [[rmin - 0.1, rmax + 0.1, 1]]
        else:
            step = (rmax - rmin) / float(n_cls)
            ranges = []
            for i in range(n_cls):
                lo = rmin + i * step - (0.001 if i == 0 else 0.0)
                hi = rmax + 0.001 if i == n_cls - 1 else (rmin + (i + 1) * step)
                ranges.append([lo, hi, i + 1])
        cls_tif = os.path.join(arcpy.env.scratchFolder, "geokit_wsum_cls.tif")
        if arcpy.Exists(cls_tif):
            arcpy.Delete_management(cls_tif)
        Reclassify(out_r, "Value", RemapRange(ranges), "NODATA").save(cls_tif)
        if smooth:
            sm_tif = os.path.join(arcpy.env.scratchFolder, "geokit_wsum_sm.tif")
            if arcpy.Exists(sm_tif):
                arcpy.Delete_management(sm_tif)
            MajorityFilter(cls_tif, "EIGHT", "MAJORITY").save(sm_tif)
            cls_tif = sm_tif
        if poly_out:
            p_parent = os.path.dirname(poly_out)
            if p_parent and not os.path.exists(p_parent):
                os.makedirs(p_parent)
            raw_poly = os.path.join(arcpy.env.scratchGDB, "geokit_wsum_raw_poly")
            if arcpy.Exists(raw_poly):
                arcpy.Delete_management(raw_poly)
            if overwrite and arcpy.Exists(poly_out):
                arcpy.Delete_management(poly_out)
            arcpy.RasterToPolygon_conversion(cls_tif, raw_poly, "SIMPLIFY", "Value")
            arcpy.Dissolve_management(raw_poly, poly_out, ["gridcode"])
            if n_cls == 5:
                lbls = {1: "Sangat Rendah", 2: "Rendah", 3: "Sedang", 4: "Tinggi", 5: "Sangat Tinggi"}
            elif n_cls == 3:
                lbls = {1: "Rendah", 2: "Sedang", 3: "Tinggi"}
            else:
                lbls = dict((i + 1, "Kelas %d" % (i + 1)) for i in range(n_cls))
            arcpy.AddField_management(poly_out, "KELAS", "TEXT", field_length=32)
            arcpy.AddField_management(poly_out, "AREA_HA", "DOUBLE")
            with arcpy.da.UpdateCursor(poly_out, ["gridcode", "KELAS"]) as cur:
                for row in cur:
                    row[1] = lbls.get(int(row[0]), "Kelas %s" % row[0])
                    cur.updateRow(row)
            try:
                arcpy.CalculateField_management(poly_out, "AREA_HA", "!shape.geodesicArea@hectares!", "PYTHON_9.3")
            except Exception:
                arcpy.CalculateField_management(poly_out, "AREA_HA", "!shape.area@hectares!", "PYTHON_9.3")
            print("OK Classified polygon (%d classes) -> %s" % (n_cls, poly_out))
    return 0


def cmd_raster_interpolate(payload):
    in_pts = payload["input"]
    field = payload["field"]
    out_r = payload["out"]
    method = (payload.get("method") or "idw").lower()
    cell_size = payload.get("cell_size")
    mask = payload.get("mask")
    overwrite = _is_overwrite(payload)

    if method not in ALLOWED_INTERP_METHODS:
        sys.stderr.write("Invalid interpolation method '%s'. Allowed: idw, spline, kriging.\n" % method)
        return 1
    err = _check_safe_output(out_r, [in_pts, mask], overwrite)
    if err:
        sys.stderr.write(err + "\n")
        return 1

    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import Idw, Spline, Kriging, KrigingModelOrdinary, ExtractByMask

    parent = os.path.dirname(out_r)
    if parent and not os.path.exists(parent):
        os.makedirs(parent)
    if overwrite and arcpy.Exists(out_r):
        arcpy.Delete_management(out_r)

    prev_mask = arcpy.env.mask
    prev_ext = arcpy.env.extent
    try:
        if mask:
            arcpy.env.extent = mask
        if method == "spline":
            surf = Spline(in_pts, field, cell_size) if cell_size else Spline(in_pts, field)
        elif method == "kriging":
            surf = Kriging(in_pts, field, KrigingModelOrdinary("SPHERICAL"), cell_size) if cell_size else Kriging(in_pts, field, KrigingModelOrdinary("SPHERICAL"))
        else:
            surf = Idw(in_pts, field, cell_size) if cell_size else Idw(in_pts, field)
        if mask:
            surf = ExtractByMask(surf, mask)
        surf.save(out_r)
    finally:
        arcpy.env.mask = prev_mask
        arcpy.env.extent = prev_ext

    print("OK Interpolated (%s, field=%s) -> %s" % (method.upper(), field, out_r))
    return 0


def cmd_raster_index(payload):
    idx_type = (payload.get("index_type") or "ndvi").lower()
    out_r = payload["out"]
    poly_out = payload.get("polygon_out")
    overwrite = _is_overwrite(payload)

    if idx_type not in ALLOWED_INDEX_TYPES:
        sys.stderr.write("Invalid spectral index type '%s'. Allowed: ndvi, ndwi, ndbi, savi, nbr.\n" % idx_type)
        return 1
    err = _check_safe_output(out_r, [payload.get("band_a"), payload.get("band_b")], overwrite)
    if err:
        sys.stderr.write(err + "\n")
        return 1
    if poly_out:
        err_p = _check_safe_output(poly_out, [payload.get("band_a"), payload.get("band_b"), out_r], overwrite)
        if err_p:
            sys.stderr.write(err_p + "\n")
            return 1

    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import Float, Raster, Con, Reclassify, RemapRange

    b1 = Float(Raster(payload["band_a"]))
    b2 = Float(Raster(payload["band_b"]))
    parent = os.path.dirname(out_r)
    if parent and not os.path.exists(parent):
        os.makedirs(parent)
    if overwrite and arcpy.Exists(out_r):
        arcpy.Delete_management(out_r)

    denom = b1 + b2
    if idx_type == "savi":
        # Soil-Adjusted Vegetation Index (L = 0.5)
        res = Con((denom + 0.5) != 0, ((b1 - b2) / (denom + 0.5)) * 1.5, 0.0)
    else:
        # NDVI, NDWI, NDBI, NBR all follow normalized difference (A - B) / (A + B)
        res = Con(denom != 0, (b1 - b2) / denom, 0.0)
    res.save(out_r)
    print("OK Spectral index (%s) -> %s" % (idx_type.upper(), out_r))

    n_classes = payload.get("classes")
    poly_out = payload.get("polygon_out")
    if n_classes or poly_out:
        n_cls = int(n_classes or 5)
        try:
            arcpy.CalculateStatistics_management(out_r)
            rmin = float(str(arcpy.GetRasterProperties_management(out_r, "MINIMUM").getOutput(0)).replace(",", "."))
            rmax = float(str(arcpy.GetRasterProperties_management(out_r, "MAXIMUM").getOutput(0)).replace(",", "."))
        except Exception:
            rmin, rmax = -1.0, 1.0
        step = 2.0 / float(n_cls)
        ranges = []
        for i in range(n_cls):
            lo = (min(-1.0, rmin) - 0.001) if i == 0 else round(-1.0 + i * step, 6)
            hi = (max(1.0, rmax) + 0.001) if i == n_cls - 1 else round(-1.0 + (i + 1) * step, 6)
            ranges.append([lo, hi, i + 1])
        cls_tif = os.path.join(arcpy.env.scratchFolder, "geokit_idx_cls.tif")
        if arcpy.Exists(cls_tif):
            arcpy.Delete_management(cls_tif)
        Reclassify(out_r, "Value", RemapRange(ranges), "NODATA").save(cls_tif)
        if poly_out:
            p_parent = os.path.dirname(poly_out)
            if p_parent and not os.path.exists(p_parent):
                os.makedirs(p_parent)
            raw_poly = os.path.join(arcpy.env.scratchGDB, "geokit_idx_raw_poly")
            if arcpy.Exists(raw_poly):
                arcpy.Delete_management(raw_poly)
            if arcpy.Exists(poly_out):
                arcpy.Delete_management(poly_out)
            arcpy.RasterToPolygon_conversion(cls_tif, raw_poly, "SIMPLIFY", "Value")
            arcpy.Dissolve_management(raw_poly, poly_out, ["gridcode"])
            arcpy.AddField_management(poly_out, "KELAS", "TEXT", field_length=32)
            arcpy.AddField_management(poly_out, "AREA_HA", "DOUBLE")
            with arcpy.da.UpdateCursor(poly_out, ["gridcode", "KELAS"]) as cur:
                for row in cur:
                    row[1] = "%s Kelas %s" % (idx_type.upper(), row[0])
                    cur.updateRow(row)
            try:
                arcpy.CalculateField_management(poly_out, "AREA_HA", "!shape.area@hectares!", "PYTHON_9.3")
            except Exception:
                pass
            print("OK Classified %s polygon (%d classes) -> %s" % (idx_type.upper(), n_cls, poly_out))
    return 0


def cmd_raster_contour(payload):
    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import Contour

    in_dem = payload["input"]
    out_fc = payload["out"]
    interval = float(payload.get("interval") or 25.0)
    base_c = float(payload.get("base_contour") or 0.0)
    idx_every = int(payload.get("index_every") or 4)
    parent = os.path.dirname(out_fc)
    if parent and not os.path.exists(parent):
        os.makedirs(parent)
    if arcpy.Exists(out_fc):
        arcpy.Delete_management(out_fc)

    Contour(in_dem, out_fc, interval, base_c)
    arcpy.AddField_management(out_fc, "INDEX_KTR", "SHORT")
    idx_step = interval * float(idx_every)
    cnt = 0
    with arcpy.da.UpdateCursor(out_fc, ["Contour", "INDEX_KTR"]) as cur:
        for row in cur:
            cnt += 1
            val = float(row[0] or 0.0)
            rem = abs((val - base_c) % idx_step)
            is_idx = 1 if (rem < 1e-3 or abs(rem - idx_step) < 1e-3) else 0
            row[1] = is_idx
            cur.updateRow(row)
    print("OK Contours (interval=%.2f, index every %d -> %d lines) -> %s" % (interval, idx_every, cnt, out_fc))
    return 0


def cmd_select(payload):
    src = payload["input"]
    out_path = payload["out"]
    where = payload.get("where_clause") or ""
    ensure_output_container(out_path, is_batch=False)
    arcpy.Select_analysis(src, out_path, where)
    cnt = int(arcpy.GetCount_management(out_path).getOutput(0))
    print("OK Selected %d feature(s) (%s) -> %s" % (cnt, where, out_path))
    return 0


def cmd_export_table(payload):
    import csv

    src = payload["input"]
    out_path = payload["out"]
    parent = os.path.dirname(out_path)
    if parent and not os.path.exists(parent):
        os.makedirs(parent)
    fields = [f.name for f in arcpy.ListFields(src) if f.type not in ("Geometry", "Blob", "Raster")]
    if not fields:
        sys.stderr.write("No exportable attribute fields in %s\n" % src)
        return 1
    rows = 0
    with open(out_path, "wb") as fh:
        writer = csv.writer(fh)
        writer.writerow([f.encode("utf-8") if isinstance(f, unicode) else str(f) for f in fields])
        with arcpy.da.SearchCursor(src, fields) as cur:
            for row in cur:
                out_r = []
                for v in row:
                    if v is None:
                        out_r.append("")
                    elif isinstance(v, unicode):
                        out_r.append(v.encode("utf-8"))
                    else:
                        out_r.append(str(v))
                writer.writerow(out_r)
                rows += 1
    print("OK Exported %d row(s) x %d col(s) -> %s" % (rows, len(fields), out_path))
    return 0


def cmd_flatten_2d(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No vector datasets found to flatten.")
        return 0
    out_path = payload["out"]
    is_batch = len(items) > 1 or os.path.isdir(out_path) or not os.path.splitext(out_path)[1]
    ensure_output_container(out_path, is_batch=is_batch)
    prev_z = arcpy.env.outputZFlag
    prev_m = arcpy.env.outputMFlag
    arcpy.env.outputZFlag = "Disabled"
    arcpy.env.outputMFlag = "Disabled"
    failures = 0
    try:
        for idx, src in enumerate(items, 1):
            dst = build_output_path(out_path, src, is_batch)
            try:
                arcpy.CopyFeatures_management(src, dst)
                print("[%d/%d] OK   Flattened 2D %s -> %s" % (idx, len(items), os.path.basename(src), dst))
            except Exception as exc:
                failures += 1
                sys.stderr.write("[%d/%d] ERR  %s: %s\n" % (idx, len(items), os.path.basename(src), exc))
    finally:
        arcpy.env.outputZFlag = prev_z
        arcpy.env.outputMFlag = prev_m
    return 1 if failures else 0


def cmd_define_crs(payload):
    src = payload["input"]
    epsg = int(payload["epsg"])
    sr = arcpy.SpatialReference(epsg)
    arcpy.DefineProjection_management(src, sr)
    print("OK Defined CRS EPSG:%d (%s) -> %s" % (epsg, sr.name, src))
    return 0


def cmd_split(payload):
    src = payload["input"]
    req_field = payload["field"]
    out_dir = payload["out"]
    if not os.path.exists(out_dir):
        os.makedirs(out_dir)
    existing = dict((f.name.lower(), f) for f in arcpy.ListFields(src))
    fobj = existing.get(req_field.lower())
    if not fobj:
        sys.stderr.write("Field '%s' not found in %s\n" % (req_field, src))
        return 1
    fname = fobj.name
    is_str = fobj.type == "String"
    delim = arcpy.AddFieldDelimiters(src, fname)
    vals = []
    seen = set()
    with arcpy.da.SearchCursor(src, [fname]) as cur:
        for row in cur:
            v = row[0]
            if v is None:
                continue
            sv = unicode(v).strip()
            if sv and sv not in seen:
                seen.add(sv)
                vals.append(v)
    if not vals:
        sys.stderr.write("No non-empty values found in field '%s'.\n" % fname)
        return 1
    overwrite = _is_overwrite(payload)
    for idx, v in enumerate(vals, 1):
        safe = sanitize_gdb_name(unicode(v))
        dst = os.path.join(out_dir, "%s.shp" % safe)
        err = _check_safe_output(dst, [src], overwrite)
        if err:
            sys.stderr.write(err + "\n")
            return 1
        if is_str:
            esc = unicode(v).replace("'", "''")
            where = u"%s = '%s'" % (delim, esc)
        else:
            where = u"%s = %s" % (delim, v)
        arcpy.Select_analysis(src, dst, where)
        print("[%d/%d] OK   %s=%s -> %s" % (idx, len(vals), fname, v, dst))
    return 0


def cmd_clean_fields(payload):
    src = payload["input"]
    keep_raw = payload.get("keep_fields")
    drop_raw = payload.get("drop_fields")
    fields = arcpy.ListFields(src)
    removable = [f.name for f in fields if not f.required and f.type not in ("OID", "Geometry")]
    if keep_raw:
        keep_set = set(s.strip().lower() for s in keep_raw.split(",") if s.strip())
        to_drop = [n for n in removable if n.lower() not in keep_set]
    elif drop_raw:
        drop_set = set(s.strip().lower() for s in drop_raw.split(",") if s.strip())
        to_drop = [n for n in removable if n.lower() in drop_set]
    else:
        junk_re = re.compile(r"^(fid_.*|orig_fid|shape_le_.*|shape_ar_.*|objectid_.*|id_\d+)$", re.I)
        to_drop = [n for n in removable if junk_re.match(n)]
    if not to_drop:
        print("No matching fields to drop in %s." % src)
        return 0
    arcpy.DeleteField_management(src, to_drop)
    print("OK Dropped %d field(s) (%s) from %s" % (len(to_drop), ", ".join(to_drop), src))
    return 0


def cmd_explode(payload):
    src = payload["input"]
    out_path = payload["out"]
    ensure_output_container(out_path, is_batch=False)
    arcpy.MultipartToSinglepart_management(src, out_path)
    cnt = int(arcpy.GetCount_management(out_path).getOutput(0))
    print("OK Exploded multipart -> singlepart (%d feature(s)) -> %s" % (cnt, out_path))
    return 0


def cmd_centroid(payload):
    src = payload["input"]
    out_path = payload["out"]
    inside = bool(payload.get("inside", False))
    ensure_output_container(out_path, is_batch=False)
    try:
        arcpy.FeatureToPoint_management(src, out_path, "INSIDE" if inside else "CENTROID")
    except Exception:
        desc = arcpy.Describe(src)
        sr = getattr(desc, "spatialReference", None)
        parent = os.path.dirname(out_path) or "."
        name = os.path.basename(out_path)
        arcpy.CreateFeatureclass_management(parent, name, "POINT", template=src, spatial_reference=sr)
        attr_fields = [f.name for f in arcpy.ListFields(src) if not f.required and f.type not in ("OID", "Geometry", "Blob", "Raster")]
        read_cols = ["SHAPE@"] + attr_fields
        write_cols = ["SHAPE@XY"] + attr_fields
        with arcpy.da.SearchCursor(src, read_cols) as sc, arcpy.da.InsertCursor(out_path, write_cols) as ic:
            for row in sc:
                geom = row[0]
                if geom is None:
                    continue
                pt = geom.labelPoint if inside else geom.trueCentroid
                ic.insertRow([(pt.X, pt.Y)] + list(row[1:]))
    cnt = int(arcpy.GetCount_management(out_path).getOutput(0))
    print("OK Created %d centroid point(s) -> %s" % (cnt, out_path))
    return 0


def _geom_area_m2(geom, is_geo):
    if geom is None:
        return 0.0
    if is_geo:
        try:
            return float(geom.getArea("GEODESIC", "SQUAREMETERS"))
        except Exception:
            pass
    try:
        return float(geom.area)
    except Exception:
        return 0.0


def _infer_csv_field_type(values):
    non_empty = [unicode(v).strip() for v in values if v is not None and unicode(v).strip() != u""]
    if not non_empty:
        return "TEXT", 64
    all_int = True
    all_float = True
    max_len = 16
    for s in non_empty:
        if len(s) > max_len:
            max_len = len(s)
        try:
            int(s)
        except ValueError:
            all_int = False
        try:
            float(s)
        except ValueError:
            all_float = False
    if all_int:
        return "LONG", None
    if all_float:
        return "DOUBLE", None
    return "TEXT", min(254, max(32, max_len + 16))


def _unique_field_name(raw_name, existing_lower, is_shp):
    clean = re.sub(r"[^A-Za-z0-9_]", "_", str(raw_name).strip())
    if not clean or clean[0].isdigit():
        clean = "F_" + clean
    max_len = 10 if is_shp else 60
    cand = clean[:max_len]
    idx = 1
    while cand.lower() in existing_lower:
        suf = "_%d" % idx
        cand = clean[:max(1, max_len - len(suf))] + suf
        idx += 1
    existing_lower.add(cand.lower())
    return cand


def cmd_join(payload):
    import csv

    src = payload["input"]
    csv_path = payload["table"]
    in_field_req = payload["in_field"]
    join_field_req = payload["join_field"]
    out_path = payload["out"]
    ensure_output_container(out_path, is_batch=False)

    in_fields = dict((f.name.lower(), f.name) for f in arcpy.ListFields(src))
    if in_field_req.lower() not in in_fields:
        sys.stderr.write("Input key field '%s' not found in %s\n" % (in_field_req, src))
        return 1
    in_key = in_fields[in_field_req.lower()]

    with open(csv_path, "rb") as fh:
        raw_bytes = fh.read()
    if raw_bytes.startswith("\xef\xbb\xbf"):
        raw_bytes = raw_bytes[3:]
    lines = raw_bytes.splitlines()
    reader = csv.DictReader(lines)
    if not reader.fieldnames:
        sys.stderr.write("CSV table has no header row: %s\n" % csv_path)
        return 1
    csv_map = dict((c.strip().lower(), c) for c in reader.fieldnames if c)
    if join_field_req.lower() not in csv_map:
        sys.stderr.write("Join key column '%s' not found in %s\n" % (join_field_req, csv_path))
        return 1
    join_col = csv_map[join_field_req.lower()]
    extra_cols = [c for c in reader.fieldnames if c and c != join_col]
    lookup = {}
    col_samples = dict((c, []) for c in extra_cols)
    for row in reader:
        k_raw = row.get(join_col)
        if k_raw is None:
            continue
        k = _safe_u(k_raw).strip().lower()
        if not k:
            continue
        lookup[k] = row
        for c in extra_cols:
            col_samples[c].append(_safe_u(row.get(c, "")))

    if not extra_cols:
        sys.stderr.write("CSV table has no extra columns to join beyond key column.\n")
        return 1

    arcpy.CopyFeatures_management(src, out_path)
    is_shp = out_path.lower().endswith(".shp")
    existing_lower = set(f.name.lower() for f in arcpy.ListFields(out_path))
    added = []
    for col in extra_cols:
        ftype, flen = _infer_csv_field_type(col_samples[col])
        out_f = _unique_field_name(col, existing_lower, is_shp)
        if ftype == "TEXT":
            arcpy.AddField_management(out_path, out_f, "TEXT", field_length=flen)
        else:
            arcpy.AddField_management(out_path, out_f, ftype)
        added.append((col, out_f, ftype))

    matched = 0
    total = 0
    upd_cols = [in_key] + [x[1] for x in added]
    with arcpy.da.UpdateCursor(out_path, upd_cols) as cur:
        for row in cur:
            total += 1
            k_val = row[0]
            if k_val is None:
                continue
            rec = lookup.get(_safe_u(k_val).strip().lower())
            if not rec:
                continue
            matched += 1
            for i, (col, _, ftype) in enumerate(added, 1):
                raw_v = _safe_u(rec.get(col) or "").strip()
                if raw_v == u"":
                    row[i] = None
                elif ftype == "LONG":
                    try:
                        row[i] = int(float(raw_v))
                    except ValueError:
                        row[i] = None
                elif ftype == "DOUBLE":
                    try:
                        row[i] = float(raw_v)
                    except ValueError:
                        row[i] = None
                else:
                    row[i] = raw_v[:254]
            cur.updateRow(row)

    print("OK Joined %d field(s) (%d/%d features matched) -> %s" % (len(added), matched, total, out_path))
    return 0


def cmd_multi_buffer(payload):
    src = payload["input"]
    out_path = payload["out"]
    dists_raw = payload.get("dists") or ""
    dists = []
    for tok in dists_raw.split(","):
        tok = tok.strip()
        if tok:
            dists.append(float(tok))
    if not dists:
        sys.stderr.write("No valid distances provided in --dists (e.g. '500,1000,2000').\n")
        return 1
    unit = payload.get("unit") or "Meters"
    dissolve = (payload.get("dissolve_option") or "ALL").upper()
    ensure_output_container(out_path, is_batch=False)
    arcpy.MultipleRingBuffer_analysis(src, out_path, dists, unit, "distance", dissolve)
    cnt = int(arcpy.GetCount_management(out_path).getOutput(0))
    print("OK Multi-ring buffer (%s %s, %d polygon(s)) -> %s" % (dists, unit, cnt, out_path))
    return 0


def cmd_thiessen(payload):
    src = payload["input"]
    out_path = payload["out"]
    ext_layer = payload.get("extent")
    ensure_output_container(out_path, is_batch=False)
    prev_ext = arcpy.env.extent
    try:
        if ext_layer:
            arcpy.env.extent = arcpy.Describe(ext_layer).extent
        arcpy.CreateThiessenPolygons_analysis(src, out_path, "ALL")
    finally:
        arcpy.env.extent = prev_ext
    cnt = int(arcpy.GetCount_management(out_path).getOutput(0))
    print("OK Created %d Thiessen polygon(s) -> %s" % (cnt, out_path))
    return 0


def _parse_search_radius_num(raw):
    if not raw:
        return None
    m = re.match(r"^\s*([0-9]+(?:\.[0-9]+)?)", str(raw))
    if not m:
        return None
    val = float(m.group(1))
    low = str(raw).lower()
    if "kilometer" in low or low.endswith("km"):
        return val * 1000.0
    return val


def cmd_near(payload):
    src = payload["input"]
    near_fc = payload["near_layer"]
    radius = payload.get("search_radius") or ""
    try:
        arcpy.Near_analysis(src, near_fc, radius if radius else None)
        print("OK Computed NEAR_FID & NEAR_DIST against %s -> %s" % (os.path.basename(near_fc), src))
        return 0
    except Exception:
        near_geoms = []
        with arcpy.da.SearchCursor(near_fc, ["OID@", "SHAPE@"]) as sc:
            for oid, g in sc:
                if g is not None:
                    near_geoms.append((int(oid), g))
        if not near_geoms:
            sys.stderr.write("Near layer has no valid geometries: %s\n" % near_fc)
            return 1
        existing = set(f.name.lower() for f in arcpy.ListFields(src))
        if "near_fid" not in existing:
            arcpy.AddField_management(src, "NEAR_FID", "LONG")
        if "near_dist" not in existing:
            arcpy.AddField_management(src, "NEAR_DIST", "DOUBLE")
        max_d = _parse_search_radius_num(radius)
        cnt = 0
        with arcpy.da.UpdateCursor(src, ["SHAPE@", "NEAR_FID", "NEAR_DIST"]) as uc:
            for row in uc:
                g = row[0]
                if g is None:
                    continue
                best_oid = -1
                best_d = -1.0
                for noid, ng in near_geoms:
                    d = float(g.distanceTo(ng))
                    if max_d is not None and d > max_d:
                        continue
                    if best_oid == -1 or d < best_d:
                        best_oid = noid
                        best_d = d
                row[1] = best_oid
                row[2] = best_d
                uc.updateRow(row)
                cnt += 1
        print("OK Computed NEAR_FID & NEAR_DIST (%d feature(s), Basic fallback) -> %s" % (cnt, src))
        return 0


def _eliminate_slivers_inplace(fc, min_area):
    desc = arcpy.Describe(fc)
    sr = getattr(desc, "spatialReference", None)
    is_geo = bool(sr and getattr(sr, "type", "") == "Geographic")
    records = []
    with arcpy.da.SearchCursor(fc, ["OID@", "SHAPE@"]) as sc:
        for oid, g in sc:
            if g is not None:
                records.append((int(oid), g, _geom_area_m2(g, is_geo)))
    slivers = [(oid, g, a) for oid, g, a in records if a < min_area]
    if not slivers:
        return 0

    sliver_oids = set(oid for oid, _, _ in slivers)
    try:
        oid_field = desc.OIDFieldName
        lyr = "geokit_elim_lyr"
        if arcpy.Exists(lyr):
            arcpy.Delete_management(lyr)
        arcpy.MakeFeatureLayer_management(fc, lyr)
        oid_csv = ",".join(str(x) for x in sorted(sliver_oids))
        arcpy.SelectLayerByAttribute_management(lyr, "NEW_SELECTION", "%s IN (%s)" % (oid_field, oid_csv))
        tmp_out = os.path.join("in_memory", "gk_elim_tmp")
        if arcpy.Exists(tmp_out):
            arcpy.Delete_management(tmp_out)
        arcpy.Eliminate_management(lyr, tmp_out, "LENGTH")
        arcpy.Delete_management(lyr)
        prev_ow = arcpy.env.overwriteOutput
        arcpy.env.overwriteOutput = True
        try:
            arcpy.CopyFeatures_management(tmp_out, fc)
        finally:
            arcpy.env.overwriteOutput = prev_ow
            if arcpy.Exists(tmp_out):
                arcpy.Delete_management(tmp_out)
        return len(sliver_oids)
    except Exception:
        keepers = dict((oid, g) for oid, g, a in records if a >= min_area)
        if not keepers:
            return 0
        eliminated_oids = set()
        for soid, sg, _ in slivers:
            sext = sg.extent
            best_k = None
            best_score = -1.0
            for koid, kg in keepers.items():
                kext = kg.extent
                if sext.XMax < kext.XMin or sext.XMin > kext.XMax or sext.YMax < kext.YMin or sext.YMin > kext.YMax:
                    continue
                try:
                    shared = sg.intersect(kg, 2)
                    score = float(shared.length) if shared else 0.0
                except Exception:
                    score = 0.0
                if score == 0.0 and not sg.disjoint(kg):
                    score = 1e-6
                if score > best_score:
                    best_score = score
                    best_k = koid
            if best_k is not None and best_score > 0.0:
                try:
                    keepers[best_k] = keepers[best_k].union(sg)
                    eliminated_oids.add(soid)
                except Exception:
                    pass
        if not eliminated_oids:
            return 0
        with arcpy.da.UpdateCursor(fc, ["OID@", "SHAPE@"]) as uc:
            for row in uc:
                oid = int(row[0])
                if oid in eliminated_oids:
                    uc.deleteRow()
                elif oid in keepers:
                    row[1] = keepers[oid]
                    uc.updateRow(row)
        return len(eliminated_oids)


def cmd_eliminate(payload):
    src = payload["input"]
    out_path = payload["out"]
    min_area = float(payload.get("min_area") or 100.0)
    ensure_output_container(out_path, is_batch=False)
    arcpy.CopyFeatures_management(src, out_path)
    removed = _eliminate_slivers_inplace(out_path, min_area)
    remaining = int(arcpy.GetCount_management(out_path).getOutput(0))
    print("OK Eliminated %d sliver polygon(s) (< %.2f m2), %d remaining -> %s" % (removed, min_area, remaining, out_path))
    return 0


def _delete_duplicates_inplace(fc, tolerance=None):
    before = int(arcpy.GetCount_management(fc).getOutput(0))
    try:
        arcpy.DeleteIdentical_management(fc, ["Shape"], tolerance if tolerance else None)
        after = int(arcpy.GetCount_management(fc).getOutput(0))
        return max(0, before - after)
    except Exception:
        seen = set()
        removed = 0
        with arcpy.da.UpdateCursor(fc, ["SHAPE@"]) as uc:
            for row in uc:
                g = row[0]
                if g is None:
                    continue
                try:
                    sig = str(g.WKB)
                except Exception:
                    c = g.centroid
                    sig = (round(c.X, 4), round(c.Y, 4), round(g.area, 4), round(g.length, 4))
                if sig in seen:
                    uc.deleteRow()
                    removed += 1
                else:
                    seen.add(sig)
        return removed


def cmd_delete_duplicates(payload):
    src = payload["input"]
    out_path = payload["out"]
    tol = payload.get("tolerance")
    ensure_output_container(out_path, is_batch=False)
    arcpy.CopyFeatures_management(src, out_path)
    removed = _delete_duplicates_inplace(out_path, tol)
    remaining = int(arcpy.GetCount_management(out_path).getOutput(0))
    print("OK Removed %d duplicate geometry feature(s), %d remaining -> %s" % (removed, remaining, out_path))
    return 0


def cmd_topo_check(payload):
    src = payload["input"]
    out_dir = payload.get("out_dir")
    min_sliver = float(payload.get("min_sliver") or 100.0)
    strict = bool(payload.get("strict", False))
    as_json = bool(payload.get("json", False))

    desc = arcpy.Describe(src)
    shape_type = (getattr(desc, "shapeType", "") or "").lower()
    sr = getattr(desc, "spatialReference", None)
    is_geo = bool(sr and getattr(sr, "type", "") == "Geographic")
    total_features = int(arcpy.GetCount_management(src).getOutput(0))

    if out_dir and not os.path.exists(out_dir):
        os.makedirs(out_dir)

    invalid_count = 0
    try:
        chk_tbl = os.path.join("in_memory", "gk_topo_chk")
        if arcpy.Exists(chk_tbl):
            arcpy.Delete_management(chk_tbl)
        arcpy.CheckGeometry_management(src, chk_tbl)
        invalid_count = int(arcpy.GetCount_management(chk_tbl).getOutput(0))
        arcpy.Delete_management(chk_tbl)
    except Exception:
        invalid_count = 0

    geoms = []
    seen_wkb = set()
    duplicate_count = 0
    sliver_count = 0
    with arcpy.da.SearchCursor(src, ["OID@", "SHAPE@"]) as sc:
        for oid, g in sc:
            if g is None:
                continue
            try:
                sig = str(g.WKB)
            except Exception:
                c = g.centroid
                sig = (round(c.X, 4), round(c.Y, 4), round(g.area, 4), round(g.length, 4))
            if sig in seen_wkb:
                duplicate_count += 1
            else:
                seen_wkb.add(sig)
            if shape_type == "polygon" and _geom_area_m2(g, is_geo) < min_sliver:
                sliver_count += 1
            geoms.append((int(oid), g, g.extent))

    overlap_geoms = []
    gap_geoms = []
    dangle_pts = []
    self_intersections = 0
    exported_files = []

    if shape_type == "polygon":
        n = len(geoms)
        for i in range(n):
            oid_a, ga, ea = geoms[i]
            for j in range(i + 1, n):
                oid_b, gb, eb = geoms[j]
                if ea.XMax <= eb.XMin or ea.XMin >= eb.XMax or ea.YMax <= eb.YMin or ea.YMin >= eb.YMax:
                    continue
                try:
                    inter = ga.intersect(gb, 4)
                    if inter is not None and inter.area > 0:
                        overlap_geoms.append((oid_a, oid_b, inter))
                except Exception:
                    pass

        try:
            diss_fc = os.path.join("in_memory", "gk_topo_diss")
            if arcpy.Exists(diss_fc):
                arcpy.Delete_management(diss_fc)
            arcpy.Dissolve_management(src, diss_fc)
            with arcpy.da.SearchCursor(diss_fc, ["SHAPE@"]) as sc:
                for row in sc:
                    dg = row[0]
                    if dg is None:
                        continue
                    for part in dg:
                        rings = []
                        cur_ring = []
                        for pt in part:
                            if pt is None:
                                if cur_ring:
                                    rings.append(cur_ring)
                                    cur_ring = []
                            else:
                                cur_ring.append(pt)
                        if cur_ring:
                            rings.append(cur_ring)
                        for hole in rings[1:]:
                            if len(hole) >= 4:
                                arr = arcpy.Array(hole)
                                poly_gap = arcpy.Polygon(arr, sr)
                                if poly_gap.area > 0:
                                    gap_geoms.append(poly_gap)
            arcpy.Delete_management(diss_fc)
        except Exception:
            pass

        if out_dir and overlap_geoms:
            ov_path = os.path.join(out_dir, "topo_overlaps.shp")
            if arcpy.Exists(ov_path):
                arcpy.Delete_management(ov_path)
            arcpy.CreateFeatureclass_management(out_dir, "topo_overlaps.shp", "POLYGON", spatial_reference=sr)
            arcpy.AddField_management(ov_path, "OID_A", "LONG")
            arcpy.AddField_management(ov_path, "OID_B", "LONG")
            with arcpy.da.InsertCursor(ov_path, ["SHAPE@", "OID_A", "OID_B"]) as ic:
                for oa, ob, ig in overlap_geoms:
                    ic.insertRow([ig, oa, ob])
            exported_files.append(ov_path)

        if out_dir and gap_geoms:
            gp_path = os.path.join(out_dir, "topo_gaps.shp")
            if arcpy.Exists(gp_path):
                arcpy.Delete_management(gp_path)
            arcpy.CreateFeatureclass_management(out_dir, "topo_gaps.shp", "POLYGON", spatial_reference=sr)
            with arcpy.da.InsertCursor(gp_path, ["SHAPE@"]) as ic:
                for gg in gap_geoms:
                    ic.insertRow([gg])
            exported_files.append(gp_path)

    elif shape_type == "polyline":
        node_counts = {}
        node_coords = {}
        prec = 6 if is_geo else 2
        for _, g, _ in geoms:
            for part in g:
                pts = [p for p in part if p is not None]
                if len(pts) < 2:
                    continue
                for end_pt in (pts[0], pts[-1]):
                    key = (round(end_pt.X, prec), round(end_pt.Y, prec))
                    node_counts[key] = node_counts.get(key, 0) + 1
                    node_coords[key] = end_pt
        for key, cnt in node_counts.items():
            if cnt == 1:
                dangle_pts.append(node_coords[key])

        n = len(geoms)
        for i in range(n):
            _, ga, ea = geoms[i]
            for j in range(i + 1, n):
                _, gb, eb = geoms[j]
                if ea.XMax < eb.XMin or ea.XMin > eb.XMax or ea.YMax < eb.YMin or ea.YMin > eb.YMax:
                    continue
                try:
                    if ga.crosses(gb):
                        self_intersections += 1
                except Exception:
                    pass

        if out_dir and dangle_pts:
            dg_path = os.path.join(out_dir, "topo_dangles.shp")
            if arcpy.Exists(dg_path):
                arcpy.Delete_management(dg_path)
            arcpy.CreateFeatureclass_management(out_dir, "topo_dangles.shp", "POINT", spatial_reference=sr)
            with arcpy.da.InsertCursor(dg_path, ["SHAPE@XY"]) as ic:
                for pt in dangle_pts:
                    ic.insertRow([(pt.X, pt.Y)])
            exported_files.append(dg_path)

    total_errors = (
        invalid_count
        + duplicate_count
        + sliver_count
        + len(overlap_geoms)
        + len(gap_geoms)
        + len(dangle_pts)
        + self_intersections
    )
    report = {
        "dataset": src,
        "shape_type": shape_type,
        "features": total_features,
        "min_sliver_m2": min_sliver,
        "invalid_geometries": invalid_count,
        "duplicate_geometries": duplicate_count,
        "sliver_polygons": sliver_count,
        "overlaps": len(overlap_geoms),
        "interior_gaps": len(gap_geoms),
        "dangles": len(dangle_pts),
        "line_crossings": self_intersections,
        "total_errors": total_errors,
        "exported_error_layers": exported_files,
        "passed": total_errors == 0,
    }
    if as_json:
        print(json.dumps(report, indent=2))
    else:
        status = "PASS" if total_errors == 0 else ("ISSUES (%d)" % total_errors)
        print("Topology Check [%s]: %s (%s, %d features)" % (status, os.path.basename(src), shape_type, total_features))
        print("  Invalid geometries : %d" % invalid_count)
        print("  Duplicate features : %d" % duplicate_count)
        if shape_type == "polygon":
            print("  Overlaps           : %d" % len(overlap_geoms))
            print("  Interior gaps      : %d" % len(gap_geoms))
            print("  Slivers (<%g m2): %d" % (min_sliver, sliver_count))
        elif shape_type == "polyline":
            print("  Dangling nodes     : %d" % len(dangle_pts))
            print("  Line crossings     : %d" % self_intersections)
        for ef in exported_files:
            print("  Exported error shp : %s" % ef)
    return 1 if (strict and total_errors > 0) else 0


def cmd_topo_fix(payload):
    src = payload["input"]
    out_path = payload["out"]
    min_sliver = float(payload.get("min_sliver") or 100.0)
    ensure_output_container(out_path, is_batch=False)
    arcpy.CopyFeatures_management(src, out_path)
    arcpy.RepairGeometry_management(out_path, "DELETE_NULL")
    dups_removed = _delete_duplicates_inplace(out_path)
    desc = arcpy.Describe(out_path)
    slivers_removed = 0
    if (getattr(desc, "shapeType", "") or "").lower() == "polygon" and min_sliver > 0:
        slivers_removed = _eliminate_slivers_inplace(out_path, min_sliver)
    final_cnt = int(arcpy.GetCount_management(out_path).getOutput(0))
    print(
        "OK Topology Fix complete -> %s (repaired geometry, removed %d duplicates, eliminated %d slivers, %d feature(s) remaining)"
        % (out_path, dups_removed, slivers_removed, final_cnt)
    )
    return 0


def cmd_raster_align(payload):
    inputs = payload.get("inputs") or [payload["input"]]
    ref_raster = payload["reference"]
    out_path = payload["out"]
    resamp = (payload.get("resampling") or "BILINEAR").upper()
    if resamp not in ("NEAREST", "BILINEAR", "CUBIC", "MAJORITY"):
        resamp = "BILINEAR"

    ref_desc = arcpy.Describe(ref_raster)
    ref_sr = ref_desc.spatialReference
    cell_x = float(arcpy.GetRasterProperties_management(ref_raster, "CELLSIZEX").getOutput(0))
    cell_y = float(arcpy.GetRasterProperties_management(ref_raster, "CELLSIZEY").getOutput(0))
    cell_str = "%s %s" % (cell_x, cell_y)

    is_batch = len(inputs) > 1 or os.path.isdir(out_path) or not os.path.splitext(out_path)[1]
    if is_batch:
        if not os.path.exists(out_path):
            os.makedirs(out_path)
    else:
        parent = os.path.dirname(out_path)
        if parent and not os.path.exists(parent):
            os.makedirs(parent)

    prev_snap = arcpy.env.snapRaster
    prev_ext = arcpy.env.extent
    prev_cs = arcpy.env.cellSize
    arcpy.env.snapRaster = ref_raster
    arcpy.env.extent = ref_desc.extent
    arcpy.env.cellSize = ref_raster
    try:
        for idx, src in enumerate(inputs, 1):
            if is_batch:
                stem = os.path.splitext(os.path.basename(src))[0]
                dst = os.path.join(out_path, "%s_aligned.tif" % stem)
            else:
                dst = out_path
            arcpy.ProjectRaster_management(src, dst, ref_sr, resamp, cell_str)
            print("[%d/%d] OK   Aligned %s -> %s (%gx%g, %s)" % (idx, len(inputs), os.path.basename(src), dst, cell_x, cell_y, resamp))
    finally:
        arcpy.env.snapRaster = prev_snap
        arcpy.env.extent = prev_ext
        arcpy.env.cellSize = prev_cs
    return 0


def cmd_raster_extract_points(payload):
    pts = payload["points"]
    ras = payload["raster"]
    field_name = payload.get("field") or "RAST_VAL"
    out_path = payload["out"]
    ensure_output_container(out_path, is_batch=False)

    used_sa = False
    try:
        arcpy.CheckOutExtension("Spatial")
        from arcpy.sa import ExtractValuesToPoints

        ExtractValuesToPoints(pts, ras, out_path, "NONE", "VALUE_ONLY")
        used_sa = True
        if field_name.upper() != "RASTERVALU":
            existing = set(f.name.lower() for f in arcpy.ListFields(out_path))
            if field_name.lower() not in existing:
                arcpy.AddField_management(out_path, field_name, "DOUBLE")
            with arcpy.da.UpdateCursor(out_path, ["RASTERVALU", field_name]) as uc:
                for row in uc:
                    row[1] = row[0]
                    uc.updateRow(row)
            arcpy.DeleteField_management(out_path, ["RASTERVALU"])
    except Exception:
        arcpy.CopyFeatures_management(pts, out_path)
        existing = set(f.name.lower() for f in arcpy.ListFields(out_path))
        if field_name.lower() not in existing:
            arcpy.AddField_management(out_path, field_name, "DOUBLE")
        ras_sr = arcpy.Describe(ras).spatialReference
        pts_sr = arcpy.Describe(out_path).spatialReference
        need_proj = bool(ras_sr and pts_sr and ras_sr.factoryCode != pts_sr.factoryCode)
        with arcpy.da.UpdateCursor(out_path, ["SHAPE@", field_name]) as uc:
            for row in uc:
                g = row[0]
                if g is None:
                    continue
                if need_proj:
                    g = g.projectAs(ras_sr)
                pt = g.firstPoint
                try:
                    res = arcpy.GetCellValue_management(ras, "%s %s" % (pt.X, pt.Y)).getOutput(0)
                    row[1] = None if str(res).lower() in ("nodata", "") else float(res)
                except Exception:
                    row[1] = None
                uc.updateRow(row)
    cnt = int(arcpy.GetCount_management(out_path).getOutput(0))
    mode = "Spatial Analyst" if used_sa else "GetCellValue fallback"
    print("OK Extracted raster values (%s, %d points, %s) -> %s" % (field_name, cnt, mode, out_path))
    return 0


HANDLERS = {
    "info": cmd_info,
    "project": cmd_project,
    "clip": cmd_clip,
    "merge": cmd_merge,
    "pack": cmd_pack,
    "unpack": cmd_unpack,
    "repair": cmd_repair,
    "buffer": cmd_buffer,
    "dissolve": cmd_dissolve,
    "calc_geom": cmd_calc_geom,
    "intersect": cmd_intersect,
    "erase": cmd_erase,
    "spatial_join": cmd_spatial_join,
    "gp_tools": cmd_gp_tools,
    "gp_tool_usage": cmd_gp_tool_usage,
    "gp_run": cmd_gp_run,
    "xy2shp": cmd_xy2shp,
    "convert": cmd_convert,
    "values": cmd_values,
    "classify": cmd_classify,
    "calc_field": cmd_calc_field,
    "union": cmd_union,
    "select": cmd_select,
    "export_table": cmd_export_table,
    "flatten_2d": cmd_flatten_2d,
    "define_crs": cmd_define_crs,
    "split": cmd_split,
    "clean_fields": cmd_clean_fields,
    "explode": cmd_explode,
    "centroid": cmd_centroid,
    "join": cmd_join,
    "multi_buffer": cmd_multi_buffer,
    "thiessen": cmd_thiessen,
    "near": cmd_near,
    "eliminate": cmd_eliminate,
    "delete_duplicates": cmd_delete_duplicates,
    "topo_check": cmd_topo_check,
    "topo_fix": cmd_topo_fix,
    "qa_check": cmd_qa_check,
    "layout_list": cmd_layout_list,
    "layout_export": cmd_layout_export,
    "layout_update": cmd_layout_update,
    "map_broken_links": cmd_map_broken_links,
    "map_fix_links": cmd_map_fix_links,
    "raster_mosaic": cmd_raster_mosaic,
    "raster_terrain": cmd_raster_terrain,
    "raster_zonal_stats": cmd_raster_zonal_stats,
    "raster_reclass": cmd_raster_reclass,
    "raster_weighted_sum": cmd_raster_weighted_sum,
    "raster_interpolate": cmd_raster_interpolate,
    "raster_index": cmd_raster_index,
    "raster_contour": cmd_raster_contour,
    "raster_align": cmd_raster_align,
    "raster_extract_points": cmd_raster_extract_points,
    "hydro_delineate": cmd_hydro_delineate,
    "hydro_basin": cmd_hydro_basin,
}


def _safe_u(val):
    if isinstance(val, unicode):
        return val
    try:
        return str(val).decode("utf-8", "replace")
    except Exception:
        return u""


def _run_payload_captured(payload):
    import StringIO
    import traceback

    action = payload.get("action")
    handler = HANDLERS.get(action)
    if not handler:
        return 2, u"", u"Unsupported ArcMap action: %s\n" % action

    old_out, old_err = sys.stdout, sys.stderr
    buf_out, buf_err = StringIO.StringIO(), StringIO.StringIO()
    sys.stdout, sys.stderr = buf_out, buf_err
    code = 0
    try:
        arcpy.env.overwriteOutput = _is_overwrite(payload)
        out_p = payload.get("out")
        if out_p and action not in ("layout_export", "raster_reclass", "raster_weighted_sum", "raster_interpolate", "raster_index"):
            in_list = [payload.get("input"), payload.get("aoi"), payload.get("mask"), payload.get("dem"), payload.get("zones")]
            if isinstance(payload.get("inputs"), list):
                in_list.extend(payload.get("inputs"))
            chk_err = _check_safe_output(out_p, in_list, arcpy.env.overwriteOutput)
            if chk_err and not os.path.isdir(out_p):
                return 1, u"", _safe_u(chk_err + "\n")
        code = int(handler(payload) or 0)
    except SystemExit as se:
        code = int(se.code) if isinstance(se.code, int) else 1
    except Exception:
        code = 1
        traceback.print_exc(file=buf_err)
    finally:
        sys.stdout, sys.stderr = old_out, old_err
        try:
            arcpy.env.workspace = None
            arcpy.env.overwriteOutput = False
        except Exception:
            pass
    return code, _safe_u(buf_out.getvalue()), _safe_u(buf_err.getvalue())


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


def _serve_daemon(port, idle_minutes=30):
    import socket
    import time

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", int(port)))
    srv.listen(8)
    srv.settimeout(1.0)

    started = time.time()
    last_req = started
    served = 0
    idle_sec = float(idle_minutes) * 60.0 if idle_minutes and idle_minutes > 0 else 0.0

    try:
        while True:
            if idle_sec > 0.0 and (time.time() - last_req) >= idle_sec:
                break
            try:
                conn, _ = srv.accept()
            except socket.timeout:
                continue
            last_req = time.time()
            try:
                conn.settimeout(30.0)
                max_bytes = 10 * 1024 * 1024
                chunks = []
                total_len = 0
                too_large = False
                while True:
                    data = conn.recv(65536)
                    if not data:
                        break
                    total_len += len(data)
                    if total_len > max_bytes:
                        too_large = True
                        break
                    chunks.append(data)
                    if "\n" in data:
                        break
                if too_large:
                    err_resp = {"ok": False, "code": 1, "stdout": "", "stderr": "Request payload exceeds 10 MB limit\n"}
                    conn.sendall((json.dumps(err_resp) + "\n").encode("utf-8"))
                    continue
                raw = "".join(chunks).strip()
                if not raw:
                    continue
                payload = json.loads(raw)
                if not _verify_local_token(payload.get("token")):
                    err_resp = {"ok": False, "code": 401, "stdout": "", "stderr": "Unauthorized: invalid or missing GeoKit local auth token\n"}
                    conn.sendall((json.dumps(err_resp) + "\n").encode("utf-8"))
                    continue
                action = payload.get("action")
                if action == "__ping__":
                    resp = {
                        "ok": True,
                        "code": 0,
                        "engine": "arcmap",
                        "pid": os.getpid(),
                        "port": int(port),
                        "requests": served,
                        "uptime_sec": round(time.time() - started, 1),
                    }
                    conn.sendall((json.dumps(resp) + "\n").encode("utf-8"))
                    continue
                if action == "__shutdown__":
                    conn.sendall((json.dumps({"ok": True, "code": 0, "stopped": True}) + "\n").encode("utf-8"))
                    break
                if action == "__batch__":
                    steps = payload.get("steps") or []
                    all_out = []
                    all_err = []
                    final_code = 0
                    for step in steps:
                        c, o, e = _run_payload_captured(step)
                        served += 1
                        if o:
                            all_out.append(o)
                        if e:
                            all_err.append(e)
                        if c != 0:
                            final_code = c
                            break
                    resp = {
                        "ok": final_code == 0,
                        "code": final_code,
                        "stdout": "".join(all_out),
                        "stderr": "".join(all_err),
                    }
                    conn.sendall((json.dumps(resp) + "\n").encode("utf-8"))
                    continue

                code, out_s, err_s = _run_payload_captured(payload)
                served += 1
                resp = {
                    "ok": code == 0,
                    "code": code,
                    "stdout": out_s,
                    "stderr": err_s,
                }
                conn.sendall((json.dumps(resp) + "\n").encode("utf-8"))
            except Exception as exc:
                try:
                    err_resp = {"ok": False, "code": 1, "stdout": "", "stderr": str(exc) + "\n"}
                    conn.sendall((json.dumps(err_resp) + "\n").encode("utf-8"))
                except Exception:
                    pass
            finally:
                try:
                    conn.close()
                except Exception:
                    pass
    finally:
        try:
            srv.close()
        except Exception:
            pass


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--serve":
        port = int(sys.argv[2])
        idle_min = 30
        if len(sys.argv) >= 5 and sys.argv[3] == "--idle-minutes":
            idle_min = int(sys.argv[4])
        _serve_daemon(port, idle_minutes=idle_min)
        sys.exit(0)
    if len(sys.argv) < 2:
        sys.stderr.write("Usage: arcmap_runner.py <payload.json> | --serve <port>\n")
        sys.exit(2)
    with open(sys.argv[1], "r") as fh:
        payload = json.load(fh)
    action = payload.get("action")
    handler = HANDLERS.get(action)
    if not handler:
        sys.stderr.write("Unsupported ArcMap action: %s\n" % action)
        sys.exit(2)
    arcpy.env.overwriteOutput = _is_overwrite(payload)
    out_p = payload.get("out")
    if out_p and action not in ("layout_export", "raster_reclass", "raster_weighted_sum", "raster_interpolate", "raster_index"):
        in_list = [payload.get("input"), payload.get("aoi"), payload.get("mask"), payload.get("dem"), payload.get("zones")]
        if isinstance(payload.get("inputs"), list):
            in_list.extend(payload.get("inputs"))
        chk_err = _check_safe_output(out_p, in_list, arcpy.env.overwriteOutput)
        if chk_err and not os.path.isdir(out_p):
            sys.stderr.write(chk_err + "\n")
            sys.exit(1)
    code = handler(payload)
    sys.exit(code)


if __name__ == "__main__":
    main()
