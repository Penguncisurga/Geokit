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
    streams_ok = False
    try:
        sa.StreamToFeature(stream_ras, fdir, streams_shp, "SIMPLIFY")
        if arcpy.Exists(streams_shp) and int(arcpy.management.GetCount(streams_shp)[0]) > 0:
            streams_ok = True
            print(f"      -> {streams_shp}")
        else:
            print(f"      [WARN] Stream feature class is empty at threshold {threshold} (try a lower --threshold).")
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

    if streams_ok:
        print(f"OK Hydrology delineation complete -> {out_dir}")
    else:
        print(f"PARTIAL Hydrology outputs written to {out_dir}, but stream network was NOT extracted (lower --threshold).")
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

            # Polygon pairwise overlap check (license-independent via bounding-box prefilter + geometry predicates)
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
                        issues.append(f"{ov_count} overlapping polygon pair(s)")
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

    print(f"Spatial Technical QA Report ({len(report)} dataset(s) checked — CRS/geometry/NULL/overlaps):")
    if mixed_crs:
        print(f"  [WARN] Multiple CRS detected across workspace: {', '.join(sorted(crs_set))}")
    for item in report:
        tag = "[PASS]" if item["status"] == "PASS" else "[WARN]"
        epsg_s = f"EPSG:{item['epsg']}" if item["epsg"] else "No EPSG"
        print(f"  {tag} {item['name']} ({item['crs']} / {epsg_s})")
        for iss in item["issues"]:
            print(f"         - {iss}")
    print(f"Summary: {len(report)} checked, {total_issues} issue(s) found.")
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


def cmd_select(payload):
    src = payload["input"]
    out_path = payload["out"]
    where = payload.get("where_clause") or ""
    ensure_output_container(out_path, is_batch=False)
    arcpy.analysis.Select(src, out_path, where)
    cnt = int(arcpy.management.GetCount(out_path)[0])
    print(f"OK Selected {cnt} feature(s) ({where}) -> {out_path}")
    return 0


def cmd_export_table(payload):
    import csv

    src = payload["input"]
    out_path = payload["out"]
    parent = os.path.dirname(out_path)
    if parent:
        os.makedirs(parent, exist_ok=True)
    fields = [f.name for f in arcpy.ListFields(src) if f.type not in ("Geometry", "Blob", "Raster")]
    if not fields:
        print(f"No exportable attribute fields in {src}", file=sys.stderr)
        return 1
    rows = 0
    with open(out_path, "w", encoding="utf-8", newline="") as fh:
        writer = csv.writer(fh)
        writer.writerow(fields)
        with arcpy.da.SearchCursor(src, fields) as cur:
            for row in cur:
                writer.writerow(["" if v is None else v for v in row])
                rows += 1
    print(f"OK Exported {rows} row(s) x {len(fields)} col(s) -> {out_path}")
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
                arcpy.management.CopyFeatures(src, dst)
                print(f"[{idx}/{len(items)}] OK   Flattened 2D {os.path.basename(src)} -> {dst}")
            except Exception as exc:
                failures += 1
                print(f"[{idx}/{len(items)}] ERR  {os.path.basename(src)}: {exc}", file=sys.stderr)
    finally:
        arcpy.env.outputZFlag = prev_z
        arcpy.env.outputMFlag = prev_m
    return 1 if failures else 0


def cmd_define_crs(payload):
    src = payload["input"]
    epsg = int(payload["epsg"])
    sr = arcpy.SpatialReference(epsg)
    arcpy.management.DefineProjection(src, sr)
    print(f"OK Defined CRS EPSG:{epsg} ({sr.name}) -> {src}")
    return 0


