use anyhow::{bail, Result};
use std::collections::BTreeMap;

type BandTable = &'static [(&'static str, u32)];

/// Default 1-based band positions assuming the user stacked ALL standard bands of the sensor in
/// official order into one multi-band raster (e.g. via `CompositeBands`). Override per role with
/// `--bands "red=3,nir=4"` whenever the stack order differs.
const LANDSAT_8_9: BandTable = &[
    ("coastal", 1),
    ("blue", 2),
    ("green", 3),
    ("red", 4),
    ("nir", 5),
    ("swir1", 6),
    ("swir2", 7),
    ("thermal", 10),
];
const LANDSAT_4_5_7: BandTable = &[
    ("blue", 1),
    ("green", 2),
    ("red", 3),
    ("nir", 4),
    ("swir1", 5),
    ("thermal", 6),
    ("swir2", 7),
];
/// Sentinel-2 L2A 12-band stack: B1,B2,B3,B4,B5,B6,B7,B8,B8A,B9,B11,B12.
const SENTINEL_2: BandTable = &[
    ("coastal", 1),
    ("blue", 2),
    ("green", 3),
    ("red", 4),
    ("rededge", 5),
    ("nir", 8),
    ("swir1", 11),
    ("swir2", 12),
];
/// 4-band high-resolution optical stack in B,G,R,NIR order (PlanetScope 4-band, SPOT 6/7).
const FOUR_BAND: BandTable = &[("blue", 1), ("green", 2), ("red", 3), ("nir", 4)];
const SENTINEL_1: BandTable = &[("vv", 1), ("vh", 2)];

fn normalize_sensor(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect()
}

pub fn sensor_table(raw: &str) -> Option<BandTable> {
    match normalize_sensor(raw).as_str() {
        "landsat8" | "landsat9" | "l8" | "l9" => Some(LANDSAT_8_9),
        "landsat4" | "landsat5" | "landsat7" | "l4" | "l5" | "l7" => Some(LANDSAT_4_5_7),
        "sentinel2" | "s2" => Some(SENTINEL_2),
        "planetscope" | "planet" | "spot" | "spot6" | "spot7" => Some(FOUR_BAND),
        "sentinel1" | "s1" => Some(SENTINEL_1),
        _ => None,
    }
}

/// Canonical spectral index name (accepts friendly aliases for water-quality indices).
pub fn normalize_index_name(raw: &str) -> String {
    let low = raw.trim().to_ascii_lowercase();
    match low.as_str() {
        "tss" => "nsmi".into(),
        "chla" | "chl-a" | "chlorophyll" => "ndci".into(),
        _ => low,
    }
}

pub fn index_roles(index: &str) -> Option<&'static [&'static str]> {
    Some(match index {
        "ndvi" | "savi" => &["nir", "red"],
        "ndwi" => &["green", "nir"],
        "ndbi" => &["swir1", "nir"],
        "nbr" => &["nir", "swir2"],
        "mndwi" => &["green", "swir1"],
        "ndre" => &["nir", "rededge"],
        "ndti" => &["red", "green"],
        "ndci" => &["rededge", "red"],
        "evi" => &["nir", "red", "blue"],
        "bsi" => &["swir1", "red", "nir", "blue"],
        "vari" | "exg" | "nsmi" => &["red", "green", "blue"],
        "lst" => &["thermal"],
        _ => return None,
    })
}

/// Indices computed as a plain (A-B)/(A+B) normalized difference of two separate band files.
pub fn is_two_band_index(index: &str) -> bool {
    matches!(
        index,
        "ndvi" | "ndwi" | "ndbi" | "savi" | "nbr" | "mndwi" | "ndre" | "ndti" | "ndci"
    )
}

pub const ALL_INDEX_NAMES: &str =
    "ndvi, ndwi, ndbi, savi, nbr, mndwi, evi, bsi, ndre, vari, exg, lst, ndti, nsmi (alias tss), ndci (alias chla)";

