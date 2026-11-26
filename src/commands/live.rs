use anyhow::{bail, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

use crate::engine::abs_string;

const ARCMAP_ADDIN_BYTES: &[u8] = include_bytes!("../../addin/arcmap/geokit-arcmap.esriaddin");

#[derive(Debug, Subcommand)]
pub enum LiveCommand {
    /// Install the GeoKit Live-UI Python Add-In (.esriaddin) into ArcMap 10.1-10.8 automatically
    InstallAddin {
        /// ArcMap version (e.g. 10.5, 10.7, 10.8; defaults to auto-detecting all installed Desktop10.x)
        #[arg(long)]
        version: Option<String>,
    },
    /// Ping the live ArcMap 10.x window to check connection and active map state
    Ping {
        /// Live bridge TCP port (default: 9877)
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// List all layers currently loaded in the open ArcMap window
    Layers {
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Inspect fields, CRS, feature count, and sample rows of a layer currently in ArcMap
    LayerInfo {
        /// Layer name in ArcMap Table of Contents
        layer: String,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Add a shapefile, raster, .lyr, .gdb feature class, or an entire folder of layers live onto ArcMap
    Add {
        /// Dataset path or folder containing .shp/.tif/.lyr files (missing .shp extension is auto-resolved)
        path: PathBuf,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Remove a layer by name from the open ArcMap map
    Remove {
        /// Layer name in ArcMap Table of Contents
        layer: String,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Toggle layer visibility (on/off) live in the open ArcMap map
    Visible {
        /// Layer name in ArcMap Table of Contents
        layer: String,
        /// Show (true) or hide (false)
        #[arg(long, action = clap::ArgAction::Set)]
        show: bool,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Set or clear a SQL definition query on a layer live in ArcMap
    DefinitionQuery {
        /// Layer name in ArcMap Table of Contents
        layer: String,
        /// SQL WHERE filter (pass empty string "" to clear)
        #[arg(long = "where")]
        where_clause: String,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Apply a .lyr symbology file live onto a layer in ArcMap
    Symbology {
        /// Target layer name in ArcMap Table of Contents
        layer: String,
        /// Path to source .lyr style file
        #[arg(long = "lyr")]
        lyr_file: PathBuf,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Toggle map labels (and optionally label field) live on a layer in ArcMap
    Labels {
        /// Layer name in ArcMap Table of Contents
        layer: String,
        /// Show (true) or hide (false) labels
        #[arg(long, action = clap::ArgAction::Set)]
        show: bool,
        /// Optional attribute field name to label (e.g. NAMOBJ)
        #[arg(long)]
        field: Option<String>,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Zoom the live ArcMap map view to a layer (or full extent if omitted)
    Zoom {
        /// Layer name to zoom to (omit for full extent)
        layer: Option<String>,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Select features live in ArcMap using an SQL WHERE clause (e.g. "KELAS = 'Hutan'")
    Select {
        /// Layer name in ArcMap
        layer: String,
        /// SQL WHERE expression
        #[arg(long = "where")]
        where_clause: String,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Clear feature selection live in ArcMap
    ClearSelection {
        /// Optional layer name (omit to clear all layers)
        layer: Option<String>,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Export the currently open ArcMap view/layout directly to PNG, JPG, or PDF
    Export {
        /// Output image or PDF path
        #[arg(short, long)]
        out: PathBuf,
        /// Resolution in DPI (default: 200)
        #[arg(long, default_value_t = 200)]
        dpi: u32,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Save the currently open ArcMap document (or save a copy to --out)
    Save {
        /// Optional output .mxd path to save a copy
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Search available ArcToolbox tools inside live ArcMap (e.g. `geokit live tools clip`)
    Tools {
        /// Wildcard/keyword filter (e.g. "buffer", "*_sa", "clip")
        #[arg(default_value = "*")]
        pattern: String,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Show exact parameter usage syntax for any ArcToolbox tool (e.g. Buffer_analysis, Fill_sa)
    ToolUsage {
        /// Exact ArcPy tool name
        tool: String,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Full ArcObjects COM cartography styling live in ArcMap (unlimited N classes, custom RGB, lines, points, boundaries, rasters!)
    Style {
        /// Target layer name in ArcMap Table of Contents
        layer: String,
        /// Attribute field to symbolize (omit for single-symbol lines/points/boundaries or rasters)
        #[arg(long)]
        field: Option<String>,
        /// Renderer mode: auto, unique (N categories), graduated (numeric breaks), single, or raster
        #[arg(long, default_value = "auto")]
        mode: String,
        /// Color preset: hazard, hazard5, hazard-pastel, viridis, blue, earth, terrain, gray, category, stream, road, boundary, point
        #[arg(long, default_value = "hazard")]
        preset: String,
        /// Custom comma-separated hex color stops (e.g. "#2E7D32,#81C784,#FFF176,#FFA726,#D32F2F")
        #[arg(long)]
        colors: Option<String>,
        /// Single hex color (for single/stream/road/point/boundary mode, e.g. "#0288D1")
        #[arg(long)]
        color: Option<String>,
        /// Number of classes for graduated numeric breaks (default: 5)
        #[arg(long, default_value_t = 5)]
        classes_count: usize,
        /// Break method for graduated mode: quantile or equal (default: quantile)
        #[arg(long, default_value = "quantile")]
        break_method: String,
        /// Outline color in hex (e.g. "#37474F" or "#FFFFFF")
        #[arg(long)]
        outline_color: Option<String>,
        /// Outline width in points (default: 0.0 = clean borderless fill)
        #[arg(long, default_value_t = 0.0)]
        outline_width: f64,
        /// Line/outline dash style: solid, dash, dot, dashdot (default: solid)
        #[arg(long, default_value = "solid")]
        outline_style: String,
        /// Line width in points for polyline layers (default: 1.4)
        #[arg(long, default_value_t = 1.4)]
        width: f64,
        /// Marker size in points for point layers (default: 7.0)
        #[arg(long, default_value_t = 7.0)]
        size: f64,
        /// Marker shape for point layers: circle, square, diamond, cross, x (default: circle)
        #[arg(long, default_value = "circle")]
        marker: String,
        /// Hollow polygon fill (transparent inside, outline only — great for admin boundaries)
        #[arg(long)]
        hollow: bool,
        /// Layer transparency percentage 0-100 (e.g. 30 to blend over hillshade/basemap)
        #[arg(long)]
        transparency: Option<u8>,
        /// Optional comma-separated ordered class names (e.g. "Sangat Rendah,Rendah,Sedang,Tinggi,Sangat Tinggi")
        #[arg(long)]
        classes: Option<String>,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Move a layer to the top or bottom of the ArcMap Table of Contents
    Order {
        /// Layer name in ArcMap Table of Contents
        layer: String,
        /// Target stack position: top or bottom
        #[arg(long, default_value = "top")]
        position: String,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Switch between Layout/Data view, update text elements, zoom, and hide noisy layers from the legend
    Layout {
        /// View mode: layout or data
        #[arg(long)]
        view: Option<String>,
        /// Map title text (updates layout title text element)
        #[arg(long)]
        title: Option<String>,
        /// Subtitle text (updates layout subtitle text element)
        #[arg(long)]
        subtitle: Option<String>,
        /// Data source credit text (updates layout source text element)
        #[arg(long)]
        source: Option<String>,
        /// Optional layer name to zoom the map frame extent to
        #[arg(long)]
        zoom_layer: Option<String>,
        /// Comma-separated layer names to remove from the Layout Legend (e.g. "hillshade,Basemap")
        #[arg(long)]
        hide_legend: Option<String>,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
    /// Run ANY ArcToolbox tool inside the live ArcMap window (auto-checks out Spatial/3D/Network extensions)
    RunGp {
        /// ArcPy tool name (e.g. Buffer_analysis, Clip_analysis, Slope_sa, Dissolve_management)
        tool: String,
        /// Tool arguments in order
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
        #[arg(long, env = "GEOKIT_LIVE_PORT", default_value_t = 9877)]
        port: u16,
    },
}

fn send_live_request(port: u16, payload: &Value) -> Result<Value> {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect(&addr).with_context(|| {
        format!(
            "Could not connect to GeoKit ArcMap Live Bridge at {addr}.\n\
             Make sure:\n\
             1. Close ArcMap, run `geokit live install-addin`, then open ArcMap 10.x.\n\
             2. ArcMap 10.x is currently open."
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(300)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    let msg = format!("{}\n", serde_json::to_string(payload)?);
    stream.write_all(msg.as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.trim().is_empty() {
        bail!("Empty response from GeoKit ArcMap Live Bridge.");
    }
    let resp: Value = serde_json::from_str(&line)?;
    if resp.get("ok").and_then(|v| v.as_bool()) == Some(false) {
        let err = resp
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("Unknown ArcMap bridge error");
        bail!("ArcMap Live Bridge error: {err}");
    }
    Ok(resp)
}

/// Auto-detects installed ArcMap 10.1–10.8 versions from both Program Files and %APPDATA%\ESRI
/// (works even when ArcMap is installed on a custom drive like D:\).
fn detect_arcmap_versions() -> Vec<String> {
    let mut found = Vec::new();

    if let Ok(appdata) = std::env::var("APPDATA") {
        let esri_dir = PathBuf::from(appdata).join("ESRI");
        if let Ok(entries) = std::fs::read_dir(&esri_dir) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                if let Some(name) = entry.file_name().to_str() {
                    if let Some(ver) = name.strip_prefix("Desktop") {
                        if ver.starts_with("10.") && !found.iter().any(|v| v == ver) {
                            found.push(ver.to_string());
                        }
                    }
                }
            }
        }
    }

    for ver in [
        "10.8", "10.7", "10.6", "10.5", "10.4", "10.3", "10.2", "10.1",
    ] {
        let p = Path::new(r"C:\Program Files (x86)\ArcGIS").join(format!("Desktop{ver}"));
        if p.is_dir() && !found.iter().any(|v| v == ver) {
            found.push(ver.to_string());
        }
    }

    found.sort();
    found.reverse();
    found
}

fn register_addin_folder_xml(ver: &str, dir_str: &str) -> Result<PathBuf> {
    let appdata = std::env::var("APPDATA")
        .context("APPDATA environment variable not found (Windows required for ArcMap).")?;
    let esri_dir = PathBuf::from(appdata)
        .join("ESRI")
        .join(format!("Desktop{ver}"));
    std::fs::create_dir_all(&esri_dir)?;
    let xml_path = esri_dir.join("AddInFoldersList.xml");

    let entry = format!(r#"<AddinFolder Path="{dir_str}"/>"#);
    let updated = if xml_path.is_file() {
        let existing = std::fs::read_to_string(&xml_path)?;
        if existing.contains(dir_str) {
            existing
        } else if existing.contains("<ARCMAP/>") {
            existing.replace("<ARCMAP/>", &format!("<ARCMAP>{entry}</ARCMAP>"))
        } else if existing.contains("</ARCMAP>") {
            existing.replace("</ARCMAP>", &format!("{entry}</ARCMAP>"))
        } else {
            format!(
                "<?xml version=\"1.0\"?>\n<ESRI.ArcGIS.AddInFolders ver=\"1\"><ARCMAP>{entry}</ARCMAP></ESRI.ArcGIS.AddInFolders>\n"
            )
        }
    } else {
        format!(
            "<?xml version=\"1.0\"?>\n<ESRI.ArcGIS.AddInFolders ver=\"1\"><ARCMAP>{entry}</ARCMAP></ESRI.ArcGIS.AddInFolders>\n"
        )
    };

    std::fs::write(&xml_path, updated)
        .with_context(|| format!("Failed to update {}", xml_path.display()))?;
    Ok(xml_path)
}

fn install_addin(version: Option<&str>) -> Result<()> {
    let user_profile = std::env::var("USERPROFILE")
        .context("USERPROFILE environment variable not found (Windows required for ArcMap).")?;

    let versions: Vec<String> = match version {
        Some(v) => vec![v.trim_start_matches("Desktop").to_string()],
        None => {
            let detected = detect_arcmap_versions();
            if detected.is_empty() {
                bail!(
                    "No ArcGIS Desktop 10.x installation detected.\n\
                     Pass `--version 10.7` (or 10.5, 10.6, 10.8) explicitly."
                );
            }
            detected
        }
    };

    let addin_dir = PathBuf::from(&user_profile).join(".geokit").join("addins");
    std::fs::create_dir_all(&addin_dir)
        .with_context(|| format!("Failed to create {}", addin_dir.display()))?;
    let dst = addin_dir.join("geokit-arcmap.esriaddin");
    std::fs::write(&dst, ARCMAP_ADDIN_BYTES)
        .with_context(|| format!("Failed to write {}", dst.display()))?;
    println!(
        "{} Extracted GeoKit Live Add-In -> {}",
        "[OK]".green().bold(),
        dst.display()
    );

    let dir_str = addin_dir.display().to_string();
    for ver in &versions {
        let xml_path = register_addin_folder_xml(ver, &dir_str)?;
        println!(
            "{} Registered Add-In folder in {}",
            "[OK]".green().bold(),
            xml_path.display()
        );
    }

    println!(
        "\n{} Make sure ArcMap is CLOSED before running `install-addin` (ArcMap rewrites AddInFoldersList.xml on exit).",
        "Note:".yellow().bold()
    );
    println!("Next step: Open ArcMap 10.x — GeoKit Live Bridge will auto-start! Test with `geokit live ping`.");
    Ok(())
}

const LIVE_ADD_EXTS: &[&str] = &["shp", "lyr", "tif", "tiff", "img", "ecw", "jp2"];

/// Fixes common Windows Explorer copy-paste prefixes like
/// `"Lokal Disk (D:)\Users\..."` or `"Local Disk (D:)\Users\..."` -> `"D:\Users\..."`.
fn normalize_explorer_path(raw: &Path) -> PathBuf {
    if raw.exists() {
        return raw.to_path_buf();
    }
    let s = raw.to_string_lossy();
    let trimmed = s.trim();
    if let Some(open) = trimmed.find('(') {
        let rest = &trimmed[open + 1..];
        let bytes = rest.as_bytes();
        if bytes.len() >= 4
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b')'
            && (bytes[3] == b'\\' || bytes[3] == b'/')
        {
            let before = &trimmed[..open];
            if !before.contains('\\') && !before.contains('/') {
                let drive = (bytes[0] as char).to_ascii_uppercase();
                let tail = &rest[3..];
                return PathBuf::from(format!("{drive}:{tail}"));
            }
        }
    }
    PathBuf::from(trimmed)
}

fn is_gdb_dir(path: &Path) -> bool {
    path.is_dir()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("gdb"))
            .unwrap_or(false)
}

/// Returns true if `path` refers to a feature class/raster inside an existing `.gdb` folder
/// (e.g. `D:\data\project.gdb\batas_desa`, which does not exist as a standalone OS file).
fn is_gdb_member_path(path: &Path) -> bool {
    for anc in path.ancestors().skip(1) {
        if is_gdb_dir(anc) {
            return true;
        }
    }
    false
}

fn is_live_addable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| LIVE_ADD_EXTS.iter().any(|x| ext.eq_ignore_ascii_case(x)))
        .unwrap_or(false)
}

/// Resolves user input for `geokit live add`:
/// - Normalizes Windows Explorer drive label prefixes (`Lokal Disk (D:)\...` -> `D:\...`)
/// - Supports `.gdb\feature_class` virtual paths
/// - Auto-completes missing `.shp`/`.tif`/`.lyr` extensions when omitted
/// - Scans directories recursively for addable GIS layers (`.shp`, `.tif`, `.lyr`, etc.)
fn resolve_live_add_targets(raw: &Path) -> Result<Vec<PathBuf>> {
    let path = normalize_explorer_path(raw);
    if path != raw {
        eprintln!(
            "{} Normalized Windows Explorer drive path -> {}",
            "Note:".cyan().bold(),
            path.display()
        );
    }

    if is_gdb_member_path(&path) {
        return Ok(vec![path]);
    }

    if !path.exists() {
        for ext in LIVE_ADD_EXTS {
            let mut candidate = OsString::from(path.as_os_str());
            candidate.push(".");
            candidate.push(ext);
            let cand_path = PathBuf::from(candidate);
            if cand_path.is_file() {
                eprintln!(
                    "{} Auto-resolved missing '.{ext}' extension -> {}",
                    "Note:".cyan().bold(),
                    cand_path.display()
                );
                return Ok(vec![cand_path]);
            }
        }
        bail!(
            "Path does not exist: {}\n\
             Tips:\n\
             - Use a real drive letter (e.g. D:\\... instead of 'Lokal Disk (D:)\\...').\n\
             - Include the file extension (.shp, .tif, .lyr), or pass a folder containing shapefiles/rasters.",
            path.display()
        );
    }

    if is_gdb_dir(&path) {
        bail!(
            "'{}' is a File Geodatabase (.gdb) container, not a single layer.\n\
             Run `geokit gp info \"{}\"` to list layers inside, then add one with:\n\
             geokit live add \"{}\\<layer_name>\"",
            path.display(),
            path.display(),
            path.display()
        );
    }

    if path.is_dir() {
        let mut found: Vec<PathBuf> = Vec::new();
        for entry in WalkDir::new(&path)
            .into_iter()
            .filter_entry(|e| !is_gdb_dir(e.path()))
            .flatten()
        {
            if is_live_addable_file(entry.path()) {
                found.push(entry.into_path());
            }
        }
        found.sort();
        if found.is_empty() {
            bail!(
                "Folder '{}' contains no supported GIS layers (.shp, .tif, .lyr, .img).\n\
                 Pass a direct dataset file path (e.g. path\\layer.shp) or a .gdb feature class (path\\data.gdb\\layer).",
                path.display()
            );
        }
        eprintln!(
            "{} Found {} GIS layer(s) in folder '{}', adding to ArcMap...",
            "Note:".cyan().bold(),
            found.len(),
            path.display()
        );
        return Ok(found);
    }

    Ok(vec![path])
}

pub fn run(cmd: LiveCommand) -> Result<()> {
    let resp = match cmd {
        LiveCommand::InstallAddin { version } => return install_addin(version.as_deref()),
        LiveCommand::Ping { port } => send_live_request(port, &json!({ "action": "ping" }))?,
        LiveCommand::Layers { port } => {
            send_live_request(port, &json!({ "action": "list_layers" }))?
        }
        LiveCommand::LayerInfo { layer, port } => {
            send_live_request(port, &json!({ "action": "layer_info", "layer": layer }))?
        }
        LiveCommand::Add { path, port } => {
            let targets = resolve_live_add_targets(&path)?;
            if targets.len() == 1 {
                send_live_request(
                    port,
                    &json!({ "action": "add_layer", "path": abs_string(&targets[0]) }),
                )?
            } else {
                let mut added = Vec::new();
                let mut errors = Vec::new();
                for t in &targets {
                    match send_live_request(
                        port,
                        &json!({ "action": "add_layer", "path": abs_string(t) }),
                    ) {
                        Ok(r) => added.push(r),
                        Err(e) => errors.push(json!({
                            "path": abs_string(t),
                            "error": e.to_string(),
                        })),
                    }
                }
                if added.is_empty() {
                    bail!(
                        "Failed to add any layers from '{}': {}",
                        path.display(),
                        serde_json::to_string_pretty(&errors)?
                    );
                }
                json!({
                    "ok": errors.is_empty(),
                    "added_count": added.len(),
                    "error_count": errors.len(),
                    "added": added,
                    "errors": errors,
                })
            }
        }
        LiveCommand::Remove { layer, port } => {
            send_live_request(port, &json!({ "action": "remove_layer", "layer": layer }))?
        }
        LiveCommand::Visible { layer, show, port } => send_live_request(
            port,
            &json!({ "action": "set_visibility", "layer": layer, "visible": show }),
        )?,
        LiveCommand::DefinitionQuery {
            layer,
            where_clause,
            port,
        } => send_live_request(
            port,
            &json!({ "action": "set_definition_query", "layer": layer, "where": where_clause }),
        )?,
        LiveCommand::Symbology {
            layer,
            lyr_file,
            port,
        } => {
            if !lyr_file.exists() {
                bail!("Symbology .lyr file does not exist: {}", lyr_file.display());
            }
            send_live_request(
                port,
                &json!({
                    "action": "apply_symbology",
                    "layer": layer,
                    "lyr_file": abs_string(&lyr_file)
                }),
            )?
        }
        LiveCommand::Labels {
            layer,
            show,
            field,
            port,
        } => send_live_request(
            port,
            &json!({ "action": "set_labels", "layer": layer, "show": show, "field": field }),
        )?,
        LiveCommand::Zoom { layer, port } => {
            let target = layer.unwrap_or_else(|| "__full__".to_string());
            send_live_request(port, &json!({ "action": "zoom_to_layer", "layer": target }))?
        }
        LiveCommand::Select {
            layer,
            where_clause,
            port,
        } => send_live_request(
            port,
            &json!({ "action": "select", "layer": layer, "where": where_clause }),
        )?,
        LiveCommand::ClearSelection { layer, port } => send_live_request(
            port,
            &json!({ "action": "clear_selection", "layer": layer }),
        )?,
        LiveCommand::Export { out, dpi, port } => send_live_request(
            port,
            &json!({ "action": "export_view", "out": abs_string(&out), "dpi": dpi }),
        )?,
        LiveCommand::Save { out, port } => send_live_request(
            port,
            &json!({ "action": "save_mxd", "out": out.as_deref().map(abs_string) }),
        )?,
        LiveCommand::Tools { pattern, port } => {
            send_live_request(port, &json!({ "action": "list_tools", "pattern": pattern }))?
        }
        LiveCommand::ToolUsage { tool, port } => {
            send_live_request(port, &json!({ "action": "tool_usage", "tool": tool }))?
        }
        LiveCommand::Style {
            layer,
            field,
            mode,
            preset,
            colors,
            color,
            classes_count,
            break_method,
            outline_color,
            outline_width,
            outline_style,
            width,
            size,
            marker,
            hollow,
            transparency,
            classes,
            port,
        } => send_live_request(
            port,
            &json!({
                "action": "style_layer",
                "layer": layer,
                "field": field,
                "mode": mode,
                "preset": preset,
                "colors": colors,
                "color": color,
                "classes_count": classes_count,
                "break_method": break_method,
                "outline_color": outline_color,
                "outline_width": outline_width,
                "outline_style": outline_style,
                "width": width,
                "size": size,
                "marker": marker,
                "hollow": hollow,
                "transparency": transparency.map(|t| t.min(100)),
                "classes": classes,
            }),
        )?,
        LiveCommand::Order {
            layer,
            position,
            port,
        } => send_live_request(
            port,
            &json!({ "action": "order_layer", "layer": layer, "position": position }),
        )?,
        LiveCommand::Layout {
            view,
            title,
            subtitle,
            source,
            zoom_layer,
            hide_legend,
            port,
        } => send_live_request(
            port,
            &json!({
                "action": "layout_update",
                "view": view,
                "title": title,
                "subtitle": subtitle,
                "source": source,
                "zoom_layer": zoom_layer,
                "hide_legend": hide_legend,
            }),
        )?,
        LiveCommand::RunGp { tool, args, port } => send_live_request(
            port,
            &json!({ "action": "run_gp", "tool": tool, "args": args, "add_to_map": true }),
        )?,
    };

    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_normalize_explorer_drive_prefix() {
        let p = normalize_explorer_path(Path::new(r"Lokal Disk (D:)\Users\acer\PETA\batas.shp"));
        assert_eq!(p, PathBuf::from(r"D:\Users\acer\PETA\batas.shp"));

        let p2 = normalize_explorer_path(Path::new(r"Local Disk (e:)\Data\dem.tif"));
        assert_eq!(p2, PathBuf::from(r"E:\Data\dem.tif"));
    }

    #[test]
    fn test_resolve_live_add_missing_extension_and_folder_and_gdb() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // 1. Missing .shp extension auto-resolution
        let shp = root.join("adm_jateng.shp");
        fs::write(&shp, b"shp").unwrap();
        let no_ext = root.join("adm_jateng");
        let resolved = resolve_live_add_targets(&no_ext).unwrap();
        assert_eq!(resolved, vec![shp.clone()]);

        // 2. Folder auto-discovery
        let sub = root.join("peta_dasar");
        fs::create_dir(&sub).unwrap();
        let sub_shp = sub.join("jalan.shp");
        let sub_tif = sub.join("dem.tif");
        fs::write(&sub_shp, b"shp").unwrap();
        fs::write(&sub_tif, b"tif").unwrap();
        let folder_res = resolve_live_add_targets(&sub).unwrap();
        assert_eq!(folder_res.len(), 2);
        assert!(folder_res.contains(&sub_shp));
        assert!(folder_res.contains(&sub_tif));

        // 3. .gdb container rejected with helpful message, while .gdb\layer passes through
        let gdb = root.join("proyek.gdb");
        fs::create_dir(&gdb).unwrap();
        assert!(resolve_live_add_targets(&gdb).is_err());
        let fc = gdb.join("batas_desa");
        let fc_res = resolve_live_add_targets(&fc).unwrap();
        assert_eq!(fc_res, vec![fc]);
    }
}
