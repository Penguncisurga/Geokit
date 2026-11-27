use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct GcpPoint {
    pub pixel_x: f64,
    pub pixel_y: f64,
    pub map_x: f64,
    pub map_y: f64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GcpResidual {
    pub index: usize,
    pub pixel_x: f64,
    pub pixel_y: f64,
    pub map_x: f64,
    pub map_y: f64,
    pub residual_x: f64,
    pub residual_y: f64,
    pub residual: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GcpRmseReport {
    pub count: usize,
    pub rmse_x: f64,
    pub rmse_y: f64,
    pub total_rmse: f64,
    pub max_residual: f64,
    pub residuals: Vec<GcpResidual>,
}

/// Parses GCPs from a QGIS `.points` file (`mapX,mapY,sourceX,sourceY,enable,...`)
/// or a standard CSV file (`pixel_x,pixel_y,map_x,map_y` with or without headers).
pub fn parse_gcp_file(path: &Path) -> Result<Vec<GcpPoint>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read GCP file: {}", path.display()))?;
    let lines: Vec<&str> = raw
        .lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}'))
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("CRS:"))
        .collect();
    if lines.is_empty() {
        bail!("GCP file is empty: {}", path.display());
    }

    let first_cols: Vec<String> = lines[0]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_ascii_lowercase())
        .collect();

    // Detect header vs headerless numeric row
    let has_header = first_cols.iter().any(|c| c.parse::<f64>().is_err());

    let (px_idx, py_idx, mx_idx, my_idx, en_idx, is_qgis, start_row) = if has_header {
        let find = |names: &[&str]| -> Option<usize> {
            first_cols.iter().position(|c| names.iter().any(|n| c == n))
        };
        let is_qgis_fmt = find(&["mapx"]).is_some() && find(&["sourcex"]).is_some();
        let px = find(&["sourcex", "pixel_x", "pixelx", "src_x", "col", "px"]);
        let py = find(&["sourcey", "pixel_y", "pixely", "src_y", "row", "py"]);
        let mx = find(&["mapx", "map_x", "dst_x", "target_x", "x", "lon", "easting"]);
        let my = find(&["mapy", "map_y", "dst_y", "target_y", "y", "lat", "northing"]);
        let en = find(&["enable", "enabled", "active"]);
        let (Some(px), Some(py), Some(mx), Some(my)) = (px, py, mx, my) else {
            bail!(
                "Unrecognized GCP header in {}. Expected QGIS (.points: mapX,mapY,sourceX,sourceY) or CSV (pixel_x,pixel_y,map_x,map_y).",
                path.display()
            );
        };
        (px, py, mx, my, en, is_qgis_fmt, 1usize)
    } else {
        if first_cols.len() < 4 {
            bail!(
                "Headerless GCP CSV must have at least 4 columns (pixel_x,pixel_y,map_x,map_y): {}",
                path.display()
            );
        }
        (0, 1, 2, 3, None, false, 0usize)
    };

    let mut gcps = Vec::new();
    for (row_no, line) in lines.iter().enumerate().skip(start_row) {
        let cols: Vec<&str> = line
            .split(',')
            .map(|s| s.trim().trim_matches('"'))
            .collect();
        let need = px_idx.max(py_idx).max(mx_idx).max(my_idx);
        if cols.len() <= need {
            continue;
        }
        let enabled = en_idx
            .and_then(|ei| cols.get(ei))
            .map(|v| !matches!(v.to_ascii_lowercase().as_str(), "0" | "false" | "no"))
            .unwrap_or(true);
        if !enabled {
            continue;
        }
        let pixel_x: f64 = cols[px_idx]
            .parse()
            .with_context(|| format!("Invalid pixel_x at line {}", row_no + 1))?;
        let mut pixel_y: f64 = cols[py_idx]
            .parse()
            .with_context(|| format!("Invalid pixel_y at line {}", row_no + 1))?;
        if is_qgis && pixel_y < 0.0 {
            // QGIS Georeferencer stores raster line index as negative sourceY
            pixel_y = -pixel_y;
        }
        let map_x: f64 = cols[mx_idx]
            .parse()
            .with_context(|| format!("Invalid map_x at line {}", row_no + 1))?;
        let map_y: f64 = cols[my_idx]
            .parse()
            .with_context(|| format!("Invalid map_y at line {}", row_no + 1))?;
        if !pixel_x.is_finite() || !pixel_y.is_finite() || !map_x.is_finite() || !map_y.is_finite()
        {
            bail!("Non-finite GCP coordinate at line {}", row_no + 1);
        }
        gcps.push(GcpPoint {
            pixel_x,
            pixel_y,
            map_x,
            map_y,
            enabled: true,
        });
    }

    if gcps.is_empty() {
        bail!("No enabled GCP points found in {}", path.display());
    }
    Ok(gcps)
}

