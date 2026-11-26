use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;
use std::path::Path;

use crate::config::GeokitConfig;
use crate::engine::{
    arcmap_daemon_port, arcpy_daemon_port, ping_daemon, resolve_backend, start_daemon, stop_daemon,
    EngineBackend, ResolvedBackend,
};

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start a persistent warm ArcPy worker daemon (imports arcpy ONCE, making all `geokit gp/raster/hydro/qa` commands 5x-15x faster!)
    Start {
        /// Auto-exit after N minutes of inactivity (0 = stay running until `geokit daemon stop`)
        #[arg(long, default_value_t = 30)]
        idle_minutes: u32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Check whether ArcMap / ArcGIS Pro warm worker daemons are currently running
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Stop running ArcMap / ArcGIS Pro warm worker daemon(s)
    Stop {
        /// Which daemon to stop: all (default), arcmap, or arcgis
        #[arg(long, default_value = "all")]
        target: String,
    },
}

pub fn run(
    cmd: DaemonCommand,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    match cmd {
        DaemonCommand::Start {
            idle_minutes,
            json: as_json,
        } => {
            let resolved =
                resolve_backend(backend, override_propy, cfg.arcgis.arcmap_python.as_deref());
            if !as_json {
                let name = match resolved {
                    ResolvedBackend::Arcmap => "ArcMap 10.x",
                    ResolvedBackend::Arcgis => "ArcGIS Pro",
                    ResolvedBackend::Gdal => "GDAL",
                };
                println!(
                    "{} Warming up {name} ArcPy worker daemon (1-time import)...",
                    "[daemon]".cyan().bold()
                );
            }
            let info = start_daemon(
                resolved,
                override_propy,
                cfg.arcgis.arcmap_python.as_deref(),
                idle_minutes,
            )?;
            if as_json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                let port = info.get("port").and_then(|v| v.as_u64()).unwrap_or(0);
                let pid = info.get("pid").and_then(|v| v.as_u64()).unwrap_or(0);
                let warm = info
                    .get("warmup_sec")
                    .and_then(|v| v.as_f64())
                    .map(|s| format!(", warmed in {s:.1}s"))
                    .unwrap_or_else(|| " (already warm)".into());
                println!(
                    "{} Hot-worker ready on 127.0.0.1:{port} (PID {pid}{warm}). All geokit gp/raster/hydro/qa commands now skip `import arcpy`!",
                    "[OK]".green().bold()
                );
            }
            Ok(())
        }
        DaemonCommand::Status { json: as_json } => {
            let p_map = arcmap_daemon_port();
            let p_pro = arcpy_daemon_port();
            let map_info = ping_daemon(p_map);
            let pro_info = ping_daemon(p_pro);

            if as_json {
                let payload = json!({
                    "arcmap": {
                        "port": p_map,
                        "running": map_info.is_some(),
                        "info": map_info,
                    },
                    "arcgis": {
                        "port": p_pro,
                        "running": pro_info.is_some(),
                        "info": pro_info,
                    }
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
                return Ok(());
            }

            println!("{}", "GeoKit Hot-Worker Daemon Status:".bold());
            match map_info {
                Some(v) => println!(
                    "  {} ArcMap 10.x  (127.0.0.1:{p_map}) | PID {} | {} req(s) | uptime {}s",
                    "[RUNNING]".green().bold(),
                    v["pid"],
                    v["requests"],
                    v["uptime_sec"]
                ),
                None => println!(
                    "  {} ArcMap 10.x  (127.0.0.1:{p_map})",
                    "[STOPPED]".dimmed()
                ),
            }
            match pro_info {
                Some(v) => println!(
                    "  {} ArcGIS Pro   (127.0.0.1:{p_pro}) | PID {} | {} req(s) | uptime {}s",
                    "[RUNNING]".green().bold(),
                    v["pid"],
                    v["requests"],
                    v["uptime_sec"]
                ),
                None => println!(
                    "  {} ArcGIS Pro   (127.0.0.1:{p_pro})",
                    "[STOPPED]".dimmed()
                ),
            }
            Ok(())
        }
        DaemonCommand::Stop { target } => {
            let t = target.to_ascii_lowercase();
            let mut stopped_any = false;
            if t == "all" || t == "arcmap" {
                let p = arcmap_daemon_port();
                if stop_daemon(p) {
                    println!(
                        "{} Stopped ArcMap daemon (127.0.0.1:{p})",
                        "[OK]".green().bold()
                    );
                    stopped_any = true;
                }
            }
            if t == "all" || t == "arcgis" || t == "pro" {
                let p = arcpy_daemon_port();
                if stop_daemon(p) {
                    println!(
                        "{} Stopped ArcGIS Pro daemon (127.0.0.1:{p})",
                        "[OK]".green().bold()
                    );
                    stopped_any = true;
                }
            }
            if !stopped_any {
                println!("{}", "No running GeoKit hot-worker daemons found.".yellow());
            }
            Ok(())
        }
    }
}
