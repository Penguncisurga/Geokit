use anyhow::{bail, Result};
use clap::Subcommand;
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Table};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::preflight::check_shapefile_bundle;
use crate::gis::shp::{
    read_geojson_wgs84, write_point_shapefile_wgs84, write_polygon_shapefile_wgs84,
};

#[derive(Debug, Subcommand)]
pub enum SurveyCommand {
    /// Split an AOI polygon into uniform drone flight blocks (.shp, .kml, .geojson, .csv)
    Blocks {
        /// Input AOI polygon shapefile (.shp) or GeoJSON (.geojson)
        aoi: PathBuf,
        /// Target square flight block width/height in meters (default: 1000 m = 100 ha)
        #[arg(long)]
        block_size_m: Option<f64>,
        /// Target block area in hectares (alternative to --block-size-m, e.g. 50)
        #[arg(long)]
        block_ha: Option<f64>,
        /// Explicit grid columns (use with --rows instead of metric size)
        #[arg(long)]
        cols: Option<usize>,
        /// Explicit grid rows (use with --cols instead of metric size)
        #[arg(long)]
        rows: Option<usize>,
        /// Overlap between adjacent flight blocks in meters (default: 0)
        #[arg(long, default_value_t = 0.0)]
        overlap_m: f64,
        /// Output directory or base path prefix (creates .shp, .kml, .geojson, .csv)
        #[arg(short, long)]
        out: PathBuf,
        /// Also write individual KML files per block (BLOK_01.kml, ...) for DJI Pilot 2
        #[arg(long)]
        split_kml: bool,
        /// Print JSON summary to stdout
        #[arg(long)]
        json: bool,
    },
    /// Plan evenly distributed GCP & ICP ground-control points inside an AOI (.shp, .kml, .geojson, .csv)
    Gcp {
        /// Input AOI polygon shapefile (.shp) or GeoJSON (.geojson)
        aoi: PathBuf,
        /// Number of Ground Control Points (GCP) to place (default: 8)
        #[arg(long, default_value_t = 8)]
        gcp_count: usize,
        /// Number of Independent Check Points (ICP) to place (default: 4)
        #[arg(long, default_value_t = 4)]
        icp_count: usize,
        /// Inward margin from AOI boundary in meters (default: 25)
        #[arg(long, default_value_t = 25.0)]
        margin_m: f64,
        /// Output directory or base path prefix (creates .shp, .kml, .geojson, .csv)
        #[arg(short, long)]
        out: PathBuf,
        /// Print JSON summary to stdout
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: SurveyCommand) -> Result<()> {
    match cmd {
        SurveyCommand::Blocks {
            aoi,
            block_size_m,
            block_ha,
            cols,
            rows,
            overlap_m,
            out,
            split_kml,
            json,
        } => run_blocks(
            &aoi,
            block_size_m,
            block_ha,
            cols,
            rows,
            overlap_m,
            &out,
            split_kml,
            json,
        ),
        SurveyCommand::Gcp {
            aoi,
            gcp_count,
            icp_count,
            margin_m,
            out,
            json,
        } => run_gcp(&aoi, gcp_count, icp_count, margin_m, &out, json),
    }
}

type BBox = (f64, f64, f64, f64);

#[derive(Clone)]
struct PolyPart {
    outer: Vec<[f64; 2]>,
    holes: Vec<Vec<[f64; 2]>>,
    bbox: BBox,
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

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn meters_per_deg(lat_deg: f64) -> (f64, f64) {
    let phi = lat_deg.to_radians();
    let m_lat = 111_132.92 - 559.82 * (2.0 * phi).cos() + 1.175 * (4.0 * phi).cos();
    let m_lon = (111_412.84 * phi.cos() - 93.5 * (3.0 * phi).cos()).max(1_000.0);
    (m_lon, m_lat)
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

fn load_aoi_polygons(aoi: &Path) -> Result<(Vec<PolyPart>, BBox)> {
    if !aoi.is_file() {
        bail!("Input AOI dataset does not exist: {}", aoi.display());
    }
    check_shapefile_bundle(aoi)?;
    let fc = read_geojson_wgs84(aoi, 0.0)?;
    let feats = fc
        .get("features")
        .and_then(|f| f.as_array())
        .ok_or_else(|| anyhow::anyhow!("No features found in {}", aoi.display()))?;
    let mut parts = Vec::new();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );

    for feat in feats {
        let Some(geom) = feat.get("geometry") else {
            continue;
        };
        let gtype = geom.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let Some(coords) = geom.get("coordinates") else {
            continue;
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
            min_x = min_x.min(x0);
            min_y = min_y.min(y0);
            max_x = max_x.max(x1);
            max_y = max_y.max(y1);
            parts.push(PolyPart {
                outer,
                holes,
                bbox: (x0, y0, x1, y1),
            });
        }
    }
    if parts.is_empty() {
        bail!(
            "No Polygon/MultiPolygon features found in AOI: {}",
            aoi.display()
        );
    }
    Ok((parts, (min_x, min_y, max_x, max_y)))
}

fn point_in_ring(x: f64, y: f64, ring: &[[f64; 2]]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / ((yj - yi) + 1e-30) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn point_in_aoi(x: f64, y: f64, parts: &[PolyPart]) -> bool {
    parts.iter().any(|p| {
        x >= p.bbox.0
            && x <= p.bbox.2
            && y >= p.bbox.1
            && y <= p.bbox.3
            && point_in_ring(x, y, &p.outer)
            && !p.holes.iter().any(|h| point_in_ring(x, y, h))
    })
}

fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let orient = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| -> f64 {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    (o1 * o2 < 0.0) && (o3 * o4 < 0.0)
}

fn rect_intersects_aoi(x0: f64, y0: f64, x1: f64, y1: f64, parts: &[PolyPart]) -> bool {
    // 1. Sub-grid check inside cell (catches interior & corners)
    for ix in 0..=4 {
        let tx = x0 + (x1 - x0) * (ix as f64 / 4.0);
        for iy in 0..=4 {
            let ty = y0 + (y1 - y0) * (iy as f64 / 4.0);
            if point_in_aoi(tx, ty, parts) {
                return true;
            }
        }
    }
    // 2. AOI vertices inside cell or ring edges crossing cell boundary
    let rect_edges = [
        ([x0, y0], [x1, y0]),
        ([x1, y0], [x1, y1]),
        ([x1, y1], [x0, y1]),
        ([x0, y1], [x0, y0]),
    ];
    for p in parts {
        if p.bbox.2 < x0 || p.bbox.0 > x1 || p.bbox.3 < y0 || p.bbox.1 > y1 {
            continue;
        }
        for &[vx, vy] in &p.outer {
            if vx >= x0 && vx <= x1 && vy >= y0 && vy <= y1 {
                return true;
            }
        }
        for w in p.outer.windows(2) {
            for &(ra, rb) in &rect_edges {
                if segments_intersect(w[0], w[1], ra, rb) {
                    return true;
                }
            }
        }
    }
    false
}

#[derive(Serialize)]
struct BlockRecord {
    blok_id: String,
    center_lon: f64,
    center_lat: f64,
    width_m: f64,
    height_m: f64,
    blok_ha: f64,
    aoi_cover_pct: f64,
}

#[allow(clippy::too_many_arguments)]
fn run_blocks(
    aoi: &Path,
    block_size_m: Option<f64>,
    block_ha: Option<f64>,
    cols_opt: Option<usize>,
    rows_opt: Option<usize>,
    overlap_m: f64,
    out: &Path,
    split_kml: bool,
    as_json: bool,
) -> Result<()> {
    let (parts, (min_x, min_y, max_x, max_y)) = load_aoi_polygons(aoi)?;
    let mid_lat = 0.5 * (min_y + max_y);
    let (m_lon, m_lat) = meters_per_deg(mid_lat);

    if overlap_m < 0.0 {
        bail!("--overlap-m must be >= 0.");
    }

    let (width_m, height_m, n_cols, n_rows) = match (cols_opt, rows_opt) {
        (Some(c), Some(r)) => {
            if c == 0 || r == 0 {
                bail!("--cols and --rows must be >= 1.");
            }
            let total_w_m = (max_x - min_x) * m_lon;
            let total_h_m = (max_y - min_y) * m_lat;
            (
                (total_w_m / c as f64).max(10.0),
                (total_h_m / r as f64).max(10.0),
                c,
                r,
            )
        }
        (Some(_), None) | (None, Some(_)) => {
            bail!("Specify both --cols and --rows together, or use --block-size-m / --block-ha.");
        }
        (None, None) => {
            let side_m = if let Some(ha) = block_ha {
                if !ha.is_finite() || ha <= 0.0 {
                    bail!("--block-ha must be > 0.");
                }
                (ha * 10_000.0).sqrt()
            } else {
                let s = block_size_m.unwrap_or(1000.0);
                if !s.is_finite() || s <= 0.0 {
                    bail!("--block-size-m must be > 0.");
                }
                s
            };
            let step_m = (side_m - overlap_m).max(side_m * 0.1);
            let total_w_m = ((max_x - min_x) * m_lon).max(1.0);
            let total_h_m = ((max_y - min_y) * m_lat).max(1.0);
            let c = (((total_w_m - overlap_m).max(1.0)) / step_m)
                .ceil()
                .max(1.0) as usize;
            let r = (((total_h_m - overlap_m).max(1.0)) / step_m)
                .ceil()
                .max(1.0) as usize;
            if c * r > 10_000 {
                bail!(
                    "Grid too dense ({} x {} = {} blocks). Increase --block-size-m or --block-ha.",
                    c,
                    r,
                    c * r
                );
            }
            (side_m, side_m, c, r)
        }
    };

    let dlon = width_m / m_lon;
    let dlat = height_m / m_lat;
    let step_lon = ((width_m - overlap_m).max(width_m * 0.1)) / m_lon;
    let step_lat = ((height_m - overlap_m).max(height_m * 0.1)) / m_lat;

    let mut raw_cells: Vec<(Vec<[f64; 2]>, f64, f64, f64)> = Vec::new();
    for r in 0..n_rows {
        let y1 = max_y - (r as f64) * step_lat;
        let y0 = y1 - dlat;
        for c_idx in 0..n_cols {
            // Serpentine ordering reduces drone ferry distance between consecutive blocks
            let c = if r % 2 == 0 {
                c_idx
            } else {
                n_cols - 1 - c_idx
            };
            let x0 = min_x + (c as f64) * step_lon;
            let x1 = x0 + dlon;
            if !rect_intersects_aoi(x0, y0, x1, y1, &parts) {
                continue;
            }
            let mut inside_cnt = 0usize;
            for ix in 0..5 {
                let tx = x0 + dlon * ((ix as f64 + 0.5) / 5.0);
                for iy in 0..5 {
                    let ty = y0 + dlat * ((iy as f64 + 0.5) / 5.0);
                    if point_in_aoi(tx, ty, &parts) {
                        inside_cnt += 1;
                    }
                }
            }
            let cover_pct = (inside_cnt as f64 / 25.0) * 100.0;
            let ring = vec![[x0, y1], [x1, y1], [x1, y0], [x0, y0], [x0, y1]];
            raw_cells.push((ring, 0.5 * (x0 + x1), 0.5 * (y0 + y1), cover_pct));
        }
    }

    if raw_cells.is_empty() {
        bail!("No flight blocks intersected the AOI.");
    }

    let base = if out.extension().is_some() {
        out.with_extension("")
    } else {
        out.join("flight_blocks")
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

    let pad = if raw_cells.len() >= 100 { 3 } else { 2 };
    let blok_ha = (width_m * height_m) / 10_000.0;
    let mut polys = Vec::with_capacity(raw_cells.len());
    let mut rows = Vec::with_capacity(raw_cells.len());
    let mut gj_feats = Vec::with_capacity(raw_cells.len());
    let mut summaries = Vec::with_capacity(raw_cells.len());

    let mut kml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<Document>\n<name>GeoKit Drone Flight Blocks</name>\n\
         <Style id=\"blkStyle\"><LineStyle><color>ff00ffff</color><width>2.2</width></LineStyle><PolyStyle><color>3300ffff</color></PolyStyle></Style>\n",
    );
    let mut csv_s =
        String::from("BLOK_ID,CENTER_LON,CENTER_LAT,WIDTH_M,HEIGHT_M,BLOK_HA,AOI_PCT\n");

    let split_dir = base
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("blocks_kml");
    if split_kml {
        fs::create_dir_all(&split_dir)?;
    }

    for (idx, (ring, cx, cy, cover_pct)) in raw_cells.into_iter().enumerate() {
        let blok_id = format!("BLOK_{:0width$}", idx + 1, width = pad);
        let cx_r = (cx * 1_000_000.0).round() / 1_000_000.0;
        let cy_r = (cy * 1_000_000.0).round() / 1_000_000.0;
        let w_r = (width_m * 100.0).round() / 100.0;
        let h_r = (height_m * 100.0).round() / 100.0;
        let ha_r = (blok_ha * 100.0).round() / 100.0;
        let pct_r = (cover_pct * 10.0).round() / 10.0;

        let mut m = Map::new();
        m.insert("BLOK_ID".into(), json!(blok_id));
        m.insert("CENTER_LON".into(), json!(cx_r));
        m.insert("CENTER_LAT".into(), json!(cy_r));
        m.insert("WIDTH_M".into(), json!(w_r));
        m.insert("HEIGHT_M".into(), json!(h_r));
        m.insert("BLOK_HA".into(), json!(ha_r));
        m.insert("AOI_PCT".into(), json!(pct_r));

        let coord_str = ring
            .iter()
            .map(|p| format!("{:.6},{:.6},0", p[0], p[1]))
            .collect::<Vec<_>>()
            .join(" ");
        let placemark = format!(
            "  <Placemark><name>{}</name><styleUrl>#blkStyle</styleUrl><description>Size: {:.1} x {:.1} m ({:.2} ha)</description><Polygon><outerBoundaryIs><LinearRing><coordinates>{}</coordinates></LinearRing></outerBoundaryIs></Polygon></Placemark>\n",
            esc_html(&blok_id),
            w_r,
            h_r,
            ha_r,
            coord_str
        );
        kml.push_str(&placemark);
        if split_kml {
            let one_kml_path = split_dir.join(format!("{blok_id}.kml"));
            check_out_safe(&one_kml_path)?;
            let one_kml = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<Document>\n<name>{}</name>\n\
                 <Style id=\"blkStyle\"><LineStyle><color>ff00ffff</color><width>2.2</width></LineStyle><PolyStyle><color>3300ffff</color></PolyStyle></Style>\n\
                 {}</Document>\n</kml>\n",
                esc_html(&blok_id),
                placemark
            );
            fs::write(one_kml_path, one_kml)?;
        }

        csv_s.push_str(&format!(
            "{blok_id},{cx_r:.6},{cy_r:.6},{w_r:.2},{h_r:.2},{ha_r:.2},{pct_r:.1}\n"
        ));

        gj_feats.push(json!({
            "type": "Feature",
            "properties": m,
            "geometry": { "type": "Polygon", "coordinates": [ring] },
        }));
        polys.push(ring);
        rows.push(m);
        summaries.push(BlockRecord {
            blok_id,
            center_lon: cx_r,
            center_lat: cy_r,
            width_m: w_r,
            height_m: h_r,
            blok_ha: ha_r,
            aoi_cover_pct: pct_r,
        });
    }
    kml.push_str("</Document>\n</kml>\n");

    write_polygon_shapefile_wgs84(&shp_out, &polys, &rows)?;
    fs::write(&kml_out, kml)?;
    fs::write(
        &gj_out,
        serde_json::to_string_pretty(&json!({
            "type": "FeatureCollection",
            "features": gj_feats,
        }))?,
    )?;
    fs::write(&csv_out, csv_s)?;

    let out_json = json!({
        "action": "survey_blocks",
        "aoi": aoi.display().to_string(),
        "block_count": summaries.len(),
        "width_m": (width_m * 100.0).round() / 100.0,
        "height_m": (height_m * 100.0).round() / 100.0,
        "overlap_m": overlap_m,
        "outputs": {
            "shp": shp_out.display().to_string(),
            "kml": kml_out.display().to_string(),
            "geojson": gj_out.display().to_string(),
            "csv": csv_out.display().to_string(),
            "split_kml_dir": if split_kml { Some(split_dir.display().to_string()) } else { None },
        },
        "blocks": summaries,
    });

    if as_json {
        println!("{}", serde_json::to_string_pretty(&out_json)?);
    } else {
        println!(
            "{} Generated {} drone flight block(s) ({:.1} x {:.1} m, {:.2} ha/block)",
            "OK".green().bold(),
            summaries.len(),
            width_m,
            height_m,
            blok_ha
        );
        let mut tbl = Table::new();
        tbl.load_preset(UTF8_FULL);
        tbl.set_header(vec!["BLOK_ID", "CENTER_LON", "CENTER_LAT", "HA", "AOI_%"]);
        for b in summaries.iter().take(12) {
            tbl.add_row(vec![
                b.blok_id.clone(),
                format!("{:.6}", b.center_lon),
                format!("{:.6}", b.center_lat),
                format!("{:.2}", b.blok_ha),
                format!("{:.1}%", b.aoi_cover_pct),
            ]);
        }
        println!("{tbl}");
        if summaries.len() > 12 {
            println!("   ... ({} more blocks)", summaries.len() - 12);
        }
        println!("   SHP     -> {}", shp_out.display());
        println!("   KML     -> {}", kml_out.display());
        println!("   GeoJSON -> {}", gj_out.display());
        println!("   CSV     -> {}", csv_out.display());
        if split_kml {
            println!("   Per-block KML -> {}", split_dir.display());
        }
    }
    Ok(())
}

fn sq_dist_m(a: (f64, f64), b: (f64, f64), m_lon: f64, m_lat: f64) -> f64 {
    let dx = (a.0 - b.0) * m_lon;
    let dy = (a.1 - b.1) * m_lat;
    dx * dx + dy * dy
}

fn min_dist_to_boundary_m(pt: (f64, f64), parts: &[PolyPart], m_lon: f64, m_lat: f64) -> f64 {
    let mut best_sq = f64::INFINITY;
    let px = pt.0 * m_lon;
    let py = pt.1 * m_lat;
    for p in parts {
        for w in p.outer.windows(2) {
            let ax = w[0][0] * m_lon;
            let ay = w[0][1] * m_lat;
            let bx = w[1][0] * m_lon;
            let by = w[1][1] * m_lat;
            let abx = bx - ax;
            let aby = by - ay;
            let denom = abx * abx + aby * aby;
            let t = if denom > 1e-12 {
                (((px - ax) * abx + (py - ay) * aby) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let cx = ax + t * abx;
            let cy = ay + t * aby;
            best_sq = best_sq.min((px - cx) * (px - cx) + (py - cy) * (py - cy));
        }
    }
    best_sq.sqrt()
}

fn run_gcp(
    aoi: &Path,
    gcp_count: usize,
    icp_count: usize,
    margin_m: f64,
    out: &Path,
    as_json: bool,
) -> Result<()> {
    if gcp_count == 0 && icp_count == 0 {
        bail!("At least one of --gcp-count or --icp-count must be > 0.");
    }
    let (parts, (min_x, min_y, max_x, max_y)) = load_aoi_polygons(aoi)?;
    let mid_lat = 0.5 * (min_y + max_y);
    let (m_lon, m_lat) = meters_per_deg(mid_lat);

    // Build dense candidate lattice inside AOI (40 x 40 = up to 1600 candidates)
    let grid_n = 40usize;
    let mut candidates: Vec<((f64, f64), f64)> = Vec::new();
    for ix in 0..grid_n {
        let x = min_x + (max_x - min_x) * ((ix as f64 + 0.5) / grid_n as f64);
        for iy in 0..grid_n {
            let y = min_y + (max_y - min_y) * ((iy as f64 + 0.5) / grid_n as f64);
            if point_in_aoi(x, y, &parts) {
                let d_bnd = min_dist_to_boundary_m((x, y), &parts, m_lon, m_lat);
                candidates.push(((x, y), d_bnd));
            }
        }
    }
    if candidates.is_empty() {
        bail!("AOI polygon is too thin/small to place interior control points.");
    }

    let eff_margin = margin_m.max(0.0);
    let safe_cands: Vec<((f64, f64), f64)> = candidates
        .iter()
        .copied()
        .filter(|&(_, d)| d >= eff_margin)
        .collect();
    let pool = if safe_cands.len() >= (gcp_count + icp_count) {
        safe_cands
    } else {
        candidates.clone()
    };

    // Place perimeter GCPs first (~60% of gcp_count) near outer envelope, then interior GCPs by farthest-point sampling
    let mut selected_gcp: Vec<((f64, f64), &'static str)> = Vec::new();
    let perim_target = if gcp_count >= 4 {
        ((gcp_count as f64 * 0.6).round() as usize).clamp(4, gcp_count)
    } else {
        gcp_count
    };

    if perim_target > 0 {
        let mut perim_pool: Vec<((f64, f64), f64)> = pool.clone();
        // Sort ascending by boundary distance so outer-band points are preferred, then maximize spacing
        perim_pool.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let band_len = (perim_pool.len() / 2)
            .max(perim_target)
            .min(perim_pool.len());
        let band = &perim_pool[..band_len];
        // Start at the northernmost/westernmost perimeter candidate
        let first = band[0].0;
        selected_gcp.push((first, "Perimeter"));
        while selected_gcp.len() < perim_target && selected_gcp.len() < band.len() {
            let mut best_pt = band[0].0;
            let mut best_score = -1.0f64;
            for &(cand, _) in band {
                let min_d = selected_gcp
                    .iter()
                    .map(|&(p, _)| sq_dist_m(cand, p, m_lon, m_lat))
                    .fold(f64::INFINITY, f64::min);
                if min_d > best_score {
                    best_score = min_d;
                    best_pt = cand;
                }
            }
            if best_score <= 1e-6 {
                break;
            }
            selected_gcp.push((best_pt, "Perimeter"));
        }
    }

    // Remaining GCPs in interior (farthest from existing GCPs + weighted toward interior)
    while selected_gcp.len() < gcp_count {
        let mut best_pt = pool[0].0;
        let mut best_score = -1.0f64;
        for &(cand, d_bnd) in &pool {
            let min_d = if selected_gcp.is_empty() {
                d_bnd * d_bnd
            } else {
                selected_gcp
                    .iter()
                    .map(|&(p, _)| sq_dist_m(cand, p, m_lon, m_lat))
                    .fold(f64::INFINITY, f64::min)
                    .min(d_bnd * d_bnd * 4.0)
            };
            if min_d > best_score {
                best_score = min_d;
                best_pt = cand;
            }
        }
        if best_score <= 1e-6 {
            break;
        }
        selected_gcp.push((best_pt, "Interior"));
    }

    // Place ICPs maximizing minimum distance from all GCPs and previously selected ICPs
    let mut selected_icp: Vec<((f64, f64), &'static str)> = Vec::new();
    while selected_icp.len() < icp_count {
        let mut best_pt = pool[0].0;
        let mut best_score = -1.0f64;
        for &(cand, _) in &pool {
            let min_d = selected_gcp
                .iter()
                .chain(selected_icp.iter())
                .map(|&(p, _)| sq_dist_m(cand, p, m_lon, m_lat))
                .fold(f64::INFINITY, f64::min);
            if min_d > best_score {
                best_score = min_d;
                best_pt = cand;
            }
        }
        if best_score <= 1e-6 {
            break;
        }
        selected_icp.push((best_pt, "Interior"));
    }

    let base = if out.extension().is_some() {
        out.with_extension("")
    } else {
        out.join("survey_gcp_icp")
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

    let mut pts_xy = Vec::new();
    let mut rows = Vec::new();
    let mut gj_feats = Vec::new();
    let mut kml_gcp = String::new();
    let mut kml_icp = String::new();
    let mut csv_s = String::from("PT_ID,KODE,TIPE,LOKASI,LON,LAT\n");

    let mut pt_id = 0i64;
    for (i, &((x, y), loc)) in selected_gcp.iter().enumerate() {
        pt_id += 1;
        let rx = (x * 1_000_000.0).round() / 1_000_000.0;
        let ry = (y * 1_000_000.0).round() / 1_000_000.0;
        let kode = format!("GCP-{:02}", i + 1);
        pts_xy.push((rx, ry));
        let mut m = Map::new();
        m.insert("PT_ID".into(), json!(pt_id));
        m.insert("KODE".into(), json!(kode));
        m.insert("TIPE".into(), json!("GCP"));
        m.insert("LOKASI".into(), json!(loc));
        m.insert("LON".into(), json!(rx));
        m.insert("LAT".into(), json!(ry));
        rows.push(m.clone());
        gj_feats.push(json!({
            "type": "Feature",
            "properties": m,
            "geometry": { "type": "Point", "coordinates": [rx, ry] },
        }));
        kml_gcp.push_str(&format!(
            "    <Placemark><name>{}</name><styleUrl>#gcpStyle</styleUrl><description>{loc}</description><Point><coordinates>{rx:.6},{ry:.6},0</coordinates></Point></Placemark>\n",
            esc_html(&kode)
        ));
        csv_s.push_str(&format!("{pt_id},{kode},GCP,{loc},{rx:.6},{ry:.6}\n"));
    }
    for (i, &((x, y), loc)) in selected_icp.iter().enumerate() {
        pt_id += 1;
        let rx = (x * 1_000_000.0).round() / 1_000_000.0;
        let ry = (y * 1_000_000.0).round() / 1_000_000.0;
        let kode = format!("ICP-{:02}", i + 1);
        pts_xy.push((rx, ry));
        let mut m = Map::new();
        m.insert("PT_ID".into(), json!(pt_id));
        m.insert("KODE".into(), json!(kode));
        m.insert("TIPE".into(), json!("ICP"));
        m.insert("LOKASI".into(), json!(loc));
        m.insert("LON".into(), json!(rx));
        m.insert("LAT".into(), json!(ry));
        rows.push(m.clone());
        gj_feats.push(json!({
            "type": "Feature",
            "properties": m,
            "geometry": { "type": "Point", "coordinates": [rx, ry] },
        }));
        kml_icp.push_str(&format!(
            "    <Placemark><name>{}</name><styleUrl>#icpStyle</styleUrl><description>{loc}</description><Point><coordinates>{rx:.6},{ry:.6},0</coordinates></Point></Placemark>\n",
            esc_html(&kode)
        ));
        csv_s.push_str(&format!("{pt_id},{kode},ICP,{loc},{rx:.6},{ry:.6}\n"));
    }

    let kml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\">\n<Document>\n<name>GeoKit Survey GCP &amp; ICP Plan</name>\n\
         <Style id=\"gcpStyle\"><IconStyle><color>ff00ffff</color><scale>1.1</scale></IconStyle></Style>\n\
         <Style id=\"icpStyle\"><IconStyle><color>ffffaa00</color><scale>1.1</scale></IconStyle></Style>\n\
         <Folder><name>GCP ({})</name>\n{kml_gcp}  </Folder>\n\
         <Folder><name>ICP ({})</name>\n{kml_icp}  </Folder>\n\
         </Document>\n</kml>\n",
        selected_gcp.len(),
        selected_icp.len()
    );

    write_point_shapefile_wgs84(&shp_out, &pts_xy, &rows)?;
    fs::write(&kml_out, kml)?;
    fs::write(
        &gj_out,
        serde_json::to_string_pretty(&json!({
            "type": "FeatureCollection",
            "features": gj_feats,
        }))?,
    )?;
    fs::write(&csv_out, csv_s)?;

    let out_json = json!({
        "action": "survey_gcp",
        "aoi": aoi.display().to_string(),
        "gcp_count": selected_gcp.len(),
        "icp_count": selected_icp.len(),
        "margin_m": margin_m,
        "outputs": {
            "shp": shp_out.display().to_string(),
            "kml": kml_out.display().to_string(),
            "geojson": gj_out.display().to_string(),
            "csv": csv_out.display().to_string(),
        },
        "points": rows,
    });

    if as_json {
        println!("{}", serde_json::to_string_pretty(&out_json)?);
    } else {
        println!(
            "{} Planned {} GCP + {} ICP point(s) (margin: {:.1} m)",
            "OK".green().bold(),
            selected_gcp.len(),
            selected_icp.len(),
            margin_m
        );
        println!("   SHP     -> {}", shp_out.display());
        println!("   KML     -> {}", kml_out.display());
        println!("   GeoJSON -> {}", gj_out.display());
        println!("   CSV     -> {}", csv_out.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_survey_blocks_and_gcp_roundtrip() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let aoi = root.join("aoi.geojson");
        fs::write(
            &aoi,
            r#"{
              "type": "FeatureCollection",
              "features": [{
                "type": "Feature",
                "properties": { "NAMA": "Area Studi" },
                "geometry": {
                  "type": "Polygon",
                  "coordinates": [[[110.0, -7.0], [110.03, -7.0], [110.03, -6.97], [110.0, -6.97], [110.0, -7.0]]]
                }
              }]
            }"#,
        )
        .unwrap();

        let blk_dir = root.join("blocks");
        run_blocks(
            &aoi,
            Some(1000.0),
            None,
            None,
            None,
            0.0,
            &blk_dir,
            true,
            false,
        )
        .unwrap();
        let blk_base = blk_dir.join("flight_blocks");
        assert!(blk_base.with_extension("shp").is_file());
        assert!(blk_base.with_extension("dbf").is_file());
        assert!(blk_base.with_extension("prj").is_file());
        assert!(blk_base.with_extension("kml").is_file());
        assert!(blk_base.with_extension("geojson").is_file());
        assert!(blk_base.with_extension("csv").is_file());
        assert!(blk_dir.join("blocks_kml").join("BLOK_01.kml").is_file());

        let gcp_dir = root.join("gcp");
        run_gcp(&aoi, 8, 4, 25.0, &gcp_dir, false).unwrap();
        let gcp_base = gcp_dir.join("survey_gcp_icp");
        assert!(gcp_base.with_extension("shp").is_file());
        assert!(gcp_base.with_extension("kml").is_file());
        let csv_txt = fs::read_to_string(gcp_base.with_extension("csv")).unwrap();
        assert_eq!(csv_txt.lines().count(), 13); // header + 8 GCP + 4 ICP
    }
}
