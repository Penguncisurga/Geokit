use anyhow::{bail, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
    /// Add a shapefile, raster, .lyr, or .gdb feature class live onto the open ArcMap map
    Add {
        /// Dataset path to add
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
    /// Run ANY ArcToolbox tool inside the live ArcMap window (auto-checks out Spatial/3D/Network extensions)
    RunGp {
        /// ArcPy tool name (e.g. Buffer_analysis, Clip_analysis, Slope_sa, Dissolve_management)
        tool: String,
        /// Tool arguments in order
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
            if !path.exists() {
                bail!("Path does not exist: {}", path.display());
            }
            send_live_request(
                port,
                &json!({ "action": "add_layer", "path": abs_string(&path) }),
            )?
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
        LiveCommand::RunGp { tool, args, port } => send_live_request(
            port,
            &json!({ "action": "run_gp", "tool": tool, "args": args, "add_to_map": true }),
        )?,
    };

    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}
