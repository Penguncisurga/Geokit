use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::bundle::{check_shapefile_health, collect_raster_bundle, collect_shapefile_bundle};
use super::{
    is_gis_safe_name, GisCategory, PROJECT_EXTS, RASTER_PRIMARY_EXTS, TABULAR_EXTS,
    VECTOR_PRIMARY_EXTS,
};

#[derive(Debug, Clone, Serialize)]
pub struct GisDataset {
    pub path: PathBuf,
    pub name: String,
    pub category: GisCategory,
    pub extension: String,
    pub size_bytes: u64,
    pub sidecar_count: usize,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub root: PathBuf,
    pub datasets: Vec<GisDataset>,
    pub lock_files: Vec<PathBuf>,
    pub temp_files: Vec<PathBuf>,
    pub total_size_bytes: u64,
}

pub fn is_gdb_dir(path: &Path) -> bool {
    path.is_dir()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("gdb"))
            .unwrap_or(false)
}

pub fn dir_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum()
}

pub fn scan_directory(root: &Path) -> ScanReport {
    let mut datasets = Vec::new();
    let mut lock_files = Vec::new();
    let mut temp_files = Vec::new();

    let mut it = WalkDir::new(root).into_iter();
    while let Some(entry_res) = it.next() {
        let Ok(entry) = entry_res else { continue };
        let path = entry.path();

        if path == root {
            continue;
        }

        // Treat File Geodatabase (.gdb) as an atomic dataset, but still inspect lock files inside
        if is_gdb_dir(path) {
            for inner in WalkDir::new(path).into_iter().flatten() {
                if inner.file_type().is_file() {
                    if let Some(ext) = inner.path().extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("lock") {
                            lock_files.push(inner.path().to_path_buf());
                        }
                    }
                }
            }

            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let mut issues = Vec::new();
            if !is_gis_safe_name(&stem) {
                issues.push("non-standard filename (spaces/uppercase/symbols)".to_string());
            }

            datasets.push(GisDataset {
                path: path.to_path_buf(),
                name: path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string(),
                category: GisCategory::Geodatabase,
                extension: "gdb".to_string(),
                size_bytes: dir_size(path),
                sidecar_count: 1,
                issues,
            });

            it.skip_current_dir();
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        let Some(ext_os) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext = ext_os.to_ascii_lowercase();

        if ext == "lock" {
            lock_files.push(path.to_path_buf());
            continue;
        }
        if ext == "tmp" || ext == "bak" {
            temp_files.push(path.to_path_buf());
            continue;
        }

        let category = if VECTOR_PRIMARY_EXTS.contains(&ext.as_str()) {
            Some(GisCategory::Vector)
        } else if RASTER_PRIMARY_EXTS.contains(&ext.as_str()) {
            Some(GisCategory::Raster)
        } else if PROJECT_EXTS.contains(&ext.as_str()) {
            Some(GisCategory::Project)
        } else if TABULAR_EXTS.contains(&ext.as_str()) {
            Some(GisCategory::Tabular)
        } else {
            None
        };

        let Some(category) = category else { continue };

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let mut issues = Vec::new();

        if !is_gis_safe_name(&stem) {
            issues.push("non-standard filename (spaces/uppercase/symbols)".to_string());
        }

        let (size_bytes, sidecar_count) = if ext == "shp" {
            let bundle = collect_shapefile_bundle(path);
            let (missing_req, missing_prj) = check_shapefile_health(path);
            if !missing_req.is_empty() {
                issues.push(format!(
                    "broken shapefile (missing {})",
                    missing_req.join(", ")
                ));
            }
            if missing_prj {
                issues.push("missing .prj (unknown CRS)".to_string());
            }
            let total: u64 = bundle
                .iter()
                .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
                .sum();
            (total, bundle.len())
        } else if category == GisCategory::Raster {
            let bundle = collect_raster_bundle(path);
            let total: u64 = bundle
                .iter()
                .filter_map(|p| fs::metadata(p).ok().map(|m| m.len()))
                .sum();
            (total, bundle.len())
        } else {
            let sz = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            (sz, 1)
        };

        datasets.push(GisDataset {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string(),
            category,
            extension: ext,
            size_bytes,
            sidecar_count,
            issues,
        });
    }

    datasets.sort_by(|a, b| a.path.cmp(&b.path));
    lock_files.sort();
    temp_files.sort();

    let total_size_bytes = datasets.iter().map(|d| d.size_bytes).sum();

    ScanReport {
        root: root.to_path_buf(),
        datasets,
        lock_files,
        temp_files,
        total_size_bytes,
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}
