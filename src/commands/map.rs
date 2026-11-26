use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::config::GeokitConfig;
use crate::engine::{abs_string, is_mxd_target, run_arcmap_job, run_arcpy_job, EngineBackend};

#[derive(Debug, Subcommand)]
pub enum MapCommand {
    /// Detect layers with broken data sources (red exclamation mark) in .aprx project(s)
    BrokenLinks {
        /// Path to an .aprx file or directory of .aprx files
        input: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Repair broken data source paths in an .aprx project by find/replace
    FixLinks {
        /// Path to an .aprx file
        input: PathBuf,
        /// Old path prefix or workspace path to find
        #[arg(long)]
        find: String,
        /// New path prefix or workspace path to replace with
        #[arg(long)]
        replace: String,
        /// Optional output .aprx path (saves a copy instead of modifying in-place)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

#[derive(Debug, Serialize)]
struct MapPayload {
    action: String,
    input: String,
    out: Option<String>,
    find: Option<String>,
    replace: Option<String>,
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
    };

    let payload = match cmd {
        MapCommand::BrokenLinks { input, json } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            MapPayload {
                action: "map_broken_links".into(),
                input: abs_string(&input),
                out: None,
                find: None,
                replace: None,
                json: Some(json),
            }
        }
        MapCommand::FixLinks {
            input,
            find,
            replace,
            out,
        } => {
            if !input.is_file() {
                bail!("Input .aprx file does not exist: {}", input.display());
            }
            MapPayload {
                action: "map_fix_links".into(),
                input: abs_string(&input),
                out: out.as_deref().map(abs_string),
                find: Some(find),
                replace: Some(replace),
                json: None,
            }
        }
    };

    if use_arcmap {
        run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
    } else {
        run_arcpy_job(override_propy, &payload)
    }
}
