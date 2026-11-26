use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

/// Exact WGS84 Inverse Transverse Mercator (UTM Easting/Northing -> Lon/Lat degrees).
pub fn utm_to_wgs84(easting: f64, northing: f64, zone: u8, south: bool) -> (f64, f64) {
    const A: f64 = 6378137.0;
    const E2: f64 = 0.00669437999014;
    const K0: f64 = 0.9996;
    let e_prime2 = E2 / (1.0 - E2);
    let e1 = (1.0 - (1.0 - E2).sqrt()) / (1.0 + (1.0 - E2).sqrt());

    let x = easting - 500_000.0;
    let y = if south {
        northing - 10_000_000.0
    } else {
        northing
    };

    let m = y / K0;
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
    let d = x / (n1 * K0);

    let lat = phi1
        - (n1 * tan_p / r1)
            * (d * d / 2.0
                - (5.0 + 3.0 * t1 + 10.0 * c1 - 4.0 * c1 * c1 - 9.0 * e_prime2) * d.powi(4) / 24.0
                + (61.0 + 90.0 * t1 + 298.0 * c1 + 45.0 * t1 * t1
                    - 252.0 * e_prime2
                    - 3.0 * c1 * c1)
                    * d.powi(6)
                    / 720.0);

    let lon0 = ((zone as f64 - 1.0) * 6.0 - 180.0 + 3.0).to_radians();
    let lon = lon0
        + (d - (1.0 + 2.0 * t1 + c1) * d.powi(3) / 6.0
            + (5.0 - 2.0 * c1 + 28.0 * t1 - 3.0 * c1 * c1 + 8.0 * e_prime2 + 24.0 * t1 * t1)
                * d.powi(5)
                / 120.0)
            / cos_p;

    (lon.to_degrees(), lat.to_degrees())
}

/// Detects UTM zone and hemisphere `(zone, is_south)` from a shapefile's `.prj` WKT text.
pub fn detect_utm_from_prj(shp_path: &Path) -> Option<(u8, bool)> {
    let prj = shp_path.with_extension("prj");
    let text = fs::read_to_string(&prj).ok()?;
    let upper = text.to_ascii_uppercase();
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
        // Fallback: parse central_meridian + false_northing
        let cm = extract_wkt_param(&upper, "CENTRAL_MERIDIAN");
        let fn_val = extract_wkt_param(&upper, "FALSE_NORTHING").unwrap_or(0.0);
        if let Some(cm_deg) = cm {
            let zone = (((cm_deg + 183.0) / 6.0).round() as i32).clamp(1, 60) as u8;
            return Some((zone, fn_val > 5_000_000.0));
        }
    }
    None
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

fn transform_xy(x: f64, y: f64, utm: Option<(u8, bool)>) -> (f64, f64) {
    if let Some((zone, south)) = utm {
        if x.abs() > 180.0 || y.abs() > 90.0 {
            return utm_to_wgs84(x, y, zone, south);
        }
    }
    (x, y)
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
        let utm = detect_utm_from_prj(path);
        let (lon1, lat1) = transform_xy(xmin, ymin, utm);
        let (lon2, lat2) = transform_xy(xmax, ymax, utm);
        let (lon3, lat3) = transform_xy(xmin, ymax, utm);
        let (lon4, lat4) = transform_xy(xmax, ymin, utm);
        let min_lon = lon1.min(lon2).min(lon3).min(lon4);
        let max_lon = lon1.max(lon2).max(lon3).max(lon4);
        let min_lat = lat1.min(lat2).min(lat3).min(lat4);
        let max_lat = lat1.max(lat2).max(lat3).max(lat4);
        if min_lon < -180.1 || max_lon > 180.1 || min_lat < -90.1 || max_lat > 90.1 {
            bail!(
                "Could not determine WGS84 extent for {} (missing .prj UTM definition?). Pass --bbox explicitly.",
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
        if arr.len() >= 2 && arr[0].is_f64() && arr[1].is_f64() {
            let x = arr[0].as_f64().unwrap();
            let y = arr[1].as_f64().unwrap();
            *min_x = min_x.min(x);
            *min_y = min_y.min(y);
            *max_x = max_x.max(x);
            *max_y = max_y.max(y);
        } else {
            for item in arr {
                collect_coord_array(item, min_x, min_y, max_x, max_y);
            }
        }
    }
}

/// Reads all attribute records from a `.dbf` file as a Vec of JSON property maps.
pub fn read_dbf_records(dbf_path: &Path) -> Result<Vec<Map<String, Value>>> {
    let data =
        fs::read(dbf_path).with_context(|| format!("Failed to read {}", dbf_path.display()))?;
    if data.len() < 32 {
        bail!("Invalid DBF header: {}", dbf_path.display());
    }
    let num_records = read_u32_le(&data, 4) as usize;
    let header_len = read_u16_le(&data, 8) as usize;
    let record_len = read_u16_le(&data, 10) as usize;

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
        if !deleted {
            out.push(map);
        }
        rec_off += record_len;
    }
    Ok(out)
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
    let utm = detect_utm_from_prj(path);
    let dbf_path = path.with_extension("dbf");
    let records = if dbf_path.is_file() {
        read_dbf_records(&dbf_path).unwrap_or_default()
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
        let props = records.get(rec_idx).cloned().unwrap_or_else(Map::new);
        rec_idx += 1;

        let geometry = match shape_type {
            1 | 11 | 21 if content_len >= 20 => {
                let x = read_f64_le(&shp, pos + 4);
                let y = read_f64_le(&shp, pos + 12);
                let (lon, lat) = transform_xy(x, y, utm);
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
                                transform_xy(read_f64_le(&shp, o), read_f64_le(&shp, o + 8), utm);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utm_49s_kendal_to_wgs84() {
        let (lon, lat) = utm_to_wgs84(412_000.0, 9_232_000.0, 49, true);
        assert!((lon - 110.203).abs() < 0.02, "lon={lon}");
        assert!((lat - (-6.948)).abs() < 0.02, "lat={lat}");
    }
}
