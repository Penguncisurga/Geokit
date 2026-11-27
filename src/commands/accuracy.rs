use anyhow::{bail, Result};
use clap::Subcommand;
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Table};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::preflight::check_shapefile_bundle;
use crate::gis::shp::{read_dbf_records, read_geojson_wgs84, write_point_shapefile_wgs84};

#[derive(Debug, Subcommand)]
pub enum AccuracyCommand {
    /// Generate Stratified Random validation sample points (.shp + .kml + .geojson + .csv) from a classified polygon layer
    Sample {
        /// Classified input polygon shapefile (.shp) or GeoJSON (.geojson)
        input: PathBuf,
        /// Attribute field containing map classes (e.g. KELAS, TUTUPAN)
        #[arg(long)]
        field: String,
        /// Total target sample points across all strata (default: 100)
        #[arg(long, default_value_t = 100)]
        total: usize,
        /// Minimum samples guaranteed per class (default: 10)
        #[arg(long, default_value_t = 10)]
        min_per_class: usize,
        /// Deterministic random seed (default: 42)
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Output directory or base path prefix (creates .shp, .kml, .geojson, and .csv)
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Compute Confusion Matrix, Overall Accuracy, Cohen's Kappa, Producer's/User's Accuracy & F1-Score
    Matrix {
        /// Input validation table (.csv, .shp, .dbf, or .geojson) with predicted and reference columns
        input: PathBuf,
        /// Field containing predicted/map class labels
        #[arg(long)]
        pred: String,
        /// Field containing reference/ground-truth class labels
        #[arg(long)]
        ref_field: String,
        /// Optional report export path (.md, .csv, .html, or .json)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Print JSON metrics to stdout
        #[arg(long)]
        json: bool,
    },
    /// Compute continuous accuracy metrics (RMSE, MAE, Bias, R2, BIG LE90 & optional 2D CE90)
    Rmse {
        /// Input validation table (.csv, .shp, .dbf, or .geojson)
        input: PathBuf,
        /// Predicted numeric value field (e.g. DEM_Z)
        #[arg(long)]
        pred: Option<String>,
        /// Reference numeric value field (e.g. GPS_Z)
        #[arg(long = "ref")]
        ref_field: Option<String>,
        /// Optional predicted X coordinate field for 2D horizontal CE90
        #[arg(long)]
        pred_x: Option<String>,
        /// Optional predicted Y coordinate field for 2D horizontal CE90
        #[arg(long)]
        pred_y: Option<String>,
        /// Optional reference X coordinate field for 2D horizontal CE90
        #[arg(long)]
        ref_x: Option<String>,
        /// Optional reference Y coordinate field for 2D horizontal CE90
        #[arg(long)]
        ref_y: Option<String>,
        /// Optional report export path (.md, .csv, .html, or .json)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Print JSON metrics to stdout
        #[arg(long)]
        json: bool,
    },
    /// Compute ROC-AUC and Youden's J optimal threshold for binary hazard/probability validation
    Roc {
        /// Input validation table (.csv, .shp, .dbf, or .geojson)
        input: PathBuf,
        /// Numeric score or probability field (higher = more susceptible)
        #[arg(long)]
        score: String,
        /// Binary ground-truth occurrence label field (e.g. LONGSOR, KEJADIAN)
        #[arg(long)]
        label: String,
        /// Label value representing positive occurrence (default: "1")
        #[arg(long, default_value = "1")]
        positive: String,
        /// Optional report export path (.md, .csv, .html, or .json)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Print JSON metrics to stdout
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: AccuracyCommand) -> Result<()> {
    match cmd {
        AccuracyCommand::Sample {
            input,
            field,
            total,
            min_per_class,
            seed,
            out,
        } => run_sample(&input, &field, total, min_per_class, seed, &out),
        AccuracyCommand::Matrix {
            input,
            pred,
            ref_field,
            out,
            json,
        } => run_matrix(&input, &pred, &ref_field, out.as_deref(), json),
        AccuracyCommand::Rmse {
            input,
            pred,
            ref_field,
            pred_x,
            pred_y,
            ref_x,
            ref_y,
            out,
            json,
        } => run_rmse(
            &input,
            pred.as_deref(),
            ref_field.as_deref(),
            pred_x.as_deref(),
            pred_y.as_deref(),
            ref_x.as_deref(),
            ref_y.as_deref(),
            out.as_deref(),
            json,
        ),
        AccuracyCommand::Roc {
            input,
            score,
            label,
            positive,
            out,
            json,
        } => run_roc(&input, &score, &label, &positive, out.as_deref(), json),
    }
}

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn csv_cell(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn check_out_safe(p: &Path) -> Result<()> {
    if p.exists() && !crate::engine::overwrite_enabled() {
        bail!(
            "Output already exists: '{}'. Pass --overwrite to replace it.",
            p.display()
        );
    }
    Ok(())
}

fn split_csv_row(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        if in_q {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    it.next();
                    cur.push('"');
                } else {
                    in_q = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' && cur.is_empty() {
            in_q = true;
        } else if c == ',' {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

fn read_table_rows(input: &Path) -> Result<Vec<Map<String, Value>>> {
    if !input.is_file() {
        bail!("Input dataset does not exist: {}", input.display());
    }
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let rows = if ext == "shp" {
        check_shapefile_bundle(input)?;
        read_dbf_records(&input.with_extension("dbf"))?
    } else if ext == "dbf" {
        read_dbf_records(input)?
    } else if ext == "geojson" || ext == "json" {
        let v: Value = serde_json::from_str(&fs::read_to_string(input)?)?;
        v.get("features")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| f.get("properties").and_then(|p| p.as_object()).cloned())
                    .collect()
            })
            .unwrap_or_default()
    } else if ext == "csv" {
        let raw = fs::read_to_string(input)?;
        let mut lines = raw.lines().filter(|l| !l.trim().is_empty());
        let Some(hdr_line) = lines.next() else {
            bail!("Empty CSV file: {}", input.display());
        };
        let headers: Vec<String> = split_csv_row(hdr_line.trim_start_matches('\u{feff}'))
            .into_iter()
            .map(|h| h.trim().to_string())
            .collect();
        let mut out = Vec::new();
        for line in lines {
            let cells = split_csv_row(line);
            let mut m = Map::new();
            for (i, h) in headers.iter().enumerate() {
                let v = cells.get(i).map(|s| s.trim()).unwrap_or("");
                m.insert(h.clone(), Value::String(v.to_string()));
            }
            out.push(m);
        }
        out
    } else {
        bail!(
            "Unsupported input format '{}'. Use .csv, .shp, .dbf, or .geojson.",
            input.display()
        );
    };
    if rows.is_empty() {
        bail!("No records found in {}", input.display());
    }
    Ok(rows)
}

fn resolve_col(rows: &[Map<String, Value>], target: &str) -> Result<String> {
    let keys: Vec<String> = rows[0].keys().cloned().collect();
    keys.iter()
        .find(|k| k.eq_ignore_ascii_case(target))
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Field '{}' not found. Available fields: {}",
                target,
                keys.join(", ")
            )
        })
}

