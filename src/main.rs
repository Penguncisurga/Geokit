mod commands;
mod config;
mod engine;
mod gis;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use commands::gp::GpCommand;
use commands::hydro::HydroCommand;
use commands::layout::LayoutCommand;
use commands::map::MapCommand;
use commands::raster::RasterCommand;
use engine::EngineBackend;

#[derive(Debug, Parser)]
#[command(
    name = "geokit",
    version,
    about = "Fast GIS CLI toolkit for spatial data hygiene and batch ArcGIS Pro / GDAL geoprocessing",
    long_about = None
)]
struct Cli {
    /// Custom path to ArcGIS Pro propy.bat (overrides auto-detection, geokit.toml, and GEOKIT_PROPY)
    #[arg(long, global = true, env = "GEOKIT_PROPY")]
    propy: Option<PathBuf>,

    /// Geoprocessing engine backend: auto (ArcGIS Pro if present, else GDAL), arcgis, or gdal
    #[arg(long, global = true, env = "GEOKIT_ENGINE", default_value = "auto")]
    engine: EngineBackend,

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
    /// Create a starter geokit.toml project configuration file
    Init {
        /// Target directory (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Overwrite existing geokit.toml
        #[arg(long)]
        force: bool,
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
    /// Inspect and batch export ArcGIS Pro (.aprx) map layouts
    Layout {
        #[command(subcommand)]
        command: LayoutCommand,
    },
    /// Inspect and repair broken data sources in ArcGIS Pro (.aprx) projects
    Map {
        #[command(subcommand)]
        command: MapCommand,
    },
    /// Batch raster processing (mosaic, terrain slope/aspect/hillshade, zonal statistics)
    Raster {
        #[command(subcommand)]
        command: RasterCommand,
    },
    /// Automated hydrology & watershed (DAS) delineation pipelines from DEM (ArcGIS Spatial Analyst)
    Hydro {
        #[command(subcommand)]
        command: HydroCommand,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let (cfg, _) = config::load_config(&cwd)?;
    let effective_propy = cli.propy.as_deref().or(cfg.arcgis.propy.as_deref());

    match cli.command {
        Commands::Doctor { json } => commands::doctor::run(effective_propy, json),
        Commands::Init { path, force } => commands::init::run(&path, force),
        Commands::Audit { path, json } => commands::audit::run(&path, json),
        Commands::Organize {
            path,
            output,
            dry_run,
        } => {
            let (dir_cfg, _) = config::load_config(&path)?;
            commands::organize::run(&path, output.as_deref(), dry_run, &dir_cfg.folders)
        }
        Commands::Sanitize { path, dry_run } => commands::sanitize::run(&path, dry_run),
        Commands::Clean {
            path,
            include_xml,
            dry_run,
        } => commands::clean::run(&path, include_xml, dry_run),
        Commands::Gp { command } => commands::gp::run(command, effective_propy, &cfg, cli.engine),
        Commands::Layout { command } => commands::layout::run(command, effective_propy, &cfg),
        Commands::Map { command } => commands::map::run(command, effective_propy),
        Commands::Raster { command } => commands::raster::run(command, effective_propy, cli.engine),
        Commands::Hydro { command } => commands::hydro::run(command, effective_propy),
    }
}
