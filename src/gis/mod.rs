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
    "shp", "gpkg", "geojson", "kml", "kmz", "gpx", "gml", "dxf", "dwg", "fgb",
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

/// Parses either a standard decimal degree string (e.g. `"-7.003333"`, `"110.4083"`)
/// or a Degrees-Minutes-Seconds (DMS / DM) coordinate string such as:
/// - `7°00'12"S`, `110°24'30.5"E`
/// - `7° 00' 12" LS`, `110° 24' 30" BT` (Indonesian hemisphere abbreviations: LU/LS/BT/BB)
/// - `S 7° 0' 12"`, `7 00 12 S`, `7°30'N`
pub fn parse_dms_or_decimal(raw: &str) -> Option<f64> {
    let trimmed = raw.trim().trim_matches('"').trim();
    if trimmed.is_empty() {
        return None;
    }
    // Fast path: plain decimal number (also allow single comma decimal like "-7,0033")
    if let Ok(v) = trimmed.parse::<f64>() {
        return v.is_finite().then_some(v);
    }
    if !trimmed.contains('°')
        && !trimmed.contains('\'')
        && !trimmed.contains(' ')
        && trimmed.matches(',').count() == 1
    {
        if let Ok(v) = trimmed.replace(',', ".").parse::<f64>() {
            return v.is_finite().then_some(v);
        }
    }

    let upper = trimmed.to_ascii_uppercase();
    // Detect hemisphere tokens (both English N/S/E/W and Indonesian LU/LS/BT/BB/U/T/B)
    let mut sign: Option<f64> = None;
    let alpha_tokens: Vec<String> = upper
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect();
    for tok in &alpha_tokens {
        match tok.as_str() {
            "S" | "W" | "LS" | "BB" | "B" => sign = Some(-1.0),
            "N" | "E" | "LU" | "BT" | "U" | "T" => {
                if sign.is_none() {
                    sign = Some(1.0);
                }
            }
            _ => return None,
        }
    }

    // Extract numeric components (degrees, optional minutes, optional seconds)
    let mut nums: Vec<f64> = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = trimmed.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_digit()
            || ch == '.'
            || (ch == ','
                && !buf.is_empty()
                && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit()))
            || ((ch == '-' || ch == '+') && buf.is_empty() && nums.is_empty())
        {
            buf.push(if ch == ',' { '.' } else { ch });
        } else if !buf.is_empty() {
            let v: f64 = buf.parse().ok()?;
            nums.push(v);
            buf.clear();
        }
    }
    if !buf.is_empty() {
        let v: f64 = buf.parse().ok()?;
        nums.push(v);
    }

    if nums.is_empty() || nums.len() > 3 {
        return None;
    }
    // Must either have DMS symbols, hemisphere indicator, or 2-3 parts
    let has_dms_hint = trimmed.contains('°')
        || trimmed.contains('\'')
        || trimmed.contains('′')
        || trimmed.contains('″')
        || !alpha_tokens.is_empty()
        || nums.len() >= 2;
    if !has_dms_hint {
        return None;
    }

    let deg_raw = nums[0];
    let deg_abs = deg_raw.abs();
    let min = if nums.len() >= 2 { nums[1] } else { 0.0 };
    let sec = if nums.len() == 3 { nums[2] } else { 0.0 };
    if !(0.0..60.0).contains(&min) || !(0.0..60.0).contains(&sec) || deg_abs > 180.0 {
        return None;
    }
    let eff_sign = if let Some(s) = sign {
        s
    } else if deg_raw < 0.0 || trimmed.starts_with('-') {
        -1.0
    } else {
        1.0
    };
    let dec = eff_sign * (deg_abs + min / 60.0 + sec / 3600.0);
    dec.is_finite().then_some(dec)
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(ch);
            }
        } else if ch == '"' && cur.is_empty() {
            in_quotes = true;
        } else if ch == ',' {
            fields.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    fields.push(cur);
    fields
}

fn quote_csv_cell(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Inspects a CSV file's `x_field` and `y_field` columns for DMS coordinates.
/// If any row contains DMS notation (e.g. `7°00'12"S`), writes a temporary normalized CSV
/// with decimal degrees and returns `Ok(Some(temp_file))` so GDAL/ArcGIS/ArcMap all accept it.
/// Returns `Ok(None)` when all coordinates are already plain decimals.
pub fn preprocess_dms_csv(
    input_csv: &Path,
    x_field: &str,
    y_field: &str,
) -> Result<Option<tempfile::NamedTempFile>> {
    let raw = std::fs::read_to_string(input_csv)?;
    let mut lines = raw.lines();
    let Some(header_line) = lines.next() else {
        return Ok(None);
    };
    let headers = split_csv_line(header_line.trim_start_matches('\u{feff}'));
    let x_idx = headers
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case(x_field));
    let y_idx = headers
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case(y_field));
    let (Some(xi), Some(yi)) = (x_idx, y_idx) else {
        return Ok(None);
    };

    let mut converted_rows: Vec<Vec<String>> = Vec::new();
    let mut any_dms = false;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let mut cells = split_csv_line(line);
        for &idx in &[xi, yi] {
            if let Some(cell) = cells.get_mut(idx) {
                let t = cell.trim();
                if !t.is_empty() && t.parse::<f64>().is_err() {
                    if let Some(dec) = parse_dms_or_decimal(t) {
                        *cell = format!("{dec:.7}");
                        any_dms = true;
                    }
                }
            }
        }
        converted_rows.push(cells);
    }

    if !any_dms {
        return Ok(None);
    }

    let mut tmp = tempfile::Builder::new()
        .prefix("geokit_dms_")
        .suffix(".csv")
        .tempfile()?;
    use std::io::Write;
    let header_out = headers
        .iter()
        .map(|h| quote_csv_cell(h.trim()))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(tmp, "{header_out}")?;
    for row in converted_rows {
        let line_out = row
            .iter()
            .map(|c| quote_csv_cell(c))
            .collect::<Vec<_>>()
            .join(",");
        writeln!(tmp, "{line_out}")?;
    }
    tmp.flush()?;
    Ok(Some(tmp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dms_or_decimal() {
        let lat = parse_dms_or_decimal(r#"7°00'12"S"#).unwrap();
        assert!((lat - (-7.0033333)).abs() < 1e-5, "got {lat}");

        let lon = parse_dms_or_decimal(r#"110°24'30"E"#).unwrap();
        assert!((lon - 110.4083333).abs() < 1e-5, "got {lon}");

        let indo_ls = parse_dms_or_decimal(r#"6° 58' 30" LS"#).unwrap();
        assert!((indo_ls - (-6.975)).abs() < 1e-5, "got {indo_ls}");

        let indo_bt = parse_dms_or_decimal(r#"110° 25' 12" BT"#).unwrap();
        assert!((indo_bt - 110.42).abs() < 1e-5, "got {indo_bt}");

        let space_dms = parse_dms_or_decimal("7 30 00 S").unwrap();
        assert!((space_dms - (-7.5)).abs() < 1e-5, "got {space_dms}");

        let plain = parse_dms_or_decimal("-6.98765").unwrap();
        assert!((plain - (-6.98765)).abs() < 1e-9);

        assert!(parse_dms_or_decimal("not_a_coord").is_none());
    }

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
