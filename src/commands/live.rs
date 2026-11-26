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
    /// Install the GeoKit Live-UI Python Add-In (.esriaddin) into ArcMap 10.x automatically
    InstallAddin {
        /// ArcMap version folder (e.g. 10.7, 10.8; defaults to auto-detecting installed Desktop10.x)
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
    /// Run an ArcPy geoprocessing tool inside the live ArcMap window and auto-add the output to the map
    RunGp {
        /// ArcPy tool name (e.g. Buffer_analysis, Clip_analysis, Dissolve_management)
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
             1. You ran `geokit live install-addin`.\n\
             2. ArcMap 10.x is open (restarted after installing the add-in)."
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
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

fn detect_arcmap_versions() -> Vec<String> {
    let mut found = Vec::new();
    for ver in [
        "10.8", "10.7", "10.6", "10.5", "10.4", "10.3", "10.2", "10.1",
    ] {
        let p = Path::new(r"C:\Program Files (x86)\ArcGIS").join(format!("Desktop{ver}"));
        if p.is_dir() {
            found.push(ver.to_string());
        }
    }
    found
}

fn resolve_documents_dirs() -> Result<Vec<PathBuf>> {
    let user_profile = std::env::var("USERPROFILE")
        .context("USERPROFILE environment variable not found (Windows required for ArcMap).")?;
    let base = PathBuf::from(&user_profile);
    let mut roots = Vec::new();
    let std_docs = base.join("Documents");
    let od_docs = base.join("OneDrive").join("Documents");
    if od_docs.join("ArcGIS").is_dir() {
        roots.push(od_docs);
    }
    if std_docs.is_dir() {
        roots.push(std_docs);
    }
    if roots.is_empty() {
        roots.push(base.join("Documents"));
    }
    Ok(roots)
}

fn install_addin(version: Option<&str>) -> Result<()> {
    let doc_roots = resolve_documents_dirs()?;

    let versions: Vec<String> = match version {
        Some(v) => vec![v.trim_start_matches("Desktop").to_string()],
        None => {
            let detected = detect_arcmap_versions();
            if detected.is_empty() {
                bail!(
                    "No ArcGIS Desktop 10.x installation detected in C:\\Program Files (x86)\\ArcGIS.\n\
                     Pass `--version 10.7` (or your ArcMap version) explicitly."
                );
            }
            detected
        }
    };

    for doc_root in &doc_roots {
        let addins_root = doc_root.join("ArcGIS").join("AddIns");
        for ver in &versions {
            let target_dir = addins_root.join(format!("Desktop{ver}"));
            std::fs::create_dir_all(&target_dir)?;
            let dst = target_dir.join("geokit-arcmap.esriaddin");
            std::fs::write(&dst, ARCMAP_ADDIN_BYTES)
                .with_context(|| format!("Failed to write {}", dst.display()))?;
            println!(
                "{} Installed GeoKit Live Add-In -> {}",
                "[OK]".green().bold(),
                dst.display()
            );
        }
    }

    println!("\nNext step: Restart ArcMap 10.x once, then test with `geokit live ping`!");
    Ok(())
}

pub fn run(cmd: LiveCommand) -> Result<()> {
    match cmd {
        LiveCommand::InstallAddin { version } => install_addin(version.as_deref()),
        LiveCommand::Ping { port } => {
            let resp = send_live_request(port, &json!({ "action": "ping" }))?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        LiveCommand::Layers { port } => {
            let resp = send_live_request(port, &json!({ "action": "list_layers" }))?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        LiveCommand::Add { path, port } => {
            if !path.exists() {
                bail!("Path does not exist: {}", path.display());
            }
            let resp = send_live_request(
                port,
                &json!({ "action": "add_layer", "path": abs_string(&path) }),
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        LiveCommand::Remove { layer, port } => {
            let resp =
                send_live_request(port, &json!({ "action": "remove_layer", "layer": layer }))?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        LiveCommand::Visible { layer, show, port } => {
            let resp = send_live_request(
                port,
                &json!({ "action": "set_visibility", "layer": layer, "visible": show }),
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        LiveCommand::Zoom { layer, port } => {
            let target = layer.unwrap_or_else(|| "__full__".to_string());
            let resp =
                send_live_request(port, &json!({ "action": "zoom_to_layer", "layer": target }))?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        LiveCommand::Select {
            layer,
            where_clause,
            port,
        } => {
            let resp = send_live_request(
                port,
                &json!({ "action": "select", "layer": layer, "where": where_clause }),
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        LiveCommand::ClearSelection { layer, port } => {
            let resp = send_live_request(
                port,
                &json!({ "action": "clear_selection", "layer": layer }),
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        LiveCommand::Export { out, dpi, port } => {
            let resp = send_live_request(
                port,
                &json!({ "action": "export_view", "out": abs_string(&out), "dpi": dpi }),
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
        LiveCommand::RunGp { tool, args, port } => {
            let resp = send_live_request(
                port,
                &json!({ "action": "run_gp", "tool": tool, "args": args, "add_to_map": true }),
            )?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
    }
}
