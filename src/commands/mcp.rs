use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::process::Command;

/// Runs a standards-compliant Model Context Protocol (MCP) JSON-RPC 2.0 server over stdio.
/// Every tool dispatches to the `geokit` executable itself so stdout is never polluted by
/// ArcPy or GDAL subprocess output.
pub fn run() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line_res in stdin.lock().lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("Parse error: {e}") }
                });
                writeln!(out, "{err_resp}")?;
                out.flush()?;
                continue;
            }
        };

        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);

        // Notifications (no id) do not receive a JSON-RPC response
        if id.is_none() {
            continue;
        }
        let id = id.unwrap();

        let response = match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {}, "prompts": {} },
                    "serverInfo": {
                        "name": "geokit",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }),
            "ping" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            }),
            "prompts/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "prompts": prompt_definitions() }
            }),
            "prompts/get" => {
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match get_prompt(name, &args) {
                    Some(res) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": res
                    }),
                    None => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32602, "message": format!("Unknown prompt: {name}") }
                    }),
                }
            }
            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": tool_definitions() }
            }),
            "tools/call" => {
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let (text, is_error) = execute_tool(name, &args);
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{ "type": "text", "text": text }],
                        "isError": is_error
                    }
                })
            }
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {method}") }
            }),
        };

        writeln!(out, "{response}")?;
        out.flush()?;
    }

    Ok(())
}

fn prompt_definitions() -> Value {
    json!([
        {
            "name": "peta_kerawanan",
            "description": "SOP pembuatan Peta Kerawanan / Kesesuaian Lahan (banjir, longsor, kekeringan, kesesuaian) dari folder bahan mentah menggunakan skoring & overlay berbobot.",
            "arguments": [
                { "name": "folder", "description": "Path folder bahan data vector/raster", "required": true },
                { "name": "tema", "description": "Tema peta, mis. Kerawanan Longsor / Kerawanan Banjir", "required": false }
            ]
        },
        {
            "name": "peta_persebaran",
            "description": "SOP pembuatan Peta Persebaran (titik lokasi dari CSV/shapefile atau choropleth poligon tematik) dari folder bahan mentah.",
            "arguments": [
                { "name": "folder", "description": "Path folder bahan data vector/CSV", "required": true },
                { "name": "tema", "description": "Tema peta persebaran, mis. Fasilitas Kesehatan / Stasiun Hujan", "required": false }
            ]
        },
        {
            "name": "analisis_das",
            "description": "SOP Analisis Hidrologi & Delineasi DAS (Watershed, jaringan sungai, slope/hillshade, dan zonal statistics) dari DEM.",
            "arguments": [
                { "name": "folder", "description": "Path folder yang berisi DEM dan batas wilayah", "required": true }
            ]
        }
    ])
}

fn get_prompt(name: &str, args: &Value) -> Option<Value> {
    let folder = str_arg(args, "folder").unwrap_or("<folder_bahan>");
    let tema = str_arg(args, "tema").unwrap_or("Kerawanan Bencana");
    let (desc, text) = match name {
        "peta_kerawanan" => (
            "SOP Peta Kerawanan / Kesesuaian Lahan",
            format!(
                "Kamu adalah analis GIS senior menggunakan `geokit` MCP.\n\
                 Target: buat peta **{tema}** dari folder `{folder}`.\n\n\
                 Ikuti SOP wajib ini secara berurutan:\n\
                 1. **Audit & Inventaris**: jalankan `geokit_audit` pada `{folder}` dan `geokit_gp` (`action=\"info\"`) untuk melihat semua layer, CRS/EPSG, dan nama field. JANGAN pernah menebak nama file atau nama field.\n\
                 2. **Intip Nilai Atribut & Statistik**: jalankan `geokit_gp` (`action=\"values\"`) pada tiap layer parameter (mis. lereng, tanah, curah hujan, tutupan lahan) untuk membaca unique values & min/max sebelum menyusun aturan skoring.\n\
                 3. **Standarisasi & Pre-processing**:\n\
                    - Samakan semua layer ke CRS proyeksi metrik (mis. UTM zona setempat) dengan `geokit_gp` (`action=\"project\"`).\n\
                    - Jika ada DEM, buat slope via `geokit_raster` (`action=\"terrain\"`) lalu konversi/reklasifikasi jika perlu.\n\
                    - Clip semua parameter ke batas studi (`action=\"clip\"`) dan perbaiki geometri (`action=\"repair\"`).\n\
                 4. **Skoring per Parameter**: gunakan `geokit_gp` (`action=\"classify\"`) untuk memberi skor (mis. 1–5) pada tiap layer parameter (`out_field=\"SKOR_...\"`).\n\
                 5. **Overlay Multi-Parameter**: gabungkan semua layer parameter dengan `geokit_gp` (`action=\"union\"`).\n\
                 6. **Hitung Total Skor Berbobot & Kelas Kerawanan**:\n\
                    - Gunakan `geokit_gp` (`action=\"calc-field\"`) untuk menghitung `TOTAL_SKOR` sesuai bobot literatur (jumlah bobot = 1.0).\n\
                    - Gunakan `geokit_gp` (`action=\"classify\"`) pada `TOTAL_SKOR` untuk membuat kolom `KELAS_RAWAN` (mis. Rendah / Sedang / Tinggi).\n\
                    - Hitung luas per kelas dengan `geokit_gp` (`action=\"dissolve\"`, `field=\"KELAS_RAWAN\"`) dan `action=\"calc-geom\"` (`unit=\"ha\"`).\n\
                 7. **Quality Assurance**: jalankan `geokit_qa` pada folder output untuk memastikan CRS seragam, geometri valid, dan tidak ada NULL.\n\
                 8. **Simbologi Proper & Tampilkan**: jika ArcMap sedang terbuka (`geokit_live` `action=\"ping\"`), tambahkan hasil ke layar (`action=\"add\"`), **WAJIB beri warna proper tanpa outline** dengan `geokit_live` (`action=\"style\"`, `field=\"KELAS_RWN\"`, `preset=\"hazard\"`, `transparency=30`), atur urutan layer (`action=\"order\"`), zoom, dan ekspor pratinjau PNG. Untuk proyek ArcGIS Pro `.aprx`, gunakan `geokit_map_links` (`action=\"style\"`) & `geokit_layout` (`action=\"update\"` lalu `export`). Ringkas tabel luas (Ha & %) untuk user."
            ),
        ),
        "peta_persebaran" => (
            "SOP Peta Persebaran",
            format!(
                "Kamu adalah analis GIS senior menggunakan `geokit` MCP.\n\
                 Target: buat peta persebaran **{tema}** dari folder `{folder}`.\n\n\
                 Ikuti SOP wajib ini:\n\
                 1. **Audit & Inventaris**: jalankan `geokit_audit` pada `{folder}` dan `geokit_gp` (`action=\"info\"`) untuk menemukan file CSV, shapefile, atau raster.\n\
                 2. **Konversi CSV ke Titik** (jika bahan berupa CSV koordinat): gunakan `geokit_gp` (`action=\"xy2shp\"`) dengan kolom X/Y yang benar dan `epsg=4326` (atau sesuai data).\n\
                 3. **Cek Nilai Atribut**: jalankan `geokit_gp` (`action=\"values\"`) untuk melihat kategori/distribusi atribut.\n\
                 4. **Samakan CRS & Integrasi Wilayah**: proyeksikan ke CRS seragam (`action=\"project\"`), lalu jika perlu gabungkan informasi kecamatan/desa ke titik menggunakan `action=\"spatial-join\"` (`match=\"within\"`).\n\
                 5. **QA & Ringkasan**: jalankan `geokit_qa` lalu `geokit_report` jika user butuh laporan inventaris.\n\
                 6. **Tampilkan di ArcMap** (jika terbuka): gunakan `geokit_live` (`add`, `labels`, `zoom`, `export`)."
            ),
        ),
        "analisis_das" => (
            "SOP Analisis Hidrologi & Delineasi DAS",
            format!(
                "Kamu adalah analis GIS hidrologi menggunakan `geokit` MCP.\n\
                 Target: analisis DAS dari folder `{folder}`.\n\n\
                 Ikuti SOP wajib ini:\n\
                 1. **Audit & Cek CRS DEM**: jalankan `geokit_audit` dan `geokit_gp` (`action=\"info\"`) + `action=\"values\"` pada DEM.\n\
                 2. **Proyeksikan DEM ke UTM**: jika DEM masih WGS84 derajat (EPSG:4326), wajib reproject ke UTM metrik setempat (`geokit_gp` `action=\"project\"`) agar perhitungan aliran & luas akurat.\n\
                 3. **Delineasi DAS & Sungai**: jalankan `geokit_hydro` (`action=\"delineate\"` jika ada titik outlet, atau `action=\"basin\"` untuk seluruh basin).\n\
                 4. **Turunan Topografi & Statistik**: buat slope/hillshade dengan `geokit_raster` (`action=\"terrain\"`), hitung luas DAS (`geokit_gp` `action=\"calc-geom\"` `unit=\"ha\"`), dan statistik elevasi per sub-DAS (`geokit_raster` `action=\"zonal-stats\"`).\n\
                 5. **QA & Visualisasi**: cek kualitas dengan `geokit_qa`, lalu tampilkan hasilnya ke ArcMap via `geokit_live` bila terbuka."
            ),
        ),
        _ => return None,
    };

    Some(json!({
        "description": desc,
        "messages": [
            {
                "role": "user",
                "content": {
                    "type": "text",
                    "text": text
                }
            }
        ]
    }))
}

