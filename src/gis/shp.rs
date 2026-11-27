use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

/// Exact WGS84/GRS80 Inverse Transverse Mercator (projected meters -> Lon/Lat degrees).
/// Used for both standard UTM (`k0=0.9996`, `FE=500000`) and Indonesian DGN95 TM-3° BPN (`k0=0.9999`, `FE=200000`, `FN=1500000`).
pub fn inverse_transverse_mercator(
    easting: f64,
    northing: f64,
    lon0_deg: f64,
    k0: f64,
    false_easting: f64,
    false_northing: f64,
) -> (f64, f64) {
    const A: f64 = 6378137.0;
    const E2: f64 = 0.00669437999014;
    let e_prime2 = E2 / (1.0 - E2);
    let e1 = (1.0 - (1.0 - E2).sqrt()) / (1.0 + (1.0 - E2).sqrt());

    let x = easting - false_easting;
    let y = northing - false_northing;

    let m = y / k0;
    let mu = m / (A * (1.0 - E2 / 4.0 - 3.0 * E2 * E2 / 64.0 - 5.0 * E2.powi(3) / 256.0));

    let phi1 = mu
        + (3.0 * e1 / 2.0 - 27.0 * e1.powi(3) / 32.0) * (2.0 * mu).sin()
        + (21.0 * e1 * e1 / 16.0 - 55.0 * e1.powi(4) / 32.0) * (4.0 * mu).sin()
        + (151.0 * e1.powi(3) / 96.0) * (6.0 * mu).sin()
        + (1097.0 * e1.powi(4) / 512.0) * (8.0 * mu).sin();

    let sin_p = phi1.sin();
    let cos_p = phi1.cos();
    let tan_p = phi1.tan();

    let n1 = A / (1.0 - E2 * sin_p * sin_p).sqrt();
    let t1 = tan_p * tan_p;
    let c1 = e_prime2 * cos_p * cos_p;
    let r1 = A * (1.0 - E2) / (1.0 - E2 * sin_p * sin_p).powf(1.5);
    let d = x / (n1 * k0);

    let lat = phi1
        - (n1 * tan_p / r1)
            * (d * d / 2.0
                - (5.0 + 3.0 * t1 + 10.0 * c1 - 4.0 * c1 * c1 - 9.0 * e_prime2) * d.powi(4) / 24.0
                + (61.0 + 90.0 * t1 + 298.0 * c1 + 45.0 * t1 * t1
                    - 252.0 * e_prime2
                    - 3.0 * c1 * c1)
                    * d.powi(6)
                    / 720.0);

    let lon0 = lon0_deg.to_radians();
    let lon = lon0
        + (d - (1.0 + 2.0 * t1 + c1) * d.powi(3) / 6.0
            + (5.0 - 2.0 * c1 + 28.0 * t1 - 3.0 * c1 * c1 + 8.0 * e_prime2 + 24.0 * t1 * t1)
                * d.powi(5)
                / 120.0)
            / cos_p;

    (lon.to_degrees(), lat.to_degrees())
}

/// Exact WGS84 Inverse Transverse Mercator (UTM Easting/Northing -> Lon/Lat degrees).
pub fn utm_to_wgs84(easting: f64, northing: f64, zone: u8, south: bool) -> (f64, f64) {
    let lon0_deg = (zone as f64 - 1.0) * 6.0 - 180.0 + 3.0;
    let false_northing = if south { 10_000_000.0 } else { 0.0 };
    inverse_transverse_mercator(
        easting,
        northing,
        lon0_deg,
        0.9996,
        500_000.0,
        false_northing,
    )
}

/// Exact Indonesian DGN95 TM-3° BPN (`EPSG:23830–23845`) -> WGS84 Lon/Lat degrees.
pub fn dgn95_tm3_to_wgs84(easting: f64, northing: f64, central_meridian_deg: f64) -> (f64, f64) {
    inverse_transverse_mercator(
        easting,
        northing,
        central_meridian_deg,
        0.9999,
        200_000.0,
        1_500_000.0,
    )
}

