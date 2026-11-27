use anyhow::{bail, Result};
use colored::Colorize;
use std::path::Path;

use crate::config::FolderConfig;
use crate::gis::bundle::{collect_raster_bundle, collect_shapefile_bundle, move_path};
use crate::gis::scanner::{is_gdb_dir, scan_directory};
use crate::gis::{validate_safe_workspace_dir, GisCategory};

pub fn run(
    path: &Path,
    output: Option<&Path>,
    dry_run: bool,
    folders: &FolderConfig,
) -> Result<()> {
    if !path.is_dir() {
        bail!("Path must be an existing directory: {}", path.display());
    }
    validate_safe_workspace_dir(path)?;
    if let Some(out) = output {
        validate_safe_workspace_dir(out)?;
    }

    let target_root = output.unwrap_or(path);
    let report = scan_directory(path);

    if report.datasets.is_empty() {
        println!("{}", "No GIS datasets found to organize.".yellow());
        return Ok(());
    }

    if dry_run {
        println!(
            "{}",
            "[DRY RUN] Previewing organization plan (no files will be moved):"
                .cyan()
                .bold()
        );
    }

    let mut moved_datasets = 0usize;
    let mut skipped = 0usize;

    for ds in &report.datasets {
        let dest_dir = target_root.join(folders.name_for(ds.category));

        // Skip if already inside the target category directory
        if ds.path.starts_with(&dest_dir) {
            skipped += 1;
            continue;
        }

        if is_gdb_dir(&ds.path) {
            let dst = dest_dir.join(&ds.name);
            if dst.exists() {
                println!(
                    "  {} {} (destination already exists)",
                    "SKIP".yellow(),
                    ds.name
                );
                skipped += 1;
                continue;
            }
            println!(
                "  {} {} -> {}",
                if dry_run {
                    "PLAN".cyan()
                } else {
                    "MOVE".green()
                },
                ds.path.display(),
                dst.display()
            );
            if !dry_run {
                move_path(&ds.path, &dst)?;
            }
            moved_datasets += 1;
            continue;
        }

        let bundle = if ds.extension == "shp" {
            collect_shapefile_bundle(&ds.path)
        } else if ds.category == GisCategory::Raster {
            collect_raster_bundle(&ds.path)
        } else {
            vec![ds.path.clone()]
        };

        let collision = bundle.iter().any(|f| {
            f.file_name()
                .map(|n| dest_dir.join(n).exists())
                .unwrap_or(false)
        });
        if collision {
            println!(
                "  {} {} (destination already exists)",
                "SKIP".yellow(),
                ds.name
            );
            skipped += 1;
            continue;
        }

        println!(
            "  {} {} ({} file{}) -> {}",
            if dry_run {
                "PLAN".cyan()
            } else {
                "MOVE".green()
            },
            ds.name,
            bundle.len(),
            if bundle.len() == 1 { "" } else { "s" },
            dest_dir.display()
        );

        if !dry_run {
            for f in &bundle {
                if let Some(fname) = f.file_name() {
                    move_path(f, &dest_dir.join(fname))?;
                }
            }
        }
        moved_datasets += 1;
    }

    println!(
        "\n{} {} dataset(s) {}, {} skipped.",
        "Done:".bold(),
        moved_datasets,
        if dry_run { "planned" } else { "organized" },
        skipped
    );

    Ok(())
}
