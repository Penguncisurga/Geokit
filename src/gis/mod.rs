pub mod bundle;
pub mod scanner;
pub mod shp;

use anyhow::{bail, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

fn clean_norm_path(p: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(p).unwrap_or_else(|_| {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|c| c.join(p))
                .unwrap_or_else(|_| p.to_path_buf())
        }
    });
    let s = resolved.to_string_lossy();
    #[cfg(windows)]
    {
        PathBuf::from(s.trim_start_matches(r"\\?\"))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(s.as_ref())
    }
}

/// Guardrail preventing destructive filesystem commands (`clean`, `organize`, `sanitize`)
/// from running on a drive root, OS system directory, bare user home root, or outside
/// `GEOKIT_WORKSPACE_ROOT` (when configured).
pub fn validate_safe_workspace_dir(path: &Path) -> Result<()> {
    let norm = clean_norm_path(path);

    // 1. Block filesystem / drive roots (e.g. C:\, D:\, /)
    if norm.parent().is_none() {
        bail!(
            "Refusing destructive workspace operation on filesystem root '{}'. Target a specific project subfolder.",
            norm.display()
        );
    }

    // 2. Block OS system directories
    let norm_lower = norm.to_string_lossy().to_ascii_lowercase();
    let trimmed = norm_lower.trim_end_matches(['\\', '/']);

    #[cfg(windows)]
    {
        let drive_prefix = if trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' {
            &trimmed[..2]
        } else {
            ""
        };
        if !drive_prefix.is_empty() {
            let blocked_rel = [
                r"\windows",
                r"\program files",
                r"\program files (x86)",
                r"\programdata",
                r"\system volume information",
                r"\$recycle.bin",
            ];
            for rel in blocked_rel {
                let full = format!("{drive_prefix}{rel}");
                if trimmed == full || trimmed.starts_with(&format!("{full}\\")) {
                    bail!(
                        "Refusing destructive workspace operation inside protected OS directory '{}'.",
                        norm.display()
                    );
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        let blocked = [
            "/etc", "/usr", "/bin", "/sbin", "/lib", "/var", "/boot", "/proc", "/sys", "/dev",
            "/system", "/library",
        ];
        for b in blocked {
            if trimmed == b || trimmed.starts_with(&format!("{b}/")) {
                bail!(
                    "Refusing destructive workspace operation inside protected OS directory '{}'.",
                    norm.display()
                );
            }
        }
    }

    // 3. Block bare user home directory itself (subfolders inside home are allowed)
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let home_norm = clean_norm_path(Path::new(&home));
        let home_lower = home_norm.to_string_lossy().to_ascii_lowercase();
        if trimmed == home_lower.trim_end_matches(['\\', '/']) {
            bail!(
                "Refusing destructive workspace operation directly on user home root '{}'. Target a specific project subfolder.",
                norm.display()
            );
        }
    }

    // 4. Enforce optional GEOKIT_WORKSPACE_ROOT confinement
    validate_within_workspace_root(&norm)?;

    Ok(())
}

/// Enforces optional `GEOKIT_WORKSPACE_ROOT` confinement for any target path.
pub fn validate_within_workspace_root(path: &Path) -> Result<()> {
    let Ok(root_env) = std::env::var("GEOKIT_WORKSPACE_ROOT") else {
        return Ok(());
    };
    if root_env.trim().is_empty() {
        return Ok(());
    }
    let allowed_root = clean_norm_path(Path::new(root_env.trim()));
    let target = clean_norm_path(path);

    #[cfg(windows)]
    let ok = {
        let a = allowed_root.to_string_lossy().to_ascii_lowercase();
        let a = a.trim_end_matches(['\\', '/']);
        let t = target.to_string_lossy().to_ascii_lowercase();
        let t = t.trim_end_matches(['\\', '/']);
        t == a || t.starts_with(&format!("{a}\\"))
    };
    #[cfg(not(windows))]
    let ok = target.starts_with(&allowed_root);

    if !ok {
        bail!(
            "Path '{}' is outside GEOKIT_WORKSPACE_ROOT ('{}').",
            target.display(),
            allowed_root.display()
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GisCategory {
    Vector,
    Raster,
    Geodatabase,
    Project,
    Tabular,
}

pub const VECTOR_PRIMARY_EXTS: &[&str] = &[
    "shp", "gpkg", "geojson", "kml", "kmz", "gml", "dxf", "dwg", "fgb",
];

pub const RASTER_PRIMARY_EXTS: &[&str] = &[
    "tif", "tiff", "img", "jp2", "ecw", "asc", "dem", "bil", "bip", "bsq", "nc", "hdf",
];

pub const PROJECT_EXTS: &[&str] = &[
    "aprx", "mxd", "qgz", "qgs", "mpk", "mpkx", "ppkx", "lyrx", "lyr",
];

pub const TABULAR_EXTS: &[&str] = &["csv", "xlsx", "xls"];

pub const SHAPEFILE_REQUIRED_EXTS: &[&str] = &["shp", "shx", "dbf"];

pub const SHAPEFILE_SIDECAR_EXTS: &[&str] = &[
    "shp", "shx", "dbf", "prj", "cpg", "sbn", "sbx", "fbn", "fbx", "ain", "aih", "ixs", "mxs",
    "atx", "qix", "xml",
];

pub const RASTER_SIDECAR_SUFFIXES: &[&str] = &[
    ".aux.xml", ".ovr", ".rrd", ".vat.dbf", ".vat.cpg", ".xml", ".tfw", ".tifw", ".jgw", ".jpw",
    ".wld", ".prj", ".stx", ".ers", ".hdr",
];

/// Checks whether a GIS dataset stem is ArcGIS/GDB-safe (lowercase ASCII alphanumeric + underscore,
/// not starting with a digit, no spaces or illegal symbols).
pub fn is_gis_safe_name(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let first = stem.chars().next().unwrap();
    if first.is_ascii_digit() || first == '_' {
        return false;
    }
    stem.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !stem.contains("__")
        && !stem.ends_with('_')
}

/// Converts an arbitrary GIS file stem into a clean, ArcGIS-safe `snake_case` stem.
pub fn sanitize_gis_name(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len());
    let mut prev_underscore = false;

    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }

    if out.is_empty() {
        return "layer".to_string();
    }

    if out.chars().next().unwrap().is_ascii_digit() {
        out.insert_str(0, "l_");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_gis_name() {
        assert_eq!(sanitize_gis_name("Batas Desa 2024"), "batas_desa_2024");
        assert_eq!(
            sanitize_gis_name("01-Jalan Utama (Fix)!.shp"),
            "l_01_jalan_utama_fix_shp"
        );
        assert_eq!(sanitize_gis_name("sungai__utama"), "sungai_utama");
        assert_eq!(sanitize_gis_name("___"), "layer");
        assert_eq!(sanitize_gis_name("2025_dem"), "l_2025_dem");
    }

    #[test]
    fn test_is_gis_safe_name() {
        assert!(is_gis_safe_name("batas_desa_2024"));
        assert!(!is_gis_safe_name("Batas Desa"));
        assert!(!is_gis_safe_name("01_jalan"));
        assert!(!is_gis_safe_name("jalan__utama"));
        assert!(!is_gis_safe_name("jalan-utama"));
    }
}
