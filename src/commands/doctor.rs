use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use std::path::Path;

use crate::engine::gdal::{detect_gdal, GdalEnvironment};
use crate::engine::{
    arcmap_daemon_port, arcpy_daemon_port, detect_arcgis, geokit_home_dir, ping_daemon,
    ArcGisEnvironment,
};

#[derive(Debug, Serialize)]
struct LiveAndDaemonStatus {
    arcmap_addin_installed: bool,
    pro_bridge_installed: bool,
    local_token_ready: bool,
    live_bridge_port: u16,
    live_bridge_running: bool,
    arcmap_daemon_port: u16,
    arcmap_daemon_running: bool,
    arcgis_daemon_port: u16,
    arcgis_daemon_running: bool,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    version: &'static str,
    config_file: Option<String>,
    arcgis: ArcGisEnvironment,
    gdal: GdalEnvironment,
    bridges: LiveAndDaemonStatus,
}

pub fn run(
    override_propy: Option<&Path>,
    config_file: Option<&Path>,
    json_output: bool,
) -> Result<()> {
    let info = detect_arcgis(override_propy);
    let gdal = detect_gdal();

    let home = geokit_home_dir();
    let arcmap_addin_installed = home
        .as_ref()
        .map(|h| h.join("addins").join("geokit-arcmap.esriaddin").is_file())
        .unwrap_or(false);
    let pro_bridge_installed = home
        .as_ref()
        .map(|h| h.join("pro").join("geokit_pro_bridge.py").is_file())
        .unwrap_or(false);
    let local_token_ready = home
        .as_ref()
        .map(|h| h.join("token").is_file())
        .unwrap_or(false)
        || std::env::var("GEOKIT_AUTH_TOKEN")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);

    let live_port: u16 = std::env::var("GEOKIT_LIVE_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9877);
    let live_bridge_running = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], live_port)),
        std::time::Duration::from_millis(80),
    )
    .is_ok();

    let am_port = arcmap_daemon_port();
    let ap_port = arcpy_daemon_port();
    let arcmap_daemon_running = ping_daemon(am_port).is_some();
    let arcgis_daemon_running = ping_daemon(ap_port).is_some();

    let bridges = LiveAndDaemonStatus {
        arcmap_addin_installed,
        pro_bridge_installed,
        local_token_ready,
        live_bridge_port: live_port,
        live_bridge_running,
        arcmap_daemon_port: am_port,
        arcmap_daemon_running,
        arcgis_daemon_port: ap_port,
        arcgis_daemon_running,
    };

    if json_output {
        let report = DoctorReport {
            version: env!("CARGO_PKG_VERSION"),
            config_file: config_file.map(|p| p.display().to_string()),
            arcgis: info,
            gdal,
            bridges,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!(
        "\n{} {}",
        "geokit Environment Doctor".bold().cyan(),
        format!("v{}", env!("CARGO_PKG_VERSION")).dimmed()
    );
    println!(
        "  {} Native GIS Data Hygiene (audit/organize/sanitize/clean): {}",
        "[OK]".green().bold(),
        "Ready".green()
    );
    match config_file {
        Some(p) => println!(
            "  {} Project Config (geokit.toml): {}",
            "[OK]".green().bold(),
            p.display()
        ),
        None => println!(
            "  {} Project Config (geokit.toml): None (using defaults; run `geokit init` to create)",
            "[INFO]".dimmed()
        ),
    }

    match &info.arcgis_pro_propy {
        Some(p) => {
            println!(
                "  {} ArcGIS Pro (propy.bat): {} ({})",
                "[OK]".green().bold(),
                p.display(),
                info.active_source.as_deref().unwrap_or("detected")
            );
        }
        None => {
            println!(
                "  {} ArcGIS Pro (propy.bat): Not found — set GEOKIT_PROPY / --propy or use `--engine gdal`",
                "[MISSING]".yellow().bold()
            );
        }
    }

    match &gdal.ogr2ogr {
        Some(p) => {
            println!(
                "  {} GDAL / OGR (ogr2ogr): {} ({})",
                "[OK]".green().bold(),
                p.display(),
                gdal.source.as_deref().unwrap_or("detected")
            );
        }
        None => {
            println!(
                "  {} GDAL / OGR (ogr2ogr): Not detected (optional cross-platform backend)",
                "[INFO]".dimmed()
            );
        }
    }

    match &info.arcmap_python {
        Some(p) => {
            let arch = if p.to_string_lossy().to_ascii_lowercase().contains("x64") {
                "64-bit"
            } else {
                "32-bit"
            };
            println!(
                "  {} ArcGIS Desktop 10.x Python ({arch}): {}",
                "[OK]".green().bold(),
                p.display()
            );
        }
        None => {
            println!(
                "  {} ArcGIS Desktop 10.x Python: Not detected",
                "[INFO]".dimmed()
            );
        }
    }

    println!(
        "  {} Live Bridges: ArcMap Add-In={} | Pro Bridge={} | Port {live_port}={}",
        if bridges.live_bridge_running {
            "[OK]".green().bold()
        } else {
            "[INFO]".dimmed()
        },
        if bridges.arcmap_addin_installed {
            "installed"
        } else {
            "not installed"
        },
        if bridges.pro_bridge_installed {
            "installed"
        } else {
            "not installed"
        },
        if bridges.live_bridge_running {
            "listening"
        } else {
            "idle"
        }
    );
    println!(
        "  {} Hot-Worker Daemons: ArcMap({am_port})={} | ArcGIS Pro({ap_port})={} | Auth Token={}",
        if bridges.arcmap_daemon_running || bridges.arcgis_daemon_running {
            "[OK]".green().bold()
        } else {
            "[INFO]".dimmed()
        },
        if bridges.arcmap_daemon_running {
            "running"
        } else {
            "stopped"
        },
        if bridges.arcgis_daemon_running {
            "running"
        } else {
            "stopped"
        },
        if bridges.local_token_ready {
            "ready"
        } else {
            "auto-created on first connect"
        }
    );

    println!();
    Ok(())
}
