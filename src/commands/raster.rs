use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::config::GeokitConfig;
use crate::engine::gdal::{detect_gdal, gdal_mosaic, gdal_terrain};
use crate::engine::{
    abs_string, resolve_backend, run_arcmap_job, run_arcpy_job, EngineBackend, ResolvedBackend,
};

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
    /// Reclassify a continuous raster into integer classes (e.g. 1-5) with optional MajorityFilter smoothing
    Reclass {
        /// Input raster file (e.g. DEM, slope, rainfall)
        input: PathBuf,
        /// ArcGIS Reclassify remap string, e.g. "-50 25 5;25 100 4;100 300 3;300 650 2;650 3000 1"
        #[arg(long, allow_hyphen_values = true)]
        rules: String,
        /// Output classified GeoTIFF path
        #[arg(short, long)]
        out: PathBuf,
        /// Apply 8-neighbor MajorityFilter smoothing to remove noisy isolated pixels
        #[arg(long)]
        smooth: bool,
    },
    /// Compute weighted overlay sum across multiple scored rasters (e.g. "slope.tif:0.4,rain.tif:0.3,soil.tif:0.3")
    WeightedSum {
        /// Comma-separated list of "<raster_path>:<weight>" pairs
        #[arg(long)]
        inputs: String,
        /// Output weighted score GeoTIFF path
        #[arg(short, long)]
        out: PathBuf,
        /// Optional number of output classes (e.g. 5) to classify & optionally polygonize
        #[arg(long)]
        classes: Option<usize>,
        /// Optional output polygon shapefile path for the classified zones
        #[arg(long)]
        polygon_out: Option<PathBuf>,
        /// Apply MajorityFilter smoothing before polygonizing
        #[arg(long)]
        smooth: bool,
    },
    /// Interpolate point samples (e.g. rainfall stations, water quality) into a continuous raster surface
    Interpolate {
        /// Input point shapefile or .gdb feature class
        input: PathBuf,
        /// Numeric Z/value field to interpolate (e.g. CH_MM, ELEV_M)
        #[arg(long)]
        field: String,
        /// Output interpolated GeoTIFF path
        #[arg(short, long)]
        out: PathBuf,
        /// Interpolation method: idw (default), spline, or kriging
        #[arg(long, default_value = "idw")]
        method: String,
        /// Optional output cell size in map units (e.g. 30 for 30m UTM)
        #[arg(long)]
        cell_size: Option<f64>,
        /// Optional polygon mask / AOI to clip the output extent
        #[arg(long)]
        mask: Option<PathBuf>,
    },
    /// Calculate remote sensing spectral indices (NDVI, NDWI, NDBI, SAVI, NBR) from satellite bands
    Index {
        /// Spectral index type: ndvi, ndwi, ndbi, savi, or nbr
        #[arg(long = "type", value_name = "INDEX")]
        index_type: String,
        /// Band A raster path (NIR for NDVI/SAVI/NBR, Green for NDWI, SWIR1 for NDBI)
        #[arg(long)]
        band_a: PathBuf,
        /// Band B raster path (Red for NDVI/SAVI, NIR for NDWI/NDBI, SWIR2 for NBR)
        #[arg(long)]
        band_b: PathBuf,
        /// Output floating-point spectral index GeoTIFF path
        #[arg(short, long)]
        out: PathBuf,
        /// Optional number of discrete classes (e.g. 5) to classify & optionally export polygon
        #[arg(long)]
        classes: Option<usize>,
        /// Optional classified polygon shapefile path
        #[arg(long)]
        polygon_out: Option<PathBuf>,
    },
    /// Generate topographic contour lines from a DEM with BIG RBI index-contour flag (INDEX_KTR)
    Contour {
        /// Input DEM raster path (.tif, .hgt, .img)
        input: PathBuf,
        /// Contour interval in elevation units (e.g. 12.5, 25, 50)
        #[arg(long, default_value_t = 25.0)]
        interval: f64,
        /// Output contour polyline shapefile or .gdb feature class
        #[arg(short, long)]
        out: PathBuf,
        /// Base contour elevation (default: 0.0)
        #[arg(long, default_value_t = 0.0)]
        base: f64,
        /// Mark every Nth contour as an Index Contour (INDEX_KTR = 1; default: 4 per BIG RBI)
        #[arg(long, default_value_t = 4)]
        index_every: usize,
    },
}

#[derive(Debug, Default, Serialize)]
struct RasterPayload {
    action: String,
    input: String,
    out: String,
    slope: Option<bool>,
    aspect: Option<bool>,
    hillshade: Option<bool>,
    zones: Option<String>,
    zone_field: Option<String>,
    rules: Option<String>,
    smooth: Option<bool>,
    weighted_inputs: Option<Vec<(String, f64)>>,
    classes: Option<usize>,
    polygon_out: Option<String>,
    field: Option<String>,
    method: Option<String>,
    cell_size: Option<f64>,
    mask: Option<String>,
    index_type: Option<String>,
    band_a: Option<String>,
    band_b: Option<String>,
    interval: Option<f64>,
    base_contour: Option<f64>,
    index_every: Option<usize>,
}

