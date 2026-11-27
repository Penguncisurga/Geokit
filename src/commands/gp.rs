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
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
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
    /// Generate a ready-to-publish class area/count summary table (Markdown, CSV, HTML, or JSON) — 100% native Rust, zero ArcGIS needed!
    Summary {
        /// Input shapefile (.shp) or GeoJSON (.geojson)
        input: PathBuf,
        /// Attribute field to group by (e.g. KELAS, TUTUPAN, KECAMATAN)
        #[arg(long)]
        group_by: String,
        /// Optional numeric area/value field to sum (auto-detects AREA_HA / LUAS_HA / AREA_KM2 if omitted)
        #[arg(long)]
        area_field: Option<String>,
        /// Optional output file path (.md, .csv, .html, or .json)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Print JSON summary to stdout
        #[arg(long)]
        json: bool,
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
    // 100% native Rust summary runs in milliseconds without ArcGIS or GDAL!
    if let GpCommand::Summary {
        input,
        group_by,
        area_field,
        out,
        json: as_json,
    } = cmd
    {
        return run_native_summary(
            &input,
            &group_by,
            area_field.as_deref(),
            out.as_deref(),
            as_json,
        );
    }

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
        GpCommand::Summary { .. } => unreachable!(),
    };

    if resolved == ResolvedBackend::Arcmap {
        run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
    } else {
        run_arcpy_job(override_propy, &payload)
    }
}

