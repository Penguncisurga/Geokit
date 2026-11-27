"""
geokit embedded ArcPy batch runner.
Imports arcpy once per batch invocation to avoid repeated Python startup overhead.
"""

import json
import os
import re
import sys

try:
    import arcpy
except ImportError as exc:
    sys.stderr.write(f"[geokit] Failed to import arcpy: {exc}\n")
    sys.exit(2)

arcpy.env.overwriteOutput = False

VECTOR_EXTS = {".shp", ".gpkg"}
RASTER_EXTS = {".tif", ".tiff", ".img", ".jp2", ".ecw", ".asc", ".hgt"}
ALLOWED_INTERP_METHODS = {"idw", "spline", "kriging"}
ALLOWED_INDEX_TYPES = {"ndvi", "ndwi", "ndbi", "savi", "nbr"}


def _norm_path(p) -> str:
    if not p:
        return ""
    try:
        return os.path.normcase(os.path.abspath(str(p)))
    except Exception:
        return str(p).lower()


def _is_overwrite(payload) -> bool:
    return bool(payload.get("overwrite")) or os.environ.get("GEOKIT_OVERWRITE") in ("1", "true", "TRUE")


def _check_safe_output(out_path, input_paths, overwrite: bool):
    if not out_path:
        return None
    norm_out = _norm_path(out_path)
    for inp in input_paths:
        if inp and _norm_path(inp) == norm_out:
            return f"Refusing to overwrite input/reference dataset: {out_path}"
    if not overwrite and (os.path.exists(out_path) or arcpy.Exists(out_path)):
        return f"Output already exists: {out_path}. Pass --overwrite to replace it."
    return None


def _compile_safe_math(expr: str):
    import ast
    tree = ast.parse(expr, mode="eval")
    allowed_nodes = (
        ast.Expression, ast.Constant, ast.Name, ast.Load,
        ast.UnaryOp, ast.UAdd, ast.USub, ast.Not,
        ast.BinOp, ast.Add, ast.Sub, ast.Mult, ast.Div, ast.FloorDiv, ast.Mod, ast.Pow,
        ast.BoolOp, ast.And, ast.Or,
        ast.Compare, ast.Eq, ast.NotEq, ast.Lt, ast.LtE, ast.Gt, ast.GtE,
        ast.IfExp, ast.Call,
    )
    for node in ast.walk(tree):
        if not isinstance(node, allowed_nodes):
            raise ValueError(f"Disallowed expression syntax: {type(node).__name__}")
        if isinstance(node, ast.Name) and node.id.startswith("_"):
            raise ValueError(f"Disallowed identifier in expression: {node.id}")
        if isinstance(node, ast.Call):
            if not isinstance(node.func, ast.Name) or node.keywords:
                raise ValueError("Only direct calls to safe math functions are allowed.")
    return tree


def _eval_safe_math_node(node, env, funcs):
    import ast
    import operator
    if isinstance(node, ast.Expression):
        return _eval_safe_math_node(node.body, env, funcs)
    if isinstance(node, ast.Constant):
        return node.value
    if isinstance(node, ast.Name):
        if node.id in env:
            return env[node.id]
        raise ValueError(f"Unknown field or variable '{node.id}' in expression.")
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
            raise ValueError(f"Disallowed function '{node.func.id}' in expression.")
        args = [_eval_safe_math_node(a, env, funcs) for a in node.args]
        return fn(*args)
    raise ValueError(f"Unsupported expression node: {type(node).__name__}")


def sanitize_gdb_name(name: str) -> str:
    stem = os.path.splitext(os.path.basename(name))[0]
    clean = re.sub(r"[^a-zA-Z0-9_]+", "_", stem).strip("_").lower()
    if not clean:
        clean = "layer"
    if clean[0].isdigit():
        clean = f"l_{clean}"
    return clean


def is_gdb(path: str) -> bool:
    return path.lower().endswith(".gdb") and os.path.isdir(path)


def collect_datasets(input_path: str, include_rasters: bool = True):
    """Returns list of tuples: (dataset_path, kind) where kind is 'vector' or 'raster'."""
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
            # Do not walk inside .gdb folders as plain files
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


def ensure_output_container(out_path: str, is_batch: bool):
    if out_path.lower().endswith(".gdb"):
        if not os.path.exists(out_path):
            parent = os.path.dirname(out_path) or "."
            os.makedirs(parent, exist_ok=True)
            arcpy.management.CreateFileGDB(parent, os.path.basename(out_path))
        return "gdb"

    if is_batch or os.path.isdir(out_path) or not os.path.splitext(out_path)[1]:
        os.makedirs(out_path, exist_ok=True)
        return "dir"

    parent = os.path.dirname(out_path)
    if parent:
        os.makedirs(parent, exist_ok=True)
    return "file"


def resolve_out_target(src_path: str, kind: str, out_path: str, container_type: str, suffix: str = "") -> str:
    if container_type == "file":
        return out_path
    if container_type == "gdb":
        name = sanitize_gdb_name(src_path)
        if suffix:
            name = f"{name}_{suffix}"
        return os.path.join(out_path, name)
    # dir
    stem = os.path.splitext(os.path.basename(src_path))[0]
    ext = os.path.splitext(src_path)[1]
    if not ext:
        ext = ".tif" if kind == "raster" else ".shp"
    if suffix:
        stem = f"{stem}_{suffix}"
    return os.path.join(out_path, f"{stem}{ext}")


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
            entry["feature_count"] = int(arcpy.management.GetCount(ds_path)[0])
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
        epsg_str = f"EPSG:{item['epsg']}" if item["epsg"] else "No EPSG"
        if item["kind"] == "vector":
            print(
                f"- {item['name']} [{item['geometry_type']}] | {item['feature_count']} features | "
                f"{item['crs']} ({epsg_str})"
            )
            print(f"    Fields: {', '.join(item['fields'])}")
        else:
            print(
                f"- {item['name']} [Raster, {item['band_count']} band(s)] | "
                f"{item['crs']} ({epsg_str})"
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
                arcpy.management.Project(src, dst, sr)
            else:
                arcpy.management.ProjectRaster(src, dst, sr)
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} -> {dst}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} succeeded, {failures} failed.")
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
                arcpy.analysis.Clip(src, aoi, dst)
            else:
                arcpy.management.Clip(
                    in_raster=src,
                    rectangle="#",
                    out_raster=dst,
                    in_template_dataset=aoi,
                    clipping_geometry="ClippingGeometry",
                    maintain_clipping_extent="NO_MAINTAIN_EXTENT",
                )
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} -> {dst}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} succeeded, {failures} failed.")
    return 1 if failures else 0


def cmd_merge(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if len(items) < 2:
        print("Need at least 2 vector datasets to merge.", file=sys.stderr)
        return 1
    ensure_output_container(payload["out"], is_batch=False)
    arcpy.management.Merge(items, payload["out"])
    print(f"OK Merged {len(items)} dataset(s) -> {payload['out']}")
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
                arcpy.conversion.FeatureClassToFeatureClass(src, out_gdb, name)
            else:
                arcpy.conversion.RasterToGeodatabase(src, out_gdb)
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} -> {out_gdb}\\{name}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} packed, {failures} failed.")
    return 1 if failures else 0


