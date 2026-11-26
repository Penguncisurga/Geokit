use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::{RASTER_SIDECAR_SUFFIXES, SHAPEFILE_REQUIRED_EXTS, SHAPEFILE_SIDECAR_EXTS};

/// Finds all files belonging to a shapefile bundle given the `.shp` path.
pub fn collect_shapefile_bundle(shp_path: &Path) -> Vec<PathBuf> {
    let Some(parent) = shp_path.parent() else {
        return vec![shp_path.to_path_buf()];
    };
    let Some(stem) = shp_path.file_stem().and_then(|s| s.to_str()) else {
        return vec![shp_path.to_path_buf()];
    };

    let stem_lower = stem.to_ascii_lowercase();
    let shp_xml_name = format!("{stem_lower}.shp.xml");
    let mut files = Vec::new();

    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let lower = file_name.to_ascii_lowercase();

            if lower == shp_xml_name {
                files.push(path);
                continue;
            }

            if let Some((f_stem, f_ext)) = lower.rsplit_once('.') {
                if f_stem == stem_lower && SHAPEFILE_SIDECAR_EXTS.contains(&f_ext) {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    files
}

/// Checks missing required components (`.shx`, `.dbf`) and missing `.prj` for a `.shp` file.
pub fn check_shapefile_health(shp_path: &Path) -> (Vec<String>, bool) {
    let bundle = collect_shapefile_bundle(shp_path);
    let present_exts: Vec<String> = bundle
        .iter()
        .filter_map(|p| p.extension().and_then(|e| e.to_str()))
        .map(|e| e.to_ascii_lowercase())
        .collect();

    let missing_required: Vec<String> = SHAPEFILE_REQUIRED_EXTS
        .iter()
        .filter(|ext| !present_exts.iter().any(|p| p == **ext))
        .map(|ext| format!(".{ext}"))
        .collect();

    let missing_prj = !present_exts.iter().any(|p| p == "prj");
    (missing_required, missing_prj)
}

/// Finds all sidecar files belonging to a raster dataset (e.g. `.tfw`, `.tif.aux.xml`, `.ovr`).
pub fn collect_raster_bundle(raster_path: &Path) -> Vec<PathBuf> {
    let Some(parent) = raster_path.parent() else {
        return vec![raster_path.to_path_buf()];
    };
    let Some(file_name) = raster_path.file_name().and_then(|n| n.to_str()) else {
        return vec![raster_path.to_path_buf()];
    };
    let Some(stem) = raster_path.file_stem().and_then(|s| s.to_str()) else {
        return vec![raster_path.to_path_buf()];
    };

    let primary_lower = file_name.to_ascii_lowercase();
    let stem_lower = stem.to_ascii_lowercase();
    let mut files = vec![raster_path.to_path_buf()];

    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path == raster_path {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let lower = name.to_ascii_lowercase();

            // Matches `<file.tif>.aux.xml`, `<file.tif>.ovr`, etc.
            if let Some(rest) = lower.strip_prefix(&primary_lower) {
                if RASTER_SIDECAR_SUFFIXES.contains(&rest) {
                    files.push(path);
                    continue;
                }
            }

            // Matches `<stem>.tfw`, `<stem>.prj`, `<stem>.ovr`, etc.
            if let Some(rest) = lower.strip_prefix(&stem_lower) {
                if RASTER_SIDECAR_SUFFIXES.contains(&rest) {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    files.dedup();
    files
}

/// Moves a file or directory across directories (handles cross-volume fallback for files).
pub fn move_path(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) if src.is_file() => {
            fs::copy(src, dst).with_context(|| {
                format!("Failed to copy {} to {}", src.display(), dst.display())
            })?;
            fs::remove_file(src)
                .with_context(|| format!("Failed to remove original {}", src.display()))?;
            Ok(())
        }
        Err(e) => {
            Err(e).with_context(|| format!("Failed to move {} to {}", src.display(), dst.display()))
        }
    }
}

/// Computes the renamed filename for a sidecar belonging to `old_stem` -> `new_stem`.
pub fn rename_sidecar_filename(file_name: &str, old_stem: &str, new_stem: &str) -> Option<String> {
    if file_name.len() < old_stem.len() {
        return None;
    }
    let prefix = &file_name[..old_stem.len()];
    if prefix.eq_ignore_ascii_case(old_stem) {
        let suffix = &file_name[old_stem.len()..];
        if suffix.is_empty() || suffix.starts_with('.') {
            return Some(format!("{new_stem}{suffix}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_shapefile_bundle_and_health() {
        let dir = tempdir().unwrap();
        let shp = dir.path().join("jalan.shp");
        fs::write(&shp, b"").unwrap();
        fs::write(dir.path().join("jalan.shx"), b"").unwrap();
        fs::write(dir.path().join("jalan.dbf"), b"").unwrap();
        fs::write(dir.path().join("jalan.shp.xml"), b"").unwrap();
        fs::write(dir.path().join("other.shp"), b"").unwrap();

        let bundle = collect_shapefile_bundle(&shp);
        assert_eq!(bundle.len(), 4);

        let (missing_req, missing_prj) = check_shapefile_health(&shp);
        assert!(missing_req.is_empty());
        assert!(missing_prj);
    }

    #[test]
    fn test_rename_sidecar_filename() {
        assert_eq!(
            rename_sidecar_filename("Batas Desa.shp.xml", "Batas Desa", "batas_desa"),
            Some("batas_desa.shp.xml".to_string())
        );
        assert_eq!(
            rename_sidecar_filename("Batas Desa.DBF", "Batas Desa", "batas_desa"),
            Some("batas_desa.DBF".to_string())
        );
    }
}
