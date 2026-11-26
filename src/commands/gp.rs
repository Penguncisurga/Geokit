use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::config::GeokitConfig;
use crate::engine::gdal::{
    detect_gdal, gdal_clip, gdal_convert, gdal_info, gdal_merge, gdal_pack_gpkg, gdal_project,
    gdal_unpack, gdal_xy2shp,
};
use crate::engine::{
    abs_string, resolve_backend, run_arcmap_job, run_arcpy_job, EngineBackend, ResolvedBackend,
};

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
    /// Discover ANY installed ArcGIS Pro / ArcMap tool dynamically (always up-to-date with new ArcGIS releases!)
    Tools {
        /// Keyword or wildcard filter (e.g. "clip", "*_sa", "zonal")
        #[arg(default_value = "*")]
        pattern: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show exact parameter usage syntax for any installed ArcGIS tool (e.g. Buffer_analysis, Fill_sa)
    ToolUsage {
        /// Exact ArcPy tool name
        tool: String,
    },
    /// Run ANY installed ArcGIS Pro / ArcMap geoprocessing tool dynamically (auto-checks out extensions)
    Run {
        /// Exact ArcPy tool name (e.g. Buffer_analysis, Slope_sa, FeatureToPoint_management)
        tool: String,
        /// Positional arguments passed directly to the ArcPy tool
        args: Vec<String>,
    },
    /// Convert a CSV table with X/Y (or Lon/Lat) columns into a point shapefile, .gpkg, or .gdb feature class
    Xy2shp {
        /// Input CSV file path
        input: PathBuf,
        /// Column name for X / Longitude (default: lon)
        #[arg(long, default_value = "lon")]
        x_field: String,
        /// Column name for Y / Latitude (default: lat)
        #[arg(long, default_value = "lat")]
        y_field: String,
        /// Coordinate system EPSG code (default: 4326 WGS84)
        #[arg(long, default_value_t = 4326)]
        epsg: u32,
        /// Output point shapefile (.shp), .gpkg, or .gdb feature class path
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Batch convert vector dataset(s) to another format: shp, gpkg, geojson, kml, or fgb
    Convert {
        /// Input file, folder, or .gdb
        input: PathBuf,
        /// Target format: shp, gpkg, geojson, kml, or fgb
        #[arg(long)]
        to: String,
        /// Output file or directory
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Inspect unique attribute values, category counts, and numeric/raster statistics (min/max/mean/std)
    Values {
        /// Input shapefile, raster, .gdb feature class, folder, or .gdb
        input: PathBuf,
        /// Specific attribute field name (omit to summarize all fields / raster bands)
        #[arg(long)]
        field: Option<String>,
        /// Maximum unique values returned per field (default: 50)
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Reclassify/score an attribute field into a new score or class field (categories or numeric ranges)
    Classify {
        /// Input shapefile or .gdb feature class
        input: PathBuf,
        /// Source field name to read values from
        #[arg(long)]
        field: String,
        /// Output field name to create/update (e.g. SKOR_LERENG or KELAS_RAWAN)
        #[arg(long)]
        out_field: String,
        /// Semicolon-separated mapping rules, e.g. "Hutan=1;Sawah=3;Permukiman=5" or "<=8:1;8-15:2;>15:3"
        #[arg(long)]
        rules: String,
        /// Output field type: LONG, DOUBLE, or TEXT (auto-detected from rules if omitted)
        #[arg(long)]
        field_type: Option<String>,
        /// Optional fallback value when a row does not match any rule
        #[arg(long)]
        default: Option<String>,
    },
    /// Add/calculate a field using a safe Python math expression (e.g. "0.4 * SKOR_A + 0.6 * SKOR_B")
    CalcField {
        /// Input shapefile or .gdb feature class
        input: PathBuf,
        /// Output field name to create/update
        #[arg(long)]
        field: String,
        /// Expression referencing existing fields (e.g. "0.4 * SKOR_LERENG + 0.3 * SKOR_TANAH + 0.3 * SKOR_HUJAN")
        #[arg(long)]
        expr: String,
        /// Output field type: DOUBLE, LONG, or TEXT (default: DOUBLE)
        #[arg(long, default_value = "DOUBLE")]
        field_type: String,
    },
    /// Overlay (Union) multiple polygon layers or all polygon layers in a folder/.gdb into one dataset
    Union {
        /// Input polygon files (2 or more) OR a single folder / .gdb containing polygon layers
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output union shapefile or .gdb feature class
        #[arg(short, long)]
        out: PathBuf,
    },
}

#[derive(Debug, Default, Serialize)]
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
    tool: Option<String>,
    args: Option<Vec<String>>,
    x_field: Option<String>,
    y_field: Option<String>,
    target_format: Option<String>,
    out_field: Option<String>,
    rules: Option<String>,
    expr: Option<String>,
    field_type: Option<String>,
    default_value: Option<String>,
    limit: Option<usize>,
    inputs: Option<Vec<String>>,
    json: Option<bool>,
}

pub fn run(
    cmd: GpCommand,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    let resolved = resolve_backend(backend, override_propy, cfg.arcgis.arcmap_python.as_deref());
    if resolved == ResolvedBackend::Gdal {
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
            GpCommand::Xy2shp {
                input,
                x_field,
                y_field,
                epsg,
                out,
            } => {
                if !input.is_file() {
                    bail!("Input CSV file does not exist: {}", input.display());
                }
                gdal_xy2shp(&gdal, &input, &x_field, &y_field, epsg, &out)
            }
            GpCommand::Convert { input, to, out } => {
                if !input.exists() {
                    bail!("Input path does not exist: {}", input.display());
                }
                gdal_convert(&gdal, &input, &to, &out)
            }
            _ => bail!(
                "This subcommand currently requires the ArcGIS Pro / ArcMap backend (`--engine arcgis` or `--engine arcmap`).\n\
                 GDAL backend supports: info, project, clip, merge, pack (.gpkg), unpack, xy2shp, convert."
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
                json: Some(json),
                ..Default::default()
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
                ..Default::default()
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
                aoi: Some(abs_string(&aoi)),
                ..Default::default()
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
                ..Default::default()
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
                include_rasters: Some(include_rasters),
                ..Default::default()
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
                ..Default::default()
            }
        }
        GpCommand::Repair { input } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "repair".into(),
                input: abs_string(&input),
                ..Default::default()
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
                dist: Some(dist),
                dissolve: Some(dissolve),
                ..Default::default()
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
                field: Some(field),
                ..Default::default()
            }
        }
        GpCommand::CalcGeom { input, unit, field } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "calc_geom".into(),
                input: abs_string(&input),
                field,
                unit,
                ..Default::default()
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
                overlay: Some(abs_string(&with)),
                ..Default::default()
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
                overlay: Some(abs_string(&mask)),
                ..Default::default()
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
                overlay: Some(abs_string(&join)),
                match_option: Some(match_option),
                ..Default::default()
            }
        }
        GpCommand::Tools { pattern, json } => GpPayload {
            action: "gp_tools".into(),
            input: pattern,
            json: Some(json),
            ..Default::default()
        },
        GpCommand::ToolUsage { tool } => GpPayload {
            action: "gp_tool_usage".into(),
            tool: Some(tool),
            ..Default::default()
        },
        GpCommand::Run { tool, args } => GpPayload {
            action: "gp_run".into(),
            tool: Some(tool),
            args: Some(args),
            ..Default::default()
        },
        GpCommand::Xy2shp {
            input,
            x_field,
            y_field,
            epsg,
            out,
        } => {
            if !input.is_file() {
                bail!("Input CSV file does not exist: {}", input.display());
            }
            GpPayload {
                action: "xy2shp".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                epsg: Some(epsg),
                x_field: Some(x_field),
                y_field: Some(y_field),
                ..Default::default()
            }
        }
        GpCommand::Convert { input, to, out } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            // Prefer GDAL if available for formats like fgb/geojson/gpkg/kml, else route to ArcPy
            let fmt = to.to_ascii_lowercase();
            if matches!(fmt.as_str(), "fgb" | "kml") {
                let gdal = detect_gdal();
                if gdal.is_available() {
                    return gdal_convert(&gdal, &input, &fmt, &out);
                }
            }
            GpPayload {
                action: "convert".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                target_format: Some(fmt),
                ..Default::default()
            }
        }
        GpCommand::Values {
            input,
            field,
            limit,
            json,
        } => {
            let is_gdb_fc = input.ancestors().skip(1).any(|a| {
                a.is_dir()
                    && a.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("gdb"))
                        .unwrap_or(false)
            });
            if !input.exists() && !is_gdb_fc {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "values".into(),
                input: abs_string(&input),
                field,
                limit: Some(limit),
                json: Some(json),
                ..Default::default()
            }
        }
        GpCommand::Classify {
            input,
            field,
            out_field,
            rules,
            field_type,
            default,
        } => {
            let is_gdb_fc = input.ancestors().skip(1).any(|a| {
                a.is_dir()
                    && a.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("gdb"))
                        .unwrap_or(false)
            });
            if !input.exists() && !is_gdb_fc {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "classify".into(),
                input: abs_string(&input),
                field: Some(field),
                out_field: Some(out_field),
                rules: Some(rules),
                field_type,
                default_value: default,
                ..Default::default()
            }
        }
        GpCommand::CalcField {
            input,
            field,
            expr,
            field_type,
        } => {
            let is_gdb_fc = input.ancestors().skip(1).any(|a| {
                a.is_dir()
                    && a.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("gdb"))
                        .unwrap_or(false)
            });
            if !input.exists() && !is_gdb_fc {
                bail!("Input path does not exist: {}", input.display());
            }
            GpPayload {
                action: "calc_field".into(),
                input: abs_string(&input),
                field: Some(field),
                expr: Some(expr),
                field_type: Some(field_type),
                ..Default::default()
            }
        }
        GpCommand::Union { inputs, out } => {
            let mut abs_inputs = Vec::new();
            for p in &inputs {
                let is_gdb_fc = p.ancestors().skip(1).any(|a| {
                    a.is_dir()
                        && a.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.eq_ignore_ascii_case("gdb"))
                            .unwrap_or(false)
                });
                if !p.exists() && !is_gdb_fc {
                    bail!("Input path does not exist: {}", p.display());
                }
                abs_inputs.push(abs_string(p));
            }
            GpPayload {
                action: "union".into(),
                input: abs_inputs[0].clone(),
                inputs: Some(abs_inputs),
                out: Some(abs_string(&out)),
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
