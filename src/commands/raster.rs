use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::engine::gdal::{detect_gdal, gdal_mosaic, gdal_terrain};
use crate::engine::{abs_string, detect_arcgis, run_arcpy_job, EngineBackend};

#[derive(Debug, Subcommand)]
pub enum RasterCommand {
    /// Mosaic multiple raster tiles (e.g. DEMNAS/SRTM) in a folder into a single GeoTIFF
    Mosaic {
        /// Input directory containing raster tiles
        input: PathBuf,
        /// Output mosaic GeoTIFF path
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Generate terrain derivatives (slope, aspect, hillshade) from a DEM
    Terrain {
        /// Input DEM raster file
        input: PathBuf,
        /// Output directory for terrain rasters
        #[arg(short, long)]
        out: PathBuf,
        /// Generate slope (degrees)
        #[arg(long)]
        slope: bool,
        /// Generate aspect
        #[arg(long)]
        aspect: bool,
        /// Generate hillshade
        #[arg(long)]
        hillshade: bool,
        /// Generate all three (slope, aspect, hillshade) — default if none specified
        #[arg(long)]
        all: bool,
    },
    /// Compute zonal statistics (min, max, mean, std, sum) per polygon zone to CSV/table
    ZonalStats {
        /// Input value raster (e.g. DEM, slope, rainfall)
        input: PathBuf,
        /// Zone polygon shapefile or feature class (e.g. sub-watersheds)
        #[arg(long)]
        zones: PathBuf,
        /// Attribute field identifying each zone (e.g. "SUB_DAS" or "DESA")
        #[arg(long)]
        zone_field: String,
        /// Output CSV or DBF table path
        #[arg(short, long)]
        out: PathBuf,
    },
}

#[derive(Debug, Serialize)]
struct RasterPayload {
    action: String,
    input: String,
    out: String,
    slope: Option<bool>,
    aspect: Option<bool>,
    hillshade: Option<bool>,
    zones: Option<String>,
    zone_field: Option<String>,
}

fn should_use_gdal(backend: EngineBackend, override_propy: Option<&Path>) -> bool {
    match backend {
        EngineBackend::Gdal => true,
        EngineBackend::Arcgis | EngineBackend::Arcmap => false,
        EngineBackend::Auto => {
            detect_arcgis(override_propy).arcgis_pro_propy.is_none() && detect_gdal().is_available()
        }
    }
}

pub fn run(
    cmd: RasterCommand,
    override_propy: Option<&Path>,
    backend: EngineBackend,
) -> Result<()> {
    let use_gdal = should_use_gdal(backend, override_propy);

    match cmd {
        RasterCommand::Mosaic { input, out } => {
            if !input.exists() {
                bail!("Input directory does not exist: {}", input.display());
            }
            if use_gdal {
                let gdal = detect_gdal();
                return gdal_mosaic(&gdal, &input, &out);
            }
            let payload = RasterPayload {
                action: "raster_mosaic".into(),
                input: abs_string(&input),
                out: abs_string(&out),
                slope: None,
                aspect: None,
                hillshade: None,
                zones: None,
                zone_field: None,
            };
            run_arcpy_job(override_propy, &payload)
        }
        RasterCommand::Terrain {
            input,
            out,
            slope,
            aspect,
            hillshade,
            all,
        } => {
            if !input.is_file() {
                bail!("Input DEM file does not exist: {}", input.display());
            }
            let none_selected = !slope && !aspect && !hillshade;
            let do_slope = all || none_selected || slope;
            let do_aspect = all || none_selected || aspect;
            let do_hillshade = all || none_selected || hillshade;

            if use_gdal {
                let gdal = detect_gdal();
                return gdal_terrain(&gdal, &input, &out, do_slope, do_aspect, do_hillshade);
            }
            let payload = RasterPayload {
                action: "raster_terrain".into(),
                input: abs_string(&input),
                out: abs_string(&out),
                slope: Some(do_slope),
                aspect: Some(do_aspect),
                hillshade: Some(do_hillshade),
                zones: None,
                zone_field: None,
            };
            run_arcpy_job(override_propy, &payload)
        }
        RasterCommand::ZonalStats {
            input,
            zones,
            zone_field,
            out,
        } => {
            if !input.is_file() {
                bail!("Input raster does not exist: {}", input.display());
            }
            if !zones.exists() {
                bail!("Zones dataset does not exist: {}", zones.display());
            }
            let payload = RasterPayload {
                action: "raster_zonal_stats".into(),
                input: abs_string(&input),
                out: abs_string(&out),
                slope: None,
                aspect: None,
                hillshade: None,
                zones: Some(abs_string(&zones)),
                zone_field: Some(zone_field),
            };
            run_arcpy_job(override_propy, &payload)
        }
    }
}
