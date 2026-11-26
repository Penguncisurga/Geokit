# Changelog

All notable changes to `geokit` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
