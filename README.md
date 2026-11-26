# geokit

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

> **Fast GIS CLI toolkit written in Rust for spatial data hygiene and batch ArcGIS Pro geoprocessing.**

[Baca dalam Bahasa Indonesia (README.id.md)](README.id.md)

---

## Why `geokit`?

GIS projects quickly accumulate messy folders, broken shapefiles, illegal filenames, stale `.lock` files, and repetitive geoprocessing tasks (reprojecting, clipping, repairing geometry, packing/unpacking `.gdb`, calculating areas/lengths).

`geokit` solves both sides in a **single lightweight binary**:

1. **Spatial Data Hygiene (100% Native Rust — works without ArcGIS)**
   - Runs in milliseconds with zero runtime dependencies.
   - Treats **shapefile bundles** (`.shp`, `.shx`, `.dbf`, `.prj`, `.cpg`, `.shp.xml`, etc.), **raster sidecars** (`.tfw`, `.ovr`, `.aux.xml`, etc.), and **File Geodatabases (`.gdb`)** as atomic units so nothing breaks when moving or renaming.
2. **Batch Geoprocessing (ArcGIS Pro / `arcpy`)**
   - Auto-detects `propy.bat` on Windows (or override via `--propy` / `GEOKIT_PROPY`).
   - **Single-startup optimization**: imports `arcpy` **once** per batch instead of once per file, saving minutes on large folders.
   - Embeds its Python helper directly inside the executable — users only need **one `.exe` file**.

---

## Installation

