use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::config::GeokitConfig;
use crate::engine::{abs_string, is_mxd_target, run_arcmap_job, run_arcpy_job, EngineBackend};

#[derive(Debug, Subcommand)]
pub enum MapCommand {
    /// Detect layers with broken data sources (red exclamation mark) in .aprx or .mxd project(s)
    BrokenLinks {
        /// Path to an .aprx, .mxd, or directory of project files
        input: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Repair broken data source paths in an .aprx or .mxd project by find/replace
    FixLinks {
        /// Path to an .aprx or .mxd file
        input: PathBuf,
        /// Old path prefix or workspace path to find
        #[arg(long)]
        find: String,
        /// New path prefix or workspace path to replace with
        #[arg(long)]
        replace: String,
        /// Optional output copy path (saves a copy instead of modifying in-place)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Add a dataset (.shp, .tif, .gdb feature class) directly into an ArcGIS Pro (.aprx) map and optionally apply a .lyrx style
    AddLayer {
        /// Path to an .aprx project file
        input: PathBuf,
        /// Dataset path to add into the map
        #[arg(long)]
        layer: PathBuf,
        /// Target map name inside the .aprx (defaults to first map)
        #[arg(long)]
        map: Option<String>,
        /// Optional .lyrx symbology file to apply onto the added layer
        #[arg(long)]
        lyrx: Option<PathBuf>,
        /// Optional output .aprx path (saves a copy instead of modifying in-place)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

#[derive(Debug, Default, Serialize)]
struct MapPayload {
    action: String,
    input: String,
    out: Option<String>,
    find: Option<String>,
    replace: Option<String>,
    layer: Option<String>,
    map_name: Option<String>,
    lyrx: Option<String>,
    json: Option<bool>,
}

pub fn run(
    cmd: MapCommand,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    let use_arcmap = match &cmd {
        MapCommand::BrokenLinks { input, .. } | MapCommand::FixLinks { input, .. } => {
            backend == EngineBackend::Arcmap || is_mxd_target(input)
        }
        MapCommand::AddLayer { .. } => false,
    };

    let payload = match cmd {
        MapCommand::BrokenLinks { input, json } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            MapPayload {
                action: "map_broken_links".into(),
                input: abs_string(&input),
                json: Some(json),
                ..Default::default()
            }
        }
        MapCommand::FixLinks {
            input,
            find,
            replace,
            out,
        } => {
            if !input.is_file() {
                bail!("Input project file does not exist: {}", input.display());
            }
            MapPayload {
                action: "map_fix_links".into(),
                input: abs_string(&input),
                out: out.as_deref().map(abs_string),
                find: Some(find),
                replace: Some(replace),
                ..Default::default()
            }
        }
        MapCommand::AddLayer {
            input,
            layer,
            map,
            lyrx,
            out,
        } => {
            if !input.is_file() {
                bail!("Input .aprx file does not exist: {}", input.display());
            }
            if !layer.exists() {
                bail!("Layer dataset does not exist: {}", layer.display());
            }
            if let Some(ref style) = lyrx {
                if !style.exists() {
                    bail!("Symbology .lyrx file does not exist: {}", style.display());
                }
            }
            MapPayload {
                action: "map_add_layer".into(),
                input: abs_string(&input),
                out: out.as_deref().map(abs_string),
                layer: Some(abs_string(&layer)),
                map_name: map,
                lyrx: lyrx.as_deref().map(abs_string),
                ..Default::default()
            }
        }
    };

    if use_arcmap {
        run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
    } else {
        run_arcpy_job(override_propy, &payload)
    }
}
