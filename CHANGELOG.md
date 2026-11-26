# Changelog

All notable changes to `geokit` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.1.2] - 2026-11-26

### Improved
- **PerKaBIG Cartographic Polish**:
  - Metric **Kilometers (`km`)** alternating scale bar (`IScaleBar.put_Units(10)`, `Divisions=2`, `Subdivisions=2`) instead of default `Miles`.
  - **Clean Round UTM Grid** (`FixedOrigin=True`, `XOrigin=0`, `YOrigin=0`, **0 decimal places** via `FormattedGridLabel` + `NumericFormat`) with **vertical Left/Right axis labels** and a wider `1.2 cm` gutter before the Right Info Panel so grid numbers never overlap `LEGENDA`.
  - **True Bold Map Title** (`IFont.put_Bold`) + balanced vertical spacing between Title/Subtitle and North Arrow (zero overlap).
  - **Smart Legend Headings**: automatically hides raw attribute field names (`ShowHeading=False`) while showing clean layer titles (`ShowLayerName=True`) for multi-class layers (`Tingkat Kerawanan`).
  - **Tighter Proportional Framing**: added intermediate standard scales (`1:125.000`, `1:300.000`, `1:350.000`, `1:400.000`, `1:750.000`) with `8%` padding so study areas fill ~80% of the MapFrame cleanly.

## [2.1.1] - 2026-11-26

### Fixed
- **ArcMap Live PerKaBIG Layout COM Fix**: Replaced `IGraphicsContainer.FindFrame` raw-pointer call (which expects a COM `VARIANT`) with direct `IMapFrame` detection via `IGraphicsContainer.Next`, and corrected `IPage.put_Units` to `8` (`esriCentimeters` instead of `9` = `esriMeters`) so A4 Landscape sheets render at true `29.7 × 21.0 cm` and export in seconds.
- **Spatial Interpolation Mask Clipping**: `geokit raster interpolate --mask <aoi>` now explicitly applies `ExtractByMask` after IDW/Spline/Kriging so output surfaces are cleanly clipped to polygon boundaries in ArcMap 10.x (where `Idw` ignores `arcpy.env.mask`).

## [2.1.0] - 2026-11-26

### Added
- **One-Command PerKaBIG A4 Landscape Layout Builder (`geokit live layout --template perkabig`)**:
  - Automatically constructs a clean publication-grade A4 Landscape (`29.7 × 21.0 cm`) Indonesian PerKaBIG layout via ArcObjects COM: 70% Left MapFrame with **UTM `MeasuredGrid`** & 12–14% framing margin + standard scale snapping, and 28% Right Info Panel with neatly boxed **Title/Subtitle**, **North Arrow**, **Alternating Scale Bar + Ratio**, **Clean Legend** (auto-hiding `hillshade`/`Basemap`), and **CRS + Source**.
- **True Cartographic Dashed & BIG Admin Boundary Lines (`CartographicLineSymbol` + `Template`)**:
  - Upgraded `geokit live style --outline-style` to support crisp thick dashes and Indonesian BIG administrative boundary patterns (`dash`, `dot`, `dashdot`, `admin-kab`, `admin-kec`, `admin-desa`) at any width.
- **Label White Halo, TOC Layer Rename, & Smart Zoom Framing**:
  - `geokit live labels --halo true --font-size 8.5` applies white halo masks (`ITextSymbol` + `IMask`) for crisp readability over dark fills.
  - `geokit live rename <layer> --name "Tingkat Kerawanan Banjir"` cleans up layer names in TOC & Legend.
  - `geokit live zoom <layer> --padding 0.12 --scale auto` adds breathing room and snaps to standard cartographic scales (`1:25k`, `1:50k`, `1:100k`, `1:250k`, etc.).
- **Scientific Raster Analysis Trio (`ArcGIS Pro` & `ArcMap 10.x` Spatial Analyst)**:
  - `geokit raster reclass` (reclassify continuous raster + optional `--smooth` `MajorityFilter`).
  - `geokit raster weighted-sum` (multi-raster weighted overlay + optional `--classes 5 --polygon-out` export).
  - `geokit raster interpolate` (`idw`, `spline`, `kriging` point-to-raster surface interpolation with `--cell-size` & `--mask`).

## [2.0.2] - 2026-11-26

### Fixed
- Fixed ArcObjects COM `QueryInterface` pointer casts in `Install/arcobjects.py` (`IID_ISymbol`, `IID_IFeatureRenderer`, `IID_IColor`, `IID_ILineSymbol`, `IID_IColorRamp`) so attaching custom renderers & symbols inside ArcMap 10.1–10.8 never causes an access violation.