def cmd_split(payload):
    src = payload["input"]
    req_field = payload["field"]
    out_dir = payload["out"]
    os.makedirs(out_dir, exist_ok=True)
    existing = {f.name.lower(): f for f in arcpy.ListFields(src)}
    fobj = existing.get(req_field.lower())
    if not fobj:
        print(f"Field '{req_field}' not found in {src}", file=sys.stderr)
        return 1
    fname = fobj.name
    is_str = fobj.type == "String"
    delim = arcpy.AddFieldDelimiters(src, fname)
    vals = []
    seen = set()
    with arcpy.da.SearchCursor(src, [fname]) as cur:
        for (v,) in cur:
            if v is None:
                continue
            sv = str(v).strip()
            if sv and sv not in seen:
                seen.add(sv)
                vals.append(v)
    if not vals:
        print(f"No non-empty values found in field '{fname}'.", file=sys.stderr)
        return 1
    overwrite = _is_overwrite(payload)
    for idx, v in enumerate(vals, 1):
        safe = sanitize_gdb_name(str(v))
        dst = os.path.join(out_dir, f"{safe}.shp")
        err = _check_safe_output(dst, [src], overwrite)
        if err:
            print(err, file=sys.stderr)
            return 1
        if is_str:
            esc = str(v).replace("'", "''")
            where = f"{delim} = '{esc}'"
        else:
            where = f"{delim} = {v}"
        arcpy.analysis.Select(src, dst, where)
        print(f"[{idx}/{len(vals)}] OK   {fname}={v} -> {dst}")
    return 0


def cmd_clean_fields(payload):
    src = payload["input"]
    keep_raw = payload.get("keep_fields")
    drop_raw = payload.get("drop_fields")
    fields = arcpy.ListFields(src)
    removable = [f.name for f in fields if not f.required and f.type not in ("OID", "Geometry")]
    if keep_raw:
        keep_set = {s.strip().lower() for s in keep_raw.split(",") if s.strip()}
        to_drop = [n for n in removable if n.lower() not in keep_set]
    elif drop_raw:
        drop_set = {s.strip().lower() for s in drop_raw.split(",") if s.strip()}
        to_drop = [n for n in removable if n.lower() in drop_set]
    else:
        junk_re = re.compile(r"^(fid_.*|orig_fid|shape_le_.*|shape_ar_.*|objectid_.*|id_\d+)$", re.I)
        to_drop = [n for n in removable if junk_re.match(n)]
    if not to_drop:
        print(f"No matching fields to drop in {src}.")
        return 0
    arcpy.management.DeleteField(src, to_drop)
    print(f"OK Dropped {len(to_drop)} field(s) ({', '.join(to_drop)}) from {src}")
    return 0


def cmd_explode(payload):
    src = payload["input"]
    out_path = payload["out"]
    ensure_output_container(out_path, is_batch=False)
    arcpy.management.MultipartToSinglepart(src, out_path)
    cnt = int(arcpy.management.GetCount(out_path)[0])
    print(f"OK Exploded multipart -> singlepart ({cnt} feature(s)) -> {out_path}")
    return 0


