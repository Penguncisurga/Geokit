use anyhow::{bail, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::gis::scanner::{format_bytes, scan_directory};
use crate::gis::validate_safe_workspace_dir;

fn is_inside_gdb(path: &Path) -> bool {
    for anc in path.ancestors().skip(1) {
        if anc
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("gdb"))
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

pub fn run(path: &Path, include_xml: bool, include_gdb_locks: bool, dry_run: bool) -> Result<()> {
    if !path.is_dir() {
        bail!("Path must be an existing directory: {}", path.display());
    }
    validate_safe_workspace_dir(path)?;

    let report = scan_directory(path);
    let mut targets = Vec::new();
    let mut skipped_gdb_locks = 0usize;
    for lock in report.lock_files {
        if !include_gdb_locks && is_inside_gdb(&lock) {
            skipped_gdb_locks += 1;
        } else {
            targets.push(lock);
        }
    }
    targets.extend(report.temp_files);

    if include_xml {
        for entry in WalkDir::new(path).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".shp.xml") || lower.ends_with(".aux.xml") {
                    targets.push(entry.path().to_path_buf());
                }
            }
        }
    }

    targets.sort();
    targets.dedup();

    if targets.is_empty() {
        if skipped_gdb_locks > 0 {
            println!(
                "{} Skipped {} .lock file(s) inside File Geodatabase (.gdb) for safety.\n  Close ArcGIS/QGIS first, then pass `--include-gdb-locks` if you are sure they are stale.",
                "[SAFE]".yellow().bold(),
                skipped_gdb_locks
            );
        } else {
            println!(
                "{}",
                "Workspace is already clean! No clutter files found.".green()
            );
        }
        return Ok(());
    }

    if skipped_gdb_locks > 0 {
        println!(
            "{} Skipping {} .lock file(s) inside .gdb geodatabase(s) (pass `--include-gdb-locks` to remove).",
            "[SAFE]".yellow().bold(),
            skipped_gdb_locks
        );
    }

    if dry_run {
        println!("{}", "[DRY RUN] Files that would be removed:".cyan().bold());
    }

    let mut removed = 0usize;
    let mut failed = 0usize;
    let mut reclaimed: u64 = 0;

    for file in &targets {
        let size = fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        if dry_run {
            println!(
                "  {} {} ({})",
                "PLAN".cyan(),
                file.display(),
                format_bytes(size)
            );
            reclaimed += size;
            removed += 1;
        } else {
            match fs::remove_file(file) {
                Ok(()) => {
                    println!(
                        "  {} {} ({})",
                        "DEL".green(),
                        file.display(),
                        format_bytes(size)
                    );
                    reclaimed += size;
                    removed += 1;
                }
                Err(e) => {
                    println!("  {} {} ({})", "ERR".red(), file.display(), e);
                    failed += 1;
                }
            }
        }
    }

    println!(
        "\n{} {} file(s) {} ({} reclaimed), {} failed.",
        "Done:".bold(),
        removed,
        if dry_run {
            "would be removed"
        } else {
            "removed"
        },
        format_bytes(reclaimed).bold(),
        failed
    );

    Ok(())
}
