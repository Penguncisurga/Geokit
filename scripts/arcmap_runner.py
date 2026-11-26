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

arcpy.env.overwriteOutput = True

VECTOR_EXTS = set([".shp"])
RASTER_EXTS = set([".tif", ".tiff", ".img", ".jp2", ".ecw", ".asc"])


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


def cmd_calc_geom(payload):
    items = [p for p, kind in collect_datasets(payload["input"], include_rasters=False) if kind == "vector"]
    unit_req = (payload.get("unit") or "").lower()
    custom_field = payload.get("field")
    failures = 0
    for idx, src in enumerate(items, 1):
        try:
            shape_type = arcpy.Describe(src).shapeType
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
    streams_shp = os.path.join(out_dir, "04_streams.shp")
    stream_ras = sa.Con(facc >= threshold, 1)
    try:
        sa.StreamToFeature(stream_ras, fdir, streams_shp, "SIMPLIFY")
        print("      -> %s" % streams_shp)
    except Exception as exc:
        print("      [WARN] No streams extracted at threshold %d (%s)" % (threshold, exc))

    if pour_point:
        print("[5/5] Snapping pour point (%s map units) & delineating watershed..." % snap_dist)
        snapped = sa.SnapPourPoint(pour_point, facc, snap_dist)
        ws_ras = sa.Watershed(fdir, snapped)
        ws_ras.save(os.path.join(out_dir, "05_watershed.tif"))
        ws_shp = os.path.join(out_dir, "05_watershed.shp")
        arcpy.RasterToPolygon_conversion(ws_ras, ws_shp, "SIMPLIFY", "VALUE")
        print("      -> %s" % ws_shp)
    else:
        print("[5/5] No --pour-point provided; skipping single-outlet watershed polygon.")

    print("OK Hydrology delineation complete -> %s" % out_dir)
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
    filled.save(os.path.join(out_dir, "01_dem_filled.tif"))

    print("[2/4] Computing D8 Flow Direction...")
    fdir = sa.FlowDirection(filled, "NORMAL")
    fdir.save(os.path.join(out_dir, "02_flow_dir.tif"))

    print("[3/4] Delineating all drainage basins...")
    basin_ras = sa.Basin(fdir)
    basin_ras.save(os.path.join(out_dir, "03_basins.tif"))

    print("[4/4] Converting basins to polygon shapefile...")
    basins_shp = os.path.join(out_dir, "04_basins.shp")
    arcpy.RasterToPolygon_conversion(basin_ras, basins_shp, "SIMPLIFY", "VALUE")
    print("OK Drainage basins saved -> %s" % basins_shp)
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
    "layout_list": cmd_layout_list,
    "layout_export": cmd_layout_export,
    "map_broken_links": cmd_map_broken_links,
    "map_fix_links": cmd_map_fix_links,
    "raster_mosaic": cmd_raster_mosaic,
    "raster_terrain": cmd_raster_terrain,
    "raster_zonal_stats": cmd_raster_zonal_stats,
    "hydro_delineate": cmd_hydro_delineate,
    "hydro_basin": cmd_hydro_basin,
}


def main():
    if len(sys.argv) < 2:
        sys.stderr.write("Usage: arcmap_runner.py <payload.json>\n")
        sys.exit(2)
    with open(sys.argv[1], "r") as fh:
        payload = json.load(fh)
    action = payload.get("action")
    handler = HANDLERS.get(action)
    if not handler:
        sys.stderr.write("Unsupported ArcMap action: %s\n" % action)
        sys.exit(2)
    code = handler(payload)
    sys.exit(code)


if __name__ == "__main__":
    main()
