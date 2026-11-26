use anyhow::Result;
use colored::Colorize;
use std::path::Path;

use crate::engine::detect_arcgis;

pub fn run(override_propy: Option<&Path>, json_output: bool) -> Result<()> {
    let info = detect_arcgis(override_propy);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&info)?);
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
                "  {} ArcGIS Pro (propy.bat): Not found — `geokit gp` commands require ArcGIS Pro (or set GEOKIT_PROPY / --propy)",
                "[MISSING]".yellow().bold()
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