### Option 1: Download Prebuilt Binary
Download the latest `geokit-windows-x86_64.exe` from [GitHub Releases](https://github.com/Penguncisurga/geokit/releases), rename to `geokit.exe`, and place it in a folder on your `PATH`.

### Option 2: Build from Source
Requires [Rust](https://rustup.rs/):

```bash
git clone https://github.com/Penguncisurga/geokit.git
cd geokit
cargo build --release
```

The compiled binary will be at `target/release/geokit.exe`.

---

## Quick Start

Check your environment first:

```bash
geokit doctor
```

---

## Commands

### 1. Spatial Data Hygiene *(No ArcGIS required)*

| Command | Description |
| :--- | :--- |
| `geokit audit <dir> [--json]` | Scan a GIS workspace, summarize datasets, flag missing `.prj`/`.dbf`/`.shx`, non-standard filenames, and `.lock` clutter. |
| `geokit organize <dir> [--output <dir>] [--dry-run]` | Organize scattered datasets into `01_Vector`, `02_Raster`, `03_Geodatabase`, `04_Projects`, `05_Tabular` while keeping bundles intact. |
| `geokit sanitize <dir> [--dry-run]` | Rename datasets (and all sidecars) to ArcGIS/GDB-safe `snake_case` (removes spaces/symbols, prefixes leading digits with `l_`). |
| `geokit clean <dir> [--include-xml] [--dry-run]` | Remove stale `.lock`, `.tmp`, and `.bak` files (optionally `.shp.xml` / `.aux.xml`). |

**Examples:**

```bash
# Inspect a messy project folder
geokit audit ./project_das

# Preview renaming and folder organization safely
geokit sanitize ./project_das --dry-run
geokit organize ./project_das --dry-run

# Apply changes
geokit sanitize ./project_das
geokit organize ./project_das

# Clean leftover ArcGIS lock files
geokit clean ./project_das
```

---

### 2. Batch Geoprocessing *(ArcGIS Pro `arcpy`)*

| Command | Description |
| :--- | :--- |
| `geokit gp info <path> [--json]` | Inspect layers, CRS/EPSG, geometry type, feature count, and fields in a file, folder, or `.gdb`. |
| `geokit gp project <input> --epsg <code> --out <out>` | Batch reproject vector & raster datasets (e.g., `--epsg 32748` for UTM 48S). |
| `geokit gp clip <input> --aoi <poly> --out <out>` | Batch clip vector & raster datasets against an AOI polygon. |
| `geokit gp merge <input_dir> --out <out>` | Merge compatible shapefiles/feature classes into one dataset. |
| `geokit gp pack <input_dir> --out <target.gdb>` | Pack all shapefiles (and optionally `--include-rasters`) into a `.gdb`. |
| `geokit gp unpack <input.gdb> --out <dir>` | Export all feature classes from a `.gdb` into shapefiles. |
| `geokit gp repair <input>` | Batch run `RepairGeometry` (`DELETE_NULL`) on a file, folder, or `.gdb`. |
| `geokit gp buffer <input> --dist "50 Meters" --out <out> [--dissolve]` | Batch buffer vector datasets. |
| `geokit gp dissolve <input> --field <field> --out <out>` | Batch dissolve features by one or more comma-separated fields. |
| `geokit gp calc-geom <input> [--unit ha\|m2\|km2\|km\|m]` | Batch calculate geodesic area (polygons) or length (lines) into a field (`AREA_HA`, `LEN_KM`, etc.). |
| `geokit gp intersect <input> --with <overlay> --out <out>` | Batch intersect vector datasets with an overlay layer. |
| `geokit gp erase <input> --mask <erase> --out <out>` | Batch erase features overlapping a mask layer. |
| `geokit gp spatial-join <input> --join <layer> --out <out> [--match intersect\|within\|contains\|closest]` | Batch spatial join attributes from another layer. |

> **GDAL / QGIS Backend:** Pass `--engine gdal` (or leave default `--engine auto` on machines without ArcGIS Pro) to run `info`, `project`, `clip`, `merge`, `pack` (`.gpkg`), and `unpack` using `ogr2ogr` / `gdalwarp` — automatically detected from `PATH` or QGIS on Windows!

**Examples:**

```bash
# Inspect all layers inside a File Geodatabase
geokit gp info ./03_Geodatabase/das_citarum.gdb

# Repair geometry across an entire folder before overlay
geokit gp repair ./01_Vector

# Batch reproject all shapefiles & rasters to UTM Zone 48S
geokit gp project ./01_Vector --epsg 32748 --out ./01_Vector_UTM48S

# Batch clip against study boundary
geokit gp clip ./01_Vector_UTM48S --aoi ./batas_studi.shp --out ./clipped.gdb

# Calculate geodesic area (hectares) for all polygons in a geodatabase
geokit gp calc-geom ./clipped.gdb --unit ha

# Unpack a geodatabase back to shapefiles for delivery
geokit gp unpack ./clipped.gdb --out ./delivery_shp
```

---

### 3. Project Layouts & Broken Links *(ArcGIS Pro `.aprx`)*

| Command | Description |
| :--- | :--- |
| `geokit layout list <path> [--json]` | List all layouts and page sizes in one `.aprx` or a folder of `.aprx` projects. |
| `geokit layout export <path> --out <dir> [--format pdf\|png\|jpg] [--dpi 300] [--filter <text>]` | Batch export layouts from one or many `.aprx` projects in a single `arcpy` session. |
| `geokit map broken-links <path> [--json]` | Detect layers with broken data sources (red exclamation mark) across `.aprx` projects. |
| `geokit map fix-links <aprx> --find <old> --replace <new> [--out <copy.aprx>]` | Batch repair broken workspace paths in an `.aprx` project. |

**Examples:**

```bash
# Export all layouts in a project folder to 300 DPI PDFs
geokit layout export ./04_Projects --out ./exports --format pdf --dpi 300

# Check for broken data sources before submitting maps
geokit map broken-links ./04_Projects

# Fix broken paths after moving a project drive
geokit map fix-links ./04_Projects/peta_das.aprx --find "D:\OldData" --replace "E:\NewData" --out ./04_Projects/peta_das_fixed.aprx
```

---

### 4. Raster & Hydrology / Watershed (DAS) Toolkit

| Command | Description |
| :--- | :--- |
| `geokit raster mosaic <dir> --out <merged.tif>` | Mosaic multiple DEM/raster tiles (e.g. DEMNAS/SRTM) into one GeoTIFF (ArcGIS Pro or `--engine gdal`). |
| `geokit raster terrain <dem.tif> --out <dir> [--all \| --slope --aspect --hillshade]` | Generate slope, aspect, and hillshade derivatives from a DEM (ArcGIS Pro Spatial Analyst or `--engine gdal`). |
| `geokit raster zonal-stats <raster.tif> --zones <poly> --zone-field <field> --out <stats.csv>` | Compute zonal statistics per polygon/sub-watershed to CSV/DBF. |
| `geokit hydro delineate <dem.tif> --out <dir> [--threshold 1000] [--pour-point <outlet.shp>]` | Automated DAS pipeline: `Fill` → `FlowDirection` → `FlowAccumulation` → `Streams (.shp)` → `Watershed (.shp)`. |
| `geokit hydro basin <dem.tif> --out <dir>` | Automatically delineate all drainage basins from a DEM into a polygon shapefile. |

**Examples:**

```bash
# Generate slope, aspect, and hillshade using GDAL (no Spatial Analyst license needed)
geokit --engine gdal raster terrain ./02_Raster/dem_das.tif --out ./terrain --all

# Run automated watershed (DAS) delineation with an outlet point
geokit hydro delineate ./02_Raster/dem_utm.tif --out ./das_output --threshold 500 --pour-point ./outlet.shp

# Compute elevation statistics per sub-watershed
geokit raster zonal-stats ./02_Raster/dem_utm.tif --zones ./sub_das.shp --zone-field SUB_DAS --out ./elev_stats.csv
```

---

### 5. Project Configuration (`geokit.toml`)

Generate a starter configuration file in your project root:

```bash
geokit init
```

Customize folder names for `organize`, default EPSG for `gp project`, default DPI/format for `layout export`, or a custom `propy.bat` path:

```toml
[defaults]
epsg = 32748
dpi = 300
layout_format = "pdf"

[folders]
vector = "01_Vector"
raster = "02_Raster"
geodatabase = "03_Geodatabase"
projects = "04_Projects"
tabular = "05_Tabular"
```

---

## Roadmap

- [x] `geokit layout list` & `geokit layout export` — Batch export `.aprx` layouts to PDF/PNG/JPG
- [x] `geokit map broken-links` & `geokit map fix-links` — Detect and repair broken data sources in `.aprx`
- [x] `geokit init` & `geokit.toml` — Custom folder templates, default EPSG, and layout defaults per project
- [x] Optional GDAL / QGIS CLI backend (`--engine auto|arcgis|gdal`) for cross-platform & non-ArcGIS environments
- [x] Batch overlay geoprocessing (`intersect`, `erase`, `spatial-join`)
- [x] Raster & Hydrology/DAS toolkit (`raster mosaic/terrain/zonal-stats`, `hydro delineate/basin`)

---

## License

This project is licensed under the [MIT License](LICENSE).