fn val_str(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.trim().to_string(),
        Some(other) => other.to_string(),
    }
}

fn val_f64(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64().filter(|x| x.is_finite()),
        Some(Value::String(s)) => {
            let t = s.trim();
            t.parse::<f64>()
                .ok()
                .or_else(|| {
                    if !t.contains('.') && t.matches(',').count() == 1 {
                        t.replace(',', ".").parse::<f64>().ok()
                    } else {
                        None
                    }
                })
                .filter(|x| x.is_finite())
        }
        _ => None,
    }
}

#[derive(Clone)]
struct PolyPart {
    outer: Vec<[f64; 2]>,
    holes: Vec<Vec<[f64; 2]>>,
    bbox: (f64, f64, f64, f64),
    area_weight: f64,
}

fn ring_area_abs(ring: &[[f64; 2]]) -> f64 {
    if ring.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for w in ring.windows(2) {
        sum += w[0][0] * w[1][1] - w[1][0] * w[0][1];
    }
    0.5 * sum.abs()
}

fn point_in_ring(x: f64, y: f64, ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        let intersect =
            ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / ((yj - yi) + 1e-30) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn point_in_poly(x: f64, y: f64, poly: &PolyPart) -> bool {
    if x < poly.bbox.0 || x > poly.bbox.2 || y < poly.bbox.1 || y > poly.bbox.3 {
        return false;
    }
    if !point_in_ring(x, y, &poly.outer) {
        return false;
    }
    !poly.holes.iter().any(|h| point_in_ring(x, y, h))
}

fn parse_ring_coords(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let a = p.as_array()?;
                    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_poly_parts(geom: &Value) -> Vec<PolyPart> {
    let gtype = geom.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let coords = match geom.get("coordinates") {
        Some(c) => c,
        None => return Vec::new(),
    };
    let raw_polys: Vec<&[Value]> = match gtype {
        "Polygon" => coords
            .as_array()
            .map(|a| vec![a.as_slice()])
            .unwrap_or_default(),
        "MultiPolygon" => coords
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_array().map(|r| r.as_slice()))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for rings in raw_polys {
        if rings.is_empty() {
            continue;
        }
        let outer = parse_ring_coords(&rings[0]);
        if outer.len() < 3 {
            continue;
        }
        let holes: Vec<Vec<[f64; 2]>> = rings[1..]
            .iter()
            .map(parse_ring_coords)
            .filter(|r| r.len() >= 3)
            .collect();
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for &[x, y] in &outer {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let hole_area: f64 = holes.iter().map(|h| ring_area_abs(h)).sum();
        let area = (ring_area_abs(&outer) - hole_area).max(1e-12);
        out.push(PolyPart {
            outer,
            holes,
            bbox: (x0, y0, x1, y1),
            area_weight: area,
        });
    }
    out
}

fn run_sample(
    input: &Path,
    field: &str,
    total: usize,
    min_per_class: usize,
    seed: u64,
    out: &Path,
) -> Result<()> {
    if !input.is_file() {
        bail!("Input polygon dataset does not exist: {}", input.display());
    }
    check_shapefile_bundle(input)?;
    let fc = read_geojson_wgs84(input, 0.0)?;
    let feats = fc
        .get("features")
        .and_then(|f| f.as_array())
        .ok_or_else(|| anyhow::anyhow!("No features found in {}", input.display()))?;
    if feats.is_empty() {
        bail!("Input layer has 0 features: {}", input.display());
    }

    let first_props = feats[0]
        .get("properties")
        .and_then(|p| p.as_object())
        .ok_or_else(|| anyhow::anyhow!("Feature missing properties"))?;
    let keys: Vec<String> = first_props.keys().cloned().collect();
    let cls_key = keys
        .iter()
        .find(|k| k.eq_ignore_ascii_case(field))
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Field '{}' not found. Available fields: {}",
                field,
                keys.join(", ")
            )
        })?;

    let mut strata: BTreeMap<String, Vec<PolyPart>> = BTreeMap::new();
    for feat in feats {
        let Some(geom) = feat.get("geometry") else {
            continue;
        };
        let parts = extract_poly_parts(geom);
        if parts.is_empty() {
            continue;
        }
        let cls = feat
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|p| val_str(p.get(&cls_key)))
            .unwrap_or_default();
        if cls.is_empty() {
            continue;
        }
        strata.entry(cls).or_default().extend(parts);
    }
    if strata.is_empty() {
        bail!(
            "No polygon features with non-empty '{}' values found.",
            cls_key
        );
    }

    // Stratified proportional allocation with guaranteed minimum per class
    let total_area: f64 = strata
        .values()
        .flat_map(|ps| ps.iter().map(|p| p.area_weight))
        .sum();
    let mut targets: BTreeMap<String, usize> = BTreeMap::new();
    let eff_total = total.max(strata.len() * min_per_class.max(1));
    for (cls, parts) in &strata {
        let cls_area: f64 = parts.iter().map(|p| p.area_weight).sum();
        let prop = if total_area > 0.0 {
            ((cls_area / total_area) * eff_total as f64).round() as usize
        } else {
            eff_total / strata.len()
        };
        targets.insert(cls.clone(), prop.max(min_per_class.max(1)));
    }

    let mut rng = Rng(seed.max(1));
    let mut pts_xy: Vec<(f64, f64)> = Vec::new();
    let mut rows: Vec<Map<String, Value>> = Vec::new();
    let mut gj_feats: Vec<Value> = Vec::new();
    let mut pt_id = 0usize;

    for (cls, parts) in &strata {
        let n_want = targets[cls];
        let cum_weights: Vec<f64> = parts
            .iter()
            .scan(0.0, |acc, p| {
                *acc += p.area_weight;
                Some(*acc)
            })
            .collect();
        let max_w = *cum_weights.last().unwrap_or(&1.0);

        let mut got = 0usize;
        let max_attempts = n_want * 500;
        let mut attempts = 0usize;
        while got < n_want && attempts < max_attempts {
            attempts += 1;
            let pick = rng.next_f64() * max_w;
            let pidx = cum_weights
                .partition_point(|&w| w < pick)
                .min(parts.len() - 1);
            let poly = &parts[pidx];
            let x = poly.bbox.0 + rng.next_f64() * (poly.bbox.2 - poly.bbox.0);
            let y = poly.bbox.1 + rng.next_f64() * (poly.bbox.3 - poly.bbox.1);
            if point_in_poly(x, y, poly) {
                pt_id += 1;
                got += 1;
                let rx = (x * 1_000_000.0).round() / 1_000_000.0;
                let ry = (y * 1_000_000.0).round() / 1_000_000.0;
                pts_xy.push((rx, ry));
                let mut m = Map::new();
                m.insert("PT_ID".into(), json!(pt_id as i64));
                m.insert("LON".into(), json!(rx));
                m.insert("LAT".into(), json!(ry));
                m.insert("PRED_CLASS".into(), json!(cls));
                m.insert("REF_CLASS".into(), json!(""));
                rows.push(m.clone());
                gj_feats.push(json!({
                    "type": "Feature",
                    "properties": m,
                    "geometry": { "type": "Point", "coordinates": [rx, ry] },
                }));
            }
        }
    }

    if pts_xy.is_empty() {
        bail!("Failed to sample points inside input polygons.");
    }

    let base = if out.extension().is_some() {
        out.with_extension("")
    } else {
        out.join("validation_points")
    };
    let shp_out = base.with_extension("shp");
    let kml_out = base.with_extension("kml");
    let gj_out = base.with_extension("geojson");
    let csv_out = base.with_extension("csv");
    for p in [&shp_out, &kml_out, &gj_out, &csv_out] {
        check_out_safe(p)?;
    }
    if let Some(parent) = shp_out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    write_point_shapefile_wgs84(&shp_out, &pts_xy, &rows)?;
    fs::write(
        &gj_out,
        serde_json::to_string_pretty(&json!({
            "type": "FeatureCollection",
            "features": gj_feats,
        }))?,
    )?;

    let mut kml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<Document>\n<name>GeoKit Validation Samples</name>\n",
    );
    let mut csv_s = String::from("PT_ID,LON,LAT,PRED_CLASS,REF_CLASS\n");
    for (r, &(x, y)) in rows.iter().zip(pts_xy.iter()) {
        let id = r["PT_ID"].as_i64().unwrap_or(0);
        let cls = r["PRED_CLASS"].as_str().unwrap_or("");
        kml.push_str(&format!(
            "  <Placemark><name>{id} ({})</name><ExtendedData><Data name=\"PRED_CLASS\"><value>{}</value></Data></ExtendedData><Point><coordinates>{x:.6},{y:.6},0</coordinates></Point></Placemark>\n",
            esc_html(cls),
            esc_html(cls),
        ));
        csv_s.push_str(&format!("{id},{x:.6},{y:.6},{},\n", csv_cell(cls)));
    }
    kml.push_str("</Document>\n</kml>\n");
    fs::write(&kml_out, kml)?;
    fs::write(&csv_out, csv_s)?;

    println!(
        "{} Sampled {} stratified random point(s) across {} classes -> {}, {}, {}, {}",
        "[OK]".green().bold(),
        pts_xy.len(),
        strata.len(),
        shp_out.display(),
        kml_out.display(),
        gj_out.display(),
        csv_out.display()
    );
    Ok(())
}