fn tool_definitions() -> Value {
    serde_json::from_str(
        r#"[
        {
            "name": "geokit_doctor",
            "description": "Check detected GIS backends (ArcGIS Pro, ArcMap 10.x, GDAL/QGIS) and environment health.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "geokit_audit",
            "description": "Audit a GIS workspace directory for datasets (.shp, .gdb, .tif, .aprx, .mxd), broken shapefiles, naming issues, and .lock clutter.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to audit" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "geokit_organize",
            "description": "Organize scattered GIS files into structured folders while keeping shapefile & raster sidecar bundles intact.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory to organize" },
                    "output": { "type": "string", "description": "Optional target output root directory" },
                    "dry_run": { "type": "boolean", "description": "Preview without moving files" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "geokit_sanitize",
            "description": "Rename GIS datasets (and all shapefile/raster sidecars) to ArcGIS-safe snake_case.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory to sanitize" },
                    "dry_run": { "type": "boolean", "description": "Preview without renaming files" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "geokit_clean",
            "description": "Remove stale .lock and temporary files from a GIS workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory to clean" },
                    "include_xml": { "type": "boolean", "description": "Also delete .shp.xml and .aux.xml metadata files" },
                    "dry_run": { "type": "boolean", "description": "Preview files that would be removed" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "geokit_gp",
            "description": "Run batch geoprocessing, attribute inspection/scoring/classification/union/change detection, OR dynamically discover/run ANY installed ArcGIS Pro / ArcMap tool (values, classify, calc-field, union, change, info, project, clip, merge, pack, unpack, repair, buffer, dissolve, calc-geom, intersect, erase, spatial-join, xy2shp, convert, tools, tool-usage, run).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["info", "values", "classify", "calc-field", "union", "change", "select", "export-table", "flatten-2d", "define-crs", "split", "clean-fields", "explode", "centroid", "join", "multi-buffer", "thiessen", "near", "eliminate", "delete-duplicates", "project", "clip", "merge", "pack", "unpack", "repair", "buffer", "dissolve", "calc-geom", "intersect", "erase", "spatial-join", "xy2shp", "convert", "tools", "tool-usage", "run"]
                    },
                    "input": { "type": "string", "description": "Input file, folder, .gdb, or keyword pattern (for tools)" },
                    "inputs": { "type": "array", "items": { "type": "string" }, "description": "List of input polygon paths (for union)" },
                    "t1": { "type": "string", "description": "Earlier period (T1) vector layer path (for change)" },
                    "t2": { "type": "string", "description": "Later period (T2) vector layer path (for change)" },
                    "field_t1": { "type": "string", "description": "Class field name in T1 layer (for change)" },
                    "field_t2": { "type": "string", "description": "Optional class field name in T2 layer (for change; defaults to field_t1)" },
                    "out": { "type": "string", "description": "Output file, folder, or .gdb (when required)" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap", "gdal"], "description": "Backend engine" },
                    "epsg": { "type": "integer", "description": "Target EPSG code (for project or xy2shp)" },
                    "aoi": { "type": "string", "description": "AOI polygon path (for clip)" },
                    "dist": { "type": "string", "description": "Buffer distance e.g. '50 Meters' (for buffer)" },
                    "dissolve": { "type": "boolean", "description": "Dissolve overlapping buffers (for buffer)" },
                    "field": { "type": "string", "description": "Field name(s) (for values, classify source, calc-field target, dissolve, or calc-geom)" },
                    "out_field": { "type": "string", "description": "Target output field name (for classify)" },
                    "rules": { "type": "string", "description": "Semicolon-separated classification/scoring rules e.g. 'Hutan=1;Sawah=3' or '<=8:1;8-15:2;>15:3' (for classify)" },
                    "expr": { "type": "string", "description": "Safe Python math expression e.g. '0.4 * SKOR_A + 0.6 * SKOR_B' (for calc-field)" },
                    "field_type": { "type": "string", "enum": ["LONG", "DOUBLE", "TEXT"], "description": "Output field type (for classify or calc-field)" },
                    "default_value": { "type": "string", "description": "Fallback value for unmatched rows (for classify)" },
                    "limit": { "type": "integer", "description": "Max unique values per field (for values, default: 50)" },
                    "unit": { "type": "string", "description": "Unit: ha, m2, km2, km, m (for calc-geom)" },
                    "with": { "type": "string", "description": "Overlay layer path (for intersect)" },
                    "mask": { "type": "string", "description": "Mask/erase layer path (for erase)" },
                    "join": { "type": "string", "description": "Join layer path (for spatial-join)" },
                    "match": { "type": "string", "enum": ["intersect", "within", "contains", "closest"], "description": "Match option (for spatial-join)" },
                    "x_field": { "type": "string", "description": "X / Longitude column name (for xy2shp, default: lon)" },
                    "y_field": { "type": "string", "description": "Y / Latitude column name (for xy2shp, default: lat)" },
                    "to": { "type": "string", "enum": ["shp", "gpkg", "geojson", "kml", "fgb"], "description": "Target vector format (for convert)" },
                    "tool": { "type": "string", "description": "Exact ArcPy tool name e.g. Buffer_analysis or Slope_sa (for tool-usage or run)" },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Positional arguments passed to ArcPy tool (for run)" },
                    "where": { "type": "string", "description": "SQL WHERE clause (for select)" },
                    "keep": { "type": "string", "description": "Comma-separated field names to keep (for clean-fields)" },
                    "drop": { "type": "string", "description": "Comma-separated field names to drop (for clean-fields)" },
                    "inside": { "type": "boolean", "description": "Force centroid strictly inside polygon (for centroid)" },
                    "table": { "type": "string", "description": "CSV table path to join attributes from (for join)" },
                    "in_field": { "type": "string", "description": "Input vector key field name (for join)" },
                    "join_field": { "type": "string", "description": "CSV key column name (for join)" },
                    "dists": { "type": "string", "description": "Comma-separated multi-ring buffer distances e.g. '500,1000,2000' (for multi-buffer)" },
                    "dissolve_option": { "type": "string", "enum": ["ALL", "NONE"], "description": "Dissolve option for multi-buffer" },
                    "extent": { "type": "string", "description": "Optional boundary polygon path to bound Thiessen polygons (for thiessen)" },
                    "near": { "type": "string", "description": "Target near layer path (for near)" },
                    "search_radius": { "type": "string", "description": "Optional search radius e.g. '5000 Meters' (for near)" },
                    "min_area": { "type": "number", "description": "Minimum sliver polygon area threshold in m2 (for eliminate)" },
                    "tolerance": { "type": "string", "description": "Optional XY tolerance e.g. '0.01 Meters' (for delete-duplicates)" },
                    "json": { "type": "boolean", "description": "Return JSON output when supported (e.g. change, values, info)" },
                    "overwrite": { "type": "boolean", "description": "Allow replacing existing output files (Safe Mode is default)" }
                },
                "required": ["action"]
            }
        },
        {
            "name": "geokit_layout",
            "description": "List, update text/extent, or batch export layouts from ArcGIS Pro (.aprx) or ArcMap (.mxd) projects to PDF, PNG, or JPG.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "export", "update"] },
                    "input": { "type": "string", "description": "Path to .aprx, .mxd, or folder" },
                    "out": { "type": "string", "description": "Output directory (for export) or copy path (for update)" },
                    "format": { "type": "string", "enum": ["pdf", "png", "jpg"] },
                    "dpi": { "type": "integer", "description": "Export DPI (default: 300)" },
                    "filter": { "type": "string", "description": "Filter layout names (for .aprx)" },
                    "title": { "type": "string", "description": "Map title text (for update)" },
                    "subtitle": { "type": "string", "description": "Map subtitle text (for update)" },
                    "source": { "type": "string", "description": "Data source credit text (for update)" },
                    "zoom_layer": { "type": "string", "description": "Layer name to zoom map frame to (for update)" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap"] }
                },
                "required": ["action", "input"]
            }
        },
        {
            "name": "geokit_map_links",
            "description": "Detect/fix broken links in .aprx/.mxd OR add layers and apply automatic publication-grade symbology (hazard/hazard5/blue/earth/category, no-outline, transparency) in ArcGIS Pro (.aprx).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["broken-links", "fix-links", "add-layer", "style"] },
                    "input": { "type": "string", "description": "Path to .aprx, .mxd, or folder" },
                    "find": { "type": "string", "description": "Old path prefix to find (for fix-links)" },
                    "replace": { "type": "string", "description": "New path prefix (for fix-links)" },
                    "layer": { "type": "string", "description": "Layer path (for add-layer) or layer name (for style)" },
                    "field": { "type": "string", "description": "Attribute field to symbolize (for style)" },
                    "preset": { "type": "string", "enum": ["hazard", "hazard5", "blue", "earth", "category"], "description": "Color ramp preset (for style)" },
                    "transparency": { "type": "integer", "description": "Transparency 0-100 (for style, default: 25)" },
                    "classes": { "type": "string", "description": "Optional comma-separated ordered class names (for style)" },
                    "out": { "type": "string", "description": "Optional output copy path" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap"] }
                },
                "required": ["action", "input"]
            }
        },
        {
            "name": "geokit_raster",
            "description": "Run raster mosaic, terrain (slope/aspect/hillshade), zonal statistics, reclass (with optional smoothing), weighted-sum multi-raster overlay, point-to-raster interpolation (IDW/Spline/Kriging), satellite spectral index (NDVI/NDWI/NDBI/SAVI/NBR/MNDWI/EVI/BSI/NDRE/VARI/ExG/LST/NDTI/NSMI/NDCI, from band files or a multi-band sensor stack), multi-sensor RGB composite (composite: true-color/cir/urban/agriculture/land-water/geology/sar for Landsat 5/7/8/9, Sentinel-1/2, PlanetScope, SPOT), topographic contour lines (with BIG INDEX_KTR), raster grid alignment (align), raster value extraction to points (extract-points), DEM morphometry (TWI/TPI/Curvature), or scanned-map georeferencing with GCP RMSE report (georef).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["mosaic", "terrain", "zonal-stats", "reclass", "weighted-sum", "interpolate", "index", "composite", "contour", "align", "extract-points", "morphometry", "georef"] },
                    "input": { "type": "string", "description": "Input raster, point layer (for interpolate), DEM (for contour/morphometry), scanned image (for georef), or folder" },
                    "inputs": { "type": "string", "description": "Comma-separated '<raster>:<weight>' pairs (for weighted-sum) OR comma-separated raster paths (for align)" },
                    "out": { "type": "string", "description": "Output file or directory" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap", "gdal"] },
                    "zones": { "type": "string", "description": "Zone polygon path (for zonal-stats)" },
                    "zone_field": { "type": "string", "description": "Zone field name (for zonal-stats)" },
                    "rules": { "type": "string", "description": "Reclassify remap string e.g. '<=8:1;8-15:2;>15:3' (for reclass)" },
                    "smooth": { "type": "boolean", "description": "Apply MajorityFilter smoothing (for reclass or weighted-sum)" },
                    "classes": { "type": "integer", "description": "Optional number of classes (for weighted-sum or index)" },
                    "polygon_out": { "type": "string", "description": "Optional classified polygon output path (for weighted-sum or index)" },
                    "field": { "type": "string", "description": "Value field to interpolate (for interpolate) or output field name (for extract-points)" },
                    "method": { "type": "string", "enum": ["idw", "spline", "kriging"], "description": "Interpolation method (for interpolate)" },
                    "cell_size": { "type": "number", "description": "Output cell size in map units (for interpolate)" },
                    "mask": { "type": "string", "description": "Optional polygon mask/AOI (for interpolate)" },
                    "index_type": { "type": "string", "enum": ["ndvi", "ndwi", "ndbi", "savi", "nbr", "mndwi", "evi", "bsi", "ndre", "vari", "exg", "lst", "ndti", "nsmi", "tss", "ndci", "chla"], "description": "Spectral index (for index). LST is Landsat C2 L2 ST_B10 -> deg C; tss/nsmi and chla/ndci are RELATIVE water-quality indices, not calibrated concentrations" },
                    "band_a": { "type": "string", "description": "Band A raster path e.g. NIR/Green/SWIR1 (for two-band index)" },
                    "band_b": { "type": "string", "description": "Band B raster path e.g. Red/NIR/SWIR2 (for two-band index)" },
                    "multiband": { "type": "string", "description": "Single multi-band sensor stack (for index with evi/bsi/vari/exg/lst/nsmi or any index without separate band files)" },
                    "sensor": { "type": "string", "enum": ["landsat5", "landsat7", "landsat8", "landsat9", "sentinel2", "planetscope", "spot", "sentinel1"], "description": "Sensor of the multi-band stack (for index with multiband, and composite)" },
                    "bands": { "type": "string", "description": "Optional per-role band number overrides e.g. 'red=3,nir=4' (1-based; for index with multiband, and composite)" },
                    "preset": { "type": "string", "enum": ["true-color", "cir", "urban", "agriculture", "land-water", "geology", "sar"], "description": "RGB composite preset (for composite)" },
                    "aoi": { "type": "string", "description": "Optional polygon AOI to clip the composite (for composite)" },
                    "interval": { "type": "number", "description": "Contour elevation interval e.g. 12.5 or 25 (for contour)" },
                    "base": { "type": "number", "description": "Base contour elevation (for contour)" },
                    "index_every": { "type": "integer", "description": "Index contour interval count (default 4 per BIG RBI, for contour)" },
                    "reference": { "type": "string", "description": "Reference raster path defining CRS, pixel size, snap grid, and extent (for align)" },
                    "resampling": { "type": "string", "enum": ["BILINEAR", "NEAREST", "CUBIC"], "description": "Resampling method (for align)" },
                    "points": { "type": "string", "description": "Input point shapefile or .gdb feature class (for extract-points)" },
                    "raster": { "type": "string", "description": "Input raster file to sample (for extract-points)" },
                    "tpi_radius": { "type": "integer", "description": "Circular focal radius in cells for TPI (default: 3, for morphometry)" },
                    "gcp": { "type": "string", "description": "GCP file path (.points QGIS or CSV, for georef)" },
                    "epsg": { "type": "integer", "description": "Target EPSG code (for georef)" },
                    "order": { "type": "integer", "description": "Polynomial warp order 1-3 (for georef)" },
                    "report": { "type": "string", "description": "Optional RMSE report output path (.json or .md, for georef)" },
                    "overwrite": { "type": "boolean", "description": "Allow replacing existing output files" }
                },
                "required": ["action", "out"]
            }
        },
        {
            "name": "geokit_hydro",
            "description": "Run automated DEM watershed (DAS) delineation or basin extraction via ArcGIS Pro or ArcMap 10.x Spatial Analyst.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["delineate", "basin"] },
                    "dem": { "type": "string", "description": "Input DEM raster path" },
                    "out": { "type": "string", "description": "Output directory" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap"] },
                    "threshold": { "type": "integer", "description": "Stream accumulation threshold (for delineate)" },
                    "pour_point": { "type": "string", "description": "Optional outlet point shapefile (for delineate)" }
                },
                "required": ["action", "dem", "out"]
            }
        },
        {
            "name": "geokit_live",
            "description": "Control an open ArcMap 10.1-10.8 OR ArcGIS Pro 2.x/3.x window in real time via the GeoKit Live-UI TCP bridge. Supports full ArcToolbox discovery (tools/tool-usage) and execution (run-gp with auto extension checkout), plus live cartography (add, remove, visible, zoom, select, clear-selection, layer-info, definition-query, symbology, style, rename, order, layout, labels, export, save, install-addin, install-pro-bridge).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "ping",
                            "layers",
                            "layer-info",
                            "add",
                            "remove",
                            "visible",
                            "definition-query",
                            "symbology",
                            "style",
                            "rename",
                            "order",
                            "layout",
                            "labels",
                            "zoom",
                            "select",
                            "clear-selection",
                            "export",
                            "save",
                            "tools",
                            "tool-usage",
                            "run-gp",
                            "install-addin",
                            "install-pro-bridge"
                        ]
                    },
                    "path": { "type": "string", "description": "Dataset path or folder (for add)" },
                    "layer": { "type": "string", "description": "Layer name in ArcMap TOC" },
                    "show": { "type": "boolean", "description": "True/false toggle (for visible or labels)" },
                    "field": { "type": "string", "description": "Field name e.g. KELAS_RWN or NAMOBJ (for style or labels; omit for single-symbol or raster)" },
                    "mode": { "type": "string", "enum": ["auto", "unique", "graduated", "single", "raster"], "description": "ArcObjects renderer mode (for style)" },
                    "preset": { "type": "string", "enum": ["hazard", "hazard5", "hazard-pastel", "viridis", "blue", "earth", "terrain", "gray", "category", "klhk", "stream", "road", "boundary", "point"], "description": "Modern cartographic palette or single-symbol preset (for style)" },
                    "colors": { "type": "string", "description": "Custom comma-separated hex color stops e.g. '#2E7D32,#FFF176,#D32F2F' (for style)" },
                    "color": { "type": "string", "description": "Single hex color e.g. '#0288D1' (for single/stream/road/boundary/point)" },
                    "classes_count": { "type": "integer", "description": "Number of numeric classes for graduated mode (default: 5)" },
                    "break_method": { "type": "string", "enum": ["quantile", "equal", "jenks"], "description": "Break method for graduated mode (quantile, equal, or jenks Natural Breaks)" },
                    "outline_color": { "type": "string", "description": "Outline hex color e.g. '#37474F'" },
                    "outline_width": { "type": "number", "description": "Outline width in pt (0.0 = borderless)" },
                    "outline_style": { "type": "string", "enum": ["solid", "dash", "dot", "dashdot", "admin-kab", "admin-kec", "admin-desa"], "description": "True CartographicLineSymbol dash or BIG admin boundary pattern" },
                    "name": { "type": "string", "description": "New clean TOC/legend display name (for rename)" },
                    "padding": { "type": "number", "description": "Zoom extent padding ratio e.g. 0.12 (for zoom)" },
                    "scale": { "type": "string", "description": "Target scale or 'auto' to snap to standard scale (for zoom)" },
                    "halo": { "type": "boolean", "description": "White halo around labels (for labels, default: true)" },
                    "font_size": { "type": "number", "description": "Label font size in pt (for labels)" },
                    "template": { "type": "string", "enum": ["perkabig"], "description": "Built-in PerKaBIG No. 3/2016 7-section layout generator (for layout)" },
                    "paper": { "type": "string", "enum": ["a4", "a3", "a1", "a0"], "description": "Paper size for perkabig layout: a4 (default), a3, a1, or a0" },
                    "region": { "type": "string", "description": "Region / study area name (bold header line 3, e.g. 'KABUPATEN KENDAL')" },
                    "sheet_number": { "type": "string", "description": "Optional BIG sheet number in blue (e.g. 'Lembar: 33.24.01.2001')" },
                    "author": { "type": "string", "description": "Map author / publisher / institution name (PerKaBIG Section 5)" },
                    "edition": { "type": "string", "description": "Edition & year text (default: 'Edisi 1 - Tahun 2026')" },
                    "note": { "type": "string", "description": "Footer disclaimer/note text" },
                    "logo": { "type": "string", "description": "Optional PNG logo path for institution block" },
                    "inset_layer": { "type": "string", "description": "Optional context polygon shapefile (e.g. Province boundaries) to build an automatic DIAGRAM LOKASI inset map with red extent box" },
                    "inset_petunjuk": { "type": "string", "description": "Optional local context polygon shapefile (e.g. Regency/District boundaries) to build dual official PerKaBIG insets (PETUNJUK LETAK PETA + DIAGRAM LOKASI)" },
                    "grid": { "type": "boolean", "description": "Attach grid/graticule to MapFrame (for perkabig layout)" },
                    "grid_mode": { "type": "string", "enum": ["utm", "dms", "dual"], "description": "Grid mode: utm (black UTM), dms (blue WGS84), or dual (RBI Cetak UTM + blue DMS ticks)" },
                    "north_style": { "type": "string", "enum": ["big", "classic"], "description": "North arrow style: big (official Indonesian PerKaBIG 'U' vector arrow, default) or classic (compass rose)" },
                    "width": { "type": "number", "description": "Line width in pt for polylines" },
                    "size": { "type": "number", "description": "Marker size in pt for points" },
                    "marker": { "type": "string", "enum": ["circle", "square", "diamond", "cross", "x"], "description": "Marker shape for points" },
                    "hollow": { "type": "boolean", "description": "Hollow polygon fill (outline only)" },
                    "transparency": { "type": "integer", "description": "Transparency 0-100 (for style)" },
                    "classes": { "type": "string", "description": "Optional comma-separated ordered class names (for style)" },
                    "position": { "type": "string", "enum": ["top", "bottom"], "description": "Layer stack position (for order)" },
                    "view": { "type": "string", "enum": ["layout", "data"], "description": "View mode (for layout)" },
                    "title": { "type": "string", "description": "Layout title text (for layout)" },
                    "subtitle": { "type": "string", "description": "Layout subtitle text (for layout)" },
                    "source": { "type": "string", "description": "Layout source credit text (for layout)" },
                    "zoom_layer": { "type": "string", "description": "Layer name to zoom to (for layout)" },
                    "hide_legend": { "type": "string", "description": "Comma-separated layer names to remove from Layout Legend e.g. 'hillshade,Basemap'" },
                    "lyr": { "type": "string", "description": "Path to .lyr symbology file (for symbology)" },
                    "where": { "type": "string", "description": "SQL WHERE clause (for select or definition-query)" },
                    "out": { "type": "string", "description": "Output file path (for export or save)" },
                    "dpi": { "type": "integer", "description": "Export DPI (default: 300 print-ready, for export)" },
                    "pattern": { "type": "string", "description": "Keyword/wildcard filter e.g. 'clip' or '*_sa' (for tools)" },
                    "tool": { "type": "string", "description": "Exact ArcPy tool name e.g. Buffer_analysis or Slope_sa (for tool-usage or run-gp)" },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Positional tool arguments in order (for run-gp)" }
                },
                "required": ["action"]
            }
        },
        {
            "name": "geokit_toolbox",
            "description": "Generate a ModelBuilder-ready ArcGIS Pro / ArcMap Python Toolbox (.pyt) with GUI parameters (templates: das, batch-prep, blank).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Toolbox name e.g. GeoKit_DAS_Tools" },
                    "out": { "type": "string", "description": "Output .pyt file path" },
                    "template": { "type": "string", "enum": ["das", "batch-prep", "blank"] },
                    "force": { "type": "boolean" }
                },
                "required": ["name", "out"]
            }
        },
        {
            "name": "geokit_pipeline",
            "description": "Create (`init`) or execute (`run`) a declarative multi-step GIS pipeline.toml workflow (text-based ModelBuilder).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["init", "run"] },
                    "file": { "type": "string", "description": "Path to pipeline.toml" },
                    "dry_run": { "type": "boolean", "description": "Preview steps without executing (for run)" },
                    "resume": { "type": "boolean", "description": "Resume from last checkpoint state, skipping already-completed steps (for run)" },
                    "force": { "type": "boolean", "description": "Overwrite existing file (for init)" }
                },
                "required": ["action"]
            }
        },
        {
            "name": "geokit_qa",
            "description": "Run Spatial Quality Assurance (QA) checks on a file, folder, or .gdb: CRS uniformity, invalid geometry (CheckGeometry), required NULL/empty fields, and optional polygon self-overlap detection.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "File, folder, or .gdb path to inspect" },
                    "expected_epsg": { "type": "integer", "description": "Expected EPSG code for CRS uniformity check" },
                    "check_nulls": { "type": "string", "description": "Comma-separated required field names to check for NULL/empty values (e.g. 'DESA,KELAS')" },
                    "check_overlaps": { "type": "boolean", "description": "Check for self-overlapping polygons within each polygon layer" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap"], "description": "Backend engine" }
                },
                "required": ["input"]
            }
        },
        {
            "name": "geokit_topo",
            "description": "Run BIG topology checks (`check`: overlaps, interior gaps, slivers, dangles, line crossings, duplicates, invalid geometry) or automated 1-command topology cleanup (`fix`: RepairGeometry + remove duplicates + eliminate slivers).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["check", "fix"] },
                    "input": { "type": "string", "description": "Input polygon/polyline shapefile or .gdb feature class" },
                    "out": { "type": "string", "description": "Output cleaned dataset path (required for fix)" },
                    "out_dir": { "type": "string", "description": "Optional directory to export error shapefiles topo_overlaps.shp / topo_gaps.shp / topo_dangles.shp (for check)" },
                    "min_sliver": { "type": "number", "description": "Minimum sliver polygon area threshold in m2 (default: 100.0)" },
                    "strict": { "type": "boolean", "description": "Exit non-zero if any topology error is found (for check)" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap"] },
                    "overwrite": { "type": "boolean", "description": "Allow replacing existing output files" }
                },
                "required": ["action", "input"]
            }
        },
        {
            "name": "geokit_report",
            "description": "Generate a clean standalone Markdown (.md), HTML (.html), or JSON (.json) GIS workspace inventory report (works without ArcGIS).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to report on" },
                    "out": { "type": "string", "description": "Output report file path (.md, .html, or .json)" }
                },
                "required": ["path", "out"]
            }
        },
        {
            "name": "geokit_fetch",
            "description": "Download free public spatial data (OpenStreetMap roads/rivers/buildings/boundary/poi or 30m SRTM DEM tiles with optional auto-mosaic, UTM reproject & AOI clip) for any AOI shapefile, GeoJSON, or WGS84 bbox.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "enum": ["osm", "dem"], "description": "Data source: osm or dem" },
                    "aoi": { "type": "string", "description": "Optional AOI .shp or .geojson file path" },
                    "bbox": { "type": "string", "description": "Optional WGS84 bbox 'min_lon,min_lat,max_lon,max_lat'" },
                    "layers": { "type": "string", "description": "Comma-separated OSM layers: road,river,building,boundary,poi (for osm)" },
                    "clip_aoi": { "type": "boolean", "description": "Auto-mosaic, reproject to AOI UTM zone, and clip SRTM DEM into dem_aoi_clipped.tif (for dem)" },
                    "epsg": { "type": "integer", "description": "Optional target EPSG code override when clip_aoi is true (e.g. 32749)" },
                    "out": { "type": "string", "description": "Output directory path" }
                },
                "required": ["source", "out"]
            }
        },
        {
            "name": "geokit_webmap",
            "description": "Export one or more .shp/.geojson layers into a single self-contained multi-layer interactive Leaflet HTML webmap with layer switcher checkboxes, basemap switcher, class colors, opacity slider, popups, and area bar chart (zero ArcGIS needed).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "Single .shp or .geojson path (or use inputs array for multi-layer map)" },
                    "inputs": { "type": "array", "items": { "type": "string" }, "description": "Multiple .shp/.geojson layer paths to overlay with interactive checkbox layer switcher" },
                    "out": { "type": "string", "description": "Output standalone .html path" },
                    "field": { "type": "string", "description": "Optional attribute field to colorize & summarize (e.g. KELAS)" },
                    "preset": { "type": "string", "enum": ["hazard-pastel", "hazard", "viridis", "blue", "category"] },
                    "title": { "type": "string", "description": "Header card title" },
                    "basemap": { "type": "string", "enum": ["satellite", "osm", "light", "topo", "dark"] },
                    "simplify": { "type": "number", "description": "Simplification tolerance in degrees (default: 0.0001)" }
                },
                "required": ["out"]
            }
        },
        {
            "name": "geokit_summary",
            "description": "Compute ready-to-publish class count, total area (ha), and percentage (%) summary tables from any .shp or .geojson into Markdown, CSV, HTML, or JSON (100% native Rust, zero ArcGIS needed).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "Input .shp or .geojson path" },
                    "group_by": { "type": "string", "description": "Attribute field to group by (e.g. KELAS)" },
                    "area_field": { "type": "string", "description": "Optional numeric area field (auto-detects AREA_HA/LUAS_HA)" },
                    "out": { "type": "string", "description": "Optional export path (.md, .csv, .html, or .json)" }
                },
                "required": ["input", "group_by"]
            }
        },
        {
            "name": "geokit_daemon",
            "description": "Start, inspect, or stop the persistent warm ArcPy worker daemon (imports arcpy ONCE, making all subsequent geokit_gp / geokit_raster / geokit_hydro / geokit_qa calls 5x-15x faster!).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["start", "status", "stop"] },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap"], "description": "Backend to warm up (for start)" },
                    "idle_minutes": { "type": "integer", "description": "Auto-shutdown idle minutes (default: 30, 0 = never)" },
                    "target": { "type": "string", "enum": ["all", "arcmap", "arcgis"], "description": "Which daemon to stop (for stop)" }
                },
                "required": ["action"]
            }
        },
        {
            "name": "geokit_accuracy",
            "description": "Scientific map accuracy assessment (100% Native Rust, no ArcGIS required): generate Stratified Random validation samples (.shp/.kml/.geojson/.csv) OR compute Confusion Matrix + Kappa + F1, continuous RMSE/MAE/Bias/R2/BIG LE90/CE90, and ROC-AUC + Youden's J.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["sample", "matrix", "rmse", "roc"] },
                    "input": { "type": "string", "description": "Input polygon shapefile/GeoJSON (sample) or validation table (.csv, .shp, .dbf, .geojson)" },
                    "out": { "type": "string", "description": "Output directory/prefix (for sample) or optional report path (.md, .csv, .html, .json)" },
                    "field": { "type": "string", "description": "Class attribute field name (for sample)" },
                    "total": { "type": "integer", "description": "Total target sample points (for sample, default: 100)" },
                    "min_per_class": { "type": "integer", "description": "Minimum samples per stratum/class (for sample, default: 10)" },
                    "seed": { "type": "integer", "description": "Deterministic random seed (for sample, default: 42)" },
                    "pred": { "type": "string", "description": "Predicted class/numeric field (for matrix or rmse)" },
                    "ref_field": { "type": "string", "description": "Reference ground-truth class/numeric field (for matrix or rmse)" },
                    "pred_x": { "type": "string", "description": "Predicted X field for 2D horizontal CE90 (for rmse)" },
                    "pred_y": { "type": "string", "description": "Predicted Y field for 2D horizontal CE90 (for rmse)" },
                    "ref_x": { "type": "string", "description": "Reference X field for 2D horizontal CE90 (for rmse)" },
                    "ref_y": { "type": "string", "description": "Reference Y field for 2D horizontal CE90 (for rmse)" },
                    "score": { "type": "string", "description": "Numeric hazard/probability score field (for roc)" },
                    "label": { "type": "string", "description": "Binary ground-truth label field (for roc)" },
                    "positive": { "type": "string", "description": "Positive class label value (for roc, default: '1')" }
                },
                "required": ["action", "input"]
            }
        },
        {
            "name": "geokit_survey",
            "description": "Drone & photogrammetry survey planning (100% Native Rust): split AOI into uniform drone flight blocks (.shp/.kml/.geojson/.csv + per-block DJI Pilot 2 KML) OR plan evenly distributed GCP/ICP ground control points.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["blocks", "gcp"] },
                    "aoi": { "type": "string", "description": "Input AOI polygon shapefile (.shp) or GeoJSON (.geojson)" },
                    "out": { "type": "string", "description": "Output directory or base path prefix" },
                    "block_size_m": { "type": "number", "description": "Square flight block size in meters (for blocks, default: 1000)" },
                    "block_ha": { "type": "number", "description": "Target block area in hectares (for blocks, alternative to block_size_m)" },
                    "cols": { "type": "integer", "description": "Explicit grid columns (for blocks, used with rows)" },
                    "rows": { "type": "integer", "description": "Explicit grid rows (for blocks, used with cols)" },
                    "overlap_m": { "type": "number", "description": "Overlap between adjacent flight blocks in meters (for blocks, default: 0)" },
                    "split_kml": { "type": "boolean", "description": "Write individual BLOK_01.kml files per block (for blocks)" },
                    "gcp_count": { "type": "integer", "description": "Number of GCP points to place (for gcp, default: 8)" },
                    "icp_count": { "type": "integer", "description": "Number of ICP check points to place (for gcp, default: 4)" },
                    "margin_m": { "type": "number", "description": "Inward margin from AOI boundary in meters (for gcp, default: 25)" }
                },
                "required": ["action", "aoi", "out"]
            }
        }
    ]"#,
    )
    .expect("valid embedded MCP tool definitions JSON")
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn bool_arg(args: &Value, key: &str) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn mcp_readonly_enabled() -> bool {
    std::env::var("GEOKIT_MCP_READONLY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn is_readonly_mcp_call(name: &str, args: &Value) -> bool {
    if bool_arg(args, "dry_run") {
        return true;
    }
    match name {
        "geokit_doctor" | "geokit_audit" | "geokit_qa" => true,
        "geokit_topo" => {
            matches!(str_arg(args, "action"), Some("check")) && str_arg(args, "out_dir").is_none()
        }
        "geokit_summary" => str_arg(args, "out").is_none(),
        "geokit_gp" => matches!(
            str_arg(args, "action"),
            Some("info" | "values" | "tools" | "tool-usage")
        ),
        "geokit_layout" => matches!(str_arg(args, "action"), Some("list")),
        "geokit_map_links" => matches!(str_arg(args, "action"), Some("broken-links")),
        "geokit_live" => matches!(
            str_arg(args, "action"),
            Some("ping" | "layers" | "layer-info" | "tools" | "tool-usage")
        ),
        "geokit_daemon" => matches!(str_arg(args, "action"), Some("status")),
        "geokit_accuracy" => {
            !matches!(str_arg(args, "action"), Some("sample")) && str_arg(args, "out").is_none()
        }
        _ => false,
    }
}

fn validate_mcp_workspace_paths(args: &Value) -> Result<(), String> {
    if std::env::var("GEOKIT_WORKSPACE_ROOT")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        return Ok(());
    }
    let path_keys = [
        "path",
        "input",
        "t1",
        "t2",
        "out",
        "output",
        "aoi",
        "file",
        "dem",
        "mask",
        "with",
        "join",
        "zones",
        "band_a",
        "band_b",
        "out_poly",
        "lyr_file",
        "logo",
        "table",
        "extent",
        "near",
        "out_dir",
        "reference",
        "points",
        "raster",
        "gcp",
        "report",
        "multiband",
    ];
    for k in path_keys {
        if let Some(p) = str_arg(args, k) {
            if let Err(e) = crate::gis::validate_within_workspace_root(std::path::Path::new(p)) {
                return Err(e.to_string());
            }
        }
    }
    if let Some(arr) = args.get("inputs").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(p) = item.as_str() {
                if let Err(e) = crate::gis::validate_within_workspace_root(std::path::Path::new(p))
                {
                    return Err(e.to_string());
                }
            }
        }
    }
    Ok(())
}

