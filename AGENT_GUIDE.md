# GeoKit AI Agent Playbook (`AGENT_GUIDE.md`)

> Standard Operating Procedures (SOP) for AI assistants (Claude, Antigravity, Cursor, Codex, Gemini) controlling ArcGIS Pro, ArcMap 10.x, and GDAL via **`geokit`** CLI or **`geokit mcp`**.

## Golden Rules (Read First!)

1. **Never guess file paths, extensions, or attribute field names.**
   - Always start with `geokit audit <folder>` and `geokit gp info <folder>` to discover exact datasets and CRS.
   - Always inspect attribute categories and numeric/raster ranges with `geokit gp values <dataset>` before writing any scoring rules or SQL queries.
2. **Always check & unify CRS before spatial overlay or hydrology.**
   - Area/length, buffer, watershed (`hydro`), and overlay analyses require a projected metric CRS (e.g. UTM `EPSG:32748` / `32749` / `32750` for Indonesia), **never** geographic WGS84 degrees (`EPSG:4326`).
3. **Always run `geokit gp repair` before `union`, `intersect`, or `clip` on third-party shapefiles.**
4. **Windows paths:** always use real drive letters (`D:\Data\layer.shp`), never Explorer display labels (`Lokal Disk (D:)\...`).
5. **Never overwrite raw user inputs** — always write outputs into a clean subfolder (e.g. `<folder>\output_geokit\`).
6. **Verify before reporting:** run `geokit qa <output_dir>` to confirm CRS uniformity, valid geometries, and zero unintended NULLs.

---

## SOP 1: Peta Kerawanan / Kesesuaian Lahan (Weighted Overlay & Scoring)

Example requests: *"Buat peta kerawanan longsor / banjir / kekeringan / kesesuaian lahan dari folder ini."*

```bash
# 1. Discover datasets, CRS, fields, and unique values
geokit audit ./bahan
geokit gp info ./bahan
geokit gp values ./bahan

# 2. Unify CRS to local UTM & clip to study area + repair geometry
geokit gp project ./bahan --epsg 32749 --out ./work/01_utm
geokit gp clip ./work/01_utm --aoi ./work/01_utm/batas_studi.shp --out ./work/02_clipped
geokit gp repair ./work/02_clipped

# 3. Assign scores per parameter (categories OR numeric ranges)
geokit gp classify ./work/02_clipped/tutupan_lahan.shp --field KELAS_PL --out-field SKOR_PL --rules "Hutan=1;Perkebunan=2;Sawah=3;Permukiman=4;Lahan Terbuka=5"
geokit gp classify ./work/02_clipped/kemiringan.shp    --field PERSEN   --out-field SKOR_LR --rules "<=8:1;8-15:2;15-25:3;25-45:4;>45:5"
geokit gp classify ./work/02_clipped/curah_hujan.shp   --field CH_MM    --out-field SKOR_CH --rules "<1500:1;1500-2000:2;2000-2500:3;2500-3000:4;>3000:5"

# 4. Union all scored parameter layers into one polygon layer
geokit gp union ./work/02_clipped/tutupan_lahan.shp ./work/02_clipped/kemiringan.shp ./work/02_clipped/curah_hujan.shp --out ./work/03_union.shp

# 5. Compute weighted total score & classify hazard levels
geokit gp calc-field ./work/03_union.shp --field TOTAL_SKOR --expr "0.4 * SKOR_LR + 0.3 * SKOR_CH + 0.3 * SKOR_PL"
geokit gp classify   ./work/03_union.shp --field TOTAL_SKOR --out-field KELAS_RAWAN --rules "<2.3:Rendah;2.3-3.6:Sedang;>3.6:Tinggi"

# 6. Dissolve by hazard class & compute area (Ha)
geokit gp dissolve  ./work/03_union.shp --field KELAS_RAWAN --out ./output/kerawanan_final.shp
geokit gp calc-geom ./output/kerawanan_final.shp --unit ha

# 7. Quality assurance check & inspect final area table
geokit qa ./output/kerawanan_final.shp --expected-epsg 32749 --check-nulls KELAS_RAWAN
geokit gp values ./output/kerawanan_final.shp
```

---

## SOP 2: Peta Persebaran (Distribution Map from CSV / Points / Polygons)

Example requests: *"Buat peta persebaran fasilitas kesehatan / stasiun hujan / titik kejadian dari folder ini."*

```bash
# 1. Audit folder & inspect CSV/shapefiles
geokit audit ./bahan
geokit gp info ./bahan

# 2. Convert CSV with Lon/Lat columns into points (if needed)
geokit gp xy2shp ./bahan/lokasi.csv --x-field lon --y-field lat --epsg 4326 --out ./work/lokasi_wgs84.shp

# 3. Project to local UTM & spatially join administrative boundary attributes
geokit gp project ./work/lokasi_wgs84.shp --epsg 32749 --out ./work/lokasi_utm.shp
geokit gp spatial-join ./work/lokasi_utm.shp --join ./work/batas_kecamatan_utm.shp --out ./output/persebaran_final.shp --match within

# 4. Inspect distribution counts & run QA
geokit gp values ./output/persebaran_final.shp
geokit qa ./output/persebaran_final.shp --expected-epsg 32749
```

---

## SOP 3: Analisis Hidrologi & Delineasi DAS

Example requests: *"Delineasi DAS, jaringan sungai, kemiringan lereng, dan statistik elevasi dari DEM di folder ini."*

```bash
# 1. Inspect DEM CRS & elevation range
geokit gp info ./bahan/dem.tif
geokit gp values ./bahan/dem.tif

# 2. Ensure projected metric CRS (UTM)
geokit gp project ./bahan/dem.tif --epsg 32749 --out ./work/dem_utm.tif

# 3. Run watershed delineation + terrain derivatives + zonal stats
geokit hydro delineate ./work/dem_utm.tif --out ./output/das --threshold 1000 --pour-point ./work/outlet_utm.shp
geokit raster terrain ./work/dem_utm.tif --out ./output/terrain --all
geokit gp calc-geom ./output/das/watershed.shp --unit ha
geokit raster zonal-stats ./work/dem_utm.tif --zones ./output/das/watershed.shp --zone-field GRIDCODE --out ./output/elev_stats.csv
```

---

## Displaying Results Live in ArcMap 10.x

If the user has ArcMap 10.1–10.8 open:
```bash
geokit live ping
geokit live add ./output/kerawanan_final.shp
geokit live style kerawanan_final --field KELAS_RAWAN --preset hazard --transparency 30
geokit live order sungai --position top
geokit live layout --view layout --title "PETA KERAWANAN BANJIR" --zoom-layer kerawanan_final
geokit live export --out ./output/preview.png --dpi 200
```

> **Tip:** All 3 SOP playbooks above are also built directly into **`geokit mcp`** as standard MCP Prompts (`prompts/list` & `prompts/get`: `peta_kerawanan`, `peta_persebaran`, `analisis_das`).
