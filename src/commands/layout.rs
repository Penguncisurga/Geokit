use anyhow::{bail, Result};
use clap::Subcommand;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::config::GeokitConfig;
use crate::engine::{abs_string, run_arcpy_job};

#[derive(Debug, Subcommand)]
pub enum LayoutCommand {
    /// List all layouts inside an .aprx file or a directory of .aprx projects
    List {
        /// Path to an .aprx file or directory
        input: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Batch export layouts from one or more .aprx projects to PDF, PNG, or JPG
    Export {
        /// Path to an .aprx file or directory of .aprx files
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
}

#[derive(Debug, Serialize)]
struct LayoutPayload {
    action: String,
    input: String,
    out: Option<String>,
    format: Option<String>,
    dpi: Option<u32>,
    filter: Option<String>,
    json: Option<bool>,
}

pub fn run(cmd: LayoutCommand, override_propy: Option<&Path>, cfg: &GeokitConfig) -> Result<()> {
    let payload = match cmd {
        LayoutCommand::List { input, json } => {
            if !input.exists() {
                bail!("Input path does not exist: {}", input.display());
            }
            LayoutPayload {
                action: "layout_list".into(),
                input: abs_string(&input),
                out: None,
                format: None,
                dpi: None,
                filter: None,
                json: Some(json),
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
                json: None,
            }
        }
    };

    run_arcpy_job(override_propy, &payload)
}
