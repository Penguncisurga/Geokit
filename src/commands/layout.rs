use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::config::GeokitConfig;
use crate::engine::{abs_string, is_mxd_target, run_arcmap_job, run_arcpy_job, EngineBackend};

#[derive(Debug, Subcommand)]
pub enum LayoutCommand {
    /// List all layouts inside an .aprx/.mxd file or a directory of projects
    List {
        /// Path to an .aprx, .mxd, or directory
        input: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Batch export layouts from one or more .aprx/.mxd projects to PDF, PNG, or JPG
    Export {
        /// Path to an .aprx, .mxd, or directory of project files
        input: PathBuf,
        /// Output directory for exported maps
        #[arg(short, long)]
        out: PathBuf,
        /// Export format: pdf, png, or jpg (defaults to geokit.toml or pdf)
        #[arg(short, long)]
        format: Option<String>,
        /// Export resolution in DPI (defaults to geokit.toml or 300)
        #[arg(long)]
        dpi: Option<u32>,
        /// Only export layouts whose name contains this text (case-insensitive)
        #[arg(long)]
        filter: Option<String>,
    },
    /// Batch export an ArcGIS Pro (.aprx) Spatial Map Series (e.g. 50 village maps from 1 layout)
    MapSeries {
        /// Path to an .aprx project with a configured Map Series layout
        input: PathBuf,
        /// Output directory (for individual files) or single multi-page PDF path
        #[arg(short, long)]
        out: PathBuf,
        /// Layout name containing the Map Series (defaults to first layout with a Map Series)
        #[arg(long)]
        layout: Option<String>,
        /// Export format: pdf or png (default: pdf)
        #[arg(short, long)]
        format: Option<String>,
        /// Export resolution in DPI (defaults to geokit.toml or 300)
        #[arg(long)]
        dpi: Option<u32>,
        /// Export one separate file per Map Series page instead of a single multi-page PDF
        #[arg(long)]
        multiple_files: bool,
    },
    /// Update layout text elements (title, subtitle, source) and map frame extent in an .aprx or .mxd project
    Update {
        /// Path to an .aprx or .mxd project file
        input: PathBuf,
        /// Layout name inside .aprx (defaults to first layout)
        #[arg(long)]
        layout: Option<String>,
        /// New map title text
        #[arg(long)]
        title: Option<String>,
        /// New subtitle text
        #[arg(long)]
        subtitle: Option<String>,
        /// New data source text
        #[arg(long)]
        source: Option<String>,
        /// Optional layer name to zoom the map frame extent to
        #[arg(long)]
        zoom_layer: Option<String>,
        /// Optional output copy path (saves a copy instead of modifying in-place)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

#[derive(Debug, Default, Serialize)]
struct LayoutPayload {
    action: String,
    input: String,
    out: Option<String>,
    format: Option<String>,
    dpi: Option<u32>,
    filter: Option<String>,
    layout_name: Option<String>,
    multiple_files: Option<bool>,
    title: Option<String>,
    subtitle: Option<String>,
    source: Option<String>,
    zoom_layer: Option<String>,
    json: Option<bool>,
}

pub fn run(
    cmd: LayoutCommand,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    let use_arcmap = match &cmd {
        LayoutCommand::List { input, .. }
        | LayoutCommand::Export { input, .. }
        | LayoutCommand::Update { input, .. } => {
            backend == EngineBackend::Arcmap || is_mxd_target(input)
        }
        LayoutCommand::MapSeries { .. } => false,
    };

    let payload = match cmd {
        LayoutCommand::List { input, json } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            LayoutPayload {
                action: "layout_list".into(),
                input: abs_string(&input),
                json: Some(json),
                ..Default::default()
            }
        }
        LayoutCommand::Export {
            input,
            out,
            format,
            dpi,
            filter,
        } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            let fmt = format
                .unwrap_or_else(|| cfg.defaults.layout_format.clone())
                .to_ascii_lowercase();
            if !matches!(fmt.as_str(), "pdf" | "png" | "jpg" | "jpeg") {
                bail!("Unsupported format '{fmt}'. Use pdf, png, or jpg.");
            }
            let resolution = dpi.unwrap_or(cfg.defaults.dpi);
            LayoutPayload {
                action: "layout_export".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                format: Some(fmt),
                dpi: Some(resolution),
                filter,
                ..Default::default()
            }
        }
        LayoutCommand::MapSeries {
            input,
            out,
            layout,
            format,
            dpi,
            multiple_files,
        } => {
            if !input.is_file() {
                bail!("Input .aprx file does not exist: {}", input.display());
            }
            let fmt = format
                .unwrap_or_else(|| "pdf".to_string())
                .to_ascii_lowercase();
            if !matches!(fmt.as_str(), "pdf" | "png") {
                bail!("Unsupported Map Series format '{fmt}'. Use pdf or png.");
            }
            let resolution = dpi.unwrap_or(cfg.defaults.dpi);
            LayoutPayload {
                action: "layout_map_series".into(),
                input: abs_string(&input),
                out: Some(abs_string(&out)),
                format: Some(fmt),
                dpi: Some(resolution),
                layout_name: layout,
                multiple_files: Some(multiple_files),
                ..Default::default()
            }
        }
        LayoutCommand::Update {
            input,
            layout,
            title,
            subtitle,
            source,
            zoom_layer,
            out,
        } => {
            if !input.is_file() {
                bail!("Input project file does not exist: {}", input.display());
            }
            LayoutPayload {
                action: "layout_update".into(),
                input: abs_string(&input),
                out: out.as_deref().map(abs_string),
                layout_name: layout,
                title,
                subtitle,
                source,
                zoom_layer,
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
