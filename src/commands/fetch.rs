use anyhow::{bail, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::gis::shp::read_wgs84_bbox;

#[derive(Debug, Subcommand)]
pub enum FetchCommand {
    /// Download OpenStreetMap vectors (roads, rivers, buildings, admin) for an AOI into clean GeoJSON (no ArcGIS needed!)
    Osm {
        /// Optional AOI polygon (.shp or .geojson) to extract bounding box automatically
        #[arg(long)]
        aoi: Option<PathBuf>,
        /// Explicit WGS84 bounding box: "min_lon,min_lat,max_lon,max_lat" (e.g. "110.1,-7.05,110.3,-6.9")
        #[arg(long)]
        bbox: Option<String>,
        /// Comma-separated layers to fetch: road, river, building, boundary, poi (default: road,river)
        #[arg(long, default_value = "road,river")]
        layers: String,
        /// Output directory for downloaded .geojson files
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Download free 30m SRTM elevation tiles (.hgt) from AWS Terrain Tiles covering an AOI (no API key or ArcGIS needed!)
    Dem {
        /// Optional AOI polygon (.shp or .geojson) to extract bounding box automatically
        #[arg(long)]
        aoi: Option<PathBuf>,
        /// Explicit WGS84 bounding box: "min_lon,min_lat,max_lon,max_lat"
        #[arg(long)]
        bbox: Option<String>,
        /// Output directory for downloaded 30m SRTM .hgt DEM tiles
        #[arg(short, long)]
        out: PathBuf,
    },
}

fn resolve_bbox(aoi: Option<&Path>, bbox_str: Option<&str>) -> Result<(f64, f64, f64, f64)> {
    if let Some(s) = bbox_str {
        let parts: Vec<f64> = s
            .split(',')
            .filter_map(|p| p.trim().parse::<f64>().ok())
            .collect();
        if parts.len() != 4 {
            bail!("--bbox must have 4 comma-separated numbers: min_lon,min_lat,max_lon,max_lat");
        }
        return Ok((parts[0], parts[1], parts[2], parts[3]));
    }
    if let Some(path) = aoi {
        return read_wgs84_bbox(path);
    }
    bail!(
        "Provide either --aoi <poly.shp|.geojson> or --bbox \"min_lon,min_lat,max_lon,max_lat\"."
    );
}

fn curl_post(url: &str, body: &str) -> Result<String> {
    let out = Command::new("curl")
        .args([
            "-4",
            "-fsSL",
            "-A",
            "geokit-gis-cli/2.4 (+https://github.com/Penguncisurga/Geokit)",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "-X",
            "POST",
            "--data-urlencode",
            &format!("data={body}"),
            url,
        ])
        .output()
        .context("Failed to execute `curl`. Ensure curl is available on PATH.")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("HTTP request to {url} failed: {err}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn curl_download(url: &str, dst: &Path) -> Result<()> {
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "120",
            "-o",
            &dst.to_string_lossy(),
            url,
        ])
        .output()
        .context("Failed to execute `curl`.")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("Download failed ({url}): {err}");
    }
    Ok(())
}

fn decompress_gz(gz_path: &Path, dst_path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let script = format!(
            "$in=[IO.File]::OpenRead('{}');$gz=New-Object IO.Compression.GZipStream($in,[IO.Compression.CompressionMode]::Decompress);$out=[IO.File]::Create('{}');$gz.CopyTo($out);$out.Close();$gz.Close();$in.Close()",
            gz_path.display().to_string().replace('\'', "''"),
            dst_path.display().to_string().replace('\'', "''"),
        );
        let status = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .status()
            .context("Failed to run PowerShell GZip decompression")?;
        if !status.success() {
            bail!("Failed to decompress {}", gz_path.display());
        }
        let _ = fs::remove_file(gz_path);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let out = Command::new("gzip")
            .args(["-dc", &gz_path.to_string_lossy()])
            .output()
            .context("Failed to run gzip")?;
        if !out.status.success() {
            bail!("gzip decompression failed for {}", gz_path.display());
        }
        fs::write(dst_path, out.stdout)?;
        let _ = fs::remove_file(gz_path);
        Ok(())
    }
}