def cmd_unpack(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No feature classes found to unpack.")
        return 0
    out_dir = payload["out"]
    os.makedirs(out_dir, exist_ok=True)
    failures = 0
    for idx, src in enumerate(items, 1):
        name = os.path.basename(src)
        try:
            arcpy.conversion.FeatureClassToShapefile(src, out_dir)
            print(f"[{idx}/{len(items)}] OK   {name} -> {out_dir}\\{name}.shp")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {name}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} unpacked, {failures} failed.")
    return 1 if failures else 0


def cmd_repair(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No vector datasets found to repair.")
        return 0
    failures = 0
    for idx, src in enumerate(items, 1):
        try:
            arcpy.management.RepairGeometry(src, "DELETE_NULL")
            print(f"[{idx}/{len(items)}] OK   Repaired {os.path.basename(src)}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} repaired, {failures} failed.")
    return 1 if failures else 0


def cmd_buffer(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No vector datasets found to buffer.")
        return 0
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    dissolve_opt = "ALL" if payload.get("dissolve") else "NONE"
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.analysis.Buffer(src, dst, payload["dist"], dissolve_option=dissolve_opt)
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} -> {dst}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} buffered, {failures} failed.")
    return 1 if failures else 0


def cmd_dissolve(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No vector datasets found to dissolve.")
        return 0
    fields = [f.strip() for f in payload["field"].split(",") if f.strip()]
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.management.Dissolve(src, dst, fields)
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} -> {dst}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} dissolved, {failures} failed.")
    return 1 if failures else 0


def cmd_calc_geom(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No vector datasets found for geometry calculation.")
        return 0
    unit_req = (payload.get("unit") or "").lower()
    custom_field = payload.get("field")
    failures = 0
    for idx, src in enumerate(items, 1):
        try:
            shape_type = arcpy.Describe(src).shapeType
            if shape_type == "Polygon":
                unit = unit_req or "ha"
                area_map = {
                    "ha": ("HECTARES", "AREA_HA"),
                    "m2": ("SQUARE_METERS", "AREA_M2"),
                    "km2": ("SQUARE_KILOMETERS", "AREA_KM2"),
                }
                if unit not in area_map:
                    raise ValueError(f"Unsupported polygon unit '{unit}'. Use ha, m2, or km2.")
                arc_unit, default_field = area_map[unit]
                field_name = custom_field or default_field
                arcpy.management.CalculateGeometryAttributes(
                    src, [[field_name, "AREA_GEODESIC"]], area_unit=arc_unit
                )
            elif shape_type == "Polyline":
                unit = unit_req or "km"
                len_map = {
                    "km": ("KILOMETERS", "LEN_KM"),
                    "m": ("METERS", "LEN_M"),
                }
                if unit not in len_map:
                    raise ValueError(f"Unsupported line unit '{unit}'. Use km or m.")
                arc_unit, default_field = len_map[unit]
                field_name = custom_field or default_field
                arcpy.management.CalculateGeometryAttributes(
                    src, [[field_name, "LENGTH_GEODESIC"]], length_unit=arc_unit
                )
            else:
                print(f"[{idx}/{len(items)}] SKIP {os.path.basename(src)} (geometry type: {shape_type})")
                continue
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} -> calculated {field_name}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} processed, {failures} failed.")
    return 1 if failures else 0


def collect_aprx_files(input_path: str):
    if os.path.isfile(input_path):
        return [input_path] if input_path.lower().endswith(".aprx") else []
    aprx_files = []
    for root, dirs, files in os.walk(input_path):
        dirs[:] = [d for d in dirs if not d.lower().endswith(".gdb")]
        for fname in sorted(files):
            if fname.lower().endswith(".aprx"):
                aprx_files.append(os.path.join(root, fname))
    return aprx_files


def cmd_layout_list(payload):
    aprx_files = collect_aprx_files(payload["input"])
    results = []
    for aprx_path in aprx_files:
        aprx = arcpy.mp.ArcGISProject(aprx_path)
        for lyt in aprx.listLayouts():
            results.append(
                {
                    "project": os.path.basename(aprx_path),
                    "project_path": aprx_path,
                    "layout": lyt.name,
                    "page_width": lyt.pageWidth,
                    "page_height": lyt.pageHeight,
                    "page_units": lyt.pageUnits,
                }
            )
    if payload.get("json"):
        print(json.dumps(results, indent=2))
        return 0
    if not results:
        print("No layouts found.")
        return 0
    for item in results:
        print(
            f"- [{item['project']}] {item['layout']} "
            f"({item['page_width']} x {item['page_height']} {item['page_units']})"
        )
    return 0


def cmd_layout_export(payload):
    aprx_files = collect_aprx_files(payload["input"])
    if not aprx_files:
        print("No .aprx files found to export.")
        return 0
    out_dir = payload["out"]
    os.makedirs(out_dir, exist_ok=True)
    fmt = (payload.get("format") or "pdf").lower()
    dpi = int(payload.get("dpi") or 300)
    filt = (payload.get("filter") or "").lower()

    exported = 0
    failures = 0
    multi_project = len(aprx_files) > 1

    for aprx_path in aprx_files:
        proj_stem = sanitize_gdb_name(aprx_path)
        aprx = arcpy.mp.ArcGISProject(aprx_path)
        for lyt in aprx.listLayouts():
            if filt and filt not in lyt.name.lower():
                continue
            lyt_stem = sanitize_gdb_name(lyt.name)
            base_name = f"{proj_stem}__{lyt_stem}" if multi_project else lyt_stem
            ext = "jpg" if fmt == "jpeg" else fmt
            out_file = os.path.join(out_dir, f"{base_name}.{ext}")
            try:
                if ext == "pdf":
                    lyt.exportToPDF(out_file, resolution=dpi)
                elif ext == "png":
                    lyt.exportToPNG(out_file, resolution=dpi)
                elif ext == "jpg":
                    lyt.exportToJPEG(out_file, resolution=dpi)
                exported += 1
                print(f"[OK]  {os.path.basename(aprx_path)} :: {lyt.name} -> {out_file}")
            except Exception as exc:
                failures += 1
                print(
                    f"[ERR] {os.path.basename(aprx_path)} :: {lyt.name}: {exc}",
                    file=sys.stderr,
                )

    print(f"Summary: {exported} layout(s) exported ({dpi} DPI, {fmt.upper()}), {failures} failed.")
    return 1 if failures else 0


