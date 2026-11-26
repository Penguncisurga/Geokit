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
    json!([
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
            "description": "Run batch geoprocessing, attribute inspection/scoring/classification/union, OR dynamically discover/run ANY installed ArcGIS Pro / ArcMap tool (values, classify, calc-field, union, info, project, clip, merge, pack, unpack, repair, buffer, dissolve, calc-geom, intersect, erase, spatial-join, xy2shp, convert, tools, tool-usage, run).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["info", "values", "classify", "calc-field", "union", "project", "clip", "merge", "pack", "unpack", "repair", "buffer", "dissolve", "calc-geom", "intersect", "erase", "spatial-join", "xy2shp", "convert", "tools", "tool-usage", "run"]
                    },
                    "input": { "type": "string", "description": "Input file, folder, .gdb, or keyword pattern (for tools)" },
                    "inputs": { "type": "array", "items": { "type": "string" }, "description": "List of input polygon paths (for union)" },
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
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Positional arguments passed to ArcPy tool (for run)" }
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
            "description": "Run raster mosaic, terrain (slope/aspect/hillshade), or zonal statistics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["mosaic", "terrain", "zonal-stats"] },
                    "input": { "type": "string", "description": "Input raster or folder" },
                    "out": { "type": "string", "description": "Output file or directory" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap", "gdal"] },
                    "zones": { "type": "string", "description": "Zone polygon path (for zonal-stats)" },
                    "zone_field": { "type": "string", "description": "Zone field name (for zonal-stats)" }
                },
                "required": ["action", "input", "out"]
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
            "description": "Control an open ArcMap 10.1-10.8 window in real time via the GeoKit Live-UI Add-In bridge. Supports full ArcToolbox discovery (tools/tool-usage) and execution (run-gp with auto extension checkout), plus live cartography (add, remove, visible, zoom, select, clear-selection, layer-info, definition-query, symbology, labels, export, save).",
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
                            "install-addin"
                        ]
                    },
                    "path": { "type": "string", "description": "Dataset path or folder (for add)" },
                    "layer": { "type": "string", "description": "Layer name in ArcMap TOC" },
                    "show": { "type": "boolean", "description": "True/false toggle (for visible or labels)" },
                    "field": { "type": "string", "description": "Field name e.g. KELAS_RWN or NAMOBJ (for style or labels)" },
                    "preset": { "type": "string", "enum": ["hazard", "hazard5", "blue", "earth", "category"], "description": "Built-in borderless color preset (for style)" },
                    "transparency": { "type": "integer", "description": "Transparency 0-100 (for style)" },
                    "classes": { "type": "string", "description": "Optional comma-separated ordered class names (for style)" },
                    "position": { "type": "string", "enum": ["top", "bottom"], "description": "Layer stack position (for order)" },
                    "view": { "type": "string", "enum": ["layout", "data"], "description": "View mode (for layout)" },
                    "title": { "type": "string", "description": "Layout title text (for layout)" },
                    "subtitle": { "type": "string", "description": "Layout subtitle text (for layout)" },
                    "source": { "type": "string", "description": "Layout source credit text (for layout)" },
                    "zoom_layer": { "type": "string", "description": "Layer name to zoom to (for layout)" },
                    "lyr": { "type": "string", "description": "Path to .lyr symbology file (for symbology)" },
                    "where": { "type": "string", "description": "SQL WHERE clause (for select or definition-query)" },
                    "out": { "type": "string", "description": "Output file path (for export or save)" },
                    "dpi": { "type": "integer", "description": "Export DPI (for export)" },
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
        }
    ])
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn bool_arg(args: &Value, key: &str) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn execute_tool(name: &str, args: &Value) -> (String, bool) {
    let mut cli_args: Vec<String> = Vec::new();

    if let Some(eng) = str_arg(args, "engine") {
        cli_args.push("--engine".into());
        cli_args.push(eng.into());
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
            if let Some(lim) = args.get("limit").and_then(|v| v.as_u64()) {
                cli_args.extend(["--limit".into(), lim.to_string()]);
            }
            if action == "values" {
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
            let (Some(action), Some(input), Some(out)) = (
                str_arg(args, "action"),
                str_arg(args, "input"),
                str_arg(args, "out"),
            ) else {
                return (
                    "Missing required arguments: action, input, out".into(),
                    true,
                );
            };
            cli_args.extend([
                "raster".into(),
                action.into(),
                input.into(),
                "--out".into(),
                out.into(),
            ]);
            if let Some(z) = str_arg(args, "zones") {
                cli_args.extend(["--zones".into(), z.into()]);
            }
            if let Some(zf) = str_arg(args, "zone_field") {
                cli_args.extend(["--zone-field".into(), zf.into()]);
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
            if let Some(pr) = str_arg(args, "preset") {
                cli_args.extend(["--preset".into(), pr.into()]);
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
        }
        "geokit_report" => {
            let (Some(path), Some(out)) = (str_arg(args, "path"), str_arg(args, "out")) else {
                return ("Missing required arguments: path, out".into(), true);
            };
            cli_args.extend(["report".into(), path.into(), "--out".into(), out.into()]);
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
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            let stderr_str = String::from_utf8_lossy(&output.stderr);
            let combined = if stderr_str.trim().is_empty() {
                stdout_str.into_owned()
            } else if stdout_str.trim().is_empty() {
                stderr_str.into_owned()
            } else {
                format!("{stdout_str}\n--- stderr ---\n{stderr_str}")
            };
            (combined, !output.status.success())
        }
        Err(e) => (format!("Failed to run geokit: {e}"), true),
    }
}
