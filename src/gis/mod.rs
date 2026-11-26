pub mod bundle;
pub mod scanner;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GisCategory {
    Vector,
    Raster,
    Geodatabase,
    Project,
    Tabular,
}

impl GisCategory {
    pub fn folder_name(self) -> &'static str {
        match self {
            GisCategory::Vector => "01_Vector",
            GisCategory::Raster => "02_Raster",
            GisCategory::Geodatabase => "03_Geodatabase",
            GisCategory::Project => "04_Projects",
            GisCategory::Tabular => "05_Tabular",
        }
    }
}

pub const VECTOR_PRIMARY_EXTS: &[&str] = &[
    "shp", "gpkg", "geojson", "json", "kml", "kmz", "gml", "dxf", "dwg", "fgb",
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