pub fn run(
    cmd: RasterCommand,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    let resolved = resolve_backend(backend, override_propy, cfg.arcgis.arcmap_python.as_deref());

    match cmd {
        RasterCommand::Mosaic { input, out } => {
            if !input.exists() {
                bail!("Input directory does not exist: {}", input.display());
            }
            if resolved == ResolvedBackend::Gdal {
                let gdal = detect_gdal();
                return gdal_mosaic(&gdal, &input, &out);
            }
            let payload = RasterPayload {
                action: "raster_mosaic".into(),
                input: abs_string(&input),
                out: abs_string(&out),
                ..Default::default()
            };
            if resolved == ResolvedBackend::Arcmap {
                run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
            } else {
                run_arcpy_job(override_propy, &payload)
            }
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

            if resolved == ResolvedBackend::Gdal {
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
                ..Default::default()
            };
            if resolved == ResolvedBackend::Arcmap {
                run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
            } else {
                run_arcpy_job(override_propy, &payload)
            }
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
                zones: Some(abs_string(&zones)),
                zone_field: Some(zone_field),
                ..Default::default()
            };
            if resolved == ResolvedBackend::Arcmap {
                run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
            } else {
                run_arcpy_job(override_propy, &payload)
            }
        }
        RasterCommand::Reclass {
            input,
            rules,
            out,
            smooth,
        } => {
            if !input.exists() {
                bail!("Input raster does not exist: {}", input.display());
            }
            if resolved == ResolvedBackend::Gdal {
                bail!(
                    "`geokit raster reclass` requires ArcGIS Pro or ArcMap 10.x Spatial Analyst."
                );
            }
            let payload = RasterPayload {
                action: "raster_reclass".into(),
                input: abs_string(&input),
                out: abs_string(&out),
                rules: Some(rules),
                smooth: Some(smooth),
                ..Default::default()
            };
            if resolved == ResolvedBackend::Arcmap {
                run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
            } else {
                run_arcpy_job(override_propy, &payload)
            }
        }
        RasterCommand::WeightedSum {
            inputs,
            out,
            classes,
            polygon_out,
            smooth,
        } => {
            if resolved == ResolvedBackend::Gdal {
                bail!("`geokit raster weighted-sum` requires ArcGIS Pro or ArcMap 10.x Spatial Analyst.");
            }
            let mut pairs = Vec::new();
            for item in inputs
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                let Some(colon) = item.rfind(':') else {
                    bail!("Invalid weighted input '{item}'. Expected '<raster_path>:<weight>'.");
                };
                let p_str = item[..colon].trim();
                let w_str = item[colon + 1..].trim();
                let weight: f64 = w_str
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid numeric weight '{w_str}' in '{item}'"))?;
                let p = PathBuf::from(p_str);
                if !p.exists() {
                    bail!("Weighted raster does not exist: {}", p.display());
                }
                pairs.push((abs_string(&p), weight));
            }
            if pairs.is_empty() {
                bail!("No weighted raster inputs provided.");
            }
            let payload = RasterPayload {
                action: "raster_weighted_sum".into(),
                input: pairs[0].0.clone(),
                out: abs_string(&out),
                weighted_inputs: Some(pairs),
                classes,
                polygon_out: polygon_out.as_deref().map(abs_string),
                smooth: Some(smooth),
                ..Default::default()
            };
            if resolved == ResolvedBackend::Arcmap {
                run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
            } else {
                run_arcpy_job(override_propy, &payload)
            }
        }
        RasterCommand::Interpolate {
            input,
            field,
            out,
            method,
            cell_size,
            mask,
        } => {
            if !input.exists() {
                bail!("Input point dataset does not exist: {}", input.display());
            }
            if resolved == ResolvedBackend::Gdal {
                bail!("`geokit raster interpolate` requires ArcGIS Pro or ArcMap 10.x Spatial Analyst.");
            }
            let payload = RasterPayload {
                action: "raster_interpolate".into(),
                input: abs_string(&input),
                out: abs_string(&out),
                field: Some(field),
                method: Some(method.to_ascii_lowercase()),
                cell_size,
                mask: mask.as_deref().map(abs_string),
                ..Default::default()
            };
            if resolved == ResolvedBackend::Arcmap {
                run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
            } else {
                run_arcpy_job(override_propy, &payload)
            }
        }
        RasterCommand::Index {
            index_type,
            band_a,
            band_b,
            out,
            classes,
            polygon_out,
        } => {
            if !band_a.exists() {
                bail!("Band A raster does not exist: {}", band_a.display());
            }
            if !band_b.exists() {
                bail!("Band B raster does not exist: {}", band_b.display());
            }
            if resolved == ResolvedBackend::Gdal {
                bail!("`geokit raster index` requires ArcGIS Pro or ArcMap 10.x Spatial Analyst.");
            }
            let payload = RasterPayload {
                action: "raster_index".into(),
                input: abs_string(&band_a),
                out: abs_string(&out),
                index_type: Some(index_type.to_ascii_lowercase()),
                band_a: Some(abs_string(&band_a)),
                band_b: Some(abs_string(&band_b)),
                classes,
                polygon_out: polygon_out.as_deref().map(abs_string),
                ..Default::default()
            };
            if resolved == ResolvedBackend::Arcmap {
                run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
            } else {
                run_arcpy_job(override_propy, &payload)
            }
        }
        RasterCommand::Contour {
            input,
            interval,
            out,
            base,
            index_every,
        } => {
            if !input.exists() {
                bail!("Input DEM does not exist: {}", input.display());
            }
            if resolved == ResolvedBackend::Gdal {
                bail!(
                    "`geokit raster contour` requires ArcGIS Pro or ArcMap 10.x Spatial Analyst."
                );
            }
            let payload = RasterPayload {
                action: "raster_contour".into(),
                input: abs_string(&input),
                out: abs_string(&out),
                interval: Some(interval),
                base_contour: Some(base),
                index_every: Some(index_every.max(1)),
                ..Default::default()
            };
            if resolved == ResolvedBackend::Arcmap {
                run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload)
            } else {
                run_arcpy_job(override_propy, &payload)
            }
        }
    }
}