fn run_native_summary(
    input: &Path,
    group_by: &str,
    area_field: Option<&str>,
    out: Option<&Path>,
    as_json: bool,
) -> Result<()> {
    use crate::gis::shp::read_dbf_records;
    use comfy_table::{presets::UTF8_FULL, Table};
    use serde_json::{json, Map, Value};
    use std::collections::BTreeMap;

    if !input.is_file() {
        bail!("Input dataset does not exist: {}", input.display());
    }
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let records: Vec<Map<String, Value>> = if ext == "shp" || ext == "dbf" {
        read_dbf_records(&input.with_extension("dbf"))?
    } else if ext == "geojson" || ext == "json" {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(input)?)?;
        v.get("features")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| f.get("properties").and_then(|p| p.as_object()).cloned())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        bail!("`geokit gp summary` supports .shp, .dbf, or .geojson files.");
    };

    if records.is_empty() {
        bail!("No records found in {}", input.display());
    }

    let first_keys: Vec<String> = records[0].keys().cloned().collect();
    let grp_key = first_keys
        .iter()
        .find(|k| k.eq_ignore_ascii_case(group_by))
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Field '{}' not found. Available fields: {}",
                group_by,
                first_keys.join(", ")
            )
        })?;

    let area_key = if let Some(af) = area_field {
        first_keys
            .iter()
            .find(|k| k.eq_ignore_ascii_case(af))
            .cloned()
    } else {
        ["AREA_HA", "LUAS_HA", "AREA_KM2", "AREA_M2", "LUAS", "AREA"]
            .iter()
            .find_map(|cand| {
                first_keys
                    .iter()
                    .find(|k| k.eq_ignore_ascii_case(cand))
                    .cloned()
            })
    };

    let mut groups: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for rec in &records {
        let label = match rec.get(&grp_key) {
            Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            Some(Value::Number(n)) => n.to_string(),
            _ => "(Kosong)".to_string(),
        };
        let area = area_key
            .as_ref()
            .and_then(|ak| rec.get(ak))
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
            })
            .unwrap_or(0.0);
        let entry = groups.entry(label).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += area;
    }

    let total_count: usize = groups.values().map(|(c, _)| *c).sum();
    let total_area: f64 = groups.values().map(|(_, a)| *a).sum();
    let has_area = area_key.is_some() && total_area > 0.0;
    let area_col_name = area_key.as_deref().unwrap_or("AREA");

    let mut rows_json = Vec::new();
    for (k, (cnt, area)) in &groups {
        let pct = if has_area {
            (*area / total_area) * 100.0
        } else if total_count > 0 {
            (*cnt as f64 / total_count as f64) * 100.0
        } else {
            0.0
        };
        rows_json.push(json!({
            "class": k,
            "count": cnt,
            "area": (area * 100.0).round() / 100.0,
            "percent": (pct * 100.0).round() / 100.0,
        }));
    }

    let summary_obj = json!({
        "dataset": input.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "group_by": grp_key,
        "area_field": area_key,
        "total_features": total_count,
        "total_area": (total_area * 100.0).round() / 100.0,
        "classes": rows_json,
    });

    if let Some(out_path) = out {
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let oext = out_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("md")
            .to_ascii_lowercase();
        let content = if oext == "json" {
            serde_json::to_string_pretty(&summary_obj)?
        } else if oext == "csv" {
            let mut s = format!("{grp_key},Jumlah_Fitur,{area_col_name},Persen\n");
            for (k, (cnt, area)) in &groups {
                let pct = if has_area {
                    (*area / total_area) * 100.0
                } else {
                    (*cnt as f64 / total_count as f64) * 100.0
                };
                s.push_str(&format!(
                    "\"{}\",{},{:.2},{:.2}\n",
                    k.replace('"', "\"\""),
                    cnt,
                    area,
                    pct
                ));
            }
            s.push_str(&format!(
                "\"TOTAL\",{},{:.2},100.00\n",
                total_count, total_area
            ));
            s
        } else if oext == "html" {
            let esc = |s: &str| {
                s.replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;")
                    .replace('\'', "&#39;")
            };
            let grp_esc = esc(&grp_key);
            let area_esc = esc(area_col_name);
            let mut trs = String::new();
            for (k, (cnt, area)) in &groups {
                let pct = if has_area {
                    (*area / total_area) * 100.0
                } else {
                    (*cnt as f64 / total_count as f64) * 100.0
                };
                trs.push_str(&format!(
                    "<tr><td>{}</td><td style='text-align:right'>{}</td><td style='text-align:right'>{:.2}</td><td style='text-align:right'>{:.2}%</td></tr>\n",
                    esc(k),
                    cnt,
                    area,
                    pct
                ));
            }
            format!(
                "<!doctype html><html><head><meta charset='utf-8'><title>Rekap {grp_esc}</title><style>body{{font-family:Segoe UI,Arial,sans-serif;max-width:760px;margin:2rem auto;color:#1e293b}}table{{width:100%;border-collapse:collapse}}th,td{{border:1px solid #cbd5e1;padding:8px 12px}}th{{background:#0f172a;color:#fff}}tfoot td{{font-weight:700;background:#f1f5f9}}</style></head><body><h2>Rekapitulasi Kelas ({grp_esc})</h2><table><thead><tr><th>{grp_esc}</th><th>Fitur</th><th>{area_esc}</th><th>Persentase</th></tr></thead><tbody>{trs}</tbody><tfoot><tr><td>TOTAL</td><td style='text-align:right'>{total_count}</td><td style='text-align:right'>{total_area:.2}</td><td style='text-align:right'>100.00%</td></tr></tfoot></table></body></html>"
            )
        } else {
            let mut s = format!("| {grp_key} | Fitur | {area_col_name} | Persentase (%) |\n| :--- | ---: | ---: | ---: |\n");
            for (k, (cnt, area)) in &groups {
                let pct = if has_area {
                    (*area / total_area) * 100.0
                } else {
                    (*cnt as f64 / total_count as f64) * 100.0
                };
                s.push_str(&format!("| {k} | {cnt} | {area:.2} | {pct:.2}% |\n"));
            }
            s.push_str(&format!(
                "| **TOTAL** | **{total_count}** | **{total_area:.2}** | **100.00%** |\n"
            ));
            s
        };
        std::fs::write(out_path, content)?;
        if !as_json {
            println!("OK Exported summary table -> {}", out_path.display());
        }
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&summary_obj)?);
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![grp_key.as_str(), "Fitur", area_col_name, "Persen (%)"]);
    for (k, (cnt, area)) in &groups {
        let pct = if has_area {
            (*area / total_area) * 100.0
        } else {
            (*cnt as f64 / total_count as f64) * 100.0
        };
        table.add_row(vec![
            k.clone(),
            cnt.to_string(),
            format!("{area:.2}"),
            format!("{pct:.2}%"),
        ]);
    }
    table.add_row(vec![
        "TOTAL".into(),
        total_count.to_string(),
        format!("{total_area:.2}"),
        "100.00%".into(),
    ]);
    println!("{table}");
    Ok(())
}
