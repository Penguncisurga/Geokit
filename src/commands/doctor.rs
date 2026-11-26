use anyhow::Result;
use colored::Colorize;
use serde::Serialize;
use std::path::Path;

use crate::engine::gdal::{detect_gdal, GdalEnvironment};
use crate::engine::{detect_arcgis, ArcGisEnvironment};

#[derive(Debug, Serialize)]
struct DoctorReport {
    arcgis: ArcGisEnvironment,
    gdal: GdalEnvironment,
}

pub fn run(override_propy: Option<&Path>, json_output: bool) -> Result<()> {
    let info = detect_arcgis(override_propy);
    let gdal = detect_gdal();

    if json_output {
        let report = DoctorReport { arcgis: info, gdal };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("\n{}", "geokit Environment Doctor".bold().cyan());
    println!(
        "  {} Native GIS Data Hygiene (audit/organize/sanitize/clean): {}",
        "[OK]".green().bold(),
        "Ready".green()
    );

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
            println!(
                "  {} ArcGIS Desktop 10.x Python: {}",
                "[INFO]".cyan().bold(),
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

    println!();
    Ok(())
}
