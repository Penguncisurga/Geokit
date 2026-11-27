use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::config::GeokitConfig;
use crate::engine::{
    abs_string, resolve_backend, run_arcmap_job, run_arcpy_job, EngineBackend, ResolvedBackend,
};

#[derive(Debug, Subcommand)]
pub enum TopoCommand {
    /// Run BIG topology checks (overlaps, interior gaps, slivers, dangles, duplicates, invalid geometry)
    Check {
        /// Input polygon or polyline shapefile / .gdb feature class
        input: PathBuf,
        /// Optional output directory to export error layers (topo_overlaps.shp, topo_gaps.shp, topo_dangles.shp)
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Minimum sliver polygon area threshold in m2 (default: 100.0)
        #[arg(long, default_value_t = 100.0)]
        min_sliver: f64,
        /// Exit with non-zero status if any topology error is found
        #[arg(long)]
        strict: bool,
        /// Output report as JSON
        #[arg(long)]
        json: bool,
    },
    /// Automatically clean common topology errors (RepairGeometry + remove duplicates + eliminate slivers)
    Fix {
        /// Input shapefile or .gdb feature class
        input: PathBuf,
        /// Output cleaned shapefile or .gdb feature class
        #[arg(short, long)]
        out: PathBuf,
        /// Minimum sliver polygon area threshold in m2 to eliminate (default: 100.0)
        #[arg(long, default_value_t = 100.0)]
        min_sliver: f64,
    },
}

#[derive(Debug, Default, Serialize)]
struct TopoPayload {
    action: String,
    input: String,
    out: Option<String>,
    out_dir: Option<String>,
    min_sliver: Option<f64>,
    strict: Option<bool>,
    json: Option<bool>,
}

fn is_gdb_member(path: &Path) -> bool {
    path.ancestors().skip(1).any(|a| {
        a.is_dir()
            && a.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("gdb"))
                .unwrap_or(false)
    })
}

pub fn run(
    cmd: TopoCommand,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    let resolved = resolve_backend(backend, override_propy, cfg.arcgis.arcmap_python.as_deref());
    if resolved == ResolvedBackend::Gdal {
        bail!(
            "`geokit topo` requires ArcGIS Pro or ArcMap 10.x (`--engine arcgis` or `--engine arcmap`)."
        );
    }

    let payload = match cmd {
        TopoCommand::Check {
            input,
            out_dir,
            min_sliver,
            strict,
            json,
        } => {
            if !input.exists() && !is_gdb_member(&input) {
                bail!("Input path does not exist: {}", input.display());
            }
            TopoPayload {
                action: "topo_check".into(),
                input: abs_string(&input),
                out_dir: out_dir.as_deref().map(abs_string),
                min_sliver: Some(min_sliver.max(0.0)),
                strict: Some(strict),
                json: Some(json),
                ..Default::default()
            }
        }
        TopoCommand::Fix {
            input,
            out,
            min_sliver,
        } => {
            if !input.exists() && !is_gdb_member(&input) {
                bail!("Input path does not exist: {}", input.display());
            }
            TopoPayload {
                action: "topo_fix".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                min_sliver: Some(min_sliver.max(0.0)),
                ..Default::default()
            }
        }
    };

    if resolved == ResolvedBackend::Arcmap {
        run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
    } else {
        run_arcpy_job(override_propy, &payload)
    }
}