#[derive(Serialize)]
struct ClassMetric {
    class: String,
    reference_total: usize,
    predicted_total: usize,
    correct: usize,
    producer_accuracy_pct: f64,
    user_accuracy_pct: f64,
    omission_error_pct: f64,
    commission_error_pct: f64,
    f1_pct: f64,
}

fn run_matrix(
    input: &Path,
    pred_field: &str,
    ref_field: &str,
    out: Option<&Path>,
    as_json: bool,
) -> Result<()> {
    let rows = read_table_rows(input)?;
    let pred_k = resolve_col(&rows, pred_field)?;
    let ref_k = resolve_col(&rows, ref_field)?;

    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut class_set: BTreeSet<String> = BTreeSet::new();
    for r in &rows {
        let p = val_str(r.get(&pred_k));
        let rf = val_str(r.get(&ref_k));
        if p.is_empty() || rf.is_empty() {
            continue;
        }
        class_set.insert(p.clone());
        class_set.insert(rf.clone());
        pairs.push((p, rf));
    }
    if pairs.is_empty() {
        bail!("No valid non-empty ({pred_k}, {ref_k}) pairs found.");
    }

    let classes: Vec<String> = class_set.into_iter().collect();
    let k = classes.len();
    let idx_map: BTreeMap<&str, usize> = classes
        .iter()
        .enumerate()
        .map(|(i, c)| (c.as_str(), i))
        .collect();
    // mat[ref][pred] (standard remote sensing convention: columns = reference, rows = predicted or vice versa;
    // keep explicit row_sums[ref] and col_sums[pred])
    let mut mat = vec![vec![0usize; k]; k];
    for (p, rf) in &pairs {
        mat[idx_map[p.as_str()]][idx_map[rf.as_str()]] += 1;
    }

    let n = pairs.len() as f64;
    let diag: usize = (0..k).map(|i| mat[i][i]).sum();
    let oa = (diag as f64) / n;

    let mut sum_prod = 0.0;
    let mut class_metrics = Vec::with_capacity(k);
    for (i, cls) in classes.iter().enumerate() {
        let pred_tot: usize = mat[i].iter().sum();
        let ref_tot: usize = (0..k).map(|r| mat[r][i]).sum();
        let corr = mat[i][i];
        sum_prod += (pred_tot as f64) * (ref_tot as f64);
        let pa = if ref_tot > 0 {
            corr as f64 / ref_tot as f64
        } else {
            0.0
        };
        let ua = if pred_tot > 0 {
            corr as f64 / pred_tot as f64
        } else {
            0.0
        };
        let f1 = if pa + ua > 0.0 {
            2.0 * pa * ua / (pa + ua)
        } else {
            0.0
        };
        class_metrics.push(ClassMetric {
            class: cls.clone(),
            reference_total: ref_tot,
            predicted_total: pred_tot,
            correct: corr,
            producer_accuracy_pct: (pa * 10000.0).round() / 100.0,
            user_accuracy_pct: (ua * 10000.0).round() / 100.0,
            omission_error_pct: ((1.0 - pa) * 10000.0).round() / 100.0,
            commission_error_pct: ((1.0 - ua) * 10000.0).round() / 100.0,
            f1_pct: (f1 * 10000.0).round() / 100.0,
        });
    }

    let pe = sum_prod / (n * n);
    let kappa = if (1.0 - pe).abs() > 1e-12 {
        (oa - pe) / (1.0 - pe)
    } else {
        1.0
    };
    let macro_f1 = class_metrics.iter().map(|m| m.f1_pct).sum::<f64>() / k as f64;
    let weighted_f1 = class_metrics
        .iter()
        .map(|m| m.f1_pct * (m.reference_total as f64 / n))
        .sum::<f64>();

    let payload = json!({
        "samples": pairs.len(),
        "overall_accuracy_pct": (oa * 10000.0).round() / 100.0,
        "kappa": (kappa * 10000.0).round() / 10000.0,
        "macro_f1_pct": (macro_f1 * 100.0).round() / 100.0,
        "weighted_f1_pct": (weighted_f1 * 100.0).round() / 100.0,
        "classes": classes,
        "matrix_pred_rows_ref_cols": mat,
        "per_class": class_metrics,
    });

    if let Some(out_path) = out {
        check_out_safe(out_path)?;
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let ext = out_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("md")
            .to_ascii_lowercase();
        let text = if ext == "json" {
            serde_json::to_string_pretty(&payload)?
        } else if ext == "csv" {
            let mut s = format!(
                "Pred \\ Ref,{},Total,User_Acc_Pct\n",
                classes
                    .iter()
                    .map(|c| csv_cell(c))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            for (i, cls) in classes.iter().enumerate() {
                let row_vals: Vec<String> = mat[i].iter().map(|v| v.to_string()).collect();
                s.push_str(&format!(
                    "{},{},{},{:.2}\n",
                    csv_cell(cls),
                    row_vals.join(","),
                    class_metrics[i].predicted_total,
                    class_metrics[i].user_accuracy_pct
                ));
            }
            s.push_str(&format!(
                "Prod_Acc_Pct,{},OA={:.2}%,Kappa={:.4}\n",
                class_metrics
                    .iter()
                    .map(|m| format!("{:.2}", m.producer_accuracy_pct))
                    .collect::<Vec<_>>()
                    .join(","),
                oa * 100.0,
                kappa
            ));
            s
        } else if ext == "html" {
            let mut ths = String::new();
            for c in &classes {
                ths.push_str(&format!("<th>{}</th>", esc_html(c)));
            }
            let mut trs = String::new();
            for (i, cls) in classes.iter().enumerate() {
                let mut tds = String::new();
                for v in &mat[i] {
                    tds.push_str(&format!("<td style='text-align:right'>{v}</td>"));
                }
                trs.push_str(&format!(
                    "<tr><td><b>{}</b></td>{tds}<td style='text-align:right'>{:.2}%</td></tr>",
                    esc_html(cls),
                    class_metrics[i].user_accuracy_pct
                ));
            }
            format!(
                "<!doctype html><html><head><meta charset='utf-8'><title>Confusion Matrix</title><style>body{{font-family:Segoe UI,Arial,sans-serif;max-width:860px;margin:2rem auto;color:#1e293b}}table{{width:100%;border-collapse:collapse;margin-top:1rem}}th,td{{border:1px solid #cbd5e1;padding:6px 10px}}th{{background:#0f172a;color:#fff}}</style></head><body><h2>Confusion Matrix (n={}, OA={:.2}%, Kappa={:.4})</h2><table><thead><tr><th>Pred \\ Ref</th>{ths}<th>User Acc</th></tr></thead><tbody>{trs}</tbody></table></body></html>",
                pairs.len(),
                oa * 100.0,
                kappa
            )
        } else {
            let mut md = format!(
                "# Confusion Matrix & Accuracy Assessment\n\n- **Samples ($N$):** {}\n- **Overall Accuracy (OA):** {:.2}%\n- **Cohen's Kappa ($\\kappa$):** {:.4}\n- **Macro F1:** {:.2}%\n\n## Per-Class Accuracy\n\n| Class | Ref | Pred | Correct | Producer's (%) | User's (%) | Omission (%) | Commission (%) | F1 (%) |\n| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
                pairs.len(),
                oa * 100.0,
                kappa,
                macro_f1
            );
            for m in &class_metrics {
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {:.2}% | {:.2}% | {:.2}% | {:.2}% | {:.2}% |\n",
                    m.class,
                    m.reference_total,
                    m.predicted_total,
                    m.correct,
                    m.producer_accuracy_pct,
                    m.user_accuracy_pct,
                    m.omission_error_pct,
                    m.commission_error_pct,
                    m.f1_pct
                ));
            }
            md
        };
        fs::write(out_path, text)?;
        if !as_json {
            println!("OK Saved accuracy matrix report -> {}", out_path.display());
        }
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!(
        "{} Samples: {} | Overall Accuracy: {:.2}% | Kappa: {:.4} | Macro-F1: {:.2}%",
        "[OK]".green().bold(),
        pairs.len(),
        oa * 100.0,
        kappa,
        macro_f1
    );
    let mut tbl = Table::new();
    tbl.load_preset(UTF8_FULL);
    tbl.set_header(vec![
        "Class", "Ref", "Pred", "Correct", "PA (%)", "UA (%)", "F1 (%)",
    ]);
    for m in &class_metrics {
        tbl.add_row(vec![
            m.class.clone(),
            m.reference_total.to_string(),
            m.predicted_total.to_string(),
            m.correct.to_string(),
            format!("{:.2}", m.producer_accuracy_pct),
            format!("{:.2}", m.user_accuracy_pct),
            format!("{:.2}", m.f1_pct),
        ]);
    }
    println!("{tbl}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_rmse(
    input: &Path,
    pred: Option<&str>,
    ref_field: Option<&str>,
    pred_x: Option<&str>,
    pred_y: Option<&str>,
    ref_x: Option<&str>,
    ref_y: Option<&str>,
    out: Option<&Path>,
    as_json: bool,
) -> Result<()> {
    let rows = read_table_rows(input)?;
    let do_1d = pred.is_some() && ref_field.is_some();
    let do_2d = pred_x.is_some() && pred_y.is_some() && ref_x.is_some() && ref_y.is_some();
    if !do_1d && !do_2d {
        bail!("Provide `--pred <col> --ref <col>` (1D/vertical LE90) and/or `--pred-x --pred-y --ref-x --ref-y` (2D horizontal CE90).");
    }

    let mut result = Map::new();
    let mut md = String::from("# Continuous Accuracy Report (RMSE / BIG LE90 / CE90)\n\n");

    if do_1d {
        let pk = resolve_col(&rows, pred.unwrap())?;
        let rk = resolve_col(&rows, ref_field.unwrap())?;
        let mut errs = Vec::new();
        let mut refs = Vec::new();
        for r in &rows {
            if let (Some(pv), Some(rv)) = (val_f64(r.get(&pk)), val_f64(r.get(&rk))) {
                errs.push(pv - rv);
                refs.push(rv);
            }
        }
        if errs.is_empty() {
            bail!("No numeric ({pk}, {rk}) pairs found.");
        }
        let n = errs.len() as f64;
        let bias = errs.iter().sum::<f64>() / n;
        let mae = errs.iter().map(|e| e.abs()).sum::<f64>() / n;
        let ss_res = errs.iter().map(|e| e * e).sum::<f64>();
        let rmse = (ss_res / n).sqrt();
        let std_dev = (errs.iter().map(|e| (e - bias).powi(2)).sum::<f64>() / n).sqrt();
        let ref_mean = refs.iter().sum::<f64>() / n;
        let ss_tot = refs.iter().map(|y| (y - ref_mean).powi(2)).sum::<f64>();
        let r2 = if ss_tot > 1e-12 {
            1.0 - (ss_res / ss_tot)
        } else {
            0.0
        };
        let le90 = 1.6449 * rmse;

        result.insert("vertical_n".into(), json!(errs.len()));
        result.insert("bias".into(), json!(bias));
        result.insert("mae".into(), json!(mae));
        result.insert("rmse".into(), json!(rmse));
        result.insert("std_dev".into(), json!(std_dev));
        result.insert("r2".into(), json!(r2));
        result.insert("le90".into(), json!(le90));
        md.push_str(&format!(
            "- **1D/Vertical Samples:** {}\n- **Bias (Mean Error):** {bias:.4}\n- **MAE:** {mae:.4}\n- **RMSE:** {rmse:.4}\n- **$R^2$:** {r2:.4}\n- **LE90 ($1.6449 \\times \\text{{RMSE}}$):** {le90:.4}\n\n",
            errs.len()
        ));
    }

    if do_2d {
        let pxk = resolve_col(&rows, pred_x.unwrap())?;
        let pyk = resolve_col(&rows, pred_y.unwrap())?;
        let rxk = resolve_col(&rows, ref_x.unwrap())?;
        let ryk = resolve_col(&rows, ref_y.unwrap())?;
        let mut dxs = Vec::new();
        let mut dys = Vec::new();
        for r in &rows {
            if let (Some(px), Some(py), Some(rx), Some(ry)) = (
                val_f64(r.get(&pxk)),
                val_f64(r.get(&pyk)),
                val_f64(r.get(&rxk)),
                val_f64(r.get(&ryk)),
            ) {
                dxs.push(px - rx);
                dys.push(py - ry);
            }
        }
        if dxs.is_empty() {
            bail!("No numeric 2D coordinate pairs found.");
        }
        let n = dxs.len() as f64;
        let rmse_x = (dxs.iter().map(|d| d * d).sum::<f64>() / n).sqrt();
        let rmse_y = (dys.iter().map(|d| d * d).sum::<f64>() / n).sqrt();
        let rmse_r = (rmse_x * rmse_x + rmse_y * rmse_y).sqrt();
        let ce90 = 1.5175 * rmse_r;
        result.insert("horizontal_n".into(), json!(dxs.len()));
        result.insert("rmse_x".into(), json!(rmse_x));
        result.insert("rmse_y".into(), json!(rmse_y));
        result.insert("rmse_r".into(), json!(rmse_r));
        result.insert("ce90".into(), json!(ce90));
        md.push_str(&format!(
            "- **2D/Horizontal Samples:** {}\n- **RMSE_X / RMSE_Y:** {rmse_x:.4} / {rmse_y:.4}\n- **RMSE_r:** {rmse_r:.4}\n- **CE90 ($1.5175 \\times \\text{{RMSE}}_r$):** {ce90:.4}\n",
            dxs.len()
        ));
    }

    if let Some(out_path) = out {
        check_out_safe(out_path)?;
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let ext = out_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("md")
            .to_ascii_lowercase();
        let body = if ext == "json" {
            serde_json::to_string_pretty(&result)?
        } else if ext == "csv" {
            let mut s = String::from("Metric,Value\n");
            for (k, v) in &result {
                s.push_str(&format!("{},{}\n", csv_cell(k), v));
            }
            s
        } else {
            md.clone()
        };
        fs::write(out_path, body)?;
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print!("{md}");
    }
    Ok(())
}

fn run_roc(
    input: &Path,
    score_field: &str,
    label_field: &str,
    positive: &str,
    out: Option<&Path>,
    as_json: bool,
) -> Result<()> {
    let rows = read_table_rows(input)?;
    let sk = resolve_col(&rows, score_field)?;
    let lk = resolve_col(&rows, label_field)?;
    let pos_norm = positive.trim().to_ascii_lowercase();

    let mut samples: Vec<(f64, bool)> = Vec::new();
    for r in &rows {
        let Some(sc) = val_f64(r.get(&sk)) else {
            continue;
        };
        let lv = val_str(r.get(&lk)).to_ascii_lowercase();
        if lv.is_empty() {
            continue;
        }
        let is_pos =
            lv == pos_norm || (pos_norm == "1" && matches!(lv.as_str(), "true" | "ya" | "yes"));
        samples.push((sc, is_pos));
    }
    let pos_total = samples.iter().filter(|s| s.1).count();
    let neg_total = samples.len().saturating_sub(pos_total);
    if pos_total == 0 || neg_total == 0 {
        bail!("ROC requires both positive ({pos_norm}) and negative samples (found pos={pos_total}, neg={neg_total}).");
    }

    samples.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut tp = 0usize;
    let mut fp = 0usize;
    let mut prev_fpr = 0.0f64;
    let mut prev_tpr = 0.0f64;
    let mut auc = 0.0f64;
    let mut best_j = f64::NEG_INFINITY;
    let mut best_thresh = samples[0].0;
    let mut best_tpr = 0.0;
    let mut best_fpr = 0.0;
    let mut curve: Vec<Value> = vec![json!({"threshold": null, "fpr": 0.0, "tpr": 0.0})];

    let mut i = 0;
    while i < samples.len() {
        let thr = samples[i].0;
        while i < samples.len() && (samples[i].0 - thr).abs() < 1e-12 {
            if samples[i].1 {
                tp += 1;
            } else {
                fp += 1;
            }
            i += 1;
        }
        let tpr = tp as f64 / pos_total as f64;
        let fpr = fp as f64 / neg_total as f64;
        auc += (fpr - prev_fpr) * (tpr + prev_tpr) * 0.5;
        let j = tpr - fpr;
        if j > best_j {
            best_j = j;
            best_thresh = thr;
            best_tpr = tpr;
            best_fpr = fpr;
        }
        curve.push(json!({"threshold": thr, "fpr": fpr, "tpr": tpr}));
        prev_fpr = fpr;
        prev_tpr = tpr;
    }

    let payload = json!({
        "samples": samples.len(),
        "positives": pos_total,
        "negatives": neg_total,
        "auc": (auc * 10000.0).round() / 10000.0,
        "youden_j": (best_j * 10000.0).round() / 10000.0,
        "optimal_threshold": best_thresh,
        "sensitivity_at_optimal": (best_tpr * 10000.0).round() / 100.0,
        "specificity_at_optimal": ((1.0 - best_fpr) * 10000.0).round() / 100.0,
        "curve": curve,
    });

    if let Some(out_path) = out {
        check_out_safe(out_path)?;
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let ext = out_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("md")
            .to_ascii_lowercase();
        let body = if ext == "json" {
            serde_json::to_string_pretty(&payload)?
        } else if ext == "csv" {
            let mut s = String::from("Threshold,FPR,TPR\n");
            for pt in payload["curve"].as_array().unwrap() {
                s.push_str(&format!(
                    "{},{:.4},{:.4}\n",
                    pt["threshold"],
                    pt["fpr"].as_f64().unwrap(),
                    pt["tpr"].as_f64().unwrap()
                ));
            }
            s
        } else {
            format!(
                "# ROC-AUC Validation Report\n\n- **Samples:** {} (Pos={}, Neg={})\n- **ROC-AUC:** {:.4}\n- **Optimal Threshold (Youden's J = {:.4}):** {:.4}\n- **Sensitivity / Specificity:** {:.2}% / {:.2}%\n",
                samples.len(),
                pos_total,
                neg_total,
                auc,
                best_j,
                best_thresh,
                best_tpr * 100.0,
                (1.0 - best_fpr) * 100.0
            )
        };
        fs::write(out_path, body)?;
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!(
            "{} Samples: {} (Pos={}, Neg={}) | ROC-AUC: {:.4} | Optimal Threshold: {:.4} (Youden J={:.4})",
            "[OK]".green().bold(),
            samples.len(),
            pos_total,
            neg_total,
            auc,
            best_thresh,
            best_j
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_accuracy_matrix_rmse_and_roc_exact() {
        let dir = tempdir().unwrap();

        // 1. Matrix: 4 samples -> 3 correct -> OA=75%, Kappa=(0.75-0.5)/(1-0.5)=0.5
        let mcsv = dir.path().join("mat.csv");
        fs::write(
            &mcsv,
            "PRED,REF\nHutan,Hutan\nHutan,Hutan\nSawah,Sawah\nHutan,Sawah\n",
        )
        .unwrap();
        let mjson = dir.path().join("mat.json");
        run_matrix(&mcsv, "PRED", "REF", Some(&mjson), true).unwrap();
        let mv: Value = serde_json::from_str(&fs::read_to_string(&mjson).unwrap()).unwrap();
        assert_eq!(mv["overall_accuracy_pct"], 75.0);
        assert!((mv["kappa"].as_f64().unwrap() - 0.5).abs() < 1e-4);

        // 2. RMSE: errors [+1, -1, +1, -1] -> bias=0, mae=1, rmse=1, le90=1.6449
        let rcsv = dir.path().join("rmse.csv");
        fs::write(&rcsv, "PRED_Z,REF_Z\n11,10\n19,20\n31,30\n39,40\n").unwrap();
        let rjson = dir.path().join("rmse.json");
        run_rmse(
            &rcsv,
            Some("PRED_Z"),
            Some("REF_Z"),
            None,
            None,
            None,
            None,
            Some(&rjson),
            true,
        )
        .unwrap();
        let rv: Value = serde_json::from_str(&fs::read_to_string(&rjson).unwrap()).unwrap();
        assert!((rv["rmse"].as_f64().unwrap() - 1.0).abs() < 1e-6);
        assert!((rv["le90"].as_f64().unwrap() - 1.6449).abs() < 1e-4);

        // 3. ROC: perfect ranking -> AUC == 1.0
        let roccsv = dir.path().join("roc.csv");
        fs::write(&roccsv, "SCORE,LABEL\n0.9,1\n0.8,1\n0.3,0\n0.1,0\n").unwrap();
        let rocjson = dir.path().join("roc.json");
        run_roc(&roccsv, "SCORE", "LABEL", "1", Some(&rocjson), true).unwrap();
        let rcv: Value = serde_json::from_str(&fs::read_to_string(&rocjson).unwrap()).unwrap();
        assert!((rcv["auc"].as_f64().unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_accuracy_sample_creates_shp_kml_geojson_csv() {
        let dir = tempdir().unwrap();
        let poly = dir.path().join("tuplah.geojson");
        fs::write(
            &poly,
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","properties":{"KELAS":"Hutan"},"geometry":{"type":"Polygon","coordinates":[[[110.0,-7.0],[110.1,-7.0],[110.1,-6.9],[110.0,-6.9],[110.0,-7.0]]]}},
                {"type":"Feature","properties":{"KELAS":"Sawah"},"geometry":{"type":"Polygon","coordinates":[[[110.1,-7.0],[110.2,-7.0],[110.2,-6.9],[110.1,-6.9],[110.1,-7.0]]]}}
            ]}"#,
        )
        .unwrap();

        let out_prefix = dir.path().join("val_pts");
        run_sample(&poly, "KELAS", 20, 5, 42, &out_prefix).unwrap();
        let out_dir = out_prefix;
        assert!(out_dir.join("validation_points.shp").is_file());
        assert!(out_dir.join("validation_points.shx").is_file());
        assert!(out_dir.join("validation_points.dbf").is_file());
        assert!(out_dir.join("validation_points.prj").is_file());
        assert!(out_dir.join("validation_points.kml").is_file());
        assert!(out_dir.join("validation_points.geojson").is_file());
        assert!(out_dir.join("validation_points.csv").is_file());

        let recs = read_dbf_records(&out_dir.join("validation_points.dbf")).unwrap();
        assert_eq!(recs.len(), 20);
    }
}
