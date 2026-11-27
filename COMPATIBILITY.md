# GeoKit Backend Compatibility & Algorithmic Semantics Matrix

This document provides a transparent, command-by-command breakdown of what each GeoKit backend supports, which extensions/licenses are required, and how key safety & analytical rules behave.

## 1. Backend Support Matrix

Legend:
- **Native**: Pure Rust (zero external GIS install required; works on Windows, Linux, macOS).
- **ArcGIS Pro**: Python 3 (`arcpy` via `propy.bat` or Hot-Worker Daemon `9879`).
- **ArcMap 10.x**: Python 2.7 (`arcpy` via `ArcGISx6410.x` / `ArcGIS10.x` or Hot-Worker Daemon `9878`).
- **GDAL**: Open-source `ogr2ogr` / `ogrinfo` / `gdalwarp` / `gdaldem` (auto-detected from `PATH`, QGIS, OSGeo4W, or ArcGIS Pro conda `Library\bin`).

| Command | Native Rust | ArcGIS Pro | ArcMap 10.x | GDAL | Required Extension / Notes |
| --- | :---: | :---: | :---: | :---: | --- |
| `doctor`, `init`, `audit`, `report` | Yes | — | — | — | Pure Rust; no GIS license needed |
| `accuracy sample\|matrix\|rmse\|roc` | Yes | — | — | — | Pure Rust validation suite: Stratified Random (`.shp`/`.kml`/`.geojson`/`.csv`), Confusion Matrix + Kappa + F1, RMSE/R²/BIG LE90/CE90, ROC-AUC + Youden's J |
| `organize`, `sanitize`, `clean` | Yes | — | — | — | Transactional bundle rollback; protects `.gdb/*.lock` by default (`--include-gdb-locks` to override) |
| `webmap` | Yes | — | — | — | Supports WGS84, UTM, & Web Mercator (`EPSG:3857`); rejects unrecognized projected CRS |
| `gp summary`, `gp export-table` | Yes | Yes (`.gdb`) | Yes (`.gdb`) | Yes (`.gpkg`/etc.) | Pure Rust for `.shp`/`.geojson`; ArcGIS/GDAL for `.gdb`/`.gpkg` |
| `fetch osm` | Yes | — | — | — | Uses `curl` + Overpass API mirrors; respects `--overwrite` |
| `fetch dem` | Yes | Optional | Optional | Optional | Download is Native; `--clip-aoi` uses ArcGIS/ArcMap/GDAL for mosaic/reproject/clip |
| `gp info`, `project`, `clip`, `merge`, `pack`, `unpack`, `xy2shp`, `convert`, `select`, `flatten-2d`, `define-crs`, `split`, `explode` | — | Yes | Yes | Yes | `xy2shp` auto-parses DMS (`7°00'12"S`, `LS`/`BT`) on all engines; `convert` supports `.gpx` & `.kmz` inputs via GDAL |
| `gp repair`, `buffer`, `multi-buffer`, `dissolve`, `calc-geom`, `intersect`, `erase`, `spatial-join`, `union`, `change`, `join`, `thiessen`, `near`, `eliminate`, `delete-duplicates`, `values`, `classify`, `calc-field`, `clean-fields`, `centroid`, `tools`, `tool-usage`, `run` | — | Yes | Yes | No | `centroid`, `near`, `eliminate`, `delete-duplicates`, `join` work on ArcGIS Basic via cursor fallbacks; `change` outputs `KLAS_T1`/`KLAS_T2`/`PERUBAHAN`/`AREA_HA` + transition matrix; `calc-field` uses AST Safe Math |
| `topo check`, `topo fix` | — | Yes | Yes | No | BIG topology validation (overlaps, interior gaps, slivers, dangles, crossings, duplicates) & 1-command cleanup; `--strict` exits non-zero |
| `raster mosaic`, `terrain`, `align`, `georef`, `composite` | — | Yes | Yes | Yes | All GeoTIFF outputs use lossless **LZW** compression; `georef` parses QGIS `.points`/CSV & computes affine RMSE; `composite` supports Landsat 5/7/8/9, Sentinel-1/2, PlanetScope/SPOT + optional `--aoi` & `--epsg` |
| `raster zonal-stats`, `reclass`, `weighted-sum`, `interpolate`, `index`, `contour`, `extract-points`, `morphometry` | — | Yes | Yes | No | `extract-points` falls back to `GetCellValue` without Spatial Analyst; `index` supports 15 indices (`ndvi..nbr`, `mndwi`, `evi`, `bsi`, `ndre`, `vari`, `exg`, `lst`, `ndti`, `nsmi`/`tss`, `ndci`/`chla`) + `--multiband --sensor`; `morphometry` & others require **Spatial Analyst** |
| `hydro delineate`, `basin` | — | Yes | Yes | No | Requires **Spatial Analyst** extension; writes `<out>/geokit_provenance.json` |
| `qa` | — | Yes | Yes | No | Technical dataset QA (CRS, geometry, NULLs, pairwise polygon overlaps); `--strict` exits non-zero on issues |
| `layout list`, `export`, `update` | — | Yes (`.aprx`) | Yes (`.mxd`) | No | `layout map-series` is ArcGIS Pro (`.aprx`) only |
| `map broken-links`, `fix-links` | — | Yes (`.aprx`) | Yes (`.mxd`) | No | `map add-layer`, `style` are `.aprx` (ArcGIS Pro) |
| `live *` (Port `9877`) | — | Yes (Bridge `.pyt`) | Yes (Add-In) | No | PerKaBIG 7-section layout is ArcMap Add-In; Pro bridge uses CIM symbology; default export **300 DPI** |
| `daemon start\|status\|stop` | — | Yes (`9879`) | Yes (`9878`) | N/A | GDAL starts natively in <0.1s; daemon auto-authenticates via `~/.geokit/token` |
| `pipeline init\|run`, `toolbox create`, `mcp` | Yes | Yes | Yes | Yes | `pipeline run` supports `--resume`, per-run logs (`pipeline-run-<ts>.log`), & RAII daemon guard; analytical tools write `<out>.geokit.json` provenance |