def cmd_centroid(payload):
    src = payload["input"]
    out_path = payload["out"]
    inside = bool(payload.get("inside", False))
    ensure_output_container(out_path, is_batch=False)
    try:
        arcpy.management.FeatureToPoint(src, out_path, "INSIDE" if inside else "CENTROID")
    except Exception:
        # License-independent fallback (works on ArcGIS Basic!)
        desc = arcpy.Describe(src)
        sr = getattr(desc, "spatialReference", None)
        parent = os.path.dirname(out_path) or "."
        name = os.path.basename(out_path)
        arcpy.management.CreateFeatureclass(parent, name, "POINT", template=src, spatial_reference=sr)
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
    cnt = int(arcpy.management.GetCount(out_path)[0])
    print(f"OK Created {cnt} centroid point(s) -> {out_path}")
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
    non_empty = [v.strip() for v in values if v is not None and str(v).strip() != ""]
    if not non_empty:
        return "TEXT", 64
    all_int = True
    all_float = True
    max_len = 16
    for s in non_empty:
        max_len = max(max_len, len(s))
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
    clean = re.sub(r"[^A-Za-z0-9_]", "_", raw_name.strip())
    if not clean or clean[0].isdigit():
        clean = "F_" + clean
    max_len = 10 if is_shp else 60
    cand = clean[:max_len]
    idx = 1
    while cand.lower() in existing_lower:
        suf = f"_{idx}"
        cand = clean[: max(1, max_len - len(suf))] + suf
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

    in_fields = {f.name.lower(): f.name for f in arcpy.ListFields(src)}
    if in_field_req.lower() not in in_fields:
        print(f"Input key field '{in_field_req}' not found in {src}", file=sys.stderr)
        return 1
    in_key = in_fields[in_field_req.lower()]

    with open(csv_path, "r", encoding="utf-8-sig", newline="") as fh:
        reader = csv.DictReader(fh)
        if not reader.fieldnames:
            print(f"CSV table has no header row: {csv_path}", file=sys.stderr)
            return 1
        csv_map = {c.strip().lower(): c for c in reader.fieldnames if c}
        if join_field_req.lower() not in csv_map:
            print(f"Join key column '{join_field_req}' not found in {csv_path}", file=sys.stderr)
            return 1
        join_col = csv_map[join_field_req.lower()]
        extra_cols = [c for c in reader.fieldnames if c and c != join_col]
        lookup = {}
        col_samples = {c: [] for c in extra_cols}
        for row in reader:
            k_raw = row.get(join_col)
            if k_raw is None:
                continue
            k = str(k_raw).strip().lower()
            if not k:
                continue
            lookup[k] = row
            for c in extra_cols:
                col_samples[c].append(row.get(c, ""))

    if not extra_cols:
        print("CSV table has no extra columns to join beyond key column.", file=sys.stderr)
        return 1

    arcpy.management.CopyFeatures(src, out_path)
    is_shp = out_path.lower().endswith(".shp")
    existing_lower = {f.name.lower() for f in arcpy.ListFields(out_path)}
    added = []  # (csv_col, out_field, ftype)
    for col in extra_cols:
        ftype, flen = _infer_csv_field_type(col_samples[col])
        out_f = _unique_field_name(col, existing_lower, is_shp)
        if ftype == "TEXT":
            arcpy.management.AddField(out_path, out_f, "TEXT", field_length=flen)
        else:
            arcpy.management.AddField(out_path, out_f, ftype)
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
            rec = lookup.get(str(k_val).strip().lower())
            if not rec:
                continue
            matched += 1
            for i, (col, _, ftype) in enumerate(added, 1):
                raw_v = (rec.get(col) or "").strip()
                if raw_v == "":
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

    print(f"OK Joined {len(added)} field(s) ({matched}/{total} features matched) -> {out_path}")
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
        print("No valid distances provided in --dists (e.g. '500,1000,2000').", file=sys.stderr)
        return 1
    unit = payload.get("unit") or "Meters"
    dissolve = (payload.get("dissolve_option") or "ALL").upper()
    ensure_output_container(out_path, is_batch=False)
    arcpy.analysis.MultipleRingBuffer(src, out_path, dists, unit, "distance", dissolve)
    cnt = int(arcpy.management.GetCount(out_path)[0])
    print(f"OK Multi-ring buffer ({dists} {unit}, {cnt} polygon(s)) -> {out_path}")
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
        arcpy.analysis.CreateThiessenPolygons(src, out_path, "ALL")
    finally:
        arcpy.env.extent = prev_ext
    cnt = int(arcpy.management.GetCount(out_path)[0])
    print(f"OK Created {cnt} Thiessen polygon(s) -> {out_path}")
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
        arcpy.analysis.Near(src, near_fc, radius if radius else None)
        print(f"OK Computed NEAR_FID & NEAR_DIST against {os.path.basename(near_fc)} -> {src}")
        return 0
    except Exception:
        # License-independent fallback (works on ArcGIS Basic!)
        near_geoms = []
        with arcpy.da.SearchCursor(near_fc, ["OID@", "SHAPE@"]) as sc:
            for oid, g in sc:
                if g is not None:
                    near_geoms.append((int(oid), g))
        if not near_geoms:
            print(f"Near layer has no valid geometries: {near_fc}", file=sys.stderr)
            return 1
        existing = {f.name.lower() for f in arcpy.ListFields(src)}
        if "near_fid" not in existing:
            arcpy.management.AddField(src, "NEAR_FID", "LONG")
        if "near_dist" not in existing:
            arcpy.management.AddField(src, "NEAR_DIST", "DOUBLE")
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
        print(f"OK Computed NEAR_FID & NEAR_DIST ({cnt} feature(s), Basic fallback) -> {src}")
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

    # Try ArcGIS Eliminate_management first (Advanced license)
    sliver_oids = {oid for oid, _, _ in slivers}
    try:
        oid_field = desc.OIDFieldName
        lyr = "geokit_elim_lyr"
        if arcpy.Exists(lyr):
            arcpy.management.Delete(lyr)
        arcpy.management.MakeFeatureLayer(fc, lyr)
        oid_csv = ",".join(str(x) for x in sorted(sliver_oids))
        arcpy.management.SelectLayerByAttribute(lyr, "NEW_SELECTION", f"{oid_field} IN ({oid_csv})")
        tmp_out = os.path.join("in_memory", "gk_elim_tmp")
        if arcpy.Exists(tmp_out):
            arcpy.management.Delete(tmp_out)
        arcpy.management.Eliminate(lyr, tmp_out, "LENGTH")
        arcpy.management.Delete(lyr)
        prev_ow = arcpy.env.overwriteOutput
        arcpy.env.overwriteOutput = True
        try:
            arcpy.management.CopyFeatures(tmp_out, fc)
        finally:
            arcpy.env.overwriteOutput = prev_ow
            if arcpy.Exists(tmp_out):
                arcpy.management.Delete(tmp_out)
        return len(sliver_oids)
    except Exception:
        # License-independent Basic fallback: merge each sliver into longest-shared-border non-sliver neighbor
        keepers = {oid: g for oid, g, a in records if a >= min_area}
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
                    shared = sg.intersect(kg, 2)  # 2 = polyline shared boundary
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
    arcpy.management.CopyFeatures(src, out_path)
    removed = _eliminate_slivers_inplace(out_path, min_area)
    remaining = int(arcpy.management.GetCount(out_path)[0])
    print(f"OK Eliminated {removed} sliver polygon(s) (< {min_area:.2f} m2), {remaining} remaining -> {out_path}")
    return 0


