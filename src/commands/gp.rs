use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::config::GeokitConfig;
use crate::engine::gdal::{
    detect_gdal, gdal_clip, gdal_info, gdal_merge, gdal_pack_gpkg, gdal_project, gdal_unpack,
};
use crate::engine::{abs_string, detect_arcgis, run_arcmap_job, run_arcpy_job, EngineBackend};

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
        /// Target EPSG code (e.g. 32748 for UTM 48S; optional if set in geokit.toml)
        #[arg(long)]
        epsg: Option<u32>,
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
    /// Pack all shapefiles (and optionally rasters) in a folder into a single .gdb (ArcGIS) or .gpkg (GDAL)
    Pack {
        /// Input folder containing shapefiles/rasters
        input: PathBuf,
        /// Target .gdb (or .gpkg with --engine gdal) path
        #[arg(short, long)]
        out: PathBuf,
        /// Also import rasters into the geodatabase (ArcGIS engine)
        #[arg(long)]
        include_rasters: bool,
    },
    /// Unpack all feature classes from a .gdb or .gpkg into shapefiles in a folder
    Unpack {
        /// Input .gdb or .gpkg path
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
    /// Batch intersect vector dataset(s) with an overlay layer
    Intersect {
        /// Input file, folder, or .gdb
        input: PathBuf,
        /// Overlay shapefile or feature class to intersect with
        #[arg(long)]
        with: PathBuf,
        /// Output file, folder, or .gdb
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Batch erase features overlapping an erase/mask layer
    Erase {
        /// Input file, folder, or .gdb
        input: PathBuf,
        /// Mask/erase shapefile or feature class
        #[arg(long)]
        mask: PathBuf,
        /// Output file, folder, or .gdb
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Batch spatial join attributes from a join layer onto target dataset(s)
    SpatialJoin {
        /// Target file, folder, or .gdb
        input: PathBuf,
        /// Join shapefile or feature class providing attributes
        #[arg(long)]
        join: PathBuf,
        /// Output file, folder, or .gdb
        #[arg(short, long)]
        out: PathBuf,
        /// Match option: intersect, within, contains, or closest
        #[arg(long = "match", default_value = "intersect")]
        match_option: String,
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
    overlay: Option<String>,
    match_option: Option<String>,
    json: Option<bool>,
}

fn should_use_gdal(backend: EngineBackend, override_propy: Option<&Path>) -> bool {
    match backend {
        EngineBackend::Gdal => true,
        EngineBackend::Arcgis | EngineBackend::Arcmap => false,
        EngineBackend::Auto => {
            let arc = detect_arcgis(override_propy);
            arc.arcgis_pro_propy.is_none() && detect_gdal().is_available()
        }
    }
}

pub fn run(
    cmd: GpCommand,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    if should_use_gdal(backend, override_propy) {
        let gdal = detect_gdal();
        if !gdal.is_available() {
            bail!(
                "GDAL (ogr2ogr) was not found. Install GDAL/QGIS or switch to `--engine arcgis`."
            );
        }
        return match cmd {
            GpCommand::Info { input, .. } => {
                if !input.exists() {
                    bail!("Input path does not exist: {}", input.display());
                }
                gdal_info(&gdal, &input)
            }
            GpCommand::Project { input, epsg, out } => {
                if !input.exists() {
                    bail!("Input path does not exist: {}", input.display());
                }
                let target_epsg = epsg.or(cfg.defaults.epsg).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Missing --epsg code (and no `defaults.epsg` set in geokit.toml)."
                    )
                })?;
                gdal_project(&gdal, &input, target_epsg, &out)
            }
            GpCommand::Clip { input, aoi, out } => {
                if !input.exists() {
                    bail!("Input path does not exist: {}", input.display());
                }
                if !aoi.exists() {
                    bail!("AOI path does not exist: {}", aoi.display());
                }
                gdal_clip(&gdal, &input, &aoi, &out)
            }
            GpCommand::Merge { input, out } => {
                if !input.exists() {
                    bail!("Input path does not exist: {}", input.display());
                }
                gdal_merge(&gdal, &input, &out)
            }
            GpCommand::Pack { input, out, .. } => {
                if !input.exists() {
                    bail!("Input path does not exist: {}", input.display());
                }
                gdal_pack_gpkg(&gdal, &input, &out)
            }
            GpCommand::Unpack { input, out } => {
                if !input.exists() {
                    bail!("Input path does not exist: {}", input.display());
                }
                gdal_unpack(&gdal, &input, &out)
            }
            _ => bail!(
                "This subcommand currently requires the ArcGIS Pro backend (`--engine arcgis`).\n\
                 GDAL backend supports: info, project, clip, merge, pack (.gpkg), unpack."
            ),
        };
    }

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
                overlay: None,
                match_option: None,
                json: Some(json),
            }
        }
        GpCommand::Project { input, epsg, out } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            let target_epsg = epsg.or(cfg.defaults.epsg).ok_or_else(|| {
                anyhow::anyhow!("Missing --epsg code (and no `defaults.epsg` set in geokit.toml).")
            })?;
            GpPayload {
                action: "project".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: Some(target_epsg),
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                overlay: None,
                match_option: None,
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
                overlay: None,
                match_option: None,
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
                overlay: None,
                match_option: None,
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
                overlay: None,
                match_option: None,
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
                overlay: None,
                match_option: None,
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
                overlay: None,
                match_option: None,
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
                overlay: None,
                match_option: None,
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
                overlay: None,
                match_option: None,
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
                overlay: None,
                match_option: None,
                json: None,
            }
        }
        GpCommand::Intersect { input, with, out } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            if !with.exists() {
                bail!("Overlay path does not exist: {}", with.display());
            }
            GpPayload {
                action: "intersect".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                overlay: Some(abs_string(&with)),
                match_option: None,
                json: None,
            }
        }
        GpCommand::Erase { input, mask, out } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            if !mask.exists() {
                bail!("Mask path does not exist: {}", mask.display());
            }
            GpPayload {
                action: "erase".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                overlay: Some(abs_string(&mask)),
                match_option: None,
                json: None,
            }
        }
        GpCommand::SpatialJoin {
            input,
            join,
            out,
            match_option,
        } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            if !join.exists() {
                bail!("Join layer path does not exist: {}", join.display());
            }
            GpPayload {
                action: "spatial_join".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: None,
                aoi: None,
                dist: None,
                dissolve: None,
                field: None,
                unit: None,
                include_rasters: None,
                overlay: Some(abs_string(&join)),
                match_option: Some(match_option),
                json: None,
            }
        }
    };

    if backend == EngineBackend::Arcmap {
        run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
    } else {
        run_arcpy_job(override_propy, &payload)
    }
}
