# GeoKit AI Agent Playbook (`AGENT_GUIDE.md` — `v2.9.0`)

> Standard Operating Procedures (SOP) for AI assistants (Claude, Antigravity, Cursor, Codex, Gemini) controlling ArcGIS Pro, ArcMap 10.x, and GDAL via **`geokit`** CLI or **`geokit mcp`** (**21 MCP tools** + 3 built-in SOP prompts).

## Golden Rules (Read First!)

1. **Never guess file paths, extensions, or attribute field names.**
   - Start with `geokit audit <folder>` and `geokit gp info <folder>` to discover exact datasets and CRS.
   - Inspect categories and numeric/raster ranges with `geokit gp values <dataset>` before writing scoring rules or SQL queries.
2. **Safe Mode is default (`overwrite = false`).**
   - GeoKit refuses to overwrite existing outputs unless `--overwrite` (or `GEOKIT_OVERWRITE=1`) is explicitly passed, and **always blocks** writing onto any input dataset.
   - Always write outputs into a clean subfolder (e.g. `<folder>\output_geokit\`).
3. **Always unify CRS before overlay, topology, or hydrology.**
   - Area/length, buffer, topology, raster morphometry, and watershed analyses require a projected metric CRS (e.g. UTM `EPSG:32748`–`32750` or DGN95 TM-3°), **never** geographic WGS84 degrees (`EPSG:4326`).
4. **Use the Hot-Worker Daemon for multi-step ArcPy workflows.**
   - Run `geokit daemon start` once (or use `geokit pipeline run pipeline.toml [--resume]`) so `import arcpy` executes once instead of per command (5×–15× faster).
5. **Verify topology & QA before cartography.**
   - Run `geokit topo check <layer> --strict` (or `geokit topo fix`) and `geokit qa <output_dir> --strict`.
6. **Print-ready cartography defaults to 300 DPI.**
   - Always export final PerKaBIG maps at `>= 300 DPI` (`geokit live export --out map.png --dpi 300`).

---

## SOP 1: Peta Kerawanan / Kesesuaian Lahan (Vector Overlay OR Raster Weighted-Sum)

```bash
# 1. Discover datasets, CRS, fields, and values
geokit audit ./bahan
geokit gp info ./bahan
geokit gp values ./bahan

# 2. Clean 3D Z/M (RBI BIG), unify CRS to UTM, clip, and fix topology
geokit gp flatten-2d ./bahan --out ./work/00_2d
geokit gp project ./work/00_2d --epsg 32749 --out ./work/01_utm
geokit gp clip ./work/01_utm --aoi ./work/01_utm/batas_studi.shp --out ./work/02_clipped
geokit gp repair ./work/02_clipped

# 3. Score parameters & overlay (Vector path)
geokit gp classify ./work/02_clipped/tutupan_lahan.shp --field KELAS_PL --out-field SKOR_PL --rules "Hutan=1;Sawah=3;Permukiman=5"
geokit gp union ./work/02_clipped/tutupan_lahan.shp ./work/02_clipped/lereng.shp --out ./work/03_union.shp
geokit gp clean-fields ./work/03_union.shp
geokit gp calc-field ./work/03_union.shp --field TOTAL_SKOR --expr "0.5 * SKOR_LR + 0.5 * SKOR_PL"
geokit gp classify ./work/03_union.shp --field TOTAL_SKOR --out-field KELAS_RWN --rules "<2.3:Rendah;2.3-3.6:Sedang;>3.6:Tinggi"
geokit gp dissolve ./work/03_union.shp --field KELAS_RWN --out ./output/kerawanan.shp
geokit topo fix ./output/kerawanan.shp --min-sliver 100 --out ./output/kerawanan_clean.shp
geokit gp calc-geom ./output/kerawanan_clean.shp --unit ha

# 4. QA, Scientific Accuracy Assessment, summary table, & standalone Leaflet webmap (zero ArcGIS needed!)
geokit qa ./output/kerawanan_clean.shp --expected-epsg 32749 --check-nulls KELAS_RWN --strict
geokit accuracy sample ./output/kerawanan_clean.shp --field KELAS_RWN --total 100 --min-per-class 10 --out ./output/validasi_titik
geokit accuracy roc ./work/validasi_lapangan.csv --score TOTAL_SKOR --label KEJADIAN --positive 1 --out ./output/roc_report.md
geokit gp summary ./output/kerawanan_clean.shp --group-by KELAS_RWN --out ./output/rekap_luas.md
geokit webmap ./output/kerawanan_clean.shp ./work/02_clipped/batas_studi.shp --field KELAS_RWN --preset hazard-pastel --out ./output/peta_interaktif.html
```

**Change Detection (T1 vs T2):**
```bash
geokit gp change ./work/tuplah_2018.shp ./work/tuplah_2024.shp --field-t1 KELAS_18 --field-t2 KELAS_24 --out ./output/perubahan.shp --table ./output/matriks_perubahan.md
```

**Raster Weighted-Sum path:** align all input rasters first with `geokit raster align <r1.tif> <r2.tif> --ref <dem_utm.tif> --out ./work/aligned`, then run `geokit raster reclass` and `geokit raster weighted-sum --inputs "r1.tif:0.4,r2.tif:0.6" --classes 5 --smooth --polygon-out ./output/zona.shp --out ./output/skor.tif`.

---

## SOP 2: Data Lapangan, CSV/DMS, GPX & Peta Persebaran

```bash
# 1. Convert CSV (supports decimal OR DMS like 7°00'12"S / 110°24'30"BT) or GPX GPS tracks
geokit gp xy2shp ./bahan/stasiun.csv --x-field lon --y-field lat --epsg 4326 --out ./work/stasiun_wgs84.shp
geokit gp convert ./bahan/survey_gps.gpx --to shp --out ./work/survey_gps.shp

# 2. Join tabular BPS/survey CSV onto administrative polygons, or Thiessen / Multi-Buffer / Near
geokit gp join ./work/batas_desa.shp --table ./bahan/data_bps.csv --in-field KODE_DESA --join-field KODE --out ./output/desa_bps.shp
geokit gp thiessen ./work/stasiun_utm.shp --extent ./work/batas_utm.shp --out ./output/thiessen_hujan.shp
geokit gp multi-buffer ./work/faskes_utm.shp --dists "500,1000,2000" --unit Meters --out ./output/jangkauan.shp
geokit gp near ./work/permukiman_utm.shp --near ./work/sungai_utm.shp
geokit raster extract-points --points ./work/stasiun_utm.shp --raster ./work/dem_utm.tif --field ELEV_M --out ./output/stasiun_elev.shp
```

---

## SOP 3: Hidrologi DAS, DEM Morphometry & Peta Scan Georeferencing

```bash
# 1. Fetch free 30m SRTM DEM & OSM vectors for any AOI (no ArcGIS required)
geokit fetch dem --aoi ./work/batas_wgs84.shp --clip-aoi --epsg 32749 --out ./work/dem
geokit fetch osm --aoi ./work/batas_wgs84.shp --layers road,river,boundary --out ./work/osm

# 2. Watershed delineation, terrain, morphometry (TWI/TPI/Curvature), & BIG contours
geokit hydro delineate ./work/dem/dem_aoi_clipped.tif --out ./output/das --threshold 1000
geokit raster terrain ./work/dem/dem_aoi_clipped.tif --out ./output/terrain --all
geokit raster morphometry ./work/dem/dem_aoi_clipped.tif --out ./output/morph --all
geokit raster contour ./work/dem/dem_aoi_clipped.tif --interval 25 --index-every 4 --out ./output/kontur.shp

# 3. Georeference scanned map from QGIS .points / CSV with RMSE report
geokit raster georef ./bahan/peta_scan.tif --gcp ./bahan/peta_scan.points --epsg 32749 --order 1 --report ./output/rmse.md --out ./output/peta_georef.tif
```

---

## SOP 4: Cartography & PerKaBIG No. 3/2016 Print Layout (ArcMap 10.x / ArcGIS Pro Live-UI)

```bash
geokit live ping
geokit live add ./output/kerawanan_clean.shp
geokit live style kerawanan_clean --field KELAS_RWN --preset hazard-pastel --transparency 20
# Official 23-class Indonesian Land Cover or Fisher-Jenks Natural Breaks:
# geokit live style tuplah --field TUTUPAN --preset klhk
# geokit live style skor --field TOTAL_SKOR --mode graduated --break-method jenks --classes-count 5 --preset viridis
geokit live layout --template perkabig --paper a4 --grid-mode dual --north-style big \
  --title "PETA KERAWANAN LONGSOR" --region "WILAYAH STUDI" \
  --inset-layer ./work/provinsi.shp --hide-legend "hillshade"
geokit live export --out ./output/peta_cetak_300dpi.png --dpi 300
```

> **Reproducibility:** Analytical commands automatically emit `<out>.geokit.json` provenance sidecars, GeoTIFFs use lossless **LZW** compression by default, and multi-step `pipeline.toml` runs write `pipeline-run-<ts>.log` + support `--resume`.
