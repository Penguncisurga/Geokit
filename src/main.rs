mod commands;
mod config;
mod engine;
mod gis;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use commands::daemon::DaemonCommand;
use commands::fetch::FetchCommand;
use commands::gp::GpCommand;
use commands::hydro::HydroCommand;
use commands::layout::LayoutCommand;
use commands::live::LiveCommand;
use commands::map::MapCommand;
use commands::pipeline::PipelineCommand;
use commands::raster::RasterCommand;
use commands::toolbox::ToolboxCommand;
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

    /// Geoprocessing engine backend: auto (ArcGIS Pro if present, else GDAL), arcgis, arcmap, or gdal
    #[arg(long, global = true, env = "GEOKIT_ENGINE")]
    engine: Option<EngineBackend>,

    /// Allow overwriting existing output files (disabled by default for data safety)
    #[arg(long, global = true, env = "GEOKIT_OVERWRITE")]
    overwrite: bool,

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
        /// Also remove .lock files inside File Geodatabases (.gdb) — close ArcGIS/QGIS first!
        #[arg(long)]
        include_gdb_locks: bool,
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
    /// Generate ModelBuilder-ready ArcGIS Python Toolboxes (.pyt)
    Toolbox {
        #[command(subcommand)]
        command: ToolboxCommand,
    },
    /// Create and execute declarative multi-step GIS pipelines (pipeline.toml)
    Pipeline {
        #[command(subcommand)]
        command: PipelineCommand,
    },
    /// Run Spatial Quality Assurance checks (CRS uniformity, invalid geometry, NULL fields, polygon overlaps)
    Qa {
        /// File, folder, or .gdb to inspect
        input: PathBuf,
        /// Expected EPSG code (defaults to geokit.toml defaults.epsg if set)
        #[arg(long)]
        expected_epsg: Option<u32>,
        /// Comma-separated required field names to check for NULL/empty values (e.g. "DESA,KELAS")
        #[arg(long)]
        check_nulls: Option<String>,
        /// Check for overlapping polygons within each polygon layer
        #[arg(long)]
        check_overlaps: bool,
        /// Return non-zero exit code when any QA issue or mixed CRS is detected (for CI/pipelines)
        #[arg(long)]
        strict: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate a clean Markdown (.md), HTML (.html), or JSON workspace inventory report (no ArcGIS needed)
    Report {
        /// Directory to report on (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output report path (.md, .html, or .json)
        #[arg(short, long, default_value = "geokit_report.html")]
        out: PathBuf,
    },
    /// Download free public spatial data (OpenStreetMap roads/rivers/buildings & 30m SRTM DEM) for any AOI (no ArcGIS needed)
    Fetch {
        #[command(subcommand)]
        command: FetchCommand,
    },
    /// Export one or more .shp/.geojson layers into a single self-contained multi-layer interactive Leaflet HTML webmap (no ArcGIS needed)
    Webmap {
        /// One or more input shapefile (.shp) or GeoJSON (.geojson) layers
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Output standalone .html file path
        #[arg(short, long, default_value = "geokit_webmap.html")]
        out: PathBuf,
        /// Attribute field to colorize & summarize in legend (e.g. KELAS, TUTUPAN)
        #[arg(long)]
        field: Option<String>,
        /// Color palette preset: hazard-pastel, hazard, viridis, blue, category, or klhk (23-class Indonesian Land Cover)
        #[arg(long, default_value = "hazard-pastel")]
        preset: String,
        /// Map title displayed in the header card
        #[arg(long)]
        title: Option<String>,
        /// Default basemap: satellite, osm, light, topo, or dark
        #[arg(long, default_value = "satellite")]
        basemap: String,
        /// Optional coordinate simplification tolerance in degrees (e.g. 0.0002 to keep huge shapefiles lightweight)
        #[arg(long, default_value_t = 0.0001)]
        simplify: f64,
    },
    /// Control a currently open ArcMap 10.x window live (add/remove/zoom/select/export/run-gp)
    Live {
        #[command(subcommand)]
        command: Box<LiveCommand>,
    },
    /// Manage a persistent warm ArcPy worker daemon (skips 8s `import arcpy` startup for 5x-15x faster commands!)
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Run geokit as a Model Context Protocol (MCP) JSON-RPC 2.0 server over stdio
    Mcp,
}

fn resolve_engine(cli_engine: Option<EngineBackend>, cfg_engine: Option<&str>) -> EngineBackend {
    if let Some(e) = cli_engine {
        return e;
    }
    match cfg_engine.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("arcgis") => EngineBackend::Arcgis,
        Some("arcmap") => EngineBackend::Arcmap,
        Some("gdal") => EngineBackend::Gdal,
        _ => EngineBackend::Auto,
    }
}

/// Runs the CLI on a dedicated thread with a large stack. The default 1 MB main-thread stack on
/// Windows (MSVC) is not enough for clap's deeply nested derive parser in unoptimized builds.
fn main() -> Result<()> {
    let handle = std::thread::Builder::new()
        .name("geokit-main".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(real_main)?;
    match handle.join() {
        Ok(result) => result,
        Err(_) => anyhow::bail!("geokit main thread panicked"),
    }
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    engine::set_overwrite(cli.overwrite);
    let cwd = std::env::current_dir()?;
    let (cfg, cfg_path) = config::load_config(&cwd)?;
    let effective_propy = cli.propy.as_deref().or(cfg.arcgis.propy.as_deref());
    let effective_engine = resolve_engine(cli.engine, cfg.defaults.engine.as_deref());

    match cli.command {
        Commands::Doctor { json } => {
            commands::doctor::run(effective_propy, cfg_path.as_deref(), json)
        }
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
            include_gdb_locks,
            dry_run,
        } => commands::clean::run(&path, include_xml, include_gdb_locks, dry_run),
        Commands::Gp { command } => {
            commands::gp::run(command, effective_propy, &cfg, effective_engine)
        }
        Commands::Layout { command } => {
            commands::layout::run(command, effective_propy, &cfg, effective_engine)
        }
        Commands::Map { command } => {
            commands::map::run(command, effective_propy, &cfg, effective_engine)
        }
        Commands::Raster { command } => {
            commands::raster::run(command, effective_propy, &cfg, effective_engine)
        }
        Commands::Hydro { command } => {
            commands::hydro::run(command, effective_propy, &cfg, effective_engine)
        }
        Commands::Toolbox { command } => commands::toolbox::run(command),
        Commands::Pipeline { command } => {
            commands::pipeline::run(command, effective_propy, &cfg, effective_engine)
        }
        Commands::Qa {
            input,
            expected_epsg,
            check_nulls,
            check_overlaps,
            strict,
            json,
        } => commands::qa::run(
            &input,
            commands::qa::QaOptions {
                expected_epsg,
                check_nulls,
                check_overlaps,
                strict,
                json,
            },
            effective_propy,
            &cfg,
            effective_engine,
        ),
        Commands::Report { path, out } => commands::report::run(&path, &out),
        Commands::Fetch { command } => commands::fetch::run(command),
        Commands::Webmap {
            inputs,
            out,
            field,
            preset,
            title,
            basemap,
            simplify,
        } => commands::webmap::run(
            &inputs,
            &out,
            field.as_deref(),
            &preset,
            title.as_deref(),
            &basemap,
            simplify,
        ),
        Commands::Live { command } => commands::live::run(*command),
        Commands::Daemon { command } => {
            commands::daemon::run(command, effective_propy, &cfg, effective_engine)
        }
        Commands::Mcp => commands::mcp::run(),
    }
}
