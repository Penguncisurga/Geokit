use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::engine::{abs_string, run_arcpy_job};

#[derive(Debug, Subcommand)]
pub enum HydroCommand {
    /// End-to-end watershed (DAS) delineation from DEM: Fill -> FlowDir -> FlowAcc -> Streams -> Watershed
    Delineate {
        /// Input DEM raster (recommended in projected CRS such as UTM)
        dem: PathBuf,
        /// Output directory for intermediate rasters and final stream/watershed shapefiles
        #[arg(short, long)]
        out: PathBuf,
        /// Flow accumulation cell threshold for stream extraction (default: 1000)
        #[arg(long, default_value_t = 1000)]
        threshold: u64,
        /// Optional outlet / pour point shapefile or feature class to delineate watershed polygon
        #[arg(long)]
        pour_point: Option<PathBuf>,
        /// Snap distance for pour point in map units (default: 100.0)
        #[arg(long, default_value_t = 100.0)]
        snap_dist: f64,
    },
    /// Delineate all drainage basins across a DEM into a polygon shapefile (no pour point required)
    Basin {
        /// Input DEM raster
        dem: PathBuf,
        /// Output directory for filled DEM, flow direction, and basins.shp
        #[arg(short, long)]
        out: PathBuf,
    },
}

#[derive(Debug, Serialize)]
struct HydroPayload {
    action: String,
    dem: String,
    out: String,
    threshold: Option<u64>,
    pour_point: Option<String>,
    snap_dist: Option<f64>,
}

pub fn run(cmd: HydroCommand, override_propy: Option<&Path>) -> Result<()> {
    let payload = match cmd {
        HydroCommand::Delineate {
            dem,
            out,
            threshold,
            pour_point,
            snap_dist,
        } => {
            if !dem.is_file() {
                bail!("Input DEM file does not exist: {}", dem.display());
            }
            if let Some(ref pp) = pour_point {
                if !pp.exists() {
                    bail!("Pour point file does not exist: {}", pp.display());
                }
            }
            HydroPayload {
                action: "hydro_delineate".into(),
                dem: abs_string(&dem),
                out: abs_string(&out),
                threshold: Some(threshold),
                pour_point: pour_point.as_deref().map(abs_string),
                snap_dist: Some(snap_dist),
            }
        }
        HydroCommand::Basin { dem, out } => {
            if !dem.is_file() {
                bail!("Input DEM file does not exist: {}", dem.display());
            }
            HydroPayload {
                action: "hydro_basin".into(),
                dem: abs_string(&dem),
                out: abs_string(&out),
                threshold: None,
                pour_point: None,
                snap_dist: None,
            }
        }
    };

    run_arcpy_job(override_propy, &payload)
}
