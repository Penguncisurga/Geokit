use anyhow::{bail, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::GeokitConfig;
use crate::engine::{
    arcmap_daemon_port, arcpy_daemon_port, ping_daemon, resolve_backend, start_daemon, stop_daemon,
    EngineBackend, ResolvedBackend,
};

#[derive(Debug, Subcommand)]
pub enum PipelineCommand {
    /// Generate a starter declarative pipeline.toml recipe (text-based ModelBuilder for GIS)
    Init {
        /// Output TOML path (defaults to ./pipeline.toml)
        #[arg(short, long, default_value = "pipeline.toml")]
        out: PathBuf,
        /// Overwrite existing file
        #[arg(long)]
        force: bool,
    },
    /// Execute a multi-step GIS pipeline defined in a TOML file (auto-warms a 1-time ArcPy daemon when 2+ ArcPy steps are present!)
    Run {
        /// Path to pipeline.toml
        file: PathBuf,
        /// Preview steps without executing them
        #[arg(long)]
        dry_run: bool,
        /// Disable automatic temporary warm ArcPy daemon for multi-step pipelines
        #[arg(long)]
        no_hot: bool,
    },
}

#[derive(Debug, Deserialize)]
struct PipelineFile {
    #[serde(default = "default_pipeline_name")]
    name: String,
    #[serde(default)]
    steps: Vec<PipelineStep>,
}

fn default_pipeline_name() -> String {
    "geokit-pipeline".to_string()
}

#[derive(Debug, Deserialize)]
struct PipelineStep {
    name: String,
    args: Vec<String>,
    #[serde(default)]
    continue_on_error: bool,
}

fn starter_pipeline_toml() -> &'static str {
    r#"# Declarative GIS Pipeline for geokit (text-based ModelBuilder!)
# Run with: geokit pipeline run pipeline.toml [--dry-run]

name = "Standard GIS Data Prep & DAS Workflow"

[[steps]]
name = "1. Audit workspace"
args = ["audit", "."]

[[steps]]
name = "2. Sanitize filenames (dry-run preview)"
args = ["sanitize", ".", "--dry-run"]

[[steps]]
name = "3. Clean stale lock files (dry-run preview)"
args = ["clean", ".", "--dry-run"]

# Example ArcGIS Pro / ArcMap / GDAL steps (uncomment and adjust paths):
# [[steps]]
# name = "4. Reproject vector folder to UTM 48S"
# args = ["gp", "project", "./01_Vector", "--epsg", "32748", "--out", "./01_Vector_UTM"]
#
# [[steps]]
# name = "5. Delineate watersheds from DEM"
# args = ["hydro", "basin", "./02_Raster/dem_utm.tif", "--out", "./05_Hydro"]
#
# [[steps]]
# name = "6. Export all .aprx layouts to 300 DPI PDF"
# args = ["layout", "export", "./04_Projects", "--out", "./exports", "--dpi", "300"]
"#
}

fn is_arcpy_step(args: &[String]) -> bool {
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--engine" || a == "--propy" {
            i += 2;
            continue;
        }
        if a.starts_with('-') {
            i += 1;
            continue;
        }
        return matches!(a, "gp" | "raster" | "hydro" | "qa" | "layout" | "map");
    }
    false
}

fn run_pipeline(
    file: &Path,
    dry_run: bool,
    no_hot: bool,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    if !file.is_file() {
        bail!("Pipeline file does not exist: {}", file.display());
    }
    let raw =
        fs::read_to_string(file).with_context(|| format!("Failed to read {}", file.display()))?;
    let pipeline: PipelineFile =
        toml::from_str(&raw).with_context(|| format!("Invalid TOML in {}", file.display()))?;

    if pipeline.steps.is_empty() {
        println!("{}", "Pipeline has no steps.".yellow());
        return Ok(());
    }

    println!(
        "\n{} {} ({} step{})",
        "Pipeline:".bold().cyan(),
        pipeline.name.bold(),
        pipeline.steps.len(),
        if pipeline.steps.len() == 1 { "" } else { "s" }
    );

    // Auto-start a temporary warm ArcPy daemon when running 2+ ArcPy steps so `import arcpy` runs ONCE!
    let mut temp_daemon_port: Option<u16> = None;
    let arcpy_steps = pipeline
        .steps
        .iter()
        .filter(|s| is_arcpy_step(&s.args))
        .count();
    if !dry_run && !no_hot && arcpy_steps >= 2 {
        let resolved =
            resolve_backend(backend, override_propy, cfg.arcgis.arcmap_python.as_deref());
        let port = match resolved {
            ResolvedBackend::Arcmap => Some(arcmap_daemon_port()),
            ResolvedBackend::Arcgis => Some(arcpy_daemon_port()),
            ResolvedBackend::Gdal => None,
        };
        if let Some(p) = port {
            if ping_daemon(p).is_none() {
                println!(
                    "{} Auto-warming single-startup ArcPy daemon for {} GIS steps...",
                    "[hot-pipeline]".cyan().bold(),
                    arcpy_steps
                );
                if start_daemon(
                    resolved,
                    override_propy,
                    cfg.arcgis.arcmap_python.as_deref(),
                    15,
                )
                .is_ok()
                {
                    temp_daemon_port = Some(p);
                }
            }
        }
    }

    let exe = std::env::current_exe().context("Failed to locate geokit executable")?;
    let total = pipeline.steps.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut fatal_err: Option<anyhow::Error> = None;

    for (idx, step) in pipeline.steps.iter().enumerate() {
        let label = format!("[{}/{}]", idx + 1, total);
        if dry_run {
            println!(
                "  {} {} {} -> geokit {}",
                label.cyan(),
                "PLAN".cyan(),
                step.name,
                step.args.join(" ")
            );
            succeeded += 1;
            continue;
        }

        println!("\n{} {}", label.cyan().bold(), step.name.bold());
        let status = Command::new(&exe)
            .args(&step.args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to execute step '{}'", step.name))?;

        if status.success() {
            println!("{} {}", "[OK]".green().bold(), step.name);
            succeeded += 1;
        } else {
            failed += 1;
            let msg = format!(
                "Step '{}' failed with exit code {:?}",
                step.name,
                status.code()
            );
            if step.continue_on_error {
                eprintln!("{} {} (continuing)", "[WARN]".yellow().bold(), msg);
            } else {
                fatal_err = Some(anyhow::anyhow!("{msg}"));
                break;
            }
        }
    }

    if let Some(p) = temp_daemon_port {
        let _ = stop_daemon(p);
    }

    if let Some(err) = fatal_err {
        return Err(err);
    }

    println!(
        "\n{} {} step(s) {}, {} failed.",
        "Pipeline complete:".bold(),
        succeeded,
        if dry_run { "planned" } else { "succeeded" },
        failed
    );

    if failed > 0 {
        bail!("{failed} pipeline step(s) failed.");
    }
    Ok(())
}

pub fn run(
    cmd: PipelineCommand,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    match cmd {
        PipelineCommand::Init { out, force } => {
            if out.exists() && !force {
                bail!(
                    "{} already exists. Use `--force` to overwrite.",
                    out.display()
                );
            }
            if let Some(parent) = out.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(&out, starter_pipeline_toml())?;
            println!("{} Created {}", "[OK]".green().bold(), out.display());
            Ok(())
        }
        PipelineCommand::Run {
            file,
            dry_run,
            no_hot,
        } => run_pipeline(&file, dry_run, no_hot, override_propy, cfg, backend),
    }
}