def cmd_map_broken_links(payload):
    aprx_files = collect_aprx_files(payload["input"])
    broken = []
    for aprx_path in aprx_files:
        aprx = arcpy.mp.ArcGISProject(aprx_path)
        for m in aprx.listMaps():
            for lyr in m.listLayers():
                if getattr(lyr, "isBroken", False):
                    ds = ""
                    try:
                        ds = lyr.dataSource
                    except Exception:
                        ds = "(unavailable)"
                    broken.append(
                        {
                            "project": os.path.basename(aprx_path),
                            "project_path": aprx_path,
                            "map": m.name,
                            "layer": lyr.longName,
                            "data_source": ds,
                        }
                    )
    if payload.get("json"):
        print(json.dumps(broken, indent=2))
        return 0
    if not broken:
        print("All layers have valid data sources! No broken links found.")
        return 0
    for item in broken:
        print(
            f"[BROKEN] {item['project']} | Map: {item['map']} | "
            f"Layer: {item['layer']} | Source: {item['data_source']}"
        )
    print(f"Summary: {len(broken)} broken layer(s) found across {len(aprx_files)} project(s).")
    return 0


def cmd_map_fix_links(payload):
    aprx_path = payload["input"]
    find_str = payload["find"]
    replace_str = payload["replace"]
    out_path = payload.get("out")

    aprx = arcpy.mp.ArcGISProject(aprx_path)
    aprx.updateConnectionProperties(find_str, replace_str)

    if out_path:
        parent = os.path.dirname(out_path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        aprx.saveACopy(out_path)
        print(f"OK Updated connection properties and saved copy -> {out_path}")
    else:
        aprx.save()
        print(f"OK Updated connection properties and saved -> {aprx_path}")
    return 0


def cmd_intersect(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No vector datasets found to intersect.")
        return 0
    overlay = payload["overlay"]
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.analysis.Intersect([src, overlay], dst)
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} ∩ {os.path.basename(overlay)} -> {dst}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} intersected, {failures} failed.")
    return 1 if failures else 0


def cmd_erase(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No vector datasets found to erase.")
        return 0
    mask = payload["overlay"]
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.analysis.Erase(src, mask, dst)
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} - {os.path.basename(mask)} -> {dst}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} erased, {failures} failed.")
    return 1 if failures else 0


def cmd_spatial_join(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No vector datasets found for spatial join.")
        return 0
    join_fc = payload["overlay"]
    match_map = {
        "intersect": "INTERSECT",
        "within": "WITHIN",
        "contains": "CONTAINS",
        "closest": "CLOSEST",
    }
    raw_match = (payload.get("match_option") or "intersect").lower()
    if raw_match not in match_map:
        print(f"Unsupported match option '{raw_match}'. Use intersect, within, contains, or closest.", file=sys.stderr)
        return 1
    match_opt = match_map[raw_match]
    ctype = ensure_output_container(payload["out"], len(items) > 1)
    failures = 0
    for idx, src in enumerate(items, 1):
        dst = resolve_out_target(src, "vector", payload["out"], ctype)
        try:
            arcpy.analysis.SpatialJoin(src, join_fc, dst, match_option=match_opt)
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} + {os.path.basename(join_fc)} ({match_opt}) -> {dst}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} joined, {failures} failed.")
    return 1 if failures else 0


def checkout_spatial_analyst():
    if arcpy.CheckExtension("Spatial") != "Available":
        raise RuntimeError(
            "ArcGIS Spatial Analyst extension is not available/licensed. "
            "Tip: For slope/aspect/hillshade or mosaic without Spatial Analyst, use `--engine gdal`."
        )
    arcpy.CheckOutExtension("Spatial")


def cmd_raster_mosaic(payload):
    rasters = [p for p, kind in collect_datasets(payload["input"], include_rasters=True) if kind == "raster"]
    if not rasters:
        print("No raster datasets found to mosaic.", file=sys.stderr)
        return 1
    out_path = payload["out"]
    out_dir = os.path.dirname(out_path) or "."
    os.makedirs(out_dir, exist_ok=True)
    out_name = os.path.basename(out_path)
    desc = arcpy.Describe(rasters[0])
    bands = getattr(desc, "bandCount", 1) or 1
    arcpy.management.MosaicToNewRaster(
        input_rasters=rasters,
        output_location=out_dir,
        raster_dataset_name_with_extension=out_name,
        pixel_type="32_BIT_FLOAT",
        number_of_bands=bands,
        mosaic_method="LAST",
    )
    print(f"OK Mosaicked {len(rasters)} raster(s) -> {out_path}")
    return 0


def cmd_raster_terrain(payload):
    checkout_spatial_analyst()
    from arcpy import sa

    dem = payload["input"]
    out_dir = payload["out"]
    os.makedirs(out_dir, exist_ok=True)
    stem = sanitize_gdb_name(dem)

    if payload.get("slope"):
        dst = os.path.join(out_dir, f"{stem}_slope.tif")
        sa.Slope(dem, "DEGREE").save(dst)
        print(f"[OK] Slope     -> {dst}")
    if payload.get("aspect"):
        dst = os.path.join(out_dir, f"{stem}_aspect.tif")
        sa.Aspect(dem).save(dst)
        print(f"[OK] Aspect    -> {dst}")
    if payload.get("hillshade"):
        dst = os.path.join(out_dir, f"{stem}_hillshade.tif")
        sa.Hillshade(dem).save(dst)
        print(f"[OK] Hillshade -> {dst}")
    return 0


def cmd_raster_zonal_stats(payload):
    checkout_spatial_analyst()
    from arcpy import sa

    raster = payload["input"]
    zones = payload["zones"]
    zone_field = payload["zone_field"]
    out_path = payload["out"]
    parent = os.path.dirname(out_path) or "."
    os.makedirs(parent, exist_ok=True)

    if out_path.lower().endswith(".csv"):
        tmp_tbl = os.path.join(arcpy.env.scratchGDB, "geokit_zonal_tmp")
        sa.ZonalStatisticsAsTable(zones, zone_field, raster, tmp_tbl, "DATA", "ALL")
        arcpy.conversion.TableToTable(tmp_tbl, parent, os.path.basename(out_path))
        arcpy.management.Delete(tmp_tbl)
    else:
        sa.ZonalStatisticsAsTable(zones, zone_field, raster, out_path, "DATA", "ALL")
    print(f"OK Zonal statistics saved -> {out_path}")
    return 0


