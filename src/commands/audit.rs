use anyhow::{bail, Result};
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use std::path::Path;

use crate::gis::scanner::{format_bytes, scan_directory};
use crate::gis::GisCategory;

pub fn run(path: &Path, json_output: bool) -> Result<()> {
    if !path.exists() {
        bail!("Path does not exist: {}", path.display());
    }

    let report = scan_directory(path);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!(
        "\n{} {}",
        "GIS Workspace Audit:".bold().cyan(),
        path.display()
    );

    if report.datasets.is_empty() && report.lock_files.is_empty() && report.temp_files.is_empty() {
        println!("{}", "No GIS datasets found in this directory.".yellow());
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Category", "Dataset", "Files", "Size", "Status"]);

    let mut issue_count = 0usize;
    let mut counts = [0usize; 5];

    for ds in &report.datasets {
        match ds.category {
            GisCategory::Vector => counts[0] += 1,
            GisCategory::Raster => counts[1] += 1,
            GisCategory::Geodatabase => counts[2] += 1,
            GisCategory::Project => counts[3] += 1,
            GisCategory::Tabular => counts[4] += 1,
        }

        let rel = ds
            .path
            .strip_prefix(path)
            .unwrap_or(&ds.path)
            .display()
            .to_string();

        let status_cell = if ds.issues.is_empty() {
            Cell::new("OK").fg(Color::Green)
        } else {
            issue_count += ds.issues.len();
            Cell::new(ds.issues.join("; ")).fg(Color::Yellow)
        };

        table.add_row(vec![
            Cell::new(format!("{:?}", ds.category)),
            Cell::new(rel),
            Cell::new(ds.sidecar_count.to_string()),
            Cell::new(format_bytes(ds.size_bytes)),
            status_cell,
        ]);
    }

    println!("{table}");
    println!(
        "\n{} {} datasets ({} vector, {} raster, {} gdb, {} project, {} tabular) | Total: {}",
        "Summary:".bold(),
        report.datasets.len(),
        counts[0],
        counts[1],
        counts[2],
        counts[3],
        counts[4],
        format_bytes(report.total_size_bytes).bold()
    );

    if issue_count > 0 {
        println!(
            "{} {} dataset issue(s) detected. Tip: run `geokit sanitize` to fix naming issues.",
            "Warnings:".yellow().bold(),
            issue_count
        );
    }
    if !report.lock_files.is_empty() || !report.temp_files.is_empty() {
        println!(
            "{} {} .lock file(s) and {} temp file(s) found. Tip: run `geokit clean --dry-run`.",
            "Clutter:".yellow().bold(),
            report.lock_files.len(),
            report.temp_files.len()
        );
    }

    Ok(())
}
