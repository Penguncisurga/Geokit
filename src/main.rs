mod commands;
mod engine;
mod gis;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use commands::gp::GpCommand;

#[derive(Debug, Parser)]
#[command(
    name = "geokit",
    version,
    about = "Fast GIS CLI toolkit for spatial data hygiene and batch ArcGIS Pro geoprocessing",
    long_about = None
)]
struct Cli {
    /// Custom path to ArcGIS Pro propy.bat (overrides auto-detection and GEOKIT_PROPY)
    #[arg(long, global = true, env = "GEOKIT_PROPY")]
    propy: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Check environment health and detected ArcGIS installations
    Doctor {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Audit a GIS workspace for datasets, broken shapefiles, naming issues, and clutter
    Audit {
        /// Directory to scan (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Organize scattered GIS files into structured folders (keeps shapefile & raster bundles intact)
    Organize {
        /// Directory to organize
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Target output root directory (defaults to organizing in-place)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Preview changes without moving any files
        #[arg(long)]
        dry_run: bool,
    },
    /// Sanitize GIS filenames to ArcGIS-safe snake_case (renames entire shapefile & raster bundles safely)
    Sanitize {
        /// Directory to sanitize
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Preview changes without renaming any files
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove stale .lock and temporary files from a GIS workspace
    Clean {
        /// Directory to clean
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Also remove .shp.xml and .aux.xml metadata sidecars
        #[arg(long)]
        include_xml: bool,
        /// Preview files that would be deleted
        #[arg(long)]
        dry_run: bool,
    },
    /// Batch geoprocessing powered by ArcGIS Pro (single-startup arcpy session)
    Gp {
        #[command(subcommand)]
        command: GpCommand,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Doctor { json } => commands::doctor::run(cli.propy.as_deref(), json),
        Commands::Audit { path, json } => commands::audit::run(&path, json),
        Commands::Organize {
            path,
            output,
            dry_run,
        } => commands::organize::run(&path, output.as_deref(), dry_run),
        Commands::Sanitize { path, dry_run } => commands::sanitize::run(&path, dry_run),
        Commands::Clean {
            path,
            include_xml,
            dry_run,
        } => commands::clean::run(&path, include_xml, dry_run),
        Commands::Gp { command } => commands::gp::run(command, cli.propy.as_deref()),
    }
}