fn osm_layer_filter(layer: &str) -> Result<(&'static str, &'static str, bool)> {
    match layer.trim().to_ascii_lowercase().as_str() {
        "road" | "roads" | "jalan" => Ok(("road", r#"way["highway"]"#, false)),
        "river" | "rivers" | "sungai" | "waterway" => Ok(("river", r#"way["waterway"]"#, false)),
        "building" | "buildings" | "bangunan" => Ok(("building", r#"way["building"]"#, true)),
        "boundary" | "admin" | "batas" => {
            Ok(("boundary", r#"way["boundary"="administrative"]"#, false))
        }
        "poi" | "amenity" | "fasilitas" => Ok(("poi", r#"node["amenity"]"#, false)),
        other => {
            bail!("Unknown OSM layer '{other}'. Supported: road, river, building, boundary, poi.")
        }
    }
}

fn overpass_to_geojson(raw_json: &str, as_polygon: bool) -> Result<(Value, usize)> {
    let parsed: Value = serde_json::from_str(raw_json).context("Invalid Overpass JSON response")?;
    let elements = parsed
        .get("elements")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut features = Vec::new();
    for el in elements {
        let etype = el.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let mut props = el
            .get("tags")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        if let Some(id) = el.get("id") {
            props.insert("osm_id".into(), id.clone());
        }

        if etype == "node" {
            if let (Some(lon), Some(lat)) = (
                el.get("lon").and_then(|v| v.as_f64()),
                el.get("lat").and_then(|v| v.as_f64()),
            ) {
                features.push(json!({
                    "type": "Feature",
                    "properties": props,
                    "geometry": { "type": "Point", "coordinates": [lon, lat] }
                }));
            }
        } else if etype == "way" {
            if let Some(geom) = el.get("geometry").and_then(|v| v.as_array()) {
                let mut coords: Vec<[f64; 2]> = geom
                    .iter()
                    .filter_map(|p| Some([p.get("lon")?.as_f64()?, p.get("lat")?.as_f64()?]))
                    .collect();
                if coords.len() < 2 {
                    continue;
                }
                if as_polygon && coords.len() >= 3 {
                    if coords.first() != coords.last() {
                        coords.push(coords[0]);
                    }
                    features.push(json!({
                        "type": "Feature",
                        "properties": props,
                        "geometry": { "type": "Polygon", "coordinates": [coords] }
                    }));
                } else {
                    features.push(json!({
                        "type": "Feature",
                        "properties": props,
                        "geometry": { "type": "LineString", "coordinates": coords }
                    }));
                }
            }
        }
    }

    let count = features.len();
    Ok((
        json!({
            "type": "FeatureCollection",
            "features": features,
        }),
        count,
    ))
}

pub fn run(cmd: FetchCommand) -> Result<()> {
    match cmd {
        FetchCommand::Osm {
            aoi,
            bbox,
            layers,
            out,
        } => {
            let (min_lon, min_lat, max_lon, max_lat) =
                resolve_bbox(aoi.as_deref(), bbox.as_deref())?;
            fs::create_dir_all(&out)?;
            println!(
                "{} AOI WGS84 bbox: ({:.5}, {:.5}) -> ({:.5}, {:.5})",
                "[fetch osm]".cyan().bold(),
                min_lon,
                min_lat,
                max_lon,
                max_lat
            );

            let endpoints = [
                "https://overpass-api.de/api/interpreter",
                "https://lz4.overpass-api.de/api/interpreter",
                "https://z.overpass-api.de/api/interpreter",
                "https://overpass.kumi.systems/api/interpreter",
            ];

            for raw_lyr in layers.split(',').filter(|s| !s.trim().is_empty()) {
                let (name, filter, as_poly) = osm_layer_filter(raw_lyr)?;
                let query = format!(
                    "[out:json][timeout:25];({filter}({min_lat:.6},{min_lon:.6},{max_lat:.6},{max_lon:.6}););out body geom;"
                );
                let mut resp = None;
                let mut last_err = String::new();
                for ep in endpoints {
                    match curl_post(ep, &query) {
                        Ok(txt) => {
                            resp = Some(txt);
                            break;
                        }
                        Err(e) => last_err = e.to_string(),
                    }
                }
                let raw = resp.ok_or_else(|| {
                    anyhow::anyhow!("All Overpass endpoints failed for '{name}': {last_err}")
                })?;
                let (fc, count) = overpass_to_geojson(&raw, as_poly)?;
                let dst = out.join(format!("osm_{name}.geojson"));
                fs::write(&dst, serde_json::to_string(&fc)?)?;
                println!(
                    "{} Downloaded OSM {} ({} features) -> {}",
                    "[OK]".green().bold(),
                    name,
                    count,
                    dst.display()
                );
            }
            Ok(())
        }
        FetchCommand::Dem { aoi, bbox, out } => {
            let (min_lon, min_lat, max_lon, max_lat) =
                resolve_bbox(aoi.as_deref(), bbox.as_deref())?;
            fs::create_dir_all(&out)?;
            let lat_start = min_lat.floor() as i32;
            let lat_end = max_lat.floor() as i32;
            let lon_start = min_lon.floor() as i32;
            let lon_end = max_lon.floor() as i32;

            let mut downloaded = 0usize;
            for lat in lat_start..=lat_end {
                let ns = if lat >= 0 {
                    format!("N{:02}", lat)
                } else {
                    format!("S{:02}", -lat)
                };
                for lon in lon_start..=lon_end {
                    let ew = if lon >= 0 {
                        format!("E{:03}", lon)
                    } else {
                        format!("W{:03}", -lon)
                    };
                    let tile = format!("{ns}{ew}");
                    let url = format!(
                        "https://s3.amazonaws.com/elevation-tiles-prod/skadi/{ns}/{tile}.hgt.gz"
                    );
                    let gz_path = out.join(format!("{tile}.hgt.gz"));
                    let hgt_path = out.join(format!("{tile}.hgt"));
                    println!(
                        "{} Downloading SRTM 30m tile {tile}...",
                        "[fetch dem]".cyan().bold()
                    );
                    curl_download(&url, &gz_path)?;
                    decompress_gz(&gz_path, &hgt_path)?;
                    println!(
                        "{} SRTM 30m DEM ready -> {}",
                        "[OK]".green().bold(),
                        hgt_path.display()
                    );
                    downloaded += 1;
                }
            }
            println!(
                "{} Downloaded {} SRTM 30m tile(s). Tip: run `geokit raster mosaic` / `geokit gp clip` to clip to your exact AOI.",
                "Done:".green().bold(),
                downloaded
            );
            Ok(())
        }
    }
}
