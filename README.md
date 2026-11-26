# geokit

[![CI](https://github.com/Penguncisurga/Geokit/actions/workflows/ci.yml/badge.svg)](https://github.com/Penguncisurga/Geokit/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Penguncisurga/Geokit)](https://github.com/Penguncisurga/Geokit/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

> **Fast GIS CLI & built-in MCP server written in Rust for spatial data hygiene, batch ArcGIS Pro / ArcMap 10.x / GDAL geoprocessing, layouts, raster terrain, and watershed (DAS) hydrology.**

[Baca dalam Bahasa Indonesia (README.id.md)](README.id.md) · [AI Agent Playbook (AGENT_GUIDE.md)](AGENT_GUIDE.md) · [Changelog](CHANGELOG.md) · [Contributing](CONTRIBUTING.md)

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

### Option 1: Install with Your AI Assistant *(Easiest for Beginners!)*
Just copy-paste this prompt into **Claude Code, Antigravity, Cursor, Codex, or Gemini CLI**:

```text
Install geokit from https://github.com/Penguncisurga/Geokit for me:
1. Run the one-line installer (`install.ps1` on Windows or `install.sh` on Linux/macOS) to download the latest binary and add it to PATH.
2. Run `geokit doctor` to verify my ArcGIS Pro, ArcMap 10.x, or QGIS/GDAL setup.
3. If ArcMap 10.x is installed, make sure ArcMap is closed and run `geokit live install-addin`.
4. Register `geokit mcp` in my MCP config so you can control my GIS tools directly.
```

### Option 2: One-Line Installer *(No Rust Required)*

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/Penguncisurga/Geokit/main/install.ps1 | iex
```

**Linux / macOS (Apple Silicon):**
```bash
curl -fsSL https://raw.githubusercontent.com/Penguncisurga/Geokit/main/install.sh | sh
```

### Option 3: Download Prebuilt Binary Manually
Download the latest binary (`geokit-windows-x86_64.exe`, `geokit-linux-x86_64`, or `geokit-macos-arm64`) and verify against `SHA256SUMS.txt` from [GitHub Releases](https://github.com/Penguncisurga/Geokit/releases), rename to `geokit.exe` (or `geokit`), and place it on your `PATH`.

### Option 4: Build from Source
Requires [Rust](https://rustup.rs/):

```bash
git clone https://github.com/Penguncisurga/Geokit.git
cd Geokit
cargo build --release --locked
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
| `geokit report <dir> --out <report.html\|.md\|.json>` | Generate a clean standalone HTML, Markdown, or JSON workspace inventory report (no ArcGIS required). |

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
| `geokit gp xy2shp <table.csv> --x-field lon --y-field lat --epsg 4326 --out <points.shp>` | Convert a CSV with X/Y (Lon/Lat) coordinates into `.shp`, `.gpkg`, or `.gdb` points (ArcGIS Pro, ArcMap, or GDAL). |
| `geokit gp convert <input> --to shp\|gpkg\|geojson\|kml\|fgb --out <out>` | Batch convert vector datasets between SHP, GeoPackage, GeoJSON, KML, and FlatGeobuf. |
| `geokit qa <input> [--expected-epsg 32748] [--check-nulls DESA,KELAS] [--check-overlaps]` | Spatial Quality Assurance check: CRS uniformity, invalid geometry, required NULL/empty fields, and polygon overlaps. |
| `geokit gp values <input> [--field <name>] [--json]` | Inspect unique attribute values/counts and numeric or raster statistics (`min`, `max`, `mean`, `std`). |
| `geokit gp classify <input> --field <src> --out-field <dst> --rules "<rules>"` | Score or classify categories (`"Hutan=1;Sawah=3"`) or numeric ranges (`"<=8:1;8-15:2;>15:3"`). |
| `geokit gp calc-field <input> --field <dst> --expr "<formula>"` | Calculate weighted overlay totals or math expressions safely across fields (`"0.4 * A + 0.6 * B"`). |
| `geokit gp union <layer1> <layer2> [...] --out <out>` | Union multiple polygon layers (or all layers in a folder/`.gdb`) for multi-parameter overlay analysis. |
| `geokit gp summary <layer.shp\|.geojson> --group-by KELAS [--out rekap.md\|.csv\|.html]` | Compute instant class count, total area (ha), and percentage (%) tables ready for Word/Excel reports (**100% native Rust, zero ArcGIS needed**). |

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
| `geokit raster reclass <in.tif> --rules "0-8:1;8-15:2;>15:3" --out <cls.tif> [--smooth]` | Reclassify continuous rasters into discrete classes with optional majority-filter smoothing (Spatial Analyst). |
| `geokit raster weighted-sum --inputs "r1.tif:0.4,r2.tif:0.6" --out <out.tif> [--classes 5 --polygon-out <poly.shp> --smooth]` | Multi-parameter raster weighted overlay with optional auto-reclass (1..N) and clean dissolved polygon export (`KELAS`, `AREA_HA`). |
| `geokit raster interpolate <pts.shp> --field <col> --out <surface.tif> [--method idw\|spline\|kriging] [--mask <aoi>]` | Spatial surface interpolation from point data (e.g. rainfall stations, boreholes) using IDW, Spline, or Ordinary Kriging. |
| `geokit raster index --type ndvi\|ndwi\|ndbi\|savi\|nbr --band-a <b1> --band-b <b2> --out <idx.tif>` | Calculate satellite spectral indices (NDVI, NDWI, NDBI, SAVI, NBR) with optional N-class polygon export. |
| `geokit raster contour <dem.tif> --interval 25 --out <kontur.shp> [--index-every 4]` | Generate topographic contours from DEM with automatic BIG RBI index-contour flag (`INDEX_KTR`). |
| `geokit fetch osm --aoi <poly.shp> --layers road,river,building --out <dir>` | Download OpenStreetMap vectors for any AOI (auto UTM→WGS84) into clean GeoJSON (**100% native Rust, zero ArcGIS needed**). |
| `geokit fetch dem --aoi <poly.shp> --out <dir> [--clip-aoi]` | Download free 30m SRTM elevation tiles (`.hgt`) covering any AOI (**100% native Rust, zero API key**); add `--clip-aoi` to auto-mosaic, reproject to AOI UTM, and clip into `dem_aoi_clipped.tif` in one step. |
| `geokit webmap <layer1.shp> [layer2.shp ...] --field KELAS --out <map.html>` | Export one or many UTM/WGS84 shapefiles/GeoJSONs into a single self-contained multi-layer interactive Leaflet HTML webmap with checkbox layer switcher, satellite basemap, popups, opacity slider & area chart (**100% native Rust**). |
| `geokit hydro delineate <dem.tif> --out <dir> [--threshold 1000] [--pour-point <outlet.shp>]` | Automated DAS pipeline: `Fill` → `FlowDirection` → `FlowAccumulation` → `Streams (.shp)` → `Watershed (.shp)`. |
| `geokit hydro basin <dem.tif> --out <dir>` | Automatically delineate all drainage basins from a DEM into a polygon shapefile. |

**Examples:**

```bash
# Generate slope, aspect, and hillshade using GDAL (no Spatial Analyst license needed)
geokit --engine gdal raster terrain ./02_Raster/dem_das.tif --out ./terrain --all

# Multi-parameter hazard modeling: reclassify rasters -> weighted overlay -> dissolved 5-class polygon
geokit raster reclass ./slope.tif --rules "0-8:1;8-15:2;15-25:3;25-45:4;>45:5" --out ./slope_cls.tif --smooth
geokit raster weighted-sum --inputs "elev_cls.tif:0.5,slope_cls.tif:0.5" --out ./hazard.tif --classes 5 --smooth --polygon-out ./hazard_5cls.shp

# Interpolate rainfall station points into a continuous surface clipped to AOI
geokit raster interpolate ./stasiun_hujan.shp --field CURAH_MM --method idw --mask ./batas_kab.shp --out ./hujan_idw.tif

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

### 6. Built-in MCP Server (`geokit mcp`) & ArcMap 10.x Support

`geokit` ships with a built-in **Model Context Protocol (MCP)** JSON-RPC 2.0 server over `stdio` exposing **19 GIS tools** (including `geokit_daemon` for 5×–15× faster hot-session ArcPy execution!) and **3 built-in GIS SOP Playbook Prompts** (`peta_kerawanan`, `peta_persebaran`, `analisis_das` — see [AGENT_GUIDE.md](AGENT_GUIDE.md)), allowing AI assistants (Antigravity, Claude Desktop, Cursor, VS Code) to run end-to-end GIS workflows natively.

Add to your MCP client configuration:

```json
{
  "mcpServers": {
    "geokit": {
      "command": "geokit",
      "args": ["mcp"]
    }
  }
}
```

**ArcMap 10.x (`.mxd` / Python 2.7) Full Support:**
- On ArcMap-only machines (without ArcGIS Pro), `--engine auto` automatically selects **ArcMap 10.x** (`ArcGIS Pro → ArcMap 10.x → GDAL`) for **all** `gp`, `raster`, `hydro`, `layout`, and `map` commands!
- Passing an `.mxd` file (or folder of `.mxd` files) to `geokit layout` or `geokit map` automatically routes to ArcMap (`arcpy.mapping`).
- Pass `--engine arcmap` (or set `engine = "arcmap"` in `geokit.toml`) to force ArcMap 10.x Python 2.7 even when ArcGIS Pro is also installed.

---

### 7. Live-UI Control for ArcMap 10.1–10.8 & ArcGIS Pro 2.x/3.x (`geokit live` & MCP `geokit_live`)

Control an **open ArcMap 10.1 – 10.8.2 OR ArcGIS Pro 2.x / 3.x window in real time** (just like QGIS MCP) — including **all 800+ ArcToolbox tools** — over a unified local TCP bridge (`127.0.0.1:9877`)!

**Setup for ArcMap 10.x (auto-start Python Add-In):**
1. **Close ArcMap**, then run once:
   ```bash
   geokit live install-addin
   ```
2. **Open ArcMap 10.x** — GeoKit Live Bridge auto-starts automatically on port `9877`.

**Setup for ArcGIS Pro 2.x / 3.x (1-click `.pyt` Toolbox + CIM styling):**
1. Extract the bridge script and 1-click Toolbox:
   ```bash
   geokit live install-pro-bridge
   ```
2. In ArcGIS Pro Catalog -> Toolboxes -> Add Toolbox -> open `%USERPROFILE%\.geokit\pro\GeoKit_Live_Bridge.pyt` and run **`Start GeoKit Live Bridge`** (or run the printed 1-liner in ArcGIS Pro's Python Window).

**Connect to Any AI Agent (Claude Desktop, Antigravity, Cursor, Claude Code, Codex):**
```json
{
  "mcpServers": {
    "geokit": {
      "command": "geokit",
      "args": ["mcp"]
    }
  }
}
```
*(For Claude Code CLI: `claude mcp add geokit -- geokit mcp`)*

**CLI Live Control Examples:**

```bash
# Check live connection & inspect loaded layers + fields/sample rows
geokit live ping
geokit live layers
geokit live layer-info batas_desa

# Add, rename, zoom (with padding + standard scale snap), filter, style, label with white halo, build PerKaBIG A4 layout, export & save live
geokit live add ./01_Vector/batas_desa.shp
geokit live rename batas_desa --name "Batas Administrasi Desa"
geokit live zoom "Batas Administrasi Desa" --padding 0.12 --scale auto
geokit live definition-query "Batas Administrasi Desa" --where "KECAMATAN = 'Boja'"
geokit live symbology "Batas Administrasi Desa" --lyr ./styles/batas_desa.lyr
# Full ArcObjects COM cartography (unlimited N classes, BIG admin dashes, white label halos, rasters!)
geokit live style kerawanan_banjir --field KELAS_RWN --preset hazard-pastel --transparency 30
geokit live style overlay_skor --field TOTAL_SKOR --mode graduated --classes-count 5 --preset viridis
geokit live style sungai --preset stream --width 1.4
geokit live style batas_kab --preset boundary --outline-style admin-kab --outline-width 1.6
geokit live style dem_kendal --mode raster --preset terrain
geokit live order sungai --position top
geokit live labels "Batas Administrasi Desa" --show true --field NAMOBJ --halo true --font-size 8.5
# Full 7-section PerKaBIG No. 3/2016 Pro layout (A4/A3, dual official insets, Indonesian 'U' North Arrow, RBI dual-grid blue ticks!)
geokit live layout --template perkabig --paper a4 --title "PETA KERAWANAN BENCANA" --region "KABUPATEN KENDAL" --author "Laboratorium SIG" --inset-petunjuk ./batas_kabupaten.shp --inset-layer ./batas_provinsi.shp --grid-mode dual --north-style big --source "1. DEM SRTM 30m & Pengolahan GeoKit" --zoom-layer batas_kab --hide-legend "hillshade,Basemap" --grid true
geokit live select "Batas Administrasi Desa" --where "KELAS = 'Hutan'"
geokit live export --out ./peta_perkabig.png --dpi 200
geokit live save --out ./peta_kerja.mxd

# Discover & run ANY tool in ArcToolbox (auto-checks out Spatial/3D/Network extensions!)
geokit live tools slope
geokit live tool-usage Slope_sa
geokit live run-gp Buffer_analysis ./sungai.shp ./sungai_buf.shp "100 Meters"
```

---

### 8. Future-Proof ArcGIS Pro Tools, ModelBuilder `.pyt` Generator & Pipelines

- **Always up-to-date with new ArcGIS Pro releases:**
  ```bash
  # Discover & run ANY tool installed in your ArcGIS Pro / ArcMap version
  geokit gp tools slope
  geokit gp tool-usage Slope_sa
  geokit gp run Buffer_analysis ./sungai.shp ./sungai_buf.shp "100 Meters"
  ```
- **ModelBuilder-ready Python Toolbox (`.pyt`) generator:**
  ```bash
  # Generate a GUI .pyt toolbox ready to drag into ArcGIS Pro ModelBuilder
  geokit toolbox create GeoKit_DAS --out ./GeoKit_DAS.pyt --template das
  geokit toolbox create Batch_Prep --out ./Batch_Prep.pyt --template batch-prep
  ```
- **Declarative Pipelines (`pipeline.toml` — text-based ModelBuilder):**
  ```bash
  geokit pipeline init --out pipeline.toml
  geokit pipeline run pipeline.toml --dry-run
  geokit pipeline run pipeline.toml
  ```
- **Advanced `.aprx` automation:**
  ```bash
  # Export all pages of an ArcGIS Pro Spatial Map Series
  geokit layout map-series ./peta_desa.aprx --out ./exports/desa_all.pdf --dpi 300

  # Add a layer and apply a .lyrx symbology file into an .aprx
  geokit map add-layer ./proyek.aprx --layer ./batas_desa.shp --lyrx ./style_desa.lyrx
  ```

---

## Roadmap

- [x] `geokit layout list` & `geokit layout export` — Batch export `.aprx` & `.mxd` layouts to PDF/PNG/JPG
- [x] `geokit map broken-links` & `geokit map fix-links` — Detect and repair broken data sources in `.aprx` & `.mxd`
- [x] `geokit init` & `geokit.toml` — Custom folder templates, default EPSG, and layout defaults per project
- [x] Optional GDAL / QGIS CLI backend (`--engine auto|arcgis|arcmap|gdal`) for cross-platform & non-ArcGIS environments
- [x] Batch overlay geoprocessing (`intersect`, `erase`, `spatial-join`)
- [x] Raster & Hydrology/DAS toolkit (`raster mosaic/terrain/zonal-stats`, `hydro delineate/basin`)
- [x] Built-in MCP Server (`geokit mcp`) + ArcMap 10.x (`.mxd` / Python 2.7) backend

---

## License

This project is licensed under the [MIT License](LICENSE).