fn append_mcp_audit_log(tool: &str, read_only: bool, is_error: bool, duration_ms: u128) {
    let Some(home) = crate::engine::geokit_home_dir() else {
        return;
    };
    let log_dir = home.join("logs");
    if std::fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let log_path = log_dir.join("mcp_audit.jsonl");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entry = json!({
        "ts_unix": ts,
        "tool": tool,
        "read_only": read_only,
        "is_error": is_error,
        "duration_ms": duration_ms,
    });
    if let Ok(mut fh) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = writeln!(fh, "{entry}");
    }
}

fn execute_tool(name: &str, args: &Value) -> (String, bool) {
    let started = std::time::Instant::now();
    let read_only = is_readonly_mcp_call(name, args);
    if mcp_readonly_enabled() && !read_only {
        let msg =
            format!("Blocked mutating MCP tool '{name}' because GEOKIT_MCP_READONLY is enabled.");
        append_mcp_audit_log(name, read_only, true, started.elapsed().as_millis());
        return (msg, true);
    }
    if let Err(msg) = validate_mcp_workspace_paths(args) {
        append_mcp_audit_log(name, read_only, true, started.elapsed().as_millis());
        return (msg, true);
    }

    let (text, is_error) = execute_tool_inner(name, args);
    append_mcp_audit_log(name, read_only, is_error, started.elapsed().as_millis());
    (text, is_error)
}