def cmd_hydro_delineate(payload):
    checkout_spatial_analyst()
    from arcpy import sa

    dem = payload["dem"]
    out_dir = payload["out"]
    threshold = int(payload.get("threshold") or 1000)
    pour_point = payload.get("pour_point")
    snap_dist = float(payload.get("snap_dist") or 100.0)
    os.makedirs(out_dir, exist_ok=True)

    print("[1/5] Filling DEM sinks...")
    filled_path = os.path.join(out_dir, "01_dem_filled.tif")
    filled = sa.Fill(dem)
    filled.save(filled_path)

    print("[2/5] Computing D8 Flow Direction...")
    fdir_path = os.path.join(out_dir, "02_flow_dir.tif")
    fdir = sa.FlowDirection(filled, "NORMAL")
    fdir.save(fdir_path)

    print("[3/5] Computing Flow Accumulation...")
    facc_path = os.path.join(out_dir, "03_flow_acc.tif")
    facc = sa.FlowAccumulation(fdir)
    facc.save(facc_path)

    print(f"[4/5] Extracting Stream Network (threshold >= {threshold} cells)...")
    streams_shp = os.path.join(out_dir, "streams.shp")
    stream_ras = sa.Con(facc >= threshold, 1)
    try:
        sa.StreamToFeature(stream_ras, fdir, streams_shp, "SIMPLIFY")
        print(f"      -> {streams_shp}")
    except Exception as exc:
        print(f"      [WARN] No streams extracted at threshold {threshold} ({exc})")

    if pour_point:
        print(f"[5/5] Snapping pour point ({snap_dist} map units) & delineating watershed...")
        snapped = sa.SnapPourPoint(pour_point, facc, snap_dist)
        ws_ras = sa.Watershed(fdir, snapped)
        ws_tif = os.path.join(out_dir, "watershed.tif")
        ws_ras.save(ws_tif)
        ws_shp = os.path.join(out_dir, "watershed.shp")
        arcpy.conversion.RasterToPolygon(ws_ras, ws_shp, "SIMPLIFY", "VALUE")
        print(f"      -> {ws_shp}")
    else:
        print("[5/5] No --pour-point provided; skipping single-outlet watershed polygon.")
        print("      Tip: Pass `--pour-point <outlet.shp>` or run `geokit hydro basin`.")

    print(f"OK Hydrology delineation complete -> {out_dir}")
    return 0


def cmd_hydro_basin(payload):
    checkout_spatial_analyst()
    from arcpy import sa

    dem = payload["dem"]
    out_dir = payload["out"]
    os.makedirs(out_dir, exist_ok=True)

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
    arcpy.conversion.RasterToPolygon(basin_ras, basins_shp, "SIMPLIFY", "VALUE")
    print(f"OK Drainage basins saved -> {basins_shp}")
    return 0


def auto_checkout_extension(tool_name: str):
    lower = tool_name.lower()
    ext_map = [
        ("_sa", "Spatial"),
        ("_3d", "3D"),
        ("_na", "Network"),
        ("_ia", "ImageAnalyst"),
        ("_ga", "GeoStats"),
        ("_ta", "Tracking"),
        ("_ba", "Business"),
    ]
    for suffix, ext_name in ext_map:
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
        pattern = f"*{pattern}*"
    tools = sorted(arcpy.ListTools(pattern) or [])
    if payload.get("json"):
        print(json.dumps({"pattern": pattern, "count": len(tools), "tools": tools}, indent=2))
        return 0
    print(f"Found {len(tools)} ArcGIS Pro tool(s) matching '{pattern}':")
    for t in tools[:150]:
        print(f"  - {t}")
    if len(tools) > 150:
        print(f"  ... ({len(tools) - 150} more; narrow your search keyword)")
    return 0


def cmd_gp_tool_usage(payload):
    tool = payload.get("tool") or ""
    print(arcpy.Usage(tool))
    return 0


def cmd_gp_run(payload):
    tool = (payload.get("tool") or "").strip()
    args = payload.get("args") or []
    if not tool or tool.startswith("_") or not tool[0].isalpha() or not all(c.isalnum() or c == "_" for c in tool):
        print(f"Invalid ArcPy tool name '{tool}'.", file=sys.stderr)
        return 1
    auto_checkout_extension(tool)
    fn = getattr(arcpy, tool, None)
    if fn is None:
        fn = getattr(arcpy.gp, tool, None)
    if fn is None or not callable(fn) or isinstance(fn, type):
        print(f"Unknown ArcPy tool '{tool}'. Run `geokit gp tools <keyword>` to search.", file=sys.stderr)
        return 1
    norm_args = []
    for a in args:
        s = str(a)
        if ("\\" in s or "/" in s) and not os.path.isabs(s) and not s.startswith(("http://", "https://")):
            norm_args.append(os.path.abspath(s))
        else:
            norm_args.append(a)
    res = fn(*norm_args)
    out_val = res.getOutput(0) if res else ""
    print(f"OK {tool} completed" + (f" -> {out_val}" if out_val else ""))
    return 0


