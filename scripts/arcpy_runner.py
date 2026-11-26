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

arcpy.env.overwriteOutput = True

VECTOR_EXTS = {".shp", ".gpkg"}
RASTER_EXTS = {".tif", ".tiff", ".img", ".jp2", ".ecw", ".asc"}


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
    tool = payload.get("tool") or ""
    args = payload.get("args") or []
    auto_checkout_extension(tool)
    fn = getattr(arcpy, tool, None)
    if fn is None:
        print(f"Unknown ArcPy tool '{tool}'. Run `geokit gp tools <keyword>` to search.", file=sys.stderr)
        return 1
    res = fn(*args)
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
    "layout_list": cmd_layout_list,
    "layout_export": cmd_layout_export,
    "layout_map_series": cmd_layout_map_series,
    "map_broken_links": cmd_map_broken_links,
    "map_fix_links": cmd_map_fix_links,
    "map_add_layer": cmd_map_add_layer,
    "raster_mosaic": cmd_raster_mosaic,
    "raster_terrain": cmd_raster_terrain,
    "raster_zonal_stats": cmd_raster_zonal_stats,
    "hydro_delineate": cmd_hydro_delineate,
    "hydro_basin": cmd_hydro_basin,
}


def main():
    if len(sys.argv) < 2:
        sys.stderr.write("Usage: arcpy_runner.py <payload.json>\n")
        sys.exit(2)
    with open(sys.argv[1], "r", encoding="utf-8") as fh:
        payload = json.load(fh)
    action = payload.get("action")
    handler = HANDLERS.get(action)
    if not handler:
        sys.stderr.write(f"Unknown geokit gp action: {action}\n")
        sys.exit(2)
    code = handler(payload)
    sys.exit(code)


if __name__ == "__main__":
    main()
