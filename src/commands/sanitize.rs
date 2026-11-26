use anyhow::{bail, Result};
use colored::Colorize;
use std::path::Path;

use crate::gis::bundle::{
    collect_raster_bundle, collect_shapefile_bundle, move_path, rename_sidecar_filename,
};
use crate::gis::scanner::{is_gdb_dir, scan_directory};
use crate::gis::{is_gis_safe_name, sanitize_gis_name, GisCategory};

pub fn run(path: &Path, dry_run: bool) -> Result<()> {
    if !path.is_dir() {
        bail!("Path must be an existing directory: {}", path.display());
    }

    let report = scan_directory(path);

    if dry_run {
        println!(
            "{}",
            "[DRY RUN] Previewing filename sanitization (no files will be renamed):"
                .cyan()
                .bold()
        );
    }

    let mut renamed = 0usize;
    let mut skipped = 0usize;

    for ds in &report.datasets {
        let Some(old_stem) = ds.path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if is_gis_safe_name(old_stem) {
            continue;
        }

        let new_stem = sanitize_gis_name(old_stem);
        if new_stem == old_stem {
            continue;
        }

        let Some(parent) = ds.path.parent() else {
            continue;
        };

        if is_gdb_dir(&ds.path) {
            let new_path = parent.join(format!("{new_stem}.gdb"));
            if new_path.exists() && !new_path.eq(&ds.path) {
                println!(
                    "  {} {} -> {} (target exists)",
                    "SKIP".yellow(),
                    ds.name,
                    new_path.display()
                );
                skipped += 1;
                continue;
            }
            println!(
                "  {} {}.gdb -> {}.gdb",
                if dry_run {
                    "PLAN".cyan()
                } else {
                    "RENAME".green()
                },
                old_stem,
                new_stem
            );
            if !dry_run {
                move_path(&ds.path, &new_path)?;
            }
            renamed += 1;
            continue;
        }

        let bundle = if ds.extension == "shp" {
            collect_shapefile_bundle(&ds.path)
        } else if ds.category == GisCategory::Raster {
            collect_raster_bundle(&ds.path)
        } else {
            vec![ds.path.clone()]
        };

        let mut plan = Vec::new();
        let mut collision = false;

        for f in &bundle {
            let Some(fname) = f.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if let Some(new_fname) = rename_sidecar_filename(fname, old_stem, &new_stem) {
                let dst = parent.join(&new_fname);
                // On case-insensitive Windows filesystems, renaming `Foo.shp` -> `foo.shp`
                // sees `dst.exists() == true` even though it is the same file.
                let is_case_only = fname.eq_ignore_ascii_case(&new_fname);
                if dst.exists() && !is_case_only {
                    collision = true;
                    break;
                }
                plan.push((f.clone(), dst));
            }
        }

        if collision {
            println!(
                "  {} {} -> {}.{} (target already exists)",
                "SKIP".yellow(),
                ds.name,
                new_stem,
                ds.extension
            );
            skipped += 1;
            continue;
        }

        println!(
            "  {} {}.{} -> {}.{} ({} file{})",
            if dry_run {
                "PLAN".cyan()
            } else {
                "RENAME".green()
            },
            old_stem,
            ds.extension,
            new_stem,
            ds.extension,
            plan.len(),
            if plan.len() == 1 { "" } else { "s" }
        );

        if !dry_run {
            for (src, dst) in plan {
                move_path(&src, &dst)?;
            }
        }
        renamed += 1;
    }

    if renamed == 0 && skipped == 0 {
        println!("{}", "All GIS dataset names are already clean!".green());
    } else {
        println!(
            "\n{} {} dataset(s) {}, {} skipped.",
            "Done:".bold(),
            renamed,
            if dry_run { "planned" } else { "renamed" },
            skipped
        );
    }

    Ok(())
}