/// Exact WGS84 Inverse Web Mercator (EPSG:3857 meters -> Lon/Lat degrees).
pub fn web_mercator_to_wgs84(x: f64, y: f64) -> (f64, f64) {
    const R: f64 = 6378137.0;
    let lon = (x / R).to_degrees();
    let lat = (2.0 * (y / R).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
    (lon, lat)
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrjCrs {
    Geographic,
    Utm(u8, bool),
    Dgn95Tm3(f64),
    WebMercator,
    UnsupportedProjected(String),
}

/// Detects CRS type from a shapefile's `.prj` WKT text so unsupported projected CRS
/// are rejected rather than silently treated as WGS84 lon/lat degrees.
pub fn detect_crs_from_prj(shp_path: &Path) -> PrjCrs {
    let prj = shp_path.with_extension("prj");
    let Ok(text) = fs::read_to_string(&prj) else {
        return PrjCrs::Geographic;
    };
    let upper = text.to_ascii_uppercase();
    if let Some((zone, south)) = parse_utm_wkt(&upper) {
        return PrjCrs::Utm(zone, south);
    }
    if let Some(cm_deg) = parse_dgn95_tm3_wkt(&upper) {
        return PrjCrs::Dgn95Tm3(cm_deg);
    }
    if upper.contains("MERCATOR_AUXILIARY_SPHERE")
        || upper.contains("PSEUDO_MERCATOR")
        || upper.contains("WEB_MERCATOR")
        || upper.contains("EPSG\",3857")
        || upper.contains("EPSG\",\"3857")
    {
        return PrjCrs::WebMercator;
    }
    if upper.trim_start().starts_with("PROJCS[") {
        let name = text
            .split('"')
            .nth(1)
            .unwrap_or("Unknown Projected CRS")
            .to_string();
        return PrjCrs::UnsupportedProjected(name);
    }
    PrjCrs::Geographic
}

fn parse_dgn95_tm3_wkt(upper: &str) -> Option<f64> {
    if !upper.contains("TRANSVERSE_MERCATOR") && !upper.contains("TM-3") && !upper.contains("TM3") {
        return None;
    }
    let sf = extract_wkt_param(upper, "SCALE_FACTOR");
    let fe = extract_wkt_param(upper, "FALSE_EASTING");
    let fn_val = extract_wkt_param(upper, "FALSE_NORTHING");
    let is_tm3_params = sf.is_some_and(|v| (v - 0.9999).abs() < 1e-5)
        && fe.is_some_and(|v| (v - 200_000.0).abs() < 1.0)
        && fn_val.is_some_and(|v| (v - 1_500_000.0).abs() < 1.0);
    if is_tm3_params {
        if let Some(cm) = extract_wkt_param(upper, "CENTRAL_MERIDIAN") {
            return Some(cm);
        }
    }
    // Fallback: match EPSG:23830..=23845 directly
    for epsg in 23830u32..=23845 {
        let p1 = format!("EPSG\",{epsg}");
        let p2 = format!("EPSG\",\"{epsg}\"");
        if upper.contains(&p1) || upper.contains(&p2) {
            return Some(94.5 + (epsg - 23830) as f64 * 3.0);
        }
    }
    None
}

fn parse_utm_wkt(upper: &str) -> Option<(u8, bool)> {
    if let Some(pos) = upper.find("UTM_ZONE_") {
        let rest = &upper[pos + 9..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(zone) = digits.parse::<u8>() {
            let after = &rest[digits.len()..];
            let south = after.starts_with('S');
            return Some((zone, south));
        }
    }
    if upper.contains("TRANSVERSE_MERCATOR") {
        // Distinguish standard UTM (scale_factor ~0.9996, false_easting 500000) from TM-3° / custom TM
        let sf = extract_wkt_param(upper, "SCALE_FACTOR").unwrap_or(0.9996);
        let fe = extract_wkt_param(upper, "FALSE_EASTING").unwrap_or(500_000.0);
        if (sf - 0.9996).abs() < 0.0002 && (fe - 500_000.0).abs() < 1.0 {
            let cm = extract_wkt_param(upper, "CENTRAL_MERIDIAN");
            let fn_val = extract_wkt_param(upper, "FALSE_NORTHING").unwrap_or(0.0);
            if let Some(cm_deg) = cm {
                let zone = (((cm_deg + 183.0) / 6.0).round() as i32).clamp(1, 60) as u8;
                return Some((zone, fn_val > 5_000_000.0));
            }
        }
    }
    None
}

/// Detects UTM zone and hemisphere `(zone, is_south)` from a shapefile's `.prj` WKT text.
pub fn detect_utm_from_prj(shp_path: &Path) -> Option<(u8, bool)> {
    match detect_crs_from_prj(shp_path) {
        PrjCrs::Utm(z, s) => Some((z, s)),
        _ => None,
    }
}

fn extract_wkt_param(upper: &str, name: &str) -> Option<f64> {
    let key = format!("\"{}\",", name);
    let pos = upper.find(&key)?;
    let rest = &upper[pos + key.len()..];
    let num: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    num.parse::<f64>().ok()
}

fn read_f64_le(buf: &[u8], off: usize) -> f64 {
    f64::from_le_bytes(buf[off..off + 8].try_into().unwrap_or([0; 8]))
}

fn read_i32_le(buf: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(buf[off..off + 4].try_into().unwrap_or([0; 4]))
}

fn read_i32_be(buf: &[u8], off: usize) -> i32 {
    i32::from_be_bytes(buf[off..off + 4].try_into().unwrap_or([0; 4]))
}

fn read_u16_le(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(buf[off..off + 2].try_into().unwrap_or([0; 2]))
}

fn read_u32_le(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap_or([0; 4]))
}

fn transform_xy(x: f64, y: f64, crs: &PrjCrs) -> (f64, f64) {
    match crs {
        PrjCrs::Utm(zone, south) if x.abs() > 180.0 || y.abs() > 90.0 => {
            utm_to_wgs84(x, y, *zone, *south)
        }
        PrjCrs::Dgn95Tm3(cm_deg) if x.abs() > 180.0 || y.abs() > 90.0 => {
            dgn95_tm3_to_wgs84(x, y, *cm_deg)
        }
        PrjCrs::WebMercator if x.abs() > 180.0 || y.abs() > 90.0 => web_mercator_to_wgs84(x, y),
        _ => (x, y),
    }
}

/// Reads the WGS84 bounding box `(min_lon, min_lat, max_lon, max_lat)` of a `.shp` or `.geojson`.
pub fn read_wgs84_bbox(path: &Path) -> Result<(f64, f64, f64, f64)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "shp" {
        let data = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
        if data.len() < 100 {
            bail!("Invalid shapefile (too small): {}", path.display());
        }
        let xmin = read_f64_le(&data, 36);
        let ymin = read_f64_le(&data, 44);
        let xmax = read_f64_le(&data, 52);
        let ymax = read_f64_le(&data, 60);
        let crs = detect_crs_from_prj(path);
        if let PrjCrs::UnsupportedProjected(name) = &crs {
            bail!(
                "Unsupported projected CRS '{}' in {}. Reproject to WGS84 (EPSG:4326) or UTM with `geokit gp project` first.",
                name,
                path.display()
            );
        }
        let (lon1, lat1) = transform_xy(xmin, ymin, &crs);
        let (lon2, lat2) = transform_xy(xmax, ymax, &crs);
        let (lon3, lat3) = transform_xy(xmin, ymax, &crs);
        let (lon4, lat4) = transform_xy(xmax, ymin, &crs);
        let min_lon = lon1.min(lon2).min(lon3).min(lon4);
        let max_lon = lon1.max(lon2).max(lon3).max(lon4);
        let min_lat = lat1.min(lat2).min(lat3).min(lat4);
        let max_lat = lat1.max(lat2).max(lat3).max(lat4);
        if min_lon < -180.1 || max_lon > 180.1 || min_lat < -90.1 || max_lat > 90.1 {
            bail!(
                "Could not determine WGS84 extent for {} (missing .prj CRS definition?). Pass --bbox explicitly or define/reproject CRS first.",
                path.display()
            );
        }
        return Ok((min_lon, min_lat, max_lon, max_lat));
    }
    if ext == "geojson" || ext == "json" {
        let text = fs::read_to_string(path)?;
        let val: Value = serde_json::from_str(&text)?;
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        collect_geojson_bounds(&val, &mut min_x, &mut min_y, &mut max_x, &mut max_y);
        if !min_x.is_finite() {
            bail!("No coordinates found in {}", path.display());
        }
        return Ok((min_x, min_y, max_x, max_y));
    }
    bail!(
        "Unsupported AOI file format '{}'. Use .shp, .geojson, or --bbox.",
        path.display()
    );
}

fn collect_geojson_bounds(
    v: &Value,
    min_x: &mut f64,
    min_y: &mut f64,
    max_x: &mut f64,
    max_y: &mut f64,
) {
    match v {
        Value::Object(m) => {
            if let Some(coords) = m.get("coordinates") {
                collect_coord_array(coords, min_x, min_y, max_x, max_y);
            }
            if let Some(feats) = m.get("features") {
                collect_geojson_bounds(feats, min_x, min_y, max_x, max_y);
            }
            if let Some(geom) = m.get("geometry") {
                collect_geojson_bounds(geom, min_x, min_y, max_x, max_y);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_geojson_bounds(item, min_x, min_y, max_x, max_y);
            }
        }
        _ => {}
    }
}

fn collect_coord_array(
    v: &Value,
    min_x: &mut f64,
    min_y: &mut f64,
    max_x: &mut f64,
    max_y: &mut f64,
) {
    if let Value::Array(arr) = v {
        if arr.len() >= 2 {
            if let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) {
                if x.is_finite() && y.is_finite() {
                    *min_x = min_x.min(x);
                    *min_y = min_y.min(y);
                    *max_x = max_x.max(x);
                    *max_y = max_y.max(y);
                    return;
                }
            }
        }
        for item in arr {
            collect_coord_array(item, min_x, min_y, max_x, max_y);
        }
    }
}

