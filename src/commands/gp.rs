use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::engine::run_arcpy_job;

#[derive(Debug, Subcommand)]
pub enum GpCommand {
    /// Inspect layers, CRS/EPSG, geometry type, feature count, and fields in a dataset, folder, or .gdb
    Info {
        /// Path to a shapefile, raster, folder, or .gdb
        input: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Batch reproject one dataset or an entire folder to a target EPSG code
    Project {
        /// Input file, folder, or .gdb
        input: PathBuf,
        /// Target EPSG code (e.g. 32748 for UTM 48S, 32749 for UTM 49S, 4326 for WGS84)
        #[arg(long)]
        epsg: u32,
        /// Output file, folder, or .gdb
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Batch clip vector and raster datasets against a polygon AOI
    Clip {
        /// Input file, folder, or .gdb
        input: PathBuf,
        /// Area of Interest (AOI) polygon shapefile or feature class
        #[arg(long)]
        aoi: PathBuf,
        /// Output file, folder, or .gdb
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Merge multiple shapefiles or feature classes from a folder or .gdb into one dataset
    Merge {
        /// Input folder or .gdb containing compatible datasets
        input: PathBuf,
        /// Output shapefile or feature class path
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Pack all shapefiles (and optionally rasters) in a folder into a single File Geodatabase (.gdb)
    Pack {
        /// Input folder containing shapefiles/rasters
        input: PathBuf,
        /// Target .gdb path (created automatically if it does not exist)
        #[arg(short, long)]
        out: PathBuf,
        /// Also import rasters into the geodatabase
        #[arg(long)]
        include_rasters: bool,
    },
    /// Unpack all feature classes from a File Geodatabase (.gdb) into shapefiles in a folder
    Unpack {
        /// Input .gdb directory
        input: PathBuf,
        /// Output directory for exported shapefiles
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Batch run Repair Geometry on a dataset, folder, or .gdb
    Repair {
        /// Input shapefile, folder, or .gdb
        input: PathBuf,
    },
    /// Batch buffer vector datasets by a linear distance (e.g. "50 Meters")
    Buffer {
        /// Input file, folder, or .gdb
        input: PathBuf,
        /// Buffer distance string accepted by ArcGIS (e.g. "50 Meters", "1 Kilometers")
        #[arg(long)]
        dist: String,
        /// Output file, folder, or .gdb
        #[arg(short, long)]
        out: PathBuf,
        /// Dissolve overlapping buffer polygons (ALL)
        #[arg(long)]
        dissolve: bool,
    },
    /// Batch dissolve features by one or more attribute fields
    Dissolve {
        /// Input file, folder, or .gdb
        input: PathBuf,
        /// Comma-separated field name(s) to dissolve by (e.g. "DESA" or "KECAMATAN,DESA")
        #[arg(long)]
        field: String,
        /// Output file, folder, or .gdb
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Batch add & calculate geometry attributes (area for polygons, length for lines)
    CalcGeom {
        /// Input file, folder, or .gdb
        input: PathBuf,
        /// Unit: ha, m2, km2 (polygons) or km, m (lines). Defaults to ha for polygons and km for lines.
        #[arg(long)]
        unit: Option<String>,
        /// Custom output field name (defaults to AREA_HA / AREA_M2 / LEN_KM / LEN_M)
        #[arg(long)]
        field: Option<String>,
    },
}

#[derive(Debug, Serialize)]
struct GpPayload {
    action: String,
    input: String,
    out: Option<String>,
    epsg: Option<u32>,
    aoi: Option<String>,
    dist: Option<String>,
    dissolve: Option<bool>,
    field: Option<String>,
    unit: Option<String>,
    include_rasters: Option<bool>,
    json: Option<bool>,
}

fn abs_string(p: &Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| {
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(p))
                    .unwrap_or_else(|_| p.to_path_buf())
            }
        })
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string()
}

pub fn run(cmd: GpCommand, override_propy: Option<&Path>) -> Result<()> {
    let payload = match cmd {
        GpCommand::Info { input, json } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "info".into(),
                input: abs_string(&input),
                out: None,
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                json: Some(json),
            }
        }
        GpCommand::Project { input, epsg, out } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "project".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: Some(epsg),
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                json: None,
            }
        }
        GpCommand::Clip { input, aoi, out } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            if !aoi.exists() {
                bail!("AOI path does not exist: {}", aoi.display());
            }
            GpPayload {
                action: "clip".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: None,
                aoi: Some(abs_string(&aoi)),
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                json: None,
            }
        }
        GpCommand::Merge { input, out } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "merge".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                json: None,
            }
        }
        GpCommand::Pack {
            input,
            out,
            include_rasters,
        } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "pack".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: Some(include_rasters),
                json: None,
            }
        }
        GpCommand::Unpack { input, out } => {
            if !input.exists() {
                bail!("Input .gdb does not exist: {}", input.display());
            }
            GpPayload {
                action: "unpack".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                json: None,
            }
        }
        GpCommand::Repair { input } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "repair".into(),
                input: abs_string(&input),
                out: None,
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                json: None,
            }
        }
        GpCommand::Buffer {
            input,
            dist,
            out,
            dissolve,
        } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "buffer".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: None,
                aoi: None,
                dist: Some(dist),
                dissolve: Some(dissolve),
                field: None,
                unit: None,
                include_rasters: None,
                json: None,
            }
        }
        GpCommand::Dissolve { input, field, out } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "dissolve".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field: Some(field),
                unit: None,
                include_rasters: None,
                json: None,
            }
        }
        GpCommand::CalcGeom { input, unit, field } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "calc_geom".into(),
                input: abs_string(&input),
                out: None,
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field,
                unit,
                include_rasters: None,
                json: None,
            }
        }
    };

    run_arcpy_job(override_propy, &payload)
}