---

## 2. Data Safety & Security Guarantees (`v2.6.1+`)

1. **Safe Mode Default (`overwrite = false`)**: GeoKit refuses to overwrite existing outputs across ArcGIS Pro, ArcMap, GDAL, and `fetch` unless `--overwrite` (or `GEOKIT_OVERWRITE=1`) is explicitly passed. Output paths equal to any input/AOI/DEM/mask dataset are **always blocked**.
2. **Workspace Guardrails**: `clean`, `organize`, and `sanitize` refuse to run on filesystem roots (`C:\`, `/`), OS system directories (`Windows`, `Program Files`, `/etc`, `/usr`), bare user home roots, or outside `GEOKIT_WORKSPACE_ROOT` (when set).
3. **Localhost TCP Authentication & Limits**: Local bridges (`9877`) and daemons (`9878`/`9879`) bind strictly to `127.0.0.1`, require a per-user 64-hex token (`~/.geokit/token` or `GEOKIT_AUTH_TOKEN`), enforce a `10 MB` payload cap, and apply a `30s` socket read timeout.
4. **AST Safe Math & Anti-XSS**: `gp calc-field` parses expressions via Python `ast.parse` (no `eval()`, no attribute/dunder access). HTML outputs (`webmap`, `gp summary`, `report`) escape HTML entities and JSON `</script>` sequences.

---

## 3. Analytical & Classification Rules

- **`gp classify` Interval Matching**: Rules are evaluated in the exact order written by the user (`;` separated):
  - `<=a` matches `x <= a`, `<a` matches `x < a`, `>=a` matches `x >= a`, `>a` matches `x > a`.
  - `a-b` matches closed interval `a <= x <= b` (first matching rule wins; to avoid boundary ambiguity, write e.g. `<=8:1;>8:2` or non-overlapping `0-8:1;8.01-15:2`).
  - `NULL`/`None` values are never matched by numeric rules and fall through to `--default` (or remain `NULL`).
- **`raster weighted-sum`**: Weights must be finite and `>= 0`. If the sum of weights differs from `1.0` by more than `0.01`, GeoKit emits a warning to `stderr` without silently normalizing your formula. Output polygon area (`AREA_HA`) uses **geodesic hectares** (`!shape.geodesicArea@hectares!`).
- **Remote Sensing Indices (`raster index`)**:
  - `evi` assumes surface reflectance scaled `[0, 1]` (or `[0, 10000]` converted first — adjust inputs before running `L = 1` EVI formula `2.5*(NIR-Red)/(NIR+6*Red-7.5*Blue+1)`).
  - `lst` uses the USGS Landsat Collection 2 Level 2 `ST_B10`/`ST_B6` scale/offset formula (`DN * 0.00341802 + 149.0 - 273.15`) to output **°C**.
  - Water-quality indices `ndti`, `nsmi` (alias `tss`), and `ndci` (alias `chla`) are **relative dimensionless spectral indices** — not calibrated mg/L or µg/L concentrations without local in-situ regression.
- **Technical QA vs. Scientific Validation**: `geokit qa` verifies **technical dataset health** (CRS consistency, geometry validity, required NULL checks, polygon overlaps). It does **not** replace scientific ground-truth validation (Confusion Matrix / Kappa / RMSE / ROC-AUC).