/// Reads all physical records from a `.dbf` file preserving 1-to-1 record indices
/// (`None` for records marked deleted with `*`), with strict header/capacity bounds checking.
pub fn read_dbf_records_indexed(dbf_path: &Path) -> Result<Vec<Option<Map<String, Value>>>> {
    let data =
        fs::read(dbf_path).with_context(|| format!("Failed to read {}", dbf_path.display()))?;
    if data.len() < 32 {
        bail!(
            "Invalid DBF header (file too small): {}",
            dbf_path.display()
        );
    }
    let declared_records = read_u32_le(&data, 4) as usize;
    let header_len = read_u16_le(&data, 8) as usize;
    let record_len = read_u16_le(&data, 10) as usize;

    if header_len < 32 || header_len > data.len() {
        bail!(
            "Invalid DBF header length ({header_len}) in {}",
            dbf_path.display()
        );
    }
    if record_len == 0 {
        return Ok(Vec::new());
    }
    // Clamp allocation to physical records that can actually fit in the file (prevents OOM on corrupt headers)
    let max_possible = (data.len() - header_len) / record_len;
    let num_records = declared_records.min(max_possible);

    let mut fields: Vec<(String, u8, usize)> = Vec::new();
    let mut off = 32;
    while off + 32 <= data.len() && off < header_len {
        if data[off] == 0x0D {
            break;
        }
        let name_end = data[off..off + 11]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(11);
        let name = String::from_utf8_lossy(&data[off..off + name_end])
            .trim()
            .to_string();
        let ftype = data[off + 11];
        let flen = data[off + 16] as usize;
        fields.push((name, ftype, flen));
        off += 32;
    }

    let mut out = Vec::with_capacity(num_records);
    let mut rec_off = header_len;
    for _ in 0..num_records {
        if rec_off + record_len > data.len() {
            break;
        }
        let deleted = data[rec_off] == b'*';
        if deleted {
            out.push(None);
            rec_off += record_len;
            continue;
        }
        let mut map = Map::new();
        let mut f_off = rec_off + 1;
        for (name, ftype, flen) in &fields {
            if f_off + *flen > data.len() {
                break;
            }
            let raw = String::from_utf8_lossy(&data[f_off..f_off + *flen])
                .trim()
                .to_string();
            let val = match *ftype {
                b'N' | b'F' => {
                    if raw.is_empty() {
                        Value::Null
                    } else if let Ok(i) = raw.parse::<i64>() {
                        json!(i)
                    } else if let Ok(f) = raw.parse::<f64>() {
                        json!(f)
                    } else {
                        json!(raw)
                    }
                }
                b'L' => json!(matches!(raw.as_str(), "T" | "t" | "Y" | "y")),
                _ => json!(raw),
            };
            map.insert(name.clone(), val);
            f_off += *flen;
        }
        out.push(Some(map));
        rec_off += record_len;
    }
    Ok(out)
}

