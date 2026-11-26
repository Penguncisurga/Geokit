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
                    "capabilities": { "tools": {} },
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
            "description": "Run batch geoprocessing OR dynamically discover/run ANY installed ArcGIS Pro / ArcMap tool (tools, tool-usage, run, info, project, clip, merge, pack, unpack, repair, buffer, dissolve, calc-geom, intersect, erase, spatial-join).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["info", "project", "clip", "merge", "pack", "unpack", "repair", "buffer", "dissolve", "calc-geom", "intersect", "erase", "spatial-join", "tools", "tool-usage", "run"]
                    },
                    "input": { "type": "string", "description": "Input file, folder, .gdb, or keyword pattern (for tools)" },
                    "out": { "type": "string", "description": "Output file, folder, or .gdb (when required)" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap", "gdal"], "description": "Backend engine" },
                    "epsg": { "type": "integer", "description": "Target EPSG code (for project)" },
                    "aoi": { "type": "string", "description": "AOI polygon path (for clip)" },
                    "dist": { "type": "string", "description": "Buffer distance e.g. '50 Meters' (for buffer)" },
                    "dissolve": { "type": "boolean", "description": "Dissolve overlapping buffers (for buffer)" },
                    "field": { "type": "string", "description": "Field name(s) (for dissolve or calc-geom)" },
                    "unit": { "type": "string", "description": "Unit: ha, m2, km2, km, m (for calc-geom)" },
                    "with": { "type": "string", "description": "Overlay layer path (for intersect)" },
                    "mask": { "type": "string", "description": "Mask/erase layer path (for erase)" },
                    "join": { "type": "string", "description": "Join layer path (for spatial-join)" },
                    "match": { "type": "string", "enum": ["intersect", "within", "contains", "closest"], "description": "Match option (for spatial-join)" },
                    "tool": { "type": "string", "description": "Exact ArcPy tool name e.g. Buffer_analysis or Slope_sa (for tool-usage or run)" },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Positional arguments passed to ArcPy tool (for run)" }
                },
                "required": ["action"]
            }
        },
        {
            "name": "geokit_layout",
            "description": "List or batch export layouts from ArcGIS Pro (.aprx) or ArcMap (.mxd) projects to PDF, PNG, or JPG.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "export"] },
                    "input": { "type": "string", "description": "Path to .aprx, .mxd, or folder" },
                    "out": { "type": "string", "description": "Output directory (for export)" },
                    "format": { "type": "string", "enum": ["pdf", "png", "jpg"] },
                    "dpi": { "type": "integer", "description": "Export DPI (default: 300)" },
                    "filter": { "type": "string", "description": "Filter layout names (for .aprx)" },
                    "engine": { "type": "string", "enum": ["auto", "arcgis", "arcmap"] }
                },
                "required": ["action", "input"]
            }
        },
        {
            "name": "geokit_map_links",
            "description": "Detect or fix broken layer data sources in ArcGIS Pro (.aprx) or ArcMap (.mxd) projects.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["broken-links", "fix-links"] },
                    "input": { "type": "string", "description": "Path to .aprx, .mxd, or folder" },
                    "find": { "type": "string", "description": "Old path prefix to find (for fix-links)" },
                    "replace": { "type": "string", "description": "New path prefix (for fix-links)" },
                    "out": { "type": "string", "description": "Optional output copy path (for fix-links)" },
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
                    "path": { "type": "string", "description": "Dataset path (for add)" },
                    "layer": { "type": "string", "description": "Layer name in ArcMap TOC" },
                    "show": { "type": "boolean", "description": "True/false toggle (for visible or labels)" },
                    "field": { "type": "string", "description": "Label field name e.g. NAMOBJ (for labels)" },
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