## [2.0.1] - 2026-11-26

### Fixed
- Added Add-In `Install/` directory to `sys.path` inside `geokit_addin.py` so `arcobjects.py` always imports reliably inside ArcMap 10.1–10.8's embedded Python runtime.

## [2.0.0] - 2026-11-26

### Added
- **Zero-Dependency Native ArcObjects COM Cartography Engine (`Install/arcobjects.py`) for ArcMap 10.1–10.8**:
  - Breaks past `arcpy.mapping`'s read-only symbology restrictions directly in-process via pure-Python `ctypes` COM vtables (no `comtypes`, no `pywin32`, no `.lyr` template hacks, and no `GK_CODE` helper fields!).
  - **Unlimited-Class `UniqueValueRenderer` (`--mode unique`)**: dynamically renders **2, 3, 5, 7, 15, or 50+ classes** directly on any field with smooth RGB interpolation across modern palettes (`hazard`, `hazard5`, `hazard-pastel`, `viridis`, `blue`, `earth`, `terrain`, `category`) or custom hex stops (`--colors "#hex1,#hex2,..."`), with zero junk legend slots.
  - **Graduated Numeric Breaks (`--mode graduated`)**: automatic `quantile` or `equal`-interval `ClassBreaksRenderer` with configurable `--classes-count N`.
  - **Full Line, Point & Boundary Styling (`--mode single`)**: `--preset stream` (cartographic blue rivers), `--preset road`, `--preset boundary` (hollow fill + dashed/solid dark outline), `--preset point` (white-halo markers), plus custom `--color`, `--width`, `--size`, `--marker`, `--outline-color`, `--outline-width`, `--outline-style`, and `--hollow`.
  - **Raster Stretch Color Ramps (`--mode raster`)**: direct `IRasterStretchColorRampRenderer` + `IMultiPartColorRamp` (CIELAB smooth gradients) for DEM, Slope, and continuous raster layers!
  - **Layout Legend Cleanup (`geokit live layout --hide-legend`)**: automatically removes background layers (e.g. `hillshade`, `Basemap`) from Layout Legends via `ILegend.RemoveItem`.

### Fixed
- Fixed Windows MSVC debug CI stack overflow by running CLI execution on a 32 MB stack thread (`src/main.rs`) and parsing MCP tool schemas via `serde_json::from_str` (`src/commands/mcp.rs`).

## [1.7.0] - 2026-11-26

### Added
- **Publication-Grade Automatic Cartography (`geokit live style` & `geokit map style`)**:
  - Built-in borderless color presets: `hazard` (Green `#2E7D32` → Yellow `#FBC02D` → Red `#D32F2F`), `hazard5` (5-class RdYlGn), `blue`, `earth`, and `category` (12-class ColorBrewer) with configurable `--transparency` and automatic severity sorting (`Rendah / Sedang / Tinggi`, `Low / Medium / High`).
  - Embeds 5 zero-outline ArcMap 10.x `.lyr` templates directly inside `geokit.exe` (`styles/arcmap/*.lyr`) — works on every ArcMap 10.1–10.8 installation with zero external Python packages!
  - Pure `arcpy.mp` CIM styling for ArcGIS Pro (`.aprx`) via `geokit map style`.
- **Live Layer Stacking & Layout Automation**:
  - `geokit live order <layer> --position top|bottom` to arrange layers cleanly (e.g. streams above hazard polygons).
  - `geokit live layout [--view layout|data] [--title ...] [--subtitle ...] [--source ...] [--zoom-layer ...]` and `geokit layout update <aprx|mxd>` for automated title/subtitle/source updates and map frame zooming.

### Fixed
- `geokit gp run` and `geokit live run-gp` now accept negative numeric arguments (e.g. `"-50 50 5;..."`) directly without requiring `--`.
- Added `arcpy.gp.<tool>` fallback in `gp run` / `live run-gp` so Spatial Analyst tools such as `Reclassify_sa` work directly.
- Fixed ArcMap 10.x `gp calc-geom` `Describe().shapeType` errors when datasets live inside directories containing a dot (e.g. `.gemini`).

## [1.6.0] - 2026-11-26