pub fn composite_roles(preset: &str) -> Option<[&'static str; 3]> {
    Some(match preset.trim().to_ascii_lowercase().as_str() {
        "true-color" | "truecolor" | "natural" => ["red", "green", "blue"],
        "cir" | "color-infrared" | "vegetation" => ["nir", "red", "green"],
        "urban" => ["swir2", "swir1", "red"],
        "agriculture" => ["swir1", "nir", "blue"],
        "land-water" | "landwater" => ["nir", "swir1", "red"],
        "geology" => ["swir2", "swir1", "blue"],
        "sar" => ["vv", "vh", "vv"],
        _ => return None,
    })
}

pub const ALL_COMPOSITE_PRESETS: &str =
    "true-color, cir, urban, agriculture, land-water, geology, sar";

/// Parses `"red=4,nir=5"` style overrides into role -> 1-based band number.
pub fn parse_band_overrides(raw: &str) -> Result<BTreeMap<String, u32>> {
    let mut out = BTreeMap::new();
    for part in raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let Some((role, num)) = part.split_once('=') else {
            bail!("Invalid --bands entry '{part}'. Expected role=number (e.g. red=4,nir=5).");
        };
        let n: u32 = num
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid band number '{}' in '{part}'.", num.trim()))?;
        if n == 0 {
            bail!("Band numbers are 1-based; got 0 in '{part}'.");
        }
        out.insert(role.trim().to_ascii_lowercase(), n);
    }
    Ok(out)
}

/// Resolves the 1-based band number for every requested role from the sensor table, letting
/// explicit `--bands` overrides win.
pub fn resolve_roles(
    sensor: &str,
    roles: &[&str],
    overrides: &BTreeMap<String, u32>,
) -> Result<BTreeMap<String, u32>> {
    let Some(table) = sensor_table(sensor) else {
        bail!(
            "Unknown sensor '{sensor}'. Supported: landsat5/7, landsat8/9, sentinel2, planetscope/spot, sentinel1."
        );
    };
    let mut resolved = BTreeMap::new();
    for role in roles {
        if let Some(n) = overrides.get(*role) {
            resolved.insert((*role).to_string(), *n);
        } else if let Some((_, n)) = table.iter().find(|(r, _)| r == role) {
            resolved.insert((*role).to_string(), *n);
        } else {
            let avail: Vec<&str> = table.iter().map(|(r, _)| *r).collect();
            bail!(
                "Sensor '{sensor}' has no default '{role}' band (available roles: {}). Provide it explicitly, e.g. --bands \"{role}=<band number>\".",
                avail.join(", ")
            );
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensor_band_maps_and_overrides() {
        let none = BTreeMap::new();
        let l8 = resolve_roles("Landsat-8", &["red", "nir", "thermal"], &none).unwrap();
        assert_eq!(l8["red"], 4);
        assert_eq!(l8["nir"], 5);
        assert_eq!(l8["thermal"], 10);

        let s2 = resolve_roles("sentinel2", &["rededge", "swir1"], &none).unwrap();
        assert_eq!((s2["rededge"], s2["swir1"]), (5, 11));

        let ov = parse_band_overrides("red=3, NIR=7").unwrap();
        let custom = resolve_roles("landsat8", &["red", "nir"], &ov).unwrap();
        assert_eq!((custom["red"], custom["nir"]), (3, 7));

        // Landsat has no red-edge, SAR has no optical roles
        assert!(resolve_roles("landsat8", &["rededge"], &none).is_err());
        assert!(resolve_roles("sentinel1", &["nir"], &none).is_err());
        assert!(resolve_roles("hubble", &["red"], &none).is_err());
        assert!(parse_band_overrides("red").is_err());
        assert!(parse_band_overrides("red=0").is_err());
    }

    #[test]
    fn test_index_and_composite_tables() {
        assert_eq!(normalize_index_name("TSS"), "nsmi");
        assert_eq!(normalize_index_name("Chl-a"), "ndci");
        assert_eq!(index_roles("evi").unwrap(), &["nir", "red", "blue"]);
        assert!(is_two_band_index("mndwi") && !is_two_band_index("evi"));
        assert!(index_roles("bogus").is_none());
        assert_eq!(composite_roles("cir").unwrap(), ["nir", "red", "green"]);
        assert_eq!(composite_roles("sar").unwrap(), ["vv", "vh", "vv"]);
        assert!(composite_roles("nope").is_none());
    }
}
