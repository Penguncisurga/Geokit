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
    try:
        sa.StreamToFeature(stream_ras, fdir, streams_shp, "SIMPLIFY")
        print("      -> %s" % streams_shp)
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
    tool = payload.get("tool") or ""
    args = payload.get("args") or []
    auto_checkout_extension(tool)
    fn = getattr(arcpy, tool, None)
    if fn is None:
        fn = getattr(arcpy.gp, tool, None)
    if fn is None:
        sys.stderr.write("Unknown ArcPy tool '%s'. Run `geokit gp tools <keyword>` to search.\n" % tool)
        return 1
    res = fn(*args)
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
                    ov_fc = os.path.join(arcpy.env.scratchGDB, "geokit_qa_ov")
                    if arcpy.Exists(ov_fc):
                        arcpy.Delete_management(ov_fc)
                    arcpy.Intersect_analysis([ds_path], ov_fc)
                    ov_count = int(arcpy.GetCount_management(ov_fc).getOutput(0))
                    if ov_count > 0:
                        issues.append("%d overlapping polygon pair(s)" % ov_count)
                    arcpy.Delete_management(ov_fc)
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

    if payload.get("json"):
        print(json.dumps({
            "datasets_checked": len(report),
            "unique_crs_count": len(crs_set),
            "total_issues": total_issues,
            "results": report,
        }, indent=2))
        return 0

    print("Spatial QA Report (%d dataset(s) checked):" % len(report))
    if len(crs_set) > 1:
        print("  [WARN] Multiple CRS detected across workspace: %s" % ", ".join(sorted(crs_set)))
    for item in report:
        tag = "[PASS]" if item["status"] == "PASS" else "[WARN]"
        epsg_s = ("EPSG:%s" % item["epsg"]) if item["epsg"] else "No EPSG"
        print("  %s %s (%s / %s)" % (tag, item["name"], item["crs"], epsg_s))
        for iss in item["issues"]:
            print("         - %s" % iss)
    print("Summary: %d checked, %d issue(s) found." % (len(report), total_issues))
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
        "sqrt": math.sqrt, "log": math.log, "pow": math.pow, "math": math,
    }

    updated = 0
    with arcpy.da.UpdateCursor(ds_path, all_fields) as cur:
        for row in cur:
            env = dict(safe_funcs)
            for idx, fname in enumerate(all_fields):
                v = row[idx]
                env[fname] = v
                env.setdefault(fname.upper(), v)
                env.setdefault(fname.lower(), v)
            val = eval(expr, {"__builtins__": {}}, env)
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
    "layout_update": cmd_layout_update,
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