def cmd_layout_map_series(payload):
    aprx = arcpy.mp.ArcGISProject(payload["input"])
    target_name = payload.get("layout_name")
    layouts = aprx.listLayouts(target_name) if target_name else aprx.listLayouts()
    lyt = None
    for candidate in layouts:
        ms = getattr(candidate, "mapSeries", None)
        if ms and ms.enabled:
            lyt = candidate
            break
    if lyt is None:
        print("No enabled Map Series found in the specified .aprx layout(s).", file=sys.stderr)
        return 1

    ms = lyt.mapSeries
    out_path = payload["out"]
    fmt = (payload.get("format") or "pdf").lower()
    dpi = int(payload.get("dpi") or 300)
    multi = bool(payload.get("multiple_files", False))

    if fmt == "pdf" and not multi:
        parent = os.path.dirname(out_path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        ms.exportToPDF(out_path, page_range_type="ALL", resolution=dpi)
        print(f"OK Exported {ms.pageCount} Map Series page(s) -> {out_path}")
        return 0

    os.makedirs(out_path, exist_ok=True)
    for page_num in range(1, ms.pageCount + 1):
        ms.currentPageNumber = page_num
        page_name = sanitize_gdb_name(str(getattr(ms.pageRow, ms.pageNameField.name, page_num)))
        dst = os.path.join(out_path, f"{page_num:03d}_{page_name}.{fmt}")
        if fmt == "pdf":
            lyt.exportToPDF(dst, resolution=dpi)
        else:
            lyt.exportToPNG(dst, resolution=dpi)
        print(f"[{page_num}/{ms.pageCount}] OK {dst}")
    print(f"Summary: Exported {ms.pageCount} Map Series page(s) -> {out_path}")
    return 0


def cmd_map_add_layer(payload):
    aprx_path = payload["input"]
    aprx = arcpy.mp.ArcGISProject(aprx_path)
    map_name = payload.get("map_name")
    maps = aprx.listMaps(map_name) if map_name else aprx.listMaps()
    if not maps:
        print("No matching map found in .aprx project.", file=sys.stderr)
        return 1
    m = maps[0]
    added = m.addDataFromPath(payload["layer"])
    if payload.get("lyrx"):
        arcpy.management.ApplySymbologyFromLayer(added, payload["lyrx"])
    out_path = payload.get("out")
    if out_path:
        parent = os.path.dirname(out_path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        aprx.saveACopy(out_path)
        print(f"OK Added '{added.name}' to map '{m.name}' and saved copy -> {out_path}")
    else:
        aprx.save()
        print(f"OK Added '{added.name}' to map '{m.name}' and saved -> {aprx_path}")
    return 0


def cmd_map_style(payload):
    aprx_path = payload["input"]
    aprx = arcpy.mp.ArcGISProject(aprx_path)
    map_name = payload.get("map_name")
    maps = aprx.listMaps(map_name) if map_name else aprx.listMaps()
    if not maps:
        print("No matching map found in .aprx project.", file=sys.stderr)
        return 1
    m = maps[0]
    target_name = (payload.get("layer") or "").lower()
    lyr = None
    for cand in m.listLayers():
        if cand.name.lower() == target_name or target_name in cand.name.lower():
            lyr = cand
            break
    if lyr is None:
        print(f"Layer '{payload.get('layer')}' not found in map '{m.name}'.", file=sys.stderr)
        return 1

    field = payload.get("field") or ""
    preset = (payload.get("preset") or "hazard").lower()
    transparency = int(payload.get("transparency") or 0)
    no_outline = bool(payload.get("no_outline", True))
    classes_raw = payload.get("classes")

    palettes = {
        "hazard": [[46, 125, 50, 100], [251, 192, 45, 100], [211, 47, 47, 100]],
        "hazard3": [[46, 125, 50, 100], [251, 192, 45, 100], [211, 47, 47, 100]],
        "hazard5": [[27, 94, 32, 100], [102, 187, 106, 100], [253, 216, 53, 100], [251, 140, 0, 100], [198, 40, 40, 100]],
        "blue": [[144, 202, 249, 100], [30, 136, 229, 100], [13, 71, 161, 100]],
        "earth": [[161, 136, 127, 100], [156, 204, 101, 100], [46, 125, 50, 100]],
        "category": [
            [31, 120, 180, 100], [51, 160, 44, 100], [227, 26, 28, 100], [255, 127, 0, 100],
            [106, 61, 154, 100], [177, 89, 40, 100], [166, 206, 227, 100], [178, 223, 138, 100],
            [251, 154, 153, 100], [253, 191, 111, 100], [202, 178, 214, 100], [255, 255, 153, 100],
        ],
    }
    colors = palettes.get(preset, palettes["hazard"])

    sym = lyr.symbology
    sym.updateRenderer("UniqueValueRenderer")
    sym.renderer.fields = [field]

    severity_rank = {
        "sangat rendah": 1, "very low": 1, "aman": 1, "tidak rawan": 1,
        "rendah": 2, "low": 2, "agak rendah": 2,
        "sedang": 3, "medium": 3, "moderate": 3,
        "tinggi": 4, "high": 4, "agak tinggi": 4,
        "sangat tinggi": 5, "very high": 5, "ekstrem": 5,
    }
    custom_order = [c.strip().lower() for c in classes_raw.split(",") if c.strip()] if classes_raw else []

    def _item_rank(it):
        low = str(it.label).strip().lower()
        if custom_order and low in custom_order:
            return (0, custom_order.index(low), low)
        if low in severity_rank:
            return (1, severity_rank[low], low)
        try:
            return (2, float(low), low)
        except ValueError:
            return (3, 0, low)

    for grp in sym.renderer.groups:
        items_sorted = sorted(grp.items, key=_item_rank)
        for idx, item in enumerate(items_sorted):
            rgba = colors[idx % len(colors)]
            item.symbol.color = {"RGB": rgba}
            if no_outline:
                item.symbol.outlineColor = {"RGB": [0, 0, 0, 0]}
                try:
                    item.symbol.outlineWidth = 0
                except Exception:
                    pass
    lyr.symbology = sym
    if transparency > 0:
        lyr.transparency = transparency

    out_path = payload.get("out")
    if out_path:
        parent = os.path.dirname(out_path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        aprx.saveACopy(out_path)
        print(f"OK Styled '{lyr.name}' ({preset}, field={field}, transparency={transparency}%) -> {out_path}")
    else:
        aprx.save()
        print(f"OK Styled '{lyr.name}' ({preset}, field={field}, transparency={transparency}%) -> {aprx_path}")
    return 0


def cmd_layout_update(payload):
    aprx_path = payload["input"]
    aprx = arcpy.mp.ArcGISProject(aprx_path)
    layout_name = payload.get("layout_name")
    layouts = aprx.listLayouts(layout_name) if layout_name else aprx.listLayouts()
    if not layouts:
        print("No matching layout found in .aprx project.", file=sys.stderr)
        return 1
    lyt = layouts[0]
    title = payload.get("title")
    subtitle = payload.get("subtitle")
    source = payload.get("source")
    zoom_layer = payload.get("zoom_layer")

    updated = 0
    for el in lyt.listElements("TEXT_ELEMENT"):
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
        mfs = lyt.listElements("MAPFRAME_ELEMENT")
        if mfs:
            mf = mfs[0]
            target = zoom_layer.lower()
            for cand in mf.map.listLayers():
                if cand.name.lower() == target or target in cand.name.lower():
                    mf.camera.setExtent(mf.getLayerExtent(cand, False, True))
                    break

    out_path = payload.get("out")
    if out_path:
        parent = os.path.dirname(out_path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        aprx.saveACopy(out_path)
        print(f"OK Updated layout '{lyt.name}' ({updated} text element(s)) -> {out_path}")
    else:
        aprx.save()
        print(f"OK Updated layout '{lyt.name}' ({updated} text element(s)) -> {aprx_path}")
    return 0


def cmd_xy2shp(payload):
    in_csv = payload["input"]
    out_path = payload["out"]
    x_field = payload.get("x_field") or "lon"
    y_field = payload.get("y_field") or "lat"
    epsg = int(payload.get("epsg") or 4326)
    ensure_output_container(out_path, is_batch=False)
    sr = arcpy.SpatialReference(epsg)
    arcpy.management.XYTableToPoint(in_csv, out_path, x_field, y_field, coordinate_system=sr)
    print(f"OK Created point dataset from CSV ({x_field}, {y_field}, EPSG:{epsg}) -> {out_path}")
    return 0


def cmd_convert(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    if not items:
        print("No vector datasets found to convert.")
        return 0
    fmt = (payload.get("target_format") or "shp").lower()
    out_path = payload["out"]
    is_batch = len(items) > 1 or os.path.isdir(out_path) or not os.path.splitext(out_path)[1]
    if is_batch:
        os.makedirs(out_path, exist_ok=True)
    else:
        parent = os.path.dirname(out_path)
        if parent:
            os.makedirs(parent, exist_ok=True)

    failures = 0
    for idx, src in enumerate(items, 1):
        stem = sanitize_gdb_name(src)
        dst = os.path.join(out_path, f"{stem}.{fmt}") if is_batch else out_path
        try:
            if fmt == "geojson":
                arcpy.conversion.FeaturesToJSON(src, dst, geoJSON="GEOJSON")
            elif fmt == "shp":
                arcpy.management.CopyFeatures(src, dst)
            elif fmt == "gpkg":
                if not os.path.exists(dst):
                    arcpy.management.CreateSQLiteDatabase(dst, "GEOPACKAGE")
                arcpy.conversion.FeatureClassToFeatureClass(src, dst, stem)
            else:
                raise ValueError(f"Format '{fmt}' via ArcPy not supported; install GDAL/QGIS or use `--engine gdal`.")
            print(f"[{idx}/{len(items)}] OK   {os.path.basename(src)} -> {dst}")
        except Exception as exc:
            failures += 1
            print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    print(f"Summary: {len(items) - failures} converted, {failures} failed.")
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
        crs_set.add(f"{crs_name} ({epsg})")
        issues = []

        if not sr or crs_name == "Unknown":
            issues.append("Missing/Unknown CRS")
        elif expected_epsg and epsg != int(expected_epsg):
            issues.append(f"CRS mismatch: got EPSG:{epsg}, expected EPSG:{expected_epsg}")

        if kind == "vector":
            # Geometry validity check
            try:
                chk_tbl = os.path.join(arcpy.env.scratchGDB, "geokit_qa_geom")
                if arcpy.Exists(chk_tbl):
                    arcpy.management.Delete(chk_tbl)
                arcpy.management.CheckGeometry(ds_path, chk_tbl)
                bad_count = int(arcpy.management.GetCount(chk_tbl)[0])
                if bad_count > 0:
                    issues.append(f"{bad_count} invalid geometry problem(s)")
                arcpy.management.Delete(chk_tbl)
            except Exception as exc:
                issues.append(f"CheckGeometry skipped: {exc}")

            # Required NULL/empty field checks
            if check_nulls:
                existing_fields = {f.name.lower(): f.name for f in arcpy.ListFields(ds_path)}
                for req_f in check_nulls:
                    actual = existing_fields.get(req_f.lower())
                    if not actual:
                        issues.append(f"Missing required field '{req_f}'")
                    else:
                        null_cnt = 0
                        with arcpy.da.SearchCursor(ds_path, [actual]) as cur:
                            for (val,) in cur:
                                if val is None or (isinstance(val, str) and not val.strip()):
                                    null_cnt += 1
                        if null_cnt > 0:
                            issues.append(f"Field '{actual}' has {null_cnt} NULL/empty value(s)")

            # Polygon self-overlap check
            if check_overlaps and getattr(desc, "shapeType", "") == "Polygon":
                try:
                    ov_fc = os.path.join(arcpy.env.scratchGDB, "geokit_qa_ov")
                    if arcpy.Exists(ov_fc):
                        arcpy.management.Delete(ov_fc)
                    arcpy.analysis.Intersect([ds_path], ov_fc)
                    ov_count = int(arcpy.management.GetCount(ov_fc)[0])
                    if ov_count > 0:
                        issues.append(f"{ov_count} overlapping polygon pair(s)")
                    arcpy.management.Delete(ov_fc)
                except Exception as exc:
                    issues.append(f"Overlap check skipped: {exc}")

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

    if payload.get("json"):
        print(json.dumps({
            "datasets_checked": len(report),
            "unique_crs_count": len(crs_set),
            "total_issues": total_issues,
            "results": report,
        }, indent=2))
        return 0

    print(f"Spatial QA Report ({len(report)} dataset(s) checked):")
    if len(crs_set) > 1:
        print(f"  [WARN] Multiple CRS detected across workspace: {', '.join(sorted(crs_set))}")
    for item in report:
        tag = "[PASS]" if item["status"] == "PASS" else "[WARN]"
        epsg_s = f"EPSG:{item['epsg']}" if item["epsg"] else "No EPSG"
        print(f"  {tag} {item['name']} ({item['crs']} / {epsg_s})")
        for iss in item["issues"]:
            print(f"         - {iss}")
    print(f"Summary: {len(report)} checked, {total_issues} issue(s) found.")
    return 0


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
            raise ValueError(f"Invalid rule syntax '{r}'. Use 'key=val' or 'range:val'.")
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
            # Check numeric range a-b (allowing negative a)
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
            if str(val).strip().lower() == a:
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
                arcpy.management.CalculateStatistics(ds_path)
            except Exception:
                pass
            stats = {}
            for prop in ("MINIMUM", "MAXIMUM", "MEAN", "STD"):
                try:
                    val = arcpy.management.GetRasterProperties(ds_path, prop).getOutput(0)
                    stats[prop.lower()] = float(val.replace(",", "."))
                except Exception:
                    stats[prop.lower()] = None
            entry["raster_stats"] = stats
        else:
            fields = [f for f in arcpy.ListFields(ds_path) if f.type not in ("Geometry", "OID", "Blob", "Raster")]
            if target_field:
                fields = [f for f in fields if f.name.lower() == target_field.lower()]
                if not fields:
                    print(f"Field '{target_field}' not found in {ds_path}", file=sys.stderr)
                    return 1
            field_names = [f.name for f in fields]
            numeric_types = {"Integer", "SmallInteger", "Single", "Double"}
            counters = {f.name: Counter() for f in fields}
            num_vals = {f.name: [] for f in fields if f.type in numeric_types}
            null_counts = {f.name: 0 for f in fields}
            total_rows = 0

            if field_names:
                with arcpy.da.SearchCursor(ds_path, field_names) as cur:
                    for row in cur:
                        total_rows += 1
                        for idx, f in enumerate(fields):
                            v = row[idx]
                            if v is None or (isinstance(v, str) and not v.strip()):
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
                    mean = sum(vals) / len(vals)
                    var = sum((x - mean) ** 2 for x in vals) / len(vals)
                    fs["min"] = min(vals)
                    fs["max"] = max(vals)
                    fs["mean"] = round(mean, 4)
                    fs["std"] = round(math.sqrt(var), 4)
                if len(c) <= limit or f.type not in numeric_types:
                    fs["top_values"] = [{"value": str(k), "count": cnt} for k, cnt in c.most_common(limit)]
                field_summaries.append(fs)
            entry["fields"] = field_summaries
        results.append(entry)

    if payload.get("json"):
        print(json.dumps(results, indent=2))
        return 0

    for item in results:
        print(f"Dataset: {item['name']} ({item['kind']})")
        if item["kind"] == "raster":
            st = item["raster_stats"]
            print(f"  Raster stats: min={st['minimum']}  max={st['maximum']}  mean={st['mean']}  std={st['std']}")
        else:
            for fs in item["fields"]:
                head = f"  - {fs['field']} ({fs['type']}): {fs['distinct_count']} unique, {fs['null_count']} null"
                if "min" in fs:
                    head += f" | min={fs['min']}, max={fs['max']}, mean={fs['mean']}, std={fs['std']}"
                print(head)
                if fs.get("top_values"):
                    preview = ", ".join(f"{tv['value']} ({tv['count']})" for tv in fs["top_values"][:12])
                    more = f" ... (+{len(fs['top_values']) - 12} more)" if len(fs["top_values"]) > 12 else ""
                    print(f"      Values: {preview}{more}")
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

    existing = {f.name.lower(): f.name for f in arcpy.ListFields(ds_path)}
    actual_src = existing.get(src_field.lower())
    if not actual_src:
        print(f"Source field '{src_field}' not found in {ds_path}", file=sys.stderr)
        return 1

    actual_out = existing.get(out_field.lower())
    if not actual_out:
        if field_type == "TEXT":
            arcpy.management.AddField(ds_path, out_field, "TEXT", field_length=64)
        else:
            arcpy.management.AddField(ds_path, out_field, field_type)
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

    print(f"OK Classified '{actual_src}' -> '{actual_out}' ({field_type}): {matched} matched, {unmatched} unmatched/defaulted.")
    return 0


def cmd_calc_field(payload):
    import math

    ds_path = payload["input"]
    out_field = payload["field"]
    expr = payload["expr"]
    field_type = (payload.get("field_type") or "DOUBLE").upper()

    existing = {f.name.lower(): f.name for f in arcpy.ListFields(ds_path)}
    actual_out = existing.get(out_field.lower())
    if not actual_out:
        if field_type == "TEXT":
            arcpy.management.AddField(ds_path, out_field, "TEXT", field_length=128)
        else:
            arcpy.management.AddField(ds_path, out_field, field_type)
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
        print(f"Invalid/unsafe calc-field expression '{expr}': {exc}", file=sys.stderr)
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

    print(f"OK Calculated field '{actual_out}' ({field_type}) = {expr} on {updated} feature(s).")
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
        print(f"Union requires at least 2 vector layers (found {len(layers)}).", file=sys.stderr)
        return 1

    ensure_output_container(out_path, is_batch=False)
    arcpy.analysis.Union(layers, out_path, "ALL")
    cnt = int(arcpy.management.GetCount(out_path)[0])
    print(f"OK Unioned {len(layers)} layer(s) -> {out_path} ({cnt} feature(s))")
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

    ranges = _parse_remap_ranges(rules)
    if not rules or not str(rules).strip():
        print("Reclassify rules cannot be empty.", file=sys.stderr)
        return 1
    err = _check_safe_output(out_r, [in_r], overwrite)
    if err:
        print(err, file=sys.stderr)
        return 1

    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import Reclassify, RemapRange, MajorityFilter

    parent = os.path.dirname(out_r)
    if parent:
        os.makedirs(parent, exist_ok=True)
    if overwrite and arcpy.Exists(out_r):
        arcpy.management.Delete(out_r)
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
    print(f"OK Reclassified raster -> {out_r}" + (" (smoothed)" if smooth else ""))
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
        print("weighted-sum requires at least one raster:weight pair.", file=sys.stderr)
        return 1
    in_paths = []
    w_sum = 0.0
    for r_path, weight in pairs:
        wf = float(weight)
        if math.isnan(wf) or math.isinf(wf) or wf < 0.0:
            print(f"Invalid weight '{weight}' for {r_path} (must be finite and >= 0).", file=sys.stderr)
            return 1
        w_sum += wf
        in_paths.append(r_path)
    if abs(w_sum - 1.0) > 0.01:
        print(f"Note: Sum of weights is {w_sum:.4f} (expected 1.0 for normalized weighted overlay).", file=sys.stderr)

    err = _check_safe_output(out_r, in_paths, overwrite)
    if err:
        print(err, file=sys.stderr)
        return 1
    if poly_out:
        err_p = _check_safe_output(poly_out, in_paths + [out_r], overwrite)
        if err_p:
            print(err_p, file=sys.stderr)
            return 1

    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import Raster, Reclassify, RemapRange, MajorityFilter

    parent = os.path.dirname(out_r)
    if parent:
        os.makedirs(parent, exist_ok=True)
    if overwrite and arcpy.Exists(out_r):
        arcpy.management.Delete(out_r)

    total = None
    for r_path, weight in pairs:
        term = Raster(r_path) * float(weight)
        total = term if total is None else (total + term)
    total.save(out_r)
    print(f"OK Weighted sum ({len(pairs)} rasters) -> {out_r}")

    if n_classes or poly_out:
        n_cls = int(n_classes or 5)
        try:
            arcpy.management.CalculateStatistics(out_r)
        except Exception:
            pass
        rmin = float(arcpy.management.GetRasterProperties(out_r, "MINIMUM").getOutput(0).replace(",", "."))
        rmax = float(arcpy.management.GetRasterProperties(out_r, "MAXIMUM").getOutput(0).replace(",", "."))
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
            arcpy.management.Delete(cls_tif)
        Reclassify(out_r, "Value", RemapRange(ranges), "NODATA").save(cls_tif)
        if smooth:
            sm_tif = os.path.join(arcpy.env.scratchFolder, "geokit_wsum_sm.tif")
            if arcpy.Exists(sm_tif):
                arcpy.management.Delete(sm_tif)
            MajorityFilter(cls_tif, "EIGHT", "MAJORITY").save(sm_tif)
            cls_tif = sm_tif
        if poly_out:
            p_parent = os.path.dirname(poly_out)
            if p_parent:
                os.makedirs(p_parent, exist_ok=True)
            raw_poly = os.path.join(arcpy.env.scratchGDB, "geokit_wsum_raw_poly")
            if arcpy.Exists(raw_poly):
                arcpy.management.Delete(raw_poly)
            if overwrite and arcpy.Exists(poly_out):
                arcpy.management.Delete(poly_out)
            arcpy.conversion.RasterToPolygon(cls_tif, raw_poly, "SIMPLIFY", "Value")
            arcpy.management.Dissolve(raw_poly, poly_out, ["gridcode"])
            if n_cls == 5:
                lbls = {1: "Sangat Rendah", 2: "Rendah", 3: "Sedang", 4: "Tinggi", 5: "Sangat Tinggi"}
            elif n_cls == 3:
                lbls = {1: "Rendah", 2: "Sedang", 3: "Tinggi"}
            else:
                lbls = {i + 1: f"Kelas {i + 1}" for i in range(n_cls)}
            arcpy.management.AddField(poly_out, "KELAS", "TEXT", field_length=32)
            arcpy.management.AddField(poly_out, "AREA_HA", "DOUBLE")
            with arcpy.da.UpdateCursor(poly_out, ["gridcode", "KELAS"]) as cur:
                for row in cur:
                    row[1] = lbls.get(int(row[0]), f"Kelas {row[0]}")
                    cur.updateRow(row)
            try:
                arcpy.management.CalculateField(poly_out, "AREA_HA", "!shape.geodesicArea@hectares!", "PYTHON3")
            except Exception:
                arcpy.management.CalculateField(poly_out, "AREA_HA", "!shape.area@hectares!", "PYTHON3")
            print(f"OK Classified polygon ({n_cls} classes) -> {poly_out}")
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
        print(f"Invalid interpolation method '{method}'. Allowed: idw, spline, kriging.", file=sys.stderr)
        return 1
    err = _check_safe_output(out_r, [in_pts, mask], overwrite)
    if err:
        print(err, file=sys.stderr)
        return 1

    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import Idw, Spline, Kriging, KrigingModelOrdinary, ExtractByMask

    parent = os.path.dirname(out_r)
    if parent:
        os.makedirs(parent, exist_ok=True)
    if overwrite and arcpy.Exists(out_r):
        arcpy.management.Delete(out_r)

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

    print(f"OK Interpolated ({method.upper()}, field={field}) -> {out_r}")
    return 0


def cmd_raster_index(payload):
    idx_type = (payload.get("index_type") or "ndvi").lower()
    out_r = payload["out"]
    poly_out = payload.get("polygon_out")
    overwrite = _is_overwrite(payload)

    if idx_type not in ALLOWED_INDEX_TYPES:
        print(f"Invalid spectral index type '{idx_type}'. Allowed: ndvi, ndwi, ndbi, savi, nbr.", file=sys.stderr)
        return 1
    err = _check_safe_output(out_r, [payload.get("band_a"), payload.get("band_b")], overwrite)
    if err:
        print(err, file=sys.stderr)
        return 1
    if poly_out:
        err_p = _check_safe_output(poly_out, [payload.get("band_a"), payload.get("band_b"), out_r], overwrite)
        if err_p:
            print(err_p, file=sys.stderr)
            return 1

    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import Float, Raster, Con, Reclassify, RemapRange

    b1 = Float(Raster(payload["band_a"]))
    b2 = Float(Raster(payload["band_b"]))
    parent = os.path.dirname(out_r)
    if parent:
        os.makedirs(parent, exist_ok=True)
    if overwrite and arcpy.Exists(out_r):
        arcpy.management.Delete(out_r)

    denom = b1 + b2
    if idx_type == "savi":
        res = Con((denom + 0.5) != 0, ((b1 - b2) / (denom + 0.5)) * 1.5, 0.0)
    else:
        res = Con(denom != 0, (b1 - b2) / denom, 0.0)
    res.save(out_r)
    print(f"OK Spectral index ({idx_type.upper()}) -> {out_r}")

    n_classes = payload.get("classes")
    poly_out = payload.get("polygon_out")
    if n_classes or poly_out:
        n_cls = int(n_classes or 5)
        try:
            arcpy.management.CalculateStatistics(out_r)
            rmin = float(str(arcpy.management.GetRasterProperties(out_r, "MINIMUM").getOutput(0)).replace(",", "."))
            rmax = float(str(arcpy.management.GetRasterProperties(out_r, "MAXIMUM").getOutput(0)).replace(",", "."))
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
            arcpy.management.Delete(cls_tif)
        Reclassify(out_r, "Value", RemapRange(ranges), "NODATA").save(cls_tif)
        if poly_out:
            p_parent = os.path.dirname(poly_out)
            if p_parent:
                os.makedirs(p_parent, exist_ok=True)
            raw_poly = os.path.join(arcpy.env.scratchGDB, "geokit_idx_raw_poly")
            if arcpy.Exists(raw_poly):
                arcpy.management.Delete(raw_poly)
            if arcpy.Exists(poly_out):
                arcpy.management.Delete(poly_out)
            arcpy.conversion.RasterToPolygon(cls_tif, raw_poly, "SIMPLIFY", "Value")
            arcpy.management.Dissolve(raw_poly, poly_out, ["gridcode"])
            arcpy.management.AddField(poly_out, "KELAS", "TEXT", field_length=32)
            arcpy.management.AddField(poly_out, "AREA_HA", "DOUBLE")
            with arcpy.da.UpdateCursor(poly_out, ["gridcode", "KELAS"]) as cur:
                for row in cur:
                    row[1] = f"{idx_type.upper()} Kelas {row[0]}"
                    cur.updateRow(row)
            try:
                arcpy.management.CalculateField(poly_out, "AREA_HA", "!shape.geodesicArea@hectares!", "PYTHON3")
            except Exception:
                pass
            print(f"OK Classified {idx_type.upper()} polygon ({n_cls} classes) -> {poly_out}")
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
    if parent:
        os.makedirs(parent, exist_ok=True)
    if arcpy.Exists(out_fc):
        arcpy.management.Delete(out_fc)

    Contour(in_dem, out_fc, interval, base_c)
    arcpy.management.AddField(out_fc, "INDEX_KTR", "SHORT")
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
    print(f"OK Contours (interval={interval:.2f}, index every {idx_every} -> {cnt} lines) -> {out_fc}")
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
    "qa_check": cmd_qa_check,
    "layout_list": cmd_layout_list,
    "layout_export": cmd_layout_export,
    "layout_map_series": cmd_layout_map_series,
    "layout_update": cmd_layout_update,
    "map_broken_links": cmd_map_broken_links,
    "map_fix_links": cmd_map_fix_links,
    "map_add_layer": cmd_map_add_layer,
    "map_style": cmd_map_style,
    "raster_mosaic": cmd_raster_mosaic,
    "raster_terrain": cmd_raster_terrain,
    "raster_zonal_stats": cmd_raster_zonal_stats,
    "raster_reclass": cmd_raster_reclass,
    "raster_weighted_sum": cmd_raster_weighted_sum,
    "raster_interpolate": cmd_raster_interpolate,
    "raster_index": cmd_raster_index,
    "raster_contour": cmd_raster_contour,
    "hydro_delineate": cmd_hydro_delineate,
    "hydro_basin": cmd_hydro_basin,
}


def _run_payload_captured(payload):
    import io
    import traceback

    action = payload.get("action")
    handler = HANDLERS.get(action)
    if not handler:
        return 2, "", f"Unknown geokit gp action: {action}\n"

    old_out, old_err = sys.stdout, sys.stderr
    buf_out, buf_err = io.StringIO(), io.StringIO()
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
                return 1, "", chk_err + "\n"
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
    return code, buf_out.getvalue(), buf_err.getvalue()


def _verify_local_token(req_token) -> bool:
    import hmac

    expected = os.environ.get("GEOKIT_AUTH_TOKEN", "").strip()
    if not expected:
        home = os.environ.get("USERPROFILE") or os.path.expanduser("~")
        tok_path = os.path.join(home, ".geokit", "token")
        if os.path.isfile(tok_path):
            try:
                with open(tok_path, "r", encoding="utf-8") as fh:
                    expected = fh.read().strip()
            except Exception:
                expected = ""
    if not expected:
        return True
    if not isinstance(req_token, str):
        return False
    return hmac.compare_digest(req_token, expected)


def _serve_daemon(port: int, idle_minutes: int = 30):
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
                    if b"\n" in data:
                        break
                if too_large:
                    err_resp = {"ok": False, "code": 1, "stdout": "", "stderr": "Request payload exceeds 10 MB limit\n"}
                    conn.sendall((json.dumps(err_resp) + "\n").encode("utf-8"))
                    continue
                raw = b"".join(chunks).decode("utf-8", errors="replace").strip()
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
                        "engine": "arcgis",
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
                    err_resp = {"ok": False, "code": 1, "stdout": "", "stderr": f"{exc}\n"}
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
        sys.stderr.write("Usage: arcpy_runner.py <payload.json> | --serve <port>\n")
        sys.exit(2)
    with open(sys.argv[1], "r", encoding="utf-8") as fh:
        payload = json.load(fh)
    action = payload.get("action")
    handler = HANDLERS.get(action)
    if not handler:
        sys.stderr.write(f"Unknown geokit gp action: {action}\n")
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