fn solve_3x3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Result<[f64; 3]> {
    for col in 0..3 {
        let mut pivot = col;
        for row in (col + 1)..3 {
            if a[row][col].abs() > a[pivot][col].abs() {
                pivot = row;
            }
        }
        if a[pivot][col].abs() < 1e-12 {
            bail!("GCP points are collinear or degenerate; cannot compute affine RMSE.");
        }
        if pivot != col {
            a.swap(col, pivot);
            b.swap(col, pivot);
        }
        let div = a[col][col];
        for row in (col + 1)..3 {
            let factor = a[row][col] / div;
            for k in col..3 {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = [0.0; 3];
    for row in (0..3).rev() {
        let mut sum = b[row];
        for k in (row + 1)..3 {
            sum -= a[row][k] * x[k];
        }
        x[row] = sum / a[row][row];
    }
    Ok(x)
}

/// Computes least-squares 1st-order affine residuals and total RMSE in target map units.
pub fn compute_affine_rmse(gcps: &[GcpPoint]) -> Result<GcpRmseReport> {
    if gcps.len() < 3 {
        bail!(
            "At least 3 enabled GCP points are required to compute affine RMSE (found {}).",
            gcps.len()
        );
    }
    let mut ata = [[0.0f64; 3]; 3];
    let mut atx = [0.0f64; 3];
    let mut aty = [0.0f64; 3];

    for g in gcps {
        let row = [1.0, g.pixel_x, g.pixel_y];
        for i in 0..3 {
            for j in 0..3 {
                ata[i][j] += row[i] * row[j];
            }
            atx[i] += row[i] * g.map_x;
            aty[i] += row[i] * g.map_y;
        }
    }

    let cx = solve_3x3(ata, atx)?;
    let cy = solve_3x3(ata, aty)?;

    let mut residuals = Vec::with_capacity(gcps.len());
    let mut sum_sq_x = 0.0;
    let mut sum_sq_y = 0.0;
    let mut max_res = 0.0f64;
    for (idx, g) in gcps.iter().enumerate() {
        let pred_x = cx[0] + cx[1] * g.pixel_x + cx[2] * g.pixel_y;
        let pred_y = cy[0] + cy[1] * g.pixel_x + cy[2] * g.pixel_y;
        let rx = pred_x - g.map_x;
        let ry = pred_y - g.map_y;
        let r = rx.hypot(ry);
        sum_sq_x += rx * rx;
        sum_sq_y += ry * ry;
        if r > max_res {
            max_res = r;
        }
        residuals.push(GcpResidual {
            index: idx + 1,
            pixel_x: g.pixel_x,
            pixel_y: g.pixel_y,
            map_x: g.map_x,
            map_y: g.map_y,
            residual_x: rx,
            residual_y: ry,
            residual: r,
        });
    }

    let n = gcps.len() as f64;
    let rmse_x = (sum_sq_x / n).sqrt();
    let rmse_y = (sum_sq_y / n).sqrt();
    let total_rmse = ((sum_sq_x + sum_sq_y) / n).sqrt();

    Ok(GcpRmseReport {
        count: gcps.len(),
        rmse_x,
        rmse_y,
        total_rmse,
        max_residual: max_res,
        residuals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_qgis_points_and_affine_rmse() {
        let dir = tempdir().unwrap();
        let pts = dir.path().join("scan.points");
        // Pure affine: mapX = 400000 + 10*px, mapY = 9250000 - 10*py (QGIS stores negative sourceY)
        std::fs::write(
            &pts,
            "#CRS: EPSG:32749\n\
             mapX,mapY,sourceX,sourceY,enable,dX,dY,residual\n\
             400000,9250000,0,0,1,0,0,0\n\
             401000,9250000,100,0,1,0,0,0\n\
             400000,9249000,0,-100,1,0,0,0\n\
             401000,9249000,100,-100,1,0,0,0\n\
             999999,9999999,50,-50,0,0,0,0\n",
        )
        .unwrap();

        let gcps = parse_gcp_file(&pts).unwrap();
        assert_eq!(gcps.len(), 4, "disabled 5th GCP must be skipped");
        assert!((gcps[2].pixel_y - 100.0).abs() < 1e-9);

        let report = compute_affine_rmse(&gcps).unwrap();
        assert!(report.total_rmse < 1e-9, "exact affine must have 0 RMSE");

        // Perturb one corner of a 4-corner rectangle by +2.0m in X -> exact least-squares plane splits residual +/-0.5m -> RMSE == 0.5
        let mut noisy = gcps.clone();
        noisy[3].map_x += 2.0;
        let report_noisy = compute_affine_rmse(&noisy).unwrap();
        assert!(
            (report_noisy.total_rmse - 0.5).abs() < 1e-9,
            "expected exact analytical RMSE 0.5, got {}",
            report_noisy.total_rmse
        );
    }
}