### Added
- **Attribute & Raster Inspection (`geokit gp values`)**: inspect unique category values with frequency counts, NULL counts, and numeric/raster statistics (`min`, `max`, `mean`, `std`) across ArcGIS Pro and ArcMap 10.x (`--json` supported).
- **One-Command Scoring & Classification (`geokit gp classify`)**: classify or score any attribute field into a new score/class column using category mappings (`"Hutan=1;Sawah=3"`) or numeric ranges (`"<=8:1;8-15:2;>15:3"`), with automatic output field type inference (`LONG`, `DOUBLE`, `TEXT`).
- **Safe Expression Field Calculator (`geokit gp calc-field`)**: compute weighted overlay totals and math formulas (e.g. `"0.4 * SKOR_LERENG + 0.3 * SKOR_TANAH + 0.3 * SKOR_HUJAN"`) via `arcpy.da.UpdateCursor` without CLI quote-escaping bugs.
- **Multi-Layer Overlay (`geokit gp union`)**: union 2+ polygon layers (or an entire folder/`.gdb`) into a single combined feature class for multi-parameter hazard/suitability analysis.
- **Built-in AI GIS Playbooks (MCP Prompts + `AGENT_GUIDE.md`)**: added standard MCP `prompts/list` & `prompts/get` support to `geokit mcp` with 3 end-to-end GIS SOP workflows (`peta_kerawanan`, `peta_persebaran`, `analisis_das`), plus [AGENT_GUIDE.md](AGENT_GUIDE.md).

## [1.5.1] - 2026-11-26

### Improved
- **Smart `geokit live add` Path Resolution**:
  - Passing a **folder** now automatically discovers and adds all `.shp`, `.tif`, `.tiff`, `.lyr`, `.img`, `.ecw`, and `.jp2` layers inside it (instead of failing with `CreateObject Layer invalid data source`).
  - Automatically resolves **missing file extensions** (e.g. `...\ADM_JAWA_TENGAH_KEC` → `...\ADM_JAWA_TENGAH_KEC.shp`) when Windows hides extensions.
  - Automatically normalizes accidental Windows Explorer copy-paste prefixes (e.g. `Lokal Disk (D:)\...` / `Local Disk (D:)\...` → `D:\...`).
  - Fixed `.gdb\<feature_class>` virtual paths being rejected by OS file-existence checks, and added a clear usage hint when a `.gdb` folder itself is passed.

## [1.5.0] - 2026-11-26

### Added
- **CSV / XY Table to Point Dataset (`geokit gp xy2shp`)**: convert tabular CSV files with X/Y (or Lon/Lat) columns straight into `.shp`, `.gpkg`, or `.gdb` point feature classes via ArcGIS Pro, ArcMap 10.x, or GDAL (`--x-field`, `--y-field`, `--epsg`).
- **Batch Vector Format Converter (`geokit gp convert`)**: convert single files, folders, or `.gdb` feature classes across `shp`, `gpkg`, `geojson`, `kml`, and `fgb` (FlatGeobuf), automatically leveraging GDAL where needed.
- **Spatial Quality Assurance Checker (`geokit qa`)**: inspect vector/raster datasets for CRS uniformity (`--expected-epsg`), invalid geometries (`CheckGeometry`), required NULL/empty attribute fields (`--check-nulls`), and optional polygon self-overlaps (`--check-overlaps`) via ArcGIS Pro or ArcMap 10.x.
- **Standalone Workspace Inventory Report (`geokit report`)**: native Rust (no ArcGIS needed) report generator exporting clean `.html`, `.md`, or `.json` inventories of all datasets, sizes, health flags, and `.lock` clutter.
- Added `geokit_qa` and `geokit_report` MCP tools and wired `xy2shp`/`convert` into `geokit_gp` (now **15 MCP tools** total).

## [1.4.0] - 2026-11-26

### Added
- **Future-Proof ArcGIS Pro / ArcMap Tool Discovery & Execution**: `geokit gp tools [pattern]`, `geokit gp tool-usage <tool>`, and `geokit gp run <tool> [args...]` with automatic extension checkout (`Spatial`, `3D`, `Network`, `ImageAnalyst`, `GeoStats`, `Tracking`, `Business`) — automatically supports every new tool added in future ArcGIS Pro 3.x/4.x updates without updating `geokit`.
- **ModelBuilder Python Toolbox Generator (`geokit toolbox create`)**: generates ModelBuilder-ready `.pyt` toolboxes with GUI parameters (`das`, `batch-prep`, and `blank` templates).
- **Declarative Pipeline Runner (`geokit pipeline init` & `geokit pipeline run`)**: text-based ModelBuilder workflows defined in `pipeline.toml` with `--dry-run` support.
- **Advanced `.aprx` Automation**: `geokit layout map-series` (batch export ArcGIS Pro Spatial Map Series) and `geokit map add-layer` (add dataset + optional `.lyrx` symbology to `.aprx`).
- Added `geokit_toolbox` and `geokit_pipeline` MCP tools (now 13 MCP tools total).

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