def _delete_duplicates_inplace(fc, tolerance=None):
    before = int(arcpy.management.GetCount(fc)[0])
    try:
        arcpy.management.DeleteIdentical(fc, ["Shape"], tolerance if tolerance else None)
        after = int(arcpy.management.GetCount(fc)[0])
        return max(0, before - after)
    except Exception:
        # License-independent fallback (works on ArcGIS Basic!)
        seen = set()
        removed = 0
        with arcpy.da.UpdateCursor(fc, ["SHAPE@"]) as uc:
            for (g,) in uc:
                if g is None:
                    continue
                try:
                    sig = g.WKB
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
    arcpy.management.CopyFeatures(src, out_path)
    removed = _delete_duplicates_inplace(out_path, tol)
    remaining = int(arcpy.management.GetCount(out_path)[0])
    print(f"OK Removed {removed} duplicate geometry feature(s), {remaining} remaining -> {out_path}")
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
    total_features = int(arcpy.management.GetCount(src)[0])

    if out_dir:
        os.makedirs(out_dir, exist_ok=True)

    # 1. Invalid geometries via CheckGeometry
    invalid_count = 0
    try:
        chk_tbl = os.path.join("in_memory", "gk_topo_chk")
        if arcpy.Exists(chk_tbl):
            arcpy.management.Delete(chk_tbl)
        arcpy.management.CheckGeometry(src, chk_tbl)
        invalid_count = int(arcpy.management.GetCount(chk_tbl)[0])
        arcpy.management.Delete(chk_tbl)
    except Exception:
        invalid_count = 0

    # 2. Load geometries
    geoms = []
    seen_wkb = set()
    duplicate_count = 0
    sliver_count = 0
    with arcpy.da.SearchCursor(src, ["OID@", "SHAPE@"]) as sc:
        for oid, g in sc:
            if g is None:
                continue
            try:
                sig = g.WKB
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
        # 3a. Pairwise polygon overlaps (bbox prefiltered)
        n = len(geoms)
        for i in range(n):
            oid_a, ga, ea = geoms[i]
            for j in range(i + 1, n):
                oid_b, gb, eb = geoms[j]
                if ea.XMax <= eb.XMin or ea.XMin >= eb.XMax or ea.YMax <= eb.YMin or ea.YMin >= eb.YMax:
                    continue
                try:
                    inter = ga.intersect(gb, 4)  # 4 = polygon
                    if inter is not None and inter.area > 0:
                        overlap_geoms.append((oid_a, oid_b, inter))
                except Exception:
                    pass

        # 3b. Interior gaps (dissolve all polygons -> inspect interior rings of dissolved parts)
        try:
            diss_fc = os.path.join("in_memory", "gk_topo_diss")
            if arcpy.Exists(diss_fc):
                arcpy.management.Delete(diss_fc)
            arcpy.management.Dissolve(src, diss_fc)
            with arcpy.da.SearchCursor(diss_fc, ["SHAPE@"]) as sc:
                for (dg,) in sc:
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
                        # Rings after index 0 are interior holes (gaps)
                        for hole in rings[1:]:
                            if len(hole) >= 4:
                                arr = arcpy.Array(hole)
                                poly_gap = arcpy.Polygon(arr, sr)
                                if poly_gap.area > 0:
                                    gap_geoms.append(poly_gap)
            arcpy.management.Delete(diss_fc)
        except Exception:
            pass

        if out_dir and overlap_geoms:
            ov_path = os.path.join(out_dir, "topo_overlaps.shp")
            if arcpy.Exists(ov_path):
                arcpy.management.Delete(ov_path)
            arcpy.management.CreateFeatureclass(out_dir, "topo_overlaps.shp", "POLYGON", spatial_reference=sr)
            arcpy.management.AddField(ov_path, "OID_A", "LONG")
            arcpy.management.AddField(ov_path, "OID_B", "LONG")
            with arcpy.da.InsertCursor(ov_path, ["SHAPE@", "OID_A", "OID_B"]) as ic:
                for oa, ob, ig in overlap_geoms:
                    ic.insertRow([ig, oa, ob])
            exported_files.append(ov_path)

        if out_dir and gap_geoms:
            gp_path = os.path.join(out_dir, "topo_gaps.shp")
            if arcpy.Exists(gp_path):
                arcpy.management.Delete(gp_path)
            arcpy.management.CreateFeatureclass(out_dir, "topo_gaps.shp", "POLYGON", spatial_reference=sr)
            with arcpy.da.InsertCursor(gp_path, ["SHAPE@"]) as ic:
                for gg in gap_geoms:
                    ic.insertRow([gg])
            exported_files.append(gp_path)

    elif shape_type == "polyline":
        # 3c. Polyline dangling endpoints (node degree == 1) & crossings
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
                arcpy.management.Delete(dg_path)
            arcpy.management.CreateFeatureclass(out_dir, "topo_dangles.shp", "POINT", spatial_reference=sr)
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
        status = "PASS" if total_errors == 0 else f"ISSUES ({total_errors})"
        print(f"Topology Check [{status}]: {os.path.basename(src)} ({shape_type}, {total_features} features)")
        print(f"  Invalid geometries : {invalid_count}")
        print(f"  Duplicate features : {duplicate_count}")
        if shape_type == "polygon":
            print(f"  Overlaps           : {len(overlap_geoms)}")
            print(f"  Interior gaps      : {len(gap_geoms)}")
            print(f"  Slivers (<{min_sliver:g} m2): {sliver_count}")
        elif shape_type == "polyline":
            print(f"  Dangling nodes     : {len(dangle_pts)}")
            print(f"  Line crossings     : {self_intersections}")
        for ef in exported_files:
            print(f"  Exported error shp : {ef}")
    return 1 if (strict and total_errors > 0) else 0