/// Reads all active (non-deleted) attribute records from a `.dbf` file as a Vec of JSON property maps.
pub fn read_dbf_records(dbf_path: &Path) -> Result<Vec<Map<String, Value>>> {
    Ok(read_dbf_records_indexed(dbf_path)?
        .into_iter()
        .flatten()
        .collect())
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

fn simplify_ring(pts: &[(f64, f64)], tol: f64, is_ring: bool) -> Vec<[f64; 2]> {
    if tol <= 0.0 || pts.len() <= 4 {
        return pts.iter().map(|&(x, y)| [round6(x), round6(y)]).collect();
    }
    let tol2 = tol * tol;
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(pts.len() / 2 + 2);
    let mut last = pts[0];
    out.push([round6(last.0), round6(last.1)]);
    for &p in &pts[1..pts.len() - 1] {
        let dx = p.0 - last.0;
        let dy = p.1 - last.1;
        if dx * dx + dy * dy >= tol2 {
            out.push([round6(p.0), round6(p.1)]);
            last = p;
        }
    }
    let end = *pts.last().unwrap();
    out.push([round6(end.0), round6(end.1)]);
    if is_ring && out.len() < 4 {
        return pts.iter().map(|&(x, y)| [round6(x), round6(y)]).collect();
    }
    out
}

/// Converts a `.shp` (+ `.dbf` + `.prj`) or `.geojson` into a clean WGS84 GeoJSON FeatureCollection.
pub fn read_geojson_wgs84(path: &Path, simplify_tol_deg: f64) -> Result<Value> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "geojson" || ext == "json" {
        let text = fs::read_to_string(path)?;
        return Ok(serde_json::from_str(&text)?);
    }
    if ext != "shp" {
        bail!(
            "Unsupported vector file: {} (use .shp or .geojson)",
            path.display()
        );
    }

    let shp = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    if shp.len() < 100 {
        bail!("Invalid shapefile: {}", path.display());
    }
    let crs = detect_crs_from_prj(path);
    if let PrjCrs::UnsupportedProjected(name) = &crs {
        bail!(
            "Unsupported projected CRS '{}' in {}. Reproject to WGS84 (EPSG:4326) or UTM with `geokit gp project` first.",
            name,
            path.display()
        );
    }
    let dbf_path = path.with_extension("dbf");
    let has_dbf = dbf_path.is_file();
    let records = if has_dbf {
        read_dbf_records_indexed(&dbf_path)?
    } else {
        Vec::new()
    };

    let mut features = Vec::new();
    let mut pos = 100usize;
    let mut rec_idx = 0usize;

    while pos + 8 <= shp.len() {
        let content_words = read_i32_be(&shp, pos + 4) as usize;
        let content_len = content_words * 2;
        pos += 8;
        if pos + content_len > shp.len() || content_len < 4 {
            break;
        }
        let shape_type = read_i32_le(&shp, pos);
        let dbf_slot = records.get(rec_idx);
        rec_idx += 1;

        // If the DBF record at this physical index is marked deleted (`*`), skip geometry too
        // so subsequent attributes NEVER shift onto the wrong geometry!
        if matches!(dbf_slot, Some(None)) {
            pos += content_len;
            continue;
        }
        let props = dbf_slot
            .and_then(|opt| opt.as_ref())
            .cloned()
            .unwrap_or_else(Map::new);

        let geometry = match shape_type {
            1 | 11 | 21 if content_len >= 20 => {
                let x = read_f64_le(&shp, pos + 4);
                let y = read_f64_le(&shp, pos + 12);
                let (lon, lat) = transform_xy(x, y, &crs);
                Some(json!({ "type": "Point", "coordinates": [round6(lon), round6(lat)] }))
            }
            3 | 13 | 23 | 5 | 15 | 25 if content_len >= 44 => {
                let is_poly = matches!(shape_type, 5 | 15 | 25);
                let num_parts = read_i32_le(&shp, pos + 36) as usize;
                let num_points = read_i32_le(&shp, pos + 40) as usize;
                let parts_off = pos + 44;
                let pts_off = parts_off + num_parts * 4;
                if pts_off + num_points * 16 <= pos + content_len && num_parts > 0 {
                    let mut part_starts = Vec::with_capacity(num_parts + 1);
                    for i in 0..num_parts {
                        part_starts.push(read_i32_le(&shp, parts_off + i * 4) as usize);
                    }
                    part_starts.push(num_points);

                    let mut parts_coords: Vec<Vec<[f64; 2]>> = Vec::with_capacity(num_parts);
                    for w in part_starts.windows(2) {
                        let mut ring = Vec::with_capacity(w[1].saturating_sub(w[0]));
                        for pi in w[0]..w[1] {
                            let o = pts_off + pi * 16;
                            let (lon, lat) =
                                transform_xy(read_f64_le(&shp, o), read_f64_le(&shp, o + 8), &crs);
                            ring.push((lon, lat));
                        }
                        if !ring.is_empty() {
                            parts_coords.push(simplify_ring(&ring, simplify_tol_deg, is_poly));
                        }
                    }

                    if is_poly {
                        // Group outer rings (clockwise in shapefile) & holes into MultiPolygon safely
                        let mut polys: Vec<Vec<Vec<[f64; 2]>>> = Vec::new();
                        for ring in parts_coords {
                            let area = signed_ring_area(&ring);
                            if area <= 0.0 || polys.is_empty() {
                                // Clockwise = outer ring in ESRI Shapefile spec
                                polys.push(vec![ring]);
                            } else {
                                // Counter-clockwise = hole of previous polygon
                                polys.last_mut().unwrap().push(ring);
                            }
                        }
                        if polys.len() == 1 {
                            Some(json!({ "type": "Polygon", "coordinates": polys[0] }))
                        } else {
                            Some(json!({ "type": "MultiPolygon", "coordinates": polys }))
                        }
                    } else if parts_coords.len() == 1 {
                        Some(json!({ "type": "LineString", "coordinates": parts_coords[0] }))
                    } else {
                        Some(json!({ "type": "MultiLineString", "coordinates": parts_coords }))
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(geom) = geometry {
            features.push(json!({
                "type": "Feature",
                "properties": props,
                "geometry": geom,
            }));
        }
        pos += content_len;
    }

    Ok(json!({
        "type": "FeatureCollection",
        "features": features,
    }))
}

fn signed_ring_area(ring: &[[f64; 2]]) -> f64 {
    let mut sum = 0.0;
    for w in ring.windows(2) {
        sum += (w[1][0] - w[0][0]) * (w[1][1] + w[0][1]);
    }
    // Positive here means clockwise in standard X-right Y-up coords
    -sum * 0.5
}

const WGS84_PRJ_WKT: &str = r#"GEOGCS["GCS_WGS_1984",DATUM["D_WGS_1984",SPHEROID["WGS_1984",6378137.0,298.257223563]],PRIMEM["Greenwich",0.0],UNIT["Degree",0.0174532925199433]]"#;

fn write_shp_header(shape_type: i32, len_bytes: usize, bbox: [f64; 4]) -> Vec<u8> {
    let mut h = vec![0u8; 100];
    h[0..4].copy_from_slice(&9994_i32.to_be_bytes());
    h[24..28].copy_from_slice(&((len_bytes / 2) as i32).to_be_bytes());
    h[28..32].copy_from_slice(&1000_i32.to_le_bytes());
    h[32..36].copy_from_slice(&shape_type.to_le_bytes());
    h[36..44].copy_from_slice(&bbox[0].to_le_bytes());
    h[44..52].copy_from_slice(&bbox[1].to_le_bytes());
    h[52..60].copy_from_slice(&bbox[2].to_le_bytes());
    h[60..68].copy_from_slice(&bbox[3].to_le_bytes());
    h
}

fn write_dbf_and_prj_wgs84(shp_path: &Path, rows: &[Map<String, Value>]) -> Result<()> {
    let mut cols: Vec<(String, String, u8, u8)> = Vec::new(); // (orig_key, dbf_name, type, len)
    for k in rows[0].keys() {
        let is_num = rows
            .iter()
            .all(|r| matches!(r.get(k), None | Some(Value::Null) | Some(Value::Number(_))));
        let is_float = is_num
            && rows
                .iter()
                .any(|r| matches!(r.get(k), Some(Value::Number(n)) if !n.is_i64() && !n.is_u64()));
        let mut dbf_name: String = k
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .take(10)
            .collect();
        if dbf_name.is_empty() {
            dbf_name = "FIELD".into();
        }
        let max_str = rows
            .iter()
            .filter_map(|r| r.get(k))
            .map(|v| match v {
                Value::String(s) => s.len(),
                Value::Null => 0,
                other => other.to_string().len(),
            })
            .max()
            .unwrap_or(16);
        let (ftype, flen) = if is_num {
            if is_float {
                (b'F', 18u8)
            } else {
                (b'N', 12u8)
            }
        } else {
            (b'C', (max_str.clamp(8, 128)) as u8)
        };
        cols.push((k.clone(), dbf_name, ftype, flen));
    }
    let header_len = (32 + cols.len() * 32 + 1) as u16;
    let record_len = (1 + cols.iter().map(|c| c.3 as usize).sum::<usize>()) as u16;
    let mut dbf = vec![0u8; header_len as usize];
    dbf[0] = 0x03;
    dbf[4..8].copy_from_slice(&(rows.len() as u32).to_le_bytes());
    dbf[8..10].copy_from_slice(&header_len.to_le_bytes());
    dbf[10..12].copy_from_slice(&record_len.to_le_bytes());
    for (i, (_, dname, ftype, flen)) in cols.iter().enumerate() {
        let base = 32 + i * 32;
        let nb = dname.as_bytes();
        let copy_n = nb.len().min(10);
        dbf[base..base + copy_n].copy_from_slice(&nb[..copy_n]);
        dbf[base + 11] = *ftype;
        dbf[base + 16] = *flen;
        dbf[base + 17] = if *ftype == b'F' { 6 } else { 0 };
    }
    dbf[header_len as usize - 1] = 0x0D;
    for row in rows {
        dbf.push(b' ');
        for (orig_key, _, ftype, flen) in &cols {
            let len = *flen as usize;
            let mut cell = vec![b' '; len];
            let s = match row.get(orig_key) {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => {
                    if *ftype == b'F' {
                        format!("{:.6}", n.as_f64().unwrap_or(0.0))
                    } else {
                        n.to_string()
                    }
                }
                Some(other) => other.to_string(),
            };
            let b = s.as_bytes();
            let n = b.len().min(len);
            if *ftype == b'N' || *ftype == b'F' {
                cell[len - n..].copy_from_slice(&b[..n]);
            } else {
                cell[..n].copy_from_slice(&b[..n]);
            }
            dbf.extend_from_slice(&cell);
        }
    }
    dbf.push(0x1A);

    fs::write(shp_path.with_extension("dbf"), dbf)?;
    fs::write(shp_path.with_extension("prj"), WGS84_PRJ_WKT)?;
    Ok(())
}

/// Writes a valid ESRI Point Shapefile bundle (`.shp`, `.shx`, `.dbf`, `.prj` EPSG:4326)
/// natively in pure Rust without external ArcGIS or GDAL dependencies.
pub fn write_point_shapefile_wgs84(
    shp_path: &Path,
    points: &[(f64, f64)],
    rows: &[Map<String, Value>],
) -> Result<()> {
    if points.len() != rows.len() {
        bail!(
            "Point count ({}) does not match row count ({}).",
            points.len(),
            rows.len()
        );
    }
    if points.is_empty() {
        bail!("Cannot write empty shapefile: {}", shp_path.display());
    }
    if let Some(parent) = shp_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut shp_body = Vec::with_capacity(points.len() * 28);
    let mut shx_body = Vec::with_capacity(points.len() * 8);
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for (idx, &(x, y)) in points.iter().enumerate() {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        let offset_words = ((100 + shp_body.len()) / 2) as i32;
        shx_body.extend_from_slice(&offset_words.to_be_bytes());
        shx_body.extend_from_slice(&10_i32.to_be_bytes());

        shp_body.extend_from_slice(&((idx as i32) + 1).to_be_bytes());
        shp_body.extend_from_slice(&10_i32.to_be_bytes());
        shp_body.extend_from_slice(&1_i32.to_le_bytes());
        shp_body.extend_from_slice(&x.to_le_bytes());
        shp_body.extend_from_slice(&y.to_le_bytes());
    }
    let bbox = [min_x, min_y, max_x, max_y];
    let mut shp_all = write_shp_header(1, 100 + shp_body.len(), bbox);
    shp_all.extend_from_slice(&shp_body);
    let mut shx_all = write_shp_header(1, 100 + shx_body.len(), bbox);
    shx_all.extend_from_slice(&shx_body);

    fs::write(shp_path, shp_all)?;
    fs::write(shp_path.with_extension("shx"), shx_all)?;
    write_dbf_and_prj_wgs84(shp_path, rows)
}

/// Writes a valid ESRI Polygon Shapefile bundle (`.shp`, `.shx`, `.dbf`, `.prj` EPSG:4326)
/// natively in pure Rust. Each polygon is represented by a closed outer ring `[lon, lat]`.
pub fn write_polygon_shapefile_wgs84(
    shp_path: &Path,
    polygons: &[Vec<[f64; 2]>],
    rows: &[Map<String, Value>],
) -> Result<()> {
    if polygons.len() != rows.len() {
        bail!(
            "Polygon count ({}) does not match row count ({}).",
            polygons.len(),
            rows.len()
        );
    }
    if polygons.is_empty() {
        bail!(
            "Cannot write empty polygon shapefile: {}",
            shp_path.display()
        );
    }
    if let Some(parent) = shp_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut shp_body = Vec::new();
    let mut shx_body = Vec::with_capacity(polygons.len() * 8);
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );

    for (idx, raw_ring) in polygons.iter().enumerate() {
        if raw_ring.len() < 3 {
            bail!("Polygon {} has fewer than 3 vertices.", idx + 1);
        }
        let mut ring = raw_ring.clone();
        if ring.first() != ring.last() {
            ring.push(ring[0]);
        }
        // Ensure clockwise orientation for ESRI Shapefile outer ring (signed area < 0)
        let mut signed_area = 0.0;
        for w in ring.windows(2) {
            signed_area += w[0][0] * w[1][1] - w[1][0] * w[0][1];
        }
        if signed_area > 0.0 {
            ring.reverse();
        }

        let (mut px0, mut py0, mut px1, mut py1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for &[x, y] in &ring {
            px0 = px0.min(x);
            py0 = py0.min(y);
            px1 = px1.max(x);
            py1 = py1.max(y);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }

        let content_bytes = 48 + ring.len() * 16;
        let content_words = (content_bytes / 2) as i32;
        let offset_words = ((100 + shp_body.len()) / 2) as i32;
        shx_body.extend_from_slice(&offset_words.to_be_bytes());
        shx_body.extend_from_slice(&content_words.to_be_bytes());

        shp_body.extend_from_slice(&((idx as i32) + 1).to_be_bytes());
        shp_body.extend_from_slice(&content_words.to_be_bytes());
        shp_body.extend_from_slice(&5_i32.to_le_bytes()); // Polygon
        shp_body.extend_from_slice(&px0.to_le_bytes());
        shp_body.extend_from_slice(&py0.to_le_bytes());
        shp_body.extend_from_slice(&px1.to_le_bytes());
        shp_body.extend_from_slice(&py1.to_le_bytes());
        shp_body.extend_from_slice(&1_i32.to_le_bytes()); // NumParts
        shp_body.extend_from_slice(&(ring.len() as i32).to_le_bytes()); // NumPoints
        shp_body.extend_from_slice(&0_i32.to_le_bytes()); // Parts[0] = 0
        for &[x, y] in &ring {
            shp_body.extend_from_slice(&x.to_le_bytes());
            shp_body.extend_from_slice(&y.to_le_bytes());
        }
    }

    let bbox = [min_x, min_y, max_x, max_y];
    let mut shp_all = write_shp_header(5, 100 + shp_body.len(), bbox);
    shp_all.extend_from_slice(&shp_body);
    let mut shx_all = write_shp_header(5, 100 + shx_body.len(), bbox);
    shx_all.extend_from_slice(&shx_body);

    fs::write(shp_path, shp_all)?;
    fs::write(shp_path.with_extension("shx"), shx_all)?;
    write_dbf_and_prj_wgs84(shp_path, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn build_point_shp(points: &[(f64, f64)]) -> Vec<u8> {
        let mut body = Vec::new();
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for (idx, &(x, y)) in points.iter().enumerate() {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            // Record header: 1-based record number (BE i32) + content length in 16-bit words (10 words = 20 bytes)
            body.extend_from_slice(&((idx as i32) + 1).to_be_bytes());
            body.extend_from_slice(&10_i32.to_be_bytes());
            // Point content: shape type 1 (LE i32) + X (LE f64) + Y (LE f64)
            body.extend_from_slice(&1_i32.to_le_bytes());
            body.extend_from_slice(&x.to_le_bytes());
            body.extend_from_slice(&y.to_le_bytes());
        }
        let total_len_words = ((100 + body.len()) / 2) as i32;
        let mut hdr = vec![0u8; 100];
        hdr[0..4].copy_from_slice(&9994_i32.to_be_bytes());
        hdr[24..28].copy_from_slice(&total_len_words.to_be_bytes());
        hdr[28..32].copy_from_slice(&1000_i32.to_le_bytes());
        hdr[32..36].copy_from_slice(&1_i32.to_le_bytes());
        hdr[36..44].copy_from_slice(&min_x.to_le_bytes());
        hdr[44..52].copy_from_slice(&min_y.to_le_bytes());
        hdr[52..60].copy_from_slice(&max_x.to_le_bytes());
        hdr[60..68].copy_from_slice(&max_y.to_le_bytes());
        hdr.extend_from_slice(&body);
        hdr
    }

    fn build_single_field_dbf(rows: &[(bool, &str)]) -> Vec<u8> {
        let field_len: u8 = 10;
        let header_len: u16 = 32 + 32 + 1;
        let record_len: u16 = 1 + field_len as u16;
        let mut buf = vec![0u8; header_len as usize];
        buf[0] = 0x03;
        buf[4..8].copy_from_slice(&(rows.len() as u32).to_le_bytes());
        buf[8..10].copy_from_slice(&header_len.to_le_bytes());
        buf[10..12].copy_from_slice(&record_len.to_le_bytes());
        // Field descriptor "NAME" (C, len 10)
        buf[32..36].copy_from_slice(b"NAME");
        buf[43] = b'C';
        buf[48] = field_len;
        buf[64] = 0x0D; // header terminator

        for &(deleted, val) in rows {
            buf.push(if deleted { b'*' } else { b' ' });
            let mut cell = [b' '; 10];
            let bytes = val.as_bytes();
            let n = bytes.len().min(10);
            cell[..n].copy_from_slice(&bytes[..n]);
            buf.extend_from_slice(&cell);
        }
        buf.push(0x1A); // EOF marker
        buf
    }

    #[test]
    fn test_utm_49s_kendal_to_wgs84() {
        let (lon, lat) = utm_to_wgs84(412_000.0, 9_232_000.0, 49, true);
        assert!((lon - 110.203).abs() < 0.02, "lon={lon}");
        assert!((lat - (-6.948)).abs() < 0.02, "lat={lat}");
    }

    #[test]
    fn test_geojson_integer_coords_bbox() {
        let dir = tempdir().unwrap();
        let gj = dir.path().join("int_aoi.geojson");
        fs::write(
            &gj,
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"Polygon","coordinates":[[[110,-7],[111,-7],[111,-6],[110,-6],[110,-7]]]}}]}"#,
        )
        .unwrap();
        let bbox = read_wgs84_bbox(&gj).expect("integer GeoJSON coords must parse");
        assert_eq!(bbox, (110.0, -7.0, 111.0, -6.0));
    }

    #[test]
    fn test_dbf_deleted_record_stays_synced_with_shp() {
        let dir = tempdir().unwrap();
        let shp_path = dir.path().join("pts.shp");
        let dbf_path = dir.path().join("pts.dbf");
        fs::write(
            &shp_path,
            build_point_shp(&[(110.0, -7.0), (111.0, -8.0), (112.0, -9.0)]),
        )
        .unwrap();
        fs::write(
            &dbf_path,
            build_single_field_dbf(&[(false, "Alpha"), (true, "Deleted"), (false, "Gamma")]),
        )
        .unwrap();

        let fc = read_geojson_wgs84(&shp_path, 0.0).unwrap();
        let feats = fc["features"].as_array().unwrap();
        assert_eq!(feats.len(), 2, "deleted DBF record must skip its geometry");
        assert_eq!(feats[0]["properties"]["NAME"], "Alpha");
        assert_eq!(feats[0]["geometry"]["coordinates"], json!([110.0, -7.0]));
        assert_eq!(feats[1]["properties"]["NAME"], "Gamma");
        assert_eq!(feats[1]["geometry"]["coordinates"], json!([112.0, -9.0]));
    }

    #[test]
    fn test_dbf_corrupt_huge_num_records_does_not_oom() {
        let dir = tempdir().unwrap();
        let dbf_path = dir.path().join("corrupt.dbf");
        let mut buf = build_single_field_dbf(&[]);
        // Overwrite num_records with 0x7FFF_FFFF (~2.1 billion) on a tiny file
        buf[4..8].copy_from_slice(&0x7FFF_FFFF_u32.to_le_bytes());
        fs::write(&dbf_path, &buf).unwrap();

        let recs = read_dbf_records_indexed(&dbf_path).expect("clamped safely without OOM");
        assert!(recs.is_empty());
    }

    #[test]
    fn test_web_mercator_and_unsupported_projected_crs() {
        let (lon, lat) = web_mercator_to_wgs84(12_245_143.987, -781_140.0);
        assert!((lon - 110.0).abs() < 0.01, "lon={lon}");
        assert!((lat - (-7.0)).abs() < 0.05, "lat={lat}");

        let dir = tempdir().unwrap();
        let shp_path = dir.path().join("tm3.shp");
        let prj_path = dir.path().join("tm3.prj");
        fs::write(&shp_path, build_point_shp(&[(200_000.0, 1_500_000.0)])).unwrap();
        fs::write(
            &prj_path,
            r#"PROJCS["DGN_1995_Indonesia_TM-3_Zone_49.1",GEOGCS["GCS_DGN_1995",DATUM["D_DGN_1995",SPHEROID["WGS_1984",6378137.0,298.257223563]]],PROJECTION["Transverse_Mercator"]]"#,
        )
        .unwrap();

        let err_bbox = read_wgs84_bbox(&shp_path).unwrap_err().to_string();
        assert!(
            err_bbox.contains("Unsupported projected CRS"),
            "err={err_bbox}"
        );
        let err_gj = read_geojson_wgs84(&shp_path, 0.0).unwrap_err().to_string();
        assert!(err_gj.contains("Unsupported projected CRS"), "err={err_gj}");
    }

    #[test]
    fn test_dgn95_tm3_bpn_conversion_and_prj_detection() {
        let dir = tempdir().unwrap();
        let shp_path = dir.path().join("persil_bpn.shp");
        let prj_path = dir.path().join("persil_bpn.prj");
        // Zone 49.1 (EPSG:23835): CM=109.5, FE=200000, FN=1500000, k0=0.9999
        // False origin (200000, 1500000) maps exactly to (109.5E, 0.0N)
        fs::write(&shp_path, build_point_shp(&[(200_000.0, 1_500_000.0)])).unwrap();
        fs::write(
            &prj_path,
            r#"PROJCS["DGN_1995_Indonesia_TM-3_Zone_49.1",GEOGCS["GCS_DGN_1995",DATUM["D_DGN_1995",SPHEROID["WGS_1984",6378137.0,298.257223563]]],PROJECTION["Transverse_Mercator"],PARAMETER["False_Easting",200000.0],PARAMETER["False_Northing",1500000.0],PARAMETER["Central_Meridian",109.5],PARAMETER["Scale_Factor",0.9999],PARAMETER["Latitude_Of_Origin",0.0],UNIT["Meter",1.0]]"#,
        )
        .unwrap();

        assert_eq!(detect_crs_from_prj(&shp_path), PrjCrs::Dgn95Tm3(109.5));
        let (min_lon, min_lat, max_lon, max_lat) = read_wgs84_bbox(&shp_path).unwrap();
        assert!((min_lon - 109.5).abs() < 1e-6 && (max_lon - 109.5).abs() < 1e-6);
        assert!(min_lat.abs() < 1e-6 && max_lat.abs() < 1e-6);
    }
}