fn execute_tool_inner(name: &str, args: &Value) -> (String, bool) {
    let mut cli_args: Vec<String> = Vec::new();

    if let Some(eng) = str_arg(args, "engine") {
        cli_args.push("--engine".into());
        cli_args.push(eng.into());
    }
    if bool_arg(args, "overwrite") {
        cli_args.push("--overwrite".into());
    }

    match name {
        "geokit_doctor" => {
            cli_args.push("doctor".into());
            cli_args.push("--json".into());
        }
        "geokit_audit" => {
            let Some(path) = str_arg(args, "path") else {
                return ("Missing required argument: path".into(), true);
            };
            cli_args.extend(["audit".into(), path.into(), "--json".into()]);
        }
        "geokit_organize" => {
            let Some(path) = str_arg(args, "path") else {
                return ("Missing required argument: path".into(), true);
            };
            cli_args.extend(["organize".into(), path.into()]);
            if let Some(out) = str_arg(args, "output") {
                cli_args.extend(["--output".into(), out.into()]);
            }
            if bool_arg(args, "dry_run") {
                cli_args.push("--dry-run".into());
            }
        }
        "geokit_sanitize" => {
            let Some(path) = str_arg(args, "path") else {
                return ("Missing required argument: path".into(), true);
            };
            cli_args.extend(["sanitize".into(), path.into()]);
            if bool_arg(args, "dry_run") {
                cli_args.push("--dry-run".into());
            }
        }
        "geokit_clean" => {
            let Some(path) = str_arg(args, "path") else {
                return ("Missing required argument: path".into(), true);
            };
            cli_args.extend(["clean".into(), path.into()]);
            if bool_arg(args, "include_xml") {
                cli_args.push("--include-xml".into());
            }
            if bool_arg(args, "include_gdb_locks") {
                cli_args.push("--include-gdb-locks".into());
            }
            if bool_arg(args, "dry_run") {
                cli_args.push("--dry-run".into());
            }
        }
        "geokit_gp" => {
            let Some(action) = str_arg(args, "action") else {
                return ("Missing required argument: action".into(), true);
            };
            cli_args.extend(["gp".into(), action.into()]);
            if let Some(input) = str_arg(args, "input") {
                cli_args.push(input.into());
            }
            if let Some(t1) = str_arg(args, "t1") {
                cli_args.push(t1.into());
            }
            if let Some(t2) = str_arg(args, "t2") {
                cli_args.push(t2.into());
            }
            if let Some(inputs) = args.get("inputs").and_then(|v| v.as_array()) {
                for item in inputs {
                    if let Some(s) = item.as_str() {
                        cli_args.push(s.into());
                    }
                }
            }
            if let Some(tool) = str_arg(args, "tool") {
                cli_args.push(tool.into());
            }
            if let Some(arr) = args.get("args").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        cli_args.push(s.into());
                    }
                }
            }
            if let Some(out) = str_arg(args, "out") {
                cli_args.extend(["--out".into(), out.into()]);
            }
            if let Some(epsg) = args.get("epsg").and_then(|v| v.as_u64()) {
                cli_args.extend(["--epsg".into(), epsg.to_string()]);
            }
            if let Some(aoi) = str_arg(args, "aoi") {
                cli_args.extend(["--aoi".into(), aoi.into()]);
            }
            if let Some(dist) = str_arg(args, "dist") {
                cli_args.extend(["--dist".into(), dist.into()]);
            }
            if bool_arg(args, "dissolve") {
                cli_args.push("--dissolve".into());
            }
            if let Some(field) = str_arg(args, "field") {
                cli_args.extend(["--field".into(), field.into()]);
            }
            if let Some(unit) = str_arg(args, "unit") {
                cli_args.extend(["--unit".into(), unit.into()]);
            }
            if let Some(with) = str_arg(args, "with") {
                cli_args.extend(["--with".into(), with.into()]);
            }
            if let Some(mask) = str_arg(args, "mask") {
                cli_args.extend(["--mask".into(), mask.into()]);
            }
            if let Some(join) = str_arg(args, "join") {
                cli_args.extend(["--join".into(), join.into()]);
            }
            if let Some(m) = str_arg(args, "match") {
                cli_args.extend(["--match".into(), m.into()]);
            }
            if let Some(xf) = str_arg(args, "x_field") {
                cli_args.extend(["--x-field".into(), xf.into()]);
            }
            if let Some(yf) = str_arg(args, "y_field") {
                cli_args.extend(["--y-field".into(), yf.into()]);
            }
            if let Some(to_fmt) = str_arg(args, "to") {
                cli_args.extend(["--to".into(), to_fmt.into()]);
            }
            if let Some(of) = str_arg(args, "out_field") {
                cli_args.extend(["--out-field".into(), of.into()]);
            }
            if let Some(r) = str_arg(args, "rules") {
                cli_args.extend(["--rules".into(), r.into()]);
            }
            if let Some(e) = str_arg(args, "expr") {
                cli_args.extend(["--expr".into(), e.into()]);
            }
            if let Some(ft) = str_arg(args, "field_type") {
                cli_args.extend(["--field-type".into(), ft.into()]);
            }
            if let Some(dv) = str_arg(args, "default_value") {
                cli_args.extend(["--default".into(), dv.into()]);
            }
            if let Some(w) = str_arg(args, "where") {
                cli_args.extend(["--where".into(), w.into()]);
            }
            if let Some(k) = str_arg(args, "keep") {
                cli_args.extend(["--keep".into(), k.into()]);
            }
            if let Some(d) = str_arg(args, "drop") {
                cli_args.extend(["--drop".into(), d.into()]);
            }
            if bool_arg(args, "inside") {
                cli_args.push("--inside".into());
            }
            if let Some(tbl) = str_arg(args, "table") {
                cli_args.extend(["--table".into(), tbl.into()]);
            }
            if let Some(inf) = str_arg(args, "in_field") {
                cli_args.extend(["--in-field".into(), inf.into()]);
            }
            if let Some(jf) = str_arg(args, "join_field") {
                cli_args.extend(["--join-field".into(), jf.into()]);
            }
            if let Some(ds) = str_arg(args, "dists") {
                cli_args.extend(["--dists".into(), ds.into()]);
            }
            if let Some(dopt) = str_arg(args, "dissolve_option") {
                cli_args.extend(["--dissolve".into(), dopt.into()]);
            }
            if let Some(ext) = str_arg(args, "extent") {
                cli_args.extend(["--extent".into(), ext.into()]);
            }
            if let Some(nr) = str_arg(args, "near") {
                cli_args.extend(["--near".into(), nr.into()]);
            }
            if let Some(sr) = str_arg(args, "search_radius") {
                cli_args.extend(["--search-radius".into(), sr.into()]);
            }
            if let Some(ma) = args.get("min_area").and_then(|v| v.as_f64()) {
                cli_args.extend(["--min-area".into(), ma.to_string()]);
            }
            if let Some(tol) = str_arg(args, "tolerance") {
                cli_args.extend(["--tolerance".into(), tol.into()]);
            }
            if let Some(ft1) = str_arg(args, "field_t1") {
                cli_args.extend(["--field-t1".into(), ft1.into()]);
            }
            if let Some(ft2) = str_arg(args, "field_t2") {
                cli_args.extend(["--field-t2".into(), ft2.into()]);
            }
            if let Some(lim) = args.get("limit").and_then(|v| v.as_u64()) {
                cli_args.extend(["--limit".into(), lim.to_string()]);
            }
            if action == "values" || bool_arg(args, "json") {
                cli_args.push("--json".into());
            }
        }
        "geokit_layout" => {
            let (Some(action), Some(input)) = (str_arg(args, "action"), str_arg(args, "input"))
            else {
                return ("Missing required arguments: action, input".into(), true);
            };
            cli_args.extend(["layout".into(), action.into(), input.into()]);
            if let Some(out) = str_arg(args, "out") {
                cli_args.extend(["--out".into(), out.into()]);
            }
            if let Some(fmt) = str_arg(args, "format") {
                cli_args.extend(["--format".into(), fmt.into()]);
            }
            if let Some(dpi) = args.get("dpi").and_then(|v| v.as_u64()) {
                cli_args.extend(["--dpi".into(), dpi.to_string()]);
            }
            if let Some(f) = str_arg(args, "filter") {
                cli_args.extend(["--filter".into(), f.into()]);
            }
            if let Some(t) = str_arg(args, "title") {
                cli_args.extend(["--title".into(), t.into()]);
            }
            if let Some(st) = str_arg(args, "subtitle") {
                cli_args.extend(["--subtitle".into(), st.into()]);
            }
            if let Some(s) = str_arg(args, "source") {
                cli_args.extend(["--source".into(), s.into()]);
            }
            if let Some(zl) = str_arg(args, "zoom_layer") {
                cli_args.extend(["--zoom-layer".into(), zl.into()]);
            }
        }
        "geokit_map_links" => {
            let (Some(action), Some(input)) = (str_arg(args, "action"), str_arg(args, "input"))
            else {
                return ("Missing required arguments: action, input".into(), true);
            };
            cli_args.extend(["map".into(), action.into(), input.into()]);
            if let Some(f) = str_arg(args, "find") {
                cli_args.extend(["--find".into(), f.into()]);
            }
            if let Some(r) = str_arg(args, "replace") {
                cli_args.extend(["--replace".into(), r.into()]);
            }
            if let Some(l) = str_arg(args, "layer") {
                cli_args.extend(["--layer".into(), l.into()]);
            }
            if let Some(fld) = str_arg(args, "field") {
                cli_args.extend(["--field".into(), fld.into()]);
            }
            if let Some(pr) = str_arg(args, "preset") {
                cli_args.extend(["--preset".into(), pr.into()]);
            }
            if let Some(tr) = args.get("transparency").and_then(|v| v.as_u64()) {
                cli_args.extend(["--transparency".into(), tr.to_string()]);
            }
            if let Some(cl) = str_arg(args, "classes") {
                cli_args.extend(["--classes".into(), cl.into()]);
            }
            if let Some(out) = str_arg(args, "out") {
                cli_args.extend(["--out".into(), out.into()]);
            }
        }
        "geokit_raster" => {
            let (Some(action), Some(out)) = (str_arg(args, "action"), str_arg(args, "out")) else {
                return ("Missing required arguments: action, out".into(), true);
            };
            cli_args.extend(["raster".into(), action.into()]);
            if action == "align" {
                if let Some(ins) = str_arg(args, "inputs") {
                    for part in ins.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                        cli_args.push(part.into());
                    }
                } else if let Some(input) = str_arg(args, "input") {
                    cli_args.push(input.into());
                }
            } else if let Some(input) = str_arg(args, "input") {
                cli_args.push(input.into());
            }
            cli_args.extend(["--out".into(), out.into()]);
            if let Some(z) = str_arg(args, "zones") {
                cli_args.extend(["--zones".into(), z.into()]);
            }
            if let Some(zf) = str_arg(args, "zone_field") {
                cli_args.extend(["--zone-field".into(), zf.into()]);
            }
            if let Some(r) = str_arg(args, "rules") {
                cli_args.extend(["--rules".into(), r.into()]);
            }
            if bool_arg(args, "smooth") {
                cli_args.push("--smooth".into());
            }
            if action != "align" {
                if let Some(ins) = str_arg(args, "inputs") {
                    cli_args.extend(["--inputs".into(), ins.into()]);
                }
            }
            if let Some(rf) = str_arg(args, "reference") {
                cli_args.extend(["--ref".into(), rf.into()]);
            }
            if let Some(rs) = str_arg(args, "resampling") {
                cli_args.extend(["--resampling".into(), rs.into()]);
            }
            if let Some(pts) = str_arg(args, "points") {
                cli_args.extend(["--points".into(), pts.into()]);
            }
            if let Some(ras) = str_arg(args, "raster") {
                cli_args.extend(["--raster".into(), ras.into()]);
            }
            if let Some(tr) = args.get("tpi_radius").and_then(|v| v.as_u64()) {
                cli_args.extend(["--tpi-radius".into(), tr.to_string()]);
            }
            if let Some(gcp) = str_arg(args, "gcp") {
                cli_args.extend(["--gcp".into(), gcp.into()]);
            }
            if let Some(epsg) = args.get("epsg").and_then(|v| v.as_u64()) {
                cli_args.extend(["--epsg".into(), epsg.to_string()]);
            }
            if let Some(ord) = args.get("order").and_then(|v| v.as_u64()) {
                cli_args.extend(["--order".into(), ord.to_string()]);
            }
            if let Some(rep) = str_arg(args, "report") {
                cli_args.extend(["--report".into(), rep.into()]);
            }
            if let Some(cls) = args.get("classes").and_then(|v| v.as_u64()) {
                cli_args.extend(["--classes".into(), cls.to_string()]);
            }
            if let Some(po) = str_arg(args, "polygon_out") {
                cli_args.extend(["--polygon-out".into(), po.into()]);
            }
            if let Some(fld) = str_arg(args, "field") {
                cli_args.extend(["--field".into(), fld.into()]);
            }
            if let Some(m) = str_arg(args, "method") {
                cli_args.extend(["--method".into(), m.into()]);
            }
            if let Some(cs) = args.get("cell_size").and_then(|v| v.as_f64()) {
                cli_args.extend(["--cell-size".into(), cs.to_string()]);
            }
            if let Some(msk) = str_arg(args, "mask") {
                cli_args.extend(["--mask".into(), msk.into()]);
            }
            if let Some(it) = str_arg(args, "index_type") {
                cli_args.extend(["--type".into(), it.into()]);
            }
            if let Some(ba) = str_arg(args, "band_a") {
                cli_args.extend(["--band-a".into(), ba.into()]);
            }
            if let Some(bb) = str_arg(args, "band_b") {
                cli_args.extend(["--band-b".into(), bb.into()]);
            }
            if action == "index" {
                if let Some(mb) = str_arg(args, "multiband") {
                    cli_args.extend(["--multiband".into(), mb.into()]);
                }
            }
            if let Some(sn) = str_arg(args, "sensor") {
                cli_args.extend(["--sensor".into(), sn.into()]);
            }
            if let Some(bd) = str_arg(args, "bands") {
                cli_args.extend(["--bands".into(), bd.into()]);
            }
            if let Some(pr) = str_arg(args, "preset") {
                cli_args.extend(["--preset".into(), pr.into()]);
            }
            if let Some(aoi) = str_arg(args, "aoi") {
                cli_args.extend(["--aoi".into(), aoi.into()]);
            }
            if let Some(iv) = args.get("interval").and_then(|v| v.as_f64()) {
                cli_args.extend(["--interval".into(), iv.to_string()]);
            }
            if let Some(bs) = args.get("base").and_then(|v| v.as_f64()) {
                cli_args.extend(["--base".into(), bs.to_string()]);
            }
            if let Some(ie) = args.get("index_every").and_then(|v| v.as_u64()) {
                cli_args.extend(["--index-every".into(), ie.to_string()]);
            }
        }
        "geokit_hydro" => {
            let (Some(action), Some(dem), Some(out)) = (
                str_arg(args, "action"),
                str_arg(args, "dem"),
                str_arg(args, "out"),
            ) else {
                return ("Missing required arguments: action, dem, out".into(), true);
            };
            cli_args.extend([
                "hydro".into(),
                action.into(),
                dem.into(),
                "--out".into(),
                out.into(),
            ]);
            if let Some(th) = args.get("threshold").and_then(|v| v.as_u64()) {
                cli_args.extend(["--threshold".into(), th.to_string()]);
            }
            if let Some(pp) = str_arg(args, "pour_point") {
                cli_args.extend(["--pour-point".into(), pp.into()]);
            }
        }
        "geokit_live" => {
            let Some(action) = str_arg(args, "action") else {
                return ("Missing required argument: action".into(), true);
            };
            cli_args.extend(["live".into(), action.into()]);
            if let Some(path) = str_arg(args, "path") {
                cli_args.push(path.into());
            }
            if let Some(layer) = str_arg(args, "layer") {
                cli_args.push(layer.into());
            }
            if let Some(pattern) = str_arg(args, "pattern") {
                cli_args.push(pattern.into());
            }
            if let Some(show) = args.get("show").and_then(|v| v.as_bool()) {
                cli_args.extend(["--show".into(), show.to_string()]);
            }
            if let Some(field) = str_arg(args, "field") {
                cli_args.extend(["--field".into(), field.into()]);
            }
            if let Some(m) = str_arg(args, "mode") {
                cli_args.extend(["--mode".into(), m.into()]);
            }
            if let Some(pr) = str_arg(args, "preset") {
                cli_args.extend(["--preset".into(), pr.into()]);
            }
            if let Some(cs) = str_arg(args, "colors") {
                cli_args.extend(["--colors".into(), cs.into()]);
            }
            if let Some(c) = str_arg(args, "color") {
                cli_args.extend(["--color".into(), c.into()]);
            }
            if let Some(cc) = args.get("classes_count").and_then(|v| v.as_u64()) {
                cli_args.extend(["--classes-count".into(), cc.to_string()]);
            }
            if let Some(bm) = str_arg(args, "break_method") {
                cli_args.extend(["--break-method".into(), bm.into()]);
            }
            if let Some(oc) = str_arg(args, "outline_color") {
                cli_args.extend(["--outline-color".into(), oc.into()]);
            }
            if let Some(ow) = args.get("outline_width").and_then(|v| v.as_f64()) {
                cli_args.extend(["--outline-width".into(), ow.to_string()]);
            }
            if let Some(os) = str_arg(args, "outline_style") {
                cli_args.extend(["--outline-style".into(), os.into()]);
            }
            if let Some(w) = args.get("width").and_then(|v| v.as_f64()) {
                cli_args.extend(["--width".into(), w.to_string()]);
            }
            if let Some(sz) = args.get("size").and_then(|v| v.as_f64()) {
                cli_args.extend(["--size".into(), sz.to_string()]);
            }
            if let Some(mk) = str_arg(args, "marker") {
                cli_args.extend(["--marker".into(), mk.into()]);
            }
            if bool_arg(args, "hollow") {
                cli_args.push("--hollow".into());
            }
            if let Some(tr) = args.get("transparency").and_then(|v| v.as_u64()) {
                cli_args.extend(["--transparency".into(), tr.to_string()]);
            }
            if let Some(cl) = str_arg(args, "classes") {
                cli_args.extend(["--classes".into(), cl.into()]);
            }
            if let Some(pos) = str_arg(args, "position") {
                cli_args.extend(["--position".into(), pos.into()]);
            }
            if let Some(v) = str_arg(args, "view") {
                cli_args.extend(["--view".into(), v.into()]);
            }
            if let Some(t) = str_arg(args, "title") {
                cli_args.extend(["--title".into(), t.into()]);
            }
            if let Some(st) = str_arg(args, "subtitle") {
                cli_args.extend(["--subtitle".into(), st.into()]);
            }
            if let Some(s) = str_arg(args, "source") {
                cli_args.extend(["--source".into(), s.into()]);
            }
            if let Some(zl) = str_arg(args, "zoom_layer") {
                cli_args.extend(["--zoom-layer".into(), zl.into()]);
            }
            if let Some(hl) = str_arg(args, "hide_legend") {
                cli_args.extend(["--hide-legend".into(), hl.into()]);
            }
            if let Some(nm) = str_arg(args, "name") {
                cli_args.extend(["--name".into(), nm.into()]);
            }
            if let Some(pad) = args.get("padding").and_then(|v| v.as_f64()) {
                cli_args.extend(["--padding".into(), pad.to_string()]);
            }
            if let Some(sc) = str_arg(args, "scale") {
                cli_args.extend(["--scale".into(), sc.into()]);
            }
            if let Some(h) = args.get("halo").and_then(|v| v.as_bool()) {
                cli_args.extend(["--halo".into(), h.to_string()]);
            }
            if let Some(fs) = args.get("font_size").and_then(|v| v.as_f64()) {
                cli_args.extend(["--font-size".into(), fs.to_string()]);
            }
            if let Some(tpl) = str_arg(args, "template") {
                cli_args.extend(["--template".into(), tpl.into()]);
            }
            if let Some(pp) = str_arg(args, "paper") {
                cli_args.extend(["--paper".into(), pp.into()]);
            }
            if let Some(reg) = str_arg(args, "region") {
                cli_args.extend(["--region".into(), reg.into()]);
            }
            if let Some(sn) = str_arg(args, "sheet_number") {
                cli_args.extend(["--sheet-number".into(), sn.into()]);
            }
            if let Some(au) = str_arg(args, "author") {
                cli_args.extend(["--author".into(), au.into()]);
            }
            if let Some(ed) = str_arg(args, "edition") {
                cli_args.extend(["--edition".into(), ed.into()]);
            }
            if let Some(nt) = str_arg(args, "note") {
                cli_args.extend(["--note".into(), nt.into()]);
            }
            if let Some(lg) = str_arg(args, "logo") {
                cli_args.extend(["--logo".into(), lg.into()]);
            }
            if let Some(il) = str_arg(args, "inset_layer") {
                cli_args.extend(["--inset-layer".into(), il.into()]);
            }
            if let Some(ip) = str_arg(args, "inset_petunjuk") {
                cli_args.extend(["--inset-petunjuk".into(), ip.into()]);
            }
            if let Some(gr) = args.get("grid").and_then(|v| v.as_bool()) {
                cli_args.extend(["--grid".into(), gr.to_string()]);
            }
            if let Some(gm) = str_arg(args, "grid_mode") {
                cli_args.extend(["--grid-mode".into(), gm.into()]);
            }
            if let Some(ns) = str_arg(args, "north_style") {
                cli_args.extend(["--north-style".into(), ns.into()]);
            }
            if let Some(lyr) = str_arg(args, "lyr") {
                cli_args.extend(["--lyr".into(), lyr.into()]);
            }
            if let Some(w) = str_arg(args, "where") {
                cli_args.extend(["--where".into(), w.into()]);
            }
            if let Some(out) = str_arg(args, "out") {
                cli_args.extend(["--out".into(), out.into()]);
            }
            if let Some(dpi) = args.get("dpi").and_then(|v| v.as_u64()) {
                cli_args.extend(["--dpi".into(), dpi.to_string()]);
            }
            if let Some(tool) = str_arg(args, "tool") {
                cli_args.push(tool.into());
            }
            if let Some(arr) = args.get("args").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        cli_args.push(s.into());
                    }
                }
            }
        }
        "geokit_toolbox" => {
            let (Some(tb_name), Some(out)) = (str_arg(args, "name"), str_arg(args, "out")) else {
                return ("Missing required arguments: name, out".into(), true);
            };
            cli_args.extend([
                "toolbox".into(),
                "create".into(),
                tb_name.into(),
                "--out".into(),
                out.into(),
            ]);
            if let Some(tpl) = str_arg(args, "template") {
                cli_args.extend(["--template".into(), tpl.into()]);
            }
            if bool_arg(args, "force") {
                cli_args.push("--force".into());
            }
        }
        "geokit_pipeline" => {
            let Some(action) = str_arg(args, "action") else {
                return ("Missing required argument: action".into(), true);
            };
            cli_args.extend(["pipeline".into(), action.into()]);
            if action == "init" {
                if let Some(f) = str_arg(args, "file") {
                    cli_args.extend(["--out".into(), f.into()]);
                }
                if bool_arg(args, "force") {
                    cli_args.push("--force".into());
                }
            } else {
                let Some(f) = str_arg(args, "file") else {
                    return ("Missing required argument: file".into(), true);
                };
                cli_args.push(f.into());
                if bool_arg(args, "dry_run") {
                    cli_args.push("--dry-run".into());
                }
                if bool_arg(args, "resume") {
                    cli_args.push("--resume".into());
                }
            }
        }
        "geokit_qa" => {
            let Some(input) = str_arg(args, "input") else {
                return ("Missing required argument: input".into(), true);
            };
            cli_args.extend(["qa".into(), input.into(), "--json".into()]);
            if let Some(epsg) = args.get("expected_epsg").and_then(|v| v.as_u64()) {
                cli_args.extend(["--expected-epsg".into(), epsg.to_string()]);
            }
            if let Some(cn) = str_arg(args, "check_nulls") {
                cli_args.extend(["--check-nulls".into(), cn.into()]);
            }
            if bool_arg(args, "check_overlaps") {
                cli_args.push("--check-overlaps".into());
            }
            if bool_arg(args, "strict") {
                cli_args.push("--strict".into());
            }
        }
        "geokit_topo" => {
            let (Some(action), Some(input)) = (str_arg(args, "action"), str_arg(args, "input"))
            else {
                return ("Missing required arguments: action, input".into(), true);
            };
            cli_args.extend(["topo".into(), action.into(), input.into()]);
            if action == "check" {
                cli_args.push("--json".into());
                if let Some(od) = str_arg(args, "out_dir") {
                    cli_args.extend(["--out-dir".into(), od.into()]);
                }
                if bool_arg(args, "strict") {
                    cli_args.push("--strict".into());
                }
            } else if let Some(out) = str_arg(args, "out") {
                cli_args.extend(["--out".into(), out.into()]);
            }
            if let Some(ms) = args.get("min_sliver").and_then(|v| v.as_f64()) {
                cli_args.extend(["--min-sliver".into(), ms.to_string()]);
            }
        }
        "geokit_report" => {
            let (Some(path), Some(out)) = (str_arg(args, "path"), str_arg(args, "out")) else {
                return ("Missing required arguments: path, out".into(), true);
            };
            cli_args.extend(["report".into(), path.into(), "--out".into(), out.into()]);
        }
        "geokit_fetch" => {
            let (Some(src), Some(out)) = (str_arg(args, "source"), str_arg(args, "out")) else {
                return ("Missing required arguments: source, out".into(), true);
            };
            cli_args.extend(["fetch".into(), src.into(), "--out".into(), out.into()]);
            if let Some(aoi) = str_arg(args, "aoi") {
                cli_args.extend(["--aoi".into(), aoi.into()]);
            }
            if let Some(bbox) = str_arg(args, "bbox") {
                cli_args.extend(["--bbox".into(), bbox.into()]);
            }
            if let Some(lyrs) = str_arg(args, "layers") {
                cli_args.extend(["--layers".into(), lyrs.into()]);
            }
            if bool_arg(args, "clip_aoi") {
                cli_args.push("--clip-aoi".into());
            }
            if let Some(epsg) = args.get("epsg").and_then(|v| v.as_u64()) {
                cli_args.extend(["--epsg".into(), epsg.to_string()]);
            }
        }
        "geokit_webmap" => {
            let Some(out) = str_arg(args, "out") else {
                return ("Missing required argument: out".into(), true);
            };
            cli_args.push("webmap".into());
            let mut pushed_any = false;
            if let Some(arr) = args.get("inputs").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        cli_args.push(s.into());
                        pushed_any = true;
                    }
                }
            }
            if let Some(input) = str_arg(args, "input") {
                cli_args.push(input.into());
                pushed_any = true;
            }
            if !pushed_any {
                return ("Missing required argument: input or inputs".into(), true);
            }
            cli_args.extend(["--out".into(), out.into()]);
            if let Some(f) = str_arg(args, "field") {
                cli_args.extend(["--field".into(), f.into()]);
            }
            if let Some(p) = str_arg(args, "preset") {
                cli_args.extend(["--preset".into(), p.into()]);
            }
            if let Some(t) = str_arg(args, "title") {
                cli_args.extend(["--title".into(), t.into()]);
            }
            if let Some(bm) = str_arg(args, "basemap") {
                cli_args.extend(["--basemap".into(), bm.into()]);
            }
            if let Some(sm) = args.get("simplify").and_then(|v| v.as_f64()) {
                cli_args.extend(["--simplify".into(), sm.to_string()]);
            }
        }
        "geokit_summary" => {
            let (Some(input), Some(gb)) = (str_arg(args, "input"), str_arg(args, "group_by"))
            else {
                return ("Missing required arguments: input, group_by".into(), true);
            };
            cli_args.extend([
                "gp".into(),
                "summary".into(),
                input.into(),
                "--group-by".into(),
                gb.into(),
                "--json".into(),
            ]);
            if let Some(af) = str_arg(args, "area_field") {
                cli_args.extend(["--area-field".into(), af.into()]);
            }
            if let Some(out) = str_arg(args, "out") {
                cli_args.extend(["--out".into(), out.into()]);
            }
        }
        "geokit_daemon" => {
            let Some(action) = str_arg(args, "action") else {
                return ("Missing required argument: action".into(), true);
            };
            cli_args.extend(["daemon".into(), action.into()]);
            match action {
                "start" => {
                    if let Some(m) = args.get("idle_minutes").and_then(|v| v.as_u64()) {
                        cli_args.extend(["--idle-minutes".into(), m.to_string()]);
                    }
                    cli_args.push("--json".into());
                }
                "status" => cli_args.push("--json".into()),
                "stop" => {
                    if let Some(t) = str_arg(args, "target") {
                        cli_args.extend(["--target".into(), t.into()]);
                    }
                }
                _ => return (format!("Unknown daemon action: {action}"), true),
            }
        }
        "geokit_accuracy" => {
            let (Some(action), Some(input)) = (str_arg(args, "action"), str_arg(args, "input"))
            else {
                return ("Missing required arguments: action, input".into(), true);
            };
            cli_args.extend(["accuracy".into(), action.into(), input.into()]);
            match action {
                "sample" => {
                    let (Some(field), Some(out)) = (str_arg(args, "field"), str_arg(args, "out"))
                    else {
                        return (
                            "Missing required arguments for sample: field, out".into(),
                            true,
                        );
                    };
                    cli_args.extend(["--field".into(), field.into(), "--out".into(), out.into()]);
                    if let Some(n) = args.get("total").and_then(|v| v.as_u64()) {
                        cli_args.extend(["--total".into(), n.to_string()]);
                    }
                    if let Some(m) = args.get("min_per_class").and_then(|v| v.as_u64()) {
                        cli_args.extend(["--min-per-class".into(), m.to_string()]);
                    }
                    if let Some(s) = args.get("seed").and_then(|v| v.as_u64()) {
                        cli_args.extend(["--seed".into(), s.to_string()]);
                    }
                }
                "matrix" => {
                    let (Some(pred), Some(rf)) =
                        (str_arg(args, "pred"), str_arg(args, "ref_field"))
                    else {
                        return (
                            "Missing required arguments for matrix: pred, ref_field".into(),
                            true,
                        );
                    };
                    cli_args.extend([
                        "--pred".into(),
                        pred.into(),
                        "--ref-field".into(),
                        rf.into(),
                        "--json".into(),
                    ]);
                    if let Some(out) = str_arg(args, "out") {
                        cli_args.extend(["--out".into(), out.into()]);
                    }
                }
                "rmse" => {
                    if let Some(pred) = str_arg(args, "pred") {
                        cli_args.extend(["--pred".into(), pred.into()]);
                    }
                    if let Some(rf) = str_arg(args, "ref_field") {
                        cli_args.extend(["--ref".into(), rf.into()]);
                    }
                    if let Some(px) = str_arg(args, "pred_x") {
                        cli_args.extend(["--pred-x".into(), px.into()]);
                    }
                    if let Some(py) = str_arg(args, "pred_y") {
                        cli_args.extend(["--pred-y".into(), py.into()]);
                    }
                    if let Some(rx) = str_arg(args, "ref_x") {
                        cli_args.extend(["--ref-x".into(), rx.into()]);
                    }
                    if let Some(ry) = str_arg(args, "ref_y") {
                        cli_args.extend(["--ref-y".into(), ry.into()]);
                    }
                    if let Some(out) = str_arg(args, "out") {
                        cli_args.extend(["--out".into(), out.into()]);
                    }
                    cli_args.push("--json".into());
                }
                "roc" => {
                    let (Some(score), Some(label)) =
                        (str_arg(args, "score"), str_arg(args, "label"))
                    else {
                        return (
                            "Missing required arguments for roc: score, label".into(),
                            true,
                        );
                    };
                    cli_args.extend([
                        "--score".into(),
                        score.into(),
                        "--label".into(),
                        label.into(),
                        "--json".into(),
                    ]);
                    if let Some(pos) = str_arg(args, "positive") {
                        cli_args.extend(["--positive".into(), pos.into()]);
                    }
                    if let Some(out) = str_arg(args, "out") {
                        cli_args.extend(["--out".into(), out.into()]);
                    }
                }
                _ => return (format!("Unknown accuracy action: {action}"), true),
            }
        }
        "geokit_survey" => {
            let (Some(action), Some(aoi), Some(out)) = (
                str_arg(args, "action"),
                str_arg(args, "aoi"),
                str_arg(args, "out"),
            ) else {
                return ("Missing required arguments: action, aoi, out".into(), true);
            };
            cli_args.extend([
                "survey".into(),
                action.into(),
                aoi.into(),
                "--out".into(),
                out.into(),
                "--json".into(),
            ]);
            match action {
                "blocks" => {
                    if let Some(s) = args.get("block_size_m").and_then(|v| v.as_f64()) {
                        cli_args.extend(["--block-size-m".into(), s.to_string()]);
                    }
                    if let Some(h) = args.get("block_ha").and_then(|v| v.as_f64()) {
                        cli_args.extend(["--block-ha".into(), h.to_string()]);
                    }
                    if let Some(c) = args.get("cols").and_then(|v| v.as_u64()) {
                        cli_args.extend(["--cols".into(), c.to_string()]);
                    }
                    if let Some(r) = args.get("rows").and_then(|v| v.as_u64()) {
                        cli_args.extend(["--rows".into(), r.to_string()]);
                    }
                    if let Some(ov) = args.get("overlap_m").and_then(|v| v.as_f64()) {
                        cli_args.extend(["--overlap-m".into(), ov.to_string()]);
                    }
                    if bool_arg(args, "split_kml") {
                        cli_args.push("--split-kml".into());
                    }
                }
                "gcp" => {
                    if let Some(g) = args.get("gcp_count").and_then(|v| v.as_u64()) {
                        cli_args.extend(["--gcp-count".into(), g.to_string()]);
                    }
                    if let Some(i) = args.get("icp_count").and_then(|v| v.as_u64()) {
                        cli_args.extend(["--icp-count".into(), i.to_string()]);
                    }
                    if let Some(m) = args.get("margin_m").and_then(|v| v.as_f64()) {
                        cli_args.extend(["--margin-m".into(), m.to_string()]);
                    }
                }
                _ => return (format!("Unknown survey action: {action}"), true),
            }
        }
        _ => return (format!("Unknown tool: {name}"), true),
    }

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return (format!("Failed to locate geokit executable: {e}"), true),
    };

    match Command::new(exe)
        .args(&cli_args)
        .env("NO_COLOR", "1")
        .output()
    {
        Ok(output) => {
            const MAX_MCP_OUTPUT_BYTES: usize = 1024 * 1024;
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            let stderr_str = String::from_utf8_lossy(&output.stderr);
            let mut combined = if stderr_str.trim().is_empty() {
                stdout_str.into_owned()
            } else if stdout_str.trim().is_empty() {
                stderr_str.into_owned()
            } else {
                format!("{stdout_str}\n--- stderr ---\n{stderr_str}")
            };
            if combined.len() > MAX_MCP_OUTPUT_BYTES {
                let mut cut = MAX_MCP_OUTPUT_BYTES;
                while !combined.is_char_boundary(cut) && cut > 0 {
                    cut -= 1;
                }
                combined.truncate(cut);
                combined.push_str("\n...[output truncated at 1 MB safety limit]");
            }
            (combined, !output.status.success())
        }
        Err(e) => (format!("Failed to run geokit: {e}"), true),
    }
}