def cmd_topo_fix(payload):
    src = payload["input"]
    out_path = payload["out"]
    min_sliver = float(payload.get("min_sliver") or 100.0)
    ensure_output_container(out_path, is_batch=False)
    arcpy.management.CopyFeatures(src, out_path)
    arcpy.management.RepairGeometry(out_path, "DELETE_NULL")
    dups_removed = _delete_duplicates_inplace(out_path)
    desc = arcpy.Describe(out_path)
    slivers_removed = 0
    if (getattr(desc, "shapeType", "") or "").lower() == "polygon" and min_sliver > 0:
        slivers_removed = _eliminate_slivers_inplace(out_path, min_sliver)
    final_cnt = int(arcpy.management.GetCount(out_path)[0])
    print(
        f"OK Topology Fix complete -> {out_path} "
        f"(repaired geometry, removed {dups_removed} duplicates, eliminated {slivers_removed} slivers, {final_cnt} feature(s) remaining)"
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
    cell_x = float(arcpy.management.GetRasterProperties(ref_raster, "CELLSIZEX")[0])
    cell_y = float(arcpy.management.GetRasterProperties(ref_raster, "CELLSIZEY")[0])
    cell_str = f"{cell_x} {cell_y}"

    is_batch = len(inputs) > 1 or os.path.isdir(out_path) or not os.path.splitext(out_path)[1]
    if is_batch:
        os.makedirs(out_path, exist_ok=True)
    else:
        parent = os.path.dirname(out_path)
        if parent:
            os.makedirs(parent, exist_ok=True)

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
                dst = os.path.join(out_path, f"{stem}_aligned.tif")
            else:
                dst = out_path
            arcpy.management.ProjectRaster(src, dst, ref_sr, resamp, cell_str)
            print(f"[{idx}/{len(inputs)}] OK   Aligned {os.path.basename(src)} -> {dst} ({cell_x:g}x{cell_y:g}, {resamp})")
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
            existing = {f.name.lower() for f in arcpy.ListFields(out_path)}
            if field_name.lower() not in existing:
                arcpy.management.AddField(out_path, field_name, "DOUBLE")
            with arcpy.da.UpdateCursor(out_path, ["RASTERVALU", field_name]) as uc:
                for row in uc:
                    row[1] = row[0]
                    uc.updateRow(row)
            arcpy.management.DeleteField(out_path, ["RASTERVALU"])
    except Exception:
        # License-independent fallback using GetCellValue_management!
        arcpy.management.CopyFeatures(pts, out_path)
        existing = {f.name.lower() for f in arcpy.ListFields(out_path)}
        if field_name.lower() not in existing:
            arcpy.management.AddField(out_path, field_name, "DOUBLE")
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
                    res = arcpy.management.GetCellValue(ras, f"{pt.X} {pt.Y}")[0]
                    row[1] = None if str(res).lower() in ("nodata", "") else float(res)
                except Exception:
                    row[1] = None
                uc.updateRow(row)
    cnt = int(arcpy.management.GetCount(out_path)[0])
    mode = "Spatial Analyst" if used_sa else "GetCellValue fallback"
    print(f"OK Extracted raster values ({field_name}, {cnt} points, {mode}) -> {out_path}")
    return 0


def cmd_raster_morphometry(payload):
    arcpy.CheckOutExtension("Spatial")
    from arcpy.sa import (
        Con,
        Curvature,
        Fill,
        FlowAccumulation,
        FlowDirection,
        FocalStatistics,
        Ln,
        NbrCircle,
        Raster,
        Slope,
        Tan,
    )

    dem = payload["input"]
    out_dir = payload["out"]
    do_twi = bool(payload.get("twi", True))
    do_tpi = bool(payload.get("tpi", True))
    do_curv = bool(payload.get("curvature", True))
    tpi_rad = max(1, int(payload.get("tpi_radius") or 3))

    os.makedirs(out_dir, exist_ok=True)
    stem = os.path.splitext(os.path.basename(dem))[0]
    overwrite = _is_overwrite(payload)
    cell_x = float(arcpy.management.GetRasterProperties(dem, "CELLSIZEX")[0])

    if do_twi:
        twi_out = os.path.join(out_dir, f"{stem}_twi.tif")
        err = _check_safe_output(twi_out, [dem], overwrite)
        if err:
            print(err, file=sys.stderr)
            return 1
        filled = Fill(dem)
        fdir = FlowDirection(filled)
        facc = FlowAccumulation(fdir)
        sca = (facc + 1.0) * cell_x
        slp_rad = Slope(filled, "DEGREE") * (math.pi / 180.0)
        tan_slp = Con(slp_rad > 0.001, Tan(slp_rad), 0.001)
        twi = Ln(sca / tan_slp)
        twi.save(twi_out)
        print(f"[OK] TWI       -> {twi_out}")

    if do_tpi:
        tpi_out = os.path.join(out_dir, f"{stem}_tpi.tif")
        err = _check_safe_output(tpi_out, [dem], overwrite)
        if err:
            print(err, file=sys.stderr)
            return 1
        focal_mean = FocalStatistics(dem, NbrCircle(tpi_rad, "CELL"), "MEAN", "DATA")
        tpi = Raster(dem) - focal_mean
        tpi.save(tpi_out)
        print(f"[OK] TPI (r={tpi_rad}) -> {tpi_out}")

    if do_curv:
        curv_out = os.path.join(out_dir, f"{stem}_curvature.tif")
        err = _check_safe_output(curv_out, [dem], overwrite)
        if err:
            print(err, file=sys.stderr)
            return 1
        curv = Curvature(dem)
        curv.save(curv_out)
        print(f"[OK] Curvature -> {curv_out}")

    return 0


def cmd_raster_georef(payload):
    src = payload["input"]
    out_path = payload["out"]
    epsg = int(payload["epsg"])
    order = max(1, min(3, int(payload.get("order") or 1)))
    gcps = payload.get("gcps") or []
    if len(gcps) < 3:
        print("At least 3 GCPs are required for georeferencing.", file=sys.stderr)
        return 1

    parent = os.path.dirname(out_path)
    if parent:
        os.makedirs(parent, exist_ok=True)

    # ArcGIS Warp uses raster coordinate space (origin at bottom-left or top-left depending on image);
    # convert pixel (col, row) into source extent coordinates when raster has an unreferenced pixel grid.
    desc = arcpy.Describe(src)
    ext = desc.extent
    rows = float(arcpy.management.GetRasterProperties(src, "ROWCOUNT")[0])
    cols = float(arcpy.management.GetRasterProperties(src, "COLUMNCOUNT")[0])
    dx = (ext.XMax - ext.XMin) / cols if cols > 0 else 1.0
    dy = (ext.YMax - ext.YMin) / rows if rows > 0 else 1.0

    src_pts = []
    dst_pts = []
    for g in gcps:
        sx = ext.XMin + float(g["pixel_x"]) * dx
        sy = ext.YMax - float(g["pixel_y"]) * dy
        src_pts.append(f"'{sx} {sy}'")
        dst_pts.append(f"'{g['map_x']} {g['map_y']}'")

    tx_type = {1: "POLYORDER1", 2: "POLYORDER2", 3: "POLYORDER3"}[order]
    arcpy.management.Warp(src, ";".join(src_pts), ";".join(dst_pts), out_path, tx_type, "BILINEAR")
    arcpy.management.DefineProjection(out_path, arcpy.SpatialReference(epsg))
    print(f"OK Georeferenced {os.path.basename(src)} ({len(gcps)} GCPs, {tx_type}, EPSG:{epsg}) -> {out_path}")
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
    "raster_align": cmd_raster_align,
    "raster_extract_points": cmd_raster_extract_points,
    "raster_morphometry": cmd_raster_morphometry,
    "raster_georef": cmd_raster_georef,
    "hydro_delineate": cmd_hydro_delineate,
    "hydro_basin": cmd_hydro_basin,
}


