use anyhow::{bail, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
        /// Resume from the last checkpoint state, skipping already-completed steps
        #[arg(long)]
        resume: bool,
    },
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PipelineState {
    pipeline_name: String,
    completed_steps: BTreeSet<usize>,
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
        return matches!(
            a,
            "gp" | "raster" | "hydro" | "qa" | "topo" | "layout" | "map"
        );
    }
    false
}

struct TempDaemonGuard(Option<u16>);

impl Drop for TempDaemonGuard {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = stop_daemon(p);
        }
    }
}

fn state_file_for(pipeline_file: &Path) -> PathBuf {
    let fname = pipeline_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("pipeline.toml");
    pipeline_file.with_file_name(format!("{fname}.state.json"))
}

fn run_pipeline(
    file: &Path,
    dry_run: bool,
    no_hot: bool,
    resume: bool,
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

    let state_path = state_file_for(file);
    let mut state = if resume && state_path.is_file() {
        fs::read_to_string(&state_path)
            .ok()
            .and_then(|s| serde_json::from_str::<PipelineState>(&s).ok())
            .unwrap_or_else(|| PipelineState {
                pipeline_name: pipeline.name.clone(),
                completed_steps: BTreeSet::new(),
            })
    } else {
        PipelineState {
            pipeline_name: pipeline.name.clone(),
            completed_steps: BTreeSet::new(),
        }
    };

    let run_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let log_path = file.with_file_name(format!("pipeline-run-{run_ts}.log"));
    let mut log_fh = if !dry_run {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok()
    } else {
        None
    };
    if let Some(ref mut lf) = log_fh {
        let _ = writeln!(
            lf,
            "=== GeoKit Pipeline Run (ts={run_ts}, resume={resume}): {} ===",
            pipeline.name
        );
    }

    println!(
        "\n{} {} ({} step{})",
        "Pipeline:".bold().cyan(),
        pipeline.name.bold(),
        pipeline.steps.len(),
        if pipeline.steps.len() == 1 { "" } else { "s" }
    );

    // Auto-start a temporary warm ArcPy daemon when running 2+ ArcPy steps so `import arcpy` runs ONCE!
    // Wrapped in RAII Drop guard so the daemon is ALWAYS stopped even on early errors.
    let mut temp_daemon_guard = TempDaemonGuard(None);
    let arcpy_steps = pipeline
        .steps
        .iter()
        .enumerate()
        .filter(|(i, s)| !(resume && state.completed_steps.contains(i)) && is_arcpy_step(&s.args))
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
                    temp_daemon_guard.0 = Some(p);
                }
            }
        }
    }

    let exe = std::env::current_exe().context("Failed to locate geokit executable")?;
    let total = pipeline.steps.len();
    let mut succeeded = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut fatal_err: Option<anyhow::Error> = None;

    for (idx, step) in pipeline.steps.iter().enumerate() {
        let label = format!("[{}/{}]", idx + 1, total);
        if resume && state.completed_steps.contains(&idx) {
            println!("  {} {} {}", label.cyan(), "SKIP".yellow(), step.name);
            if let Some(ref mut lf) = log_fh {
                let _ = writeln!(lf, "{label} SKIP {}", step.name);
            }
            skipped += 1;
            continue;
        }
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
        let step_started = Instant::now();
        let status = Command::new(&exe)
            .args(&step.args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to execute step '{}'", step.name))?;
        let elapsed = step_started.elapsed().as_secs_f64();

        if status.success() {
            println!("{} {} ({:.2}s)", "[OK]".green().bold(), step.name, elapsed);
            if let Some(ref mut lf) = log_fh {
                let _ = writeln!(lf, "{label} OK   {} ({elapsed:.2}s)", step.name);
            }
            succeeded += 1;
            state.completed_steps.insert(idx);
            if let Ok(json_s) = serde_json::to_string_pretty(&state) {
                let _ = fs::write(&state_path, json_s);
            }
        } else {
            failed += 1;
            let msg = format!(
                "Step '{}' failed with exit code {:?}",
                step.name,
                status.code()
            );
            if let Some(ref mut lf) = log_fh {
                let _ = writeln!(lf, "{label} FAIL {msg} ({elapsed:.2}s)");
            }
            if step.continue_on_error {
                eprintln!("{} {} (continuing)", "[WARN]".yellow().bold(), msg);
            } else {
                fatal_err = Some(anyhow::anyhow!(
                    "{msg}\nHint: Fix the issue and re-run with `--resume` to continue from step {}.",
                    idx + 1
                ));
                break;
            }
        }
    }

    drop(temp_daemon_guard);

    if let Some(err) = fatal_err {
        return Err(err);
    }

    if !dry_run && failed == 0 && state_path.is_file() {
        let _ = fs::remove_file(&state_path);
    }

    println!(
        "\n{} {} step(s) {}, {} skipped, {} failed.",
        "Pipeline complete:".bold(),
        succeeded,
        if dry_run { "planned" } else { "succeeded" },
        skipped,
        failed
    );
    if !dry_run {
        println!("Run log -> {}", log_path.display());
    }

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
            resume,
        } => run_pipeline(&file, dry_run, no_hot, resume, override_propy, cfg, backend),
    }
}
