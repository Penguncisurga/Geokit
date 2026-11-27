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
| `organize`, `sanitize`, `clean` | Yes | — | — | — | Transactional bundle rollback; protects `.gdb/*.lock` by default (`--include-gdb-locks` to override) |
| `webmap` | Yes | — | — | — | Supports WGS84, UTM, & Web Mercator (`EPSG:3857`); rejects unrecognized projected CRS |
| `gp summary` | Yes | — | — | — | Pure Rust `.shp`/`.geojson` attribute aggregation (`.md`/`.csv`/`.html`/`.json`) |
| `fetch osm` | Yes | — | — | — | Uses `curl` + Overpass API mirrors; respects `--overwrite` |
| `fetch dem` | Yes | Optional | Optional | Optional | Download is Native; `--clip-aoi` uses ArcGIS/ArcMap/GDAL for mosaic/reproject/clip |
| `gp info`, `project`, `clip`, `merge`, `pack`, `unpack`, `xy2shp`, `convert` | — | Yes | Yes | Yes | Full cross-platform support with Safe Mode (`--overwrite` required to replace existing outputs) |
| `gp repair`, `buffer`, `dissolve`, `calc-geom`, `intersect`, `erase`, `spatial-join`, `union`, `values`, `classify`, `calc-field`, `tools`, `tool-usage`, `run` | — | Yes | Yes | No | `erase` requires ArcGIS Advanced license; `calc-field` uses AST Safe Math Evaluator |
| `raster mosaic`, `terrain` | — | Yes | Yes | Yes | ArcGIS/ArcMap `terrain` requires **Spatial Analyst**; GDAL `gdaldem` requires no license |
| `raster zonal-stats`, `reclass`, `weighted-sum`, `interpolate`, `index`, `contour` | — | Yes | Yes | No | Requires **Spatial Analyst** extension |
| `hydro delineate`, `basin` | — | Yes | Yes | No | Requires **Spatial Analyst** extension |
| `qa` | — | Yes | Yes | No | Technical dataset QA (CRS, geometry, NULLs, pairwise polygon overlaps); `--strict` exits non-zero on issues |
| `layout list`, `export`, `update` | — | Yes (`.aprx`) | Yes (`.mxd`) | No | `layout map-series` is ArcGIS Pro (`.aprx`) only |
| `map broken-links`, `fix-links` | — | Yes (`.aprx`) | Yes (`.mxd`) | No | `map add-layer`, `style` are `.aprx` (ArcGIS Pro) |
| `live *` (Port `9877`) | — | Yes (Bridge `.pyt`) | Yes (Add-In) | No | PerKaBIG 7-section layout is ArcMap Add-In; Pro bridge uses CIM symbology |
| `daemon start\|status\|stop` | — | Yes (`9879`) | Yes (`9878`) | N/A | GDAL starts natively in <0.1s; daemon auto-authenticates via `~/.geokit/token` |
| `pipeline init\|run`, `toolbox create`, `mcp` | Yes | Yes | Yes | Yes | `pipeline run` uses RAII daemon guard; `mcp` supports `GEOKIT_MCP_READONLY=1` & `GEOKIT_WORKSPACE_ROOT` |

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
- **Technical QA vs. Scientific Validation**: `geokit qa` verifies **technical dataset health** (CRS consistency, geometry validity, required NULL checks, polygon overlaps). It does **not** replace scientific ground-truth validation (Confusion Matrix / Kappa / RMSE / ROC-AUC).