PROVENANCE_ACTIONS = {
    "classify",
    "calc_field",
    "union",
    "topo_fix",
    "raster_reclass",
    "raster_weighted_sum",
    "raster_interpolate",
    "raster_index",
    "raster_contour",
    "raster_align",
    "raster_morphometry",
    "raster_georef",
    "hydro_delineate",
    "hydro_basin",
}


def _write_provenance(action, payload, engine_name, elapsed_sec):
    if action not in PROVENANCE_ACTIONS:
        return
    if os.environ.get("GEOKIT_NO_PROVENANCE") in ("1", "true", "TRUE"):
        return
    import datetime

    target = payload.get("out") or payload.get("input")
    if not target:
        return
    try:
        if os.path.isdir(target):
            sidecar = os.path.join(target, "geokit_provenance.json")
        else:
            sidecar = target + ".geokit.json"
        parent = os.path.dirname(sidecar)
        if parent and not os.path.isdir(parent):
            return
        safe_params = {
            k: v
            for k, v in payload.items()
            if k not in ("token", "gcps") and v is not None
        }
        record = {
            "geokit_provenance_version": 1,
            "action": action,
            "engine": engine_name,
            "timestamp_utc": datetime.datetime.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ"),
            "duration_sec": round(float(elapsed_sec), 3),
            "output": target,
            "parameters": safe_params,
        }
        with open(sidecar, "w", encoding="utf-8") as fh:
            json.dump(record, fh, indent=2)
    except Exception:
        pass


