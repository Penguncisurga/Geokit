use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

/// Layer-1 Preflight: verifies that a `.shp` file has its mandatory `.shx` and `.dbf` sidecars
/// before spawning Python/ArcPy or GDAL.
pub fn check_shapefile_bundle(path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "shp" || !path.is_file() {
        return Ok(());
    }
    let mut missing = Vec::new();
    for req in ["shx", "dbf"] {
        if !path.with_extension(req).is_file() {
            missing.push(format!(".{req}"));
        }
    }
    if !missing.is_empty() {
        bail!(
            "Incomplete shapefile bundle '{}': missing required sidecar file(s): {}.\n\
             Hint: Ensure .shp, .shx, and .dbf are kept together in the same folder.",
            path.display(),
            missing.join(", ")
        );
    }
    Ok(())
}

/// Layer-1 Preflight: detects active `.lock` files on an output shapefile/dataset so we fail fast
/// with a clear hint instead of waiting for ArcPy ERROR 000210 / 000601.
pub fn check_output_not_locked(out: &Path) -> Result<()> {
    let Some(parent) = out.parent() else {
        return Ok(());
    };
    if !parent.is_dir() {
        return Ok(());
    }
    let Some(stem) = out.file_stem().and_then(|s| s.to_str()) else {
        return Ok(());
    };
    if stem.is_empty() {
        return Ok(());
    }
    let prefix = format!("{}.", stem.to_ascii_lowercase());
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let lower = fname.to_string_lossy().to_ascii_lowercase();
            if lower.starts_with(&prefix) && lower.ends_with(".lock") {
                bail!(
                    "Output '{}' is currently locked by another GIS application ('{}').\n\
                     Hint: Remove the layer from ArcMap/ArcGIS Pro/QGIS or close the app, then retry.",
                    out.display(),
                    fname.to_string_lossy()
                );
            }
        }
    }
    Ok(())
}

/// Layer-1 Preflight: natively validates that comma-separated field name(s) exist on `.shp` or
/// `.geojson` inputs before spawning ArcPy/GDAL, listing available fields when a typo is found.
pub fn check_fields_exist_native(input: &Path, fields_csv: &str) -> Result<()> {
    if !input.is_file() {
        return Ok(());
    }
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let available: Vec<String> = if ext == "shp" {
        let dbf = input.with_extension("dbf");
        if !dbf.is_file() {
            return Ok(());
        }
        let Ok(recs) = crate::gis::shp::read_dbf_records(&dbf) else {
            return Ok(());
        };
        match recs.first() {
            Some(first) => first.keys().cloned().collect(),
            None => return Ok(()),
        }
    } else if ext == "geojson" || ext == "json" {
        let Ok(raw) = fs::read_to_string(input) else {
            return Ok(());
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return Ok(());
        };
        match val
            .get("features")
            .and_then(|f| f.as_array())
            .and_then(|a| a.first())
            .and_then(|f| f.get("properties"))
            .and_then(|p| p.as_object())
        {
            Some(props) => props.keys().cloned().collect(),
            None => return Ok(()),
        }
    } else {
        return Ok(());
    };

    if available.is_empty() {
        return Ok(());
    }

    for req in fields_csv
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if !available.iter().any(|k| k.eq_ignore_ascii_case(req)) {
            bail!(
                "Field '{}' not found in '{}'.\nAvailable fields: {}",
                req,
                input.display(),
                available.join(", ")
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_preflight_bundle_lock_and_field_checks() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // 1. Missing .shx/.dbf detected immediately
        let broken_shp = root.join("broken.shp");
        fs::write(&broken_shp, b"shp").unwrap();
        let err = check_shapefile_bundle(&broken_shp).unwrap_err().to_string();
        assert!(err.contains(".shx") && err.contains(".dbf"));

        // 2. Active .lock on target output detected immediately
        let target = root.join("hasil.shp");
        fs::write(root.join("hasil.shp.DESKTOP.1234.sr.lock"), b"").unwrap();
        let lock_err = check_output_not_locked(&target).unwrap_err().to_string();
        assert!(lock_err.contains("locked"));

        // 3. Field typo on GeoJSON lists available fields
        let gj = root.join("desa.geojson");
        fs::write(
            &gj,
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"NAMOBJ":"A","KELAS":"Tinggi"},"geometry":{"type":"Point","coordinates":[110,-7]}}]}"#,
        )
        .unwrap();
        assert!(check_fields_exist_native(&gj, "KELAS").is_ok());
        let fld_err = check_fields_exist_native(&gj, "KELAS_TYPO")
            .unwrap_err()
            .to_string();
        assert!(fld_err.contains("NAMOBJ") && fld_err.contains("KELAS"));
    }
}
