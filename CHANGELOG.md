# Changelog

All notable changes to `geokit` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.3.0] - 2026-11-26

### Added
- **Full ArcToolbox AI Discovery & Execution**: added `geokit live tools <keyword>` (`arcpy.ListTools`) and `geokit live tool-usage <tool>` (`arcpy.Usage`), plus automatic extension checkout (`Spatial`, `3D`, `Network`, `GeoStats`, `Tracking`) in `geokit live run-gp`.
- **Live ArcMap Cartography & Inspection**: added `layer-info` (fields, CRS, feature count, sample rows), `definition-query`, `symbology` (apply `.lyr` style live), `labels` (toggle & set field), and `save` (`.mxd`).
- **Instant Live Repaint**: added `newDocument` / `openDocument` hooks and Win32 `RedrawWindow` repaint so layers appear immediately without pressing `F5`.
- **Universal ArcMap 10.1–10.8 Detection**: `geokit live install-addin` now also scans `%APPDATA%\ESRI\Desktop10.*`, supporting custom drive installations (e.g. `D:\`).
- Complete MCP client setup guides (Claude Desktop, Antigravity, Cursor, Claude Code, Codex) in `README.md` and `README.id.md`.

### Fixed
- Renamed hydrology shapefile outputs (`04_streams.shp` -> `streams.shp`, `04_basins.shp` -> `basins.shp`, `05_watershed.shp` -> `watershed.shp`) to avoid ArcMap 10.x `arcpy.Describe().shapeType` errors on digit-prefixed shapefiles.

## [1.2.2] - 2026-11-26

### Fixed
- `geokit live install-addin` now writes the exact `<AddinFolder Path="..."/>` schema into ArcMap's per-user `%APPDATA%\ESRI\Desktop10.x\AddInFoldersList.xml` (no admin rights required, works with OneDrive & all Windows locales).
- Fixed `Config.xml` manifest casing (`Config.xml`), version format (`1.0`), and bundled 16×16 icon inside `geokit-arcmap.esriaddin`.
- Fixed duplicate raster layer insertion when adding `.tif` rasters via `geokit live add`.

## [1.2.1] - 2026-11-26

### Fixed
- `geokit live install-addin` now extracts `geokit-arcmap.esriaddin` into `%USERPROFILE%\.geokit\addins` and registers it in ArcMap's official `HKCU\Software\ESRI\Desktop10.x\Settings\AddInFolders` registry key — avoiding OneDrive cloud-placeholder and Windows Controlled Folder Access failures on localized Windows systems.

## [1.2.0] - 2026-11-26

### Added
- **ArcMap 10.x Live-UI Bridge (`geokit-arcmap.esriaddin` + `geokit live` + MCP `geokit_live`)**: real-time control of an open ArcMap window (like QGIS MCP!) over localhost TCP (`127.0.0.1:9877`) using main-thread `SetTimer` dispatch.
- One-command installer: `geokit live install-addin` writes the embedded `.esriaddin` directly into `Documents\ArcGIS\AddIns\Desktop10.x`.
- Live commands: `ping`, `layers`, `add`, `remove`, `visible`, `zoom`, `select`, `clear-selection`, `export`, and `run-gp` (auto-adds GP output straight onto the active map).

## [1.1.0] - 2026-11-26

### Added
- Full **ArcMap 10.x (Python 2.7)** feature parity: added `raster mosaic`, `raster terrain`, `raster zonal-stats`, `hydro delineate`, and `hydro basin` to `scripts/arcmap_runner.py`.
- Smart `--engine auto` fallback order: **ArcGIS Pro → ArcMap 10.x → GDAL**, so ArcMap-only users never need to pass `--engine arcmap` manually.
- Added `arcmap` engine option to `geokit_raster` and `geokit_hydro` MCP tools.

## [1.0.0] - 2026-11-26

### Added
- `geokit.toml`: added `arcgis.arcmap_python` and `defaults.engine` options.
- `geokit doctor`: now displays active `geokit.toml` configuration status.
- Automatic `.mxd` directory detection for `layout` and `map` commands (routes to ArcMap 10.x when a folder contains `.mxd` projects without `.aprx`).
- Cross-platform Release workflow now builds Windows (`x86_64`), Linux (`x86_64`), and macOS Apple Silicon (`aarch64`) binaries with `SHA256SUMS.txt`.
- Repository governance: `CHANGELOG.md`, `CONTRIBUTING.md`, monthly Dependabot updates, and GitHub Issue templates.
- Committed `Cargo.lock` for reproducible CLI builds.

### Fixed
- Removed plain `.json` from `VECTOR_PRIMARY_EXTS` so non-GIS files like `package.json` are never moved by `geokit organize` (`.geojson` remains supported).

## [0.5.0] - 2026-11-26

### Added
- Built-in Model Context Protocol (MCP) JSON-RPC 2.0 server (`geokit mcp`) over `stdio` exposing 10 GIS tools.
- Full ArcMap 10.x (`.mxd` / Python 2.7) backend (`--engine arcmap`) for geoprocessing, MXD layout export, and broken-link detection/repair.

## [0.4.0] - 2026-11-26

### Added
- `geokit raster` toolkit: `mosaic`, `terrain` (slope/aspect/hillshade via ArcGIS Spatial Analyst or GDAL `gdaldem`), and `zonal-stats` to CSV/DBF.
- `geokit hydro` watershed (DAS) pipelines: `delineate` (`Fill` -> `FlowDirection` -> `FlowAccumulation` -> `Streams` -> `Watershed`) and `basin`.

## [0.3.1] - 2026-11-26

### Fixed
- Isolated `PROJ_DATA`, `PROJ_LIB`, and `GDAL_DATA` when invoking QGIS-bundled GDAL on Windows to prevent `proj.db` version conflicts with PostgreSQL/PostGIS.

## [0.3.0] - 2026-11-26

### Added
- Batch overlay geoprocessing: `gp intersect`, `gp erase`, and `gp spatial-join`.
- Cross-platform GDAL / QGIS CLI backend (`--engine auto|arcgis|gdal`) with automatic QGIS detection on Windows.

## [0.2.0] - 2026-11-26

### Added
- `geokit init` & `geokit.toml` project configuration (custom folder names, default EPSG, default DPI/format, custom `propy.bat`).
- `geokit layout list` and `geokit layout export` for batch `.aprx` layout export (PDF/PNG/JPG).
- `geokit map broken-links` and `geokit map fix-links` for `.aprx` data source diagnostics and repair.

## [0.1.0] - 2026-11-26

### Added
- Native Rust spatial data hygiene commands: `doctor`, `audit`, `organize`, `sanitize`, and `clean` (bundle-safe for shapefiles, rasters, and `.gdb`).
- Single-startup ArcGIS Pro (`arcpy`) batch geoprocessing: `gp info`, `project`, `clip`, `merge`, `pack`, `unpack`, `repair`, `buffer`, `dissolve`, and `calc-geom`.