def _run_payload_captured(payload):
    import io
    import time
    import traceback

    action = payload.get("action")
    handler = HANDLERS.get(action)
    if not handler:
        return 2, "", f"Unknown geokit gp action: {action}\n"

    old_out, old_err = sys.stdout, sys.stderr
    buf_out, buf_err = io.StringIO(), io.StringIO()
    sys.stdout, sys.stderr = buf_out, buf_err
    code = 0
    started = time.time()
    prev_comp = getattr(arcpy.env, "compression", None)
    try:
        arcpy.env.overwriteOutput = _is_overwrite(payload)
        try:
            arcpy.env.compression = "LZW"
        except Exception:
            pass
        out_p = payload.get("out")
        if out_p and action not in ("layout_export", "raster_reclass", "raster_weighted_sum", "raster_interpolate", "raster_index"):
            in_list = [payload.get("input"), payload.get("aoi"), payload.get("mask"), payload.get("dem"), payload.get("zones")]
            if isinstance(payload.get("inputs"), list):
                in_list.extend(payload.get("inputs"))
            chk_err = _check_safe_output(out_p, in_list, arcpy.env.overwriteOutput)
            if chk_err and not os.path.isdir(out_p):
                return 1, "", chk_err + "\n"
        code = int(handler(payload) or 0)
        if code == 0:
            _write_provenance(action, payload, "arcgis", time.time() - started)
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
            if prev_comp is not None:
                arcpy.env.compression = prev_comp
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
    import time

    arcpy.env.overwriteOutput = _is_overwrite(payload)
    try:
        arcpy.env.compression = "LZW"
    except Exception:
        pass
    out_p = payload.get("out")
    if out_p and action not in ("layout_export", "raster_reclass", "raster_weighted_sum", "raster_interpolate", "raster_index"):
        in_list = [payload.get("input"), payload.get("aoi"), payload.get("mask"), payload.get("dem"), payload.get("zones")]
        if isinstance(payload.get("inputs"), list):
            in_list.extend(payload.get("inputs"))
        chk_err = _check_safe_output(out_p, in_list, arcpy.env.overwriteOutput)
        if chk_err and not os.path.isdir(out_p):
            sys.stderr.write(chk_err + "\n")
            sys.exit(1)
    t0 = time.time()
    code = handler(payload)
    if int(code or 0) == 0:
        _write_provenance(action, payload, "arcgis", time.time() - t0)
    sys.exit(code)


if __name__ == "__main__":
    main()
