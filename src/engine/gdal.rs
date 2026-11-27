use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use walkdir::WalkDir;

use crate::gis::{RASTER_PRIMARY_EXTS, VECTOR_PRIMARY_EXTS};

#[derive(Debug, Clone, Serialize)]
pub struct GdalEnvironment {
    pub ogr2ogr: Option<PathBuf>,
    pub ogrinfo: Option<PathBuf>,
    pub gdalwarp: Option<PathBuf>,
    pub gdaldem: Option<PathBuf>,
    pub proj_data: Option<PathBuf>,
    pub gdal_data: Option<PathBuf>,
    pub source: Option<String>,
}

impl GdalEnvironment {
    pub fn is_available(&self) -> bool {
        self.ogr2ogr.is_some()
    }
}

/// Auto-detects GDAL/OGR binaries from PATH or standard QGIS installations on Windows.
pub fn detect_gdal() -> GdalEnvironment {
    if let Some(ogr2ogr) = find_on_path("ogr2ogr") {
        return GdalEnvironment {
            ogr2ogr: Some(ogr2ogr),
            ogrinfo: find_on_path("ogrinfo"),
            gdalwarp: find_on_path("gdalwarp"),
            gdaldem: find_on_path("gdaldem"),
            proj_data: None,
            gdal_data: None,
            source: Some("System PATH".to_string()),
        };
    }

    // Check QGIS installations on Windows (C:\Program Files\QGIS *\bin)
    if let Ok(entries) = fs::read_dir(r"C:\Program Files") {
        let mut qgis_roots: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.to_ascii_lowercase().starts_with("qgis"))
                        .unwrap_or(false)
            })
            .filter(|p| p.join("bin").join("ogr2ogr.exe").is_file())
            .collect();
        qgis_roots.sort();
        if let Some(root) = qgis_roots.pop() {
            let bin = root.join("bin");
            let proj_data = root.join("share").join("proj");
            let gdal_data = [
                root.join("apps").join("gdal").join("share").join("gdal"),
                root.join("share").join("gdal"),
            ]
            .into_iter()
            .find(|p| p.is_dir());

            return GdalEnvironment {
                ogr2ogr: Some(bin.join("ogr2ogr.exe")),
                ogrinfo: bin
                    .join("ogrinfo.exe")
                    .is_file()
                    .then(|| bin.join("ogrinfo.exe")),
                gdalwarp: bin
                    .join("gdalwarp.exe")
                    .is_file()
                    .then(|| bin.join("gdalwarp.exe")),
                gdaldem: bin
                    .join("gdaldem.exe")
                    .is_file()
                    .then(|| bin.join("gdaldem.exe")),
                proj_data: proj_data.is_dir().then_some(proj_data),
                gdal_data,
                source: Some(format!("QGIS ({})", bin.display())),
            };
        }
    }

    GdalEnvironment {
        ogr2ogr: None,
        ogrinfo: None,
        gdalwarp: None,
        gdaldem: None,
        proj_data: None,
        gdal_data: None,
        source: None,
    }
}

fn find_on_path(bin: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(target_os = "windows") {
        format!("{bin}.exe")
    } else {
        bin.to_string()
    };
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(&exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn collect_inputs(input: &Path, include_rasters: bool) -> Vec<(PathBuf, bool)> {
    // Vec<(path, is_raster)>
    let mut items = Vec::new();
    if input.is_file() {
        let ext = input
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let is_raster = RASTER_PRIMARY_EXTS.contains(&ext.as_str());
        if !is_raster || include_rasters {
            items.push((input.to_path_buf(), is_raster));
        }
        return items;
    }

    for entry in WalkDir::new(input).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let lower = ext.to_ascii_lowercase();
        if VECTOR_PRIMARY_EXTS.contains(&lower.as_str()) {
            items.push((p.to_path_buf(), false));
        } else if include_rasters && RASTER_PRIMARY_EXTS.contains(&lower.as_str()) {
            items.push((p.to_path_buf(), true));
        }
    }
    items.sort();
    items
}

fn run_cmd(gdal: &GdalEnvironment, bin: &Path, args: &[String]) -> Result<()> {
    let mut cmd = Command::new(bin);
    if let Some(proj) = &gdal.proj_data {
        cmd.env("PROJ_DATA", proj).env("PROJ_LIB", proj);
    }
    if let Some(gd) = &gdal.gdal_data {
        cmd.env("GDAL_DATA", gd);
    }
    let status = cmd
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute {}", bin.display()))?;
    if !status.success() {
        bail!("{} exited with status {:?}", bin.display(), status.code());
    }
    Ok(())
}

pub fn gdal_info(gdal: &GdalEnvironment, input: &Path) -> Result<()> {
    let Some(ogrinfo) = &gdal.ogrinfo else {
        bail!("ogrinfo not found in GDAL environment.");
    };
    let items = collect_inputs(input, false);
    if items.is_empty() && input.is_dir() {
        // Might be a .gdb directory readable by ogrinfo directly
        return run_cmd(gdal, ogrinfo, &["-so".into(), input.display().to_string()]);
    }
    for (p, _) in items {
        println!("{} {}", "Dataset:".bold().cyan(), p.display());
        run_cmd(
            gdal,
            ogrinfo,
            &["-al".into(), "-so".into(), p.display().to_string()],
        )?;
    }
    Ok(())
}

fn normalize_path_key(p: &Path) -> String {
    let resolved = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = resolved.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        s.trim_start_matches(r"\\?\").to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        s
    }
}

fn check_safe_gdal_output(dst: &Path, inputs: &[&Path]) -> Result<()> {
    let dst_key = normalize_path_key(dst);
    for inp in inputs {
        if normalize_path_key(inp) == dst_key {
            bail!(
                "Refusing to overwrite input dataset '{}'. Choose a different --out path.",
                dst.display()
            );
        }
    }
    if !crate::engine::overwrite_enabled() && dst.exists() {
        bail!(
            "Output already exists: '{}'. Pass --overwrite to replace it.",
            dst.display()
        );
    }
    Ok(())
}

pub fn gdal_project(gdal: &GdalEnvironment, input: &Path, epsg: u32, out: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    let items = collect_inputs(input, true);
    if items.is_empty() {
        println!("{}", "No datasets found to project.".yellow());
        return Ok(());
    }
    let is_batch = items.len() > 1 || out.is_dir() || out.extension().is_none();
    if is_batch {
        fs::create_dir_all(out)?;
    } else if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }

    let srs = format!("EPSG:{epsg}");
    let overwrite = crate::engine::overwrite_enabled();
    let mut failures = 0usize;

    for (idx, (src, is_raster)) in items.iter().enumerate() {
        let dst = if is_batch {
            out.join(src.file_name().unwrap_or_default())
        } else {
            out.to_path_buf()
        };
        let res = check_safe_gdal_output(&dst, &[src.as_path()]).and_then(|()| {
            if *is_raster {
                match &gdal.gdalwarp {
                    Some(warp) => {
                        let mut args = Vec::new();
                        if overwrite {
                            args.push("-overwrite".into());
                        }
                        args.extend([
                            "-t_srs".into(),
                            srs.clone(),
                            src.display().to_string(),
                            dst.display().to_string(),
                        ]);
                        run_cmd(gdal, warp, &args)
                    }
                    None => bail!("gdalwarp not found for raster reprojection."),
                }
            } else {
                let mut args = Vec::new();
                if overwrite {
                    args.push("-overwrite".into());
                }
                args.extend([
                    "-t_srs".into(),
                    srs.clone(),
                    dst.display().to_string(),
                    src.display().to_string(),
                ]);
                run_cmd(gdal, ogr2ogr, &args)
            }
        });
        match res {
            Ok(()) => println!(
                "[{}/{}] OK   {} -> {}",
                idx + 1,
                items.len(),
                src.display(),
                dst.display()
            ),
            Err(e) => {
                failures += 1;
                eprintln!(
                    "[{}/{}] ERR  {}: {}",
                    idx + 1,
                    items.len(),
                    src.display(),
                    e
                );
            }
        }
    }
    if failures > 0 {
        bail!("{failures} dataset(s) failed during GDAL projection.");
    }
    Ok(())
}

pub fn gdal_clip(gdal: &GdalEnvironment, input: &Path, aoi: &Path, out: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    let items = collect_inputs(input, true);
    if items.is_empty() {
        println!("{}", "No datasets found to clip.".yellow());
        return Ok(());
    }
    let is_batch = items.len() > 1 || out.is_dir() || out.extension().is_none();
    if is_batch {
        fs::create_dir_all(out)?;
    } else if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }

    let overwrite = crate::engine::overwrite_enabled();
    let mut failures = 0usize;
    for (idx, (src, is_raster)) in items.iter().enumerate() {
        let dst = if is_batch {
            out.join(src.file_name().unwrap_or_default())
        } else {
            out.to_path_buf()
        };
        let res = check_safe_gdal_output(&dst, &[src.as_path(), aoi]).and_then(|()| {
            if *is_raster {
                match &gdal.gdalwarp {
                    Some(warp) => {
                        let mut args = Vec::new();
                        if overwrite {
                            args.push("-overwrite".into());
                        }
                        args.extend([
                            "-cutline".into(),
                            aoi.display().to_string(),
                            "-crop_to_cutline".into(),
                            src.display().to_string(),
                            dst.display().to_string(),
                        ]);
                        run_cmd(gdal, warp, &args)
                    }
                    None => bail!("gdalwarp not found for raster clipping."),
                }
            } else {
                let mut args = Vec::new();
                if overwrite {
                    args.push("-overwrite".into());
                }
                args.extend([
                    "-clipsrc".into(),
                    aoi.display().to_string(),
                    dst.display().to_string(),
                    src.display().to_string(),
                ]);
                run_cmd(gdal, ogr2ogr, &args)
            }
        });
        match res {
            Ok(()) => println!(
                "[{}/{}] OK   {} -> {}",
                idx + 1,
                items.len(),
                src.display(),
                dst.display()
            ),
            Err(e) => {
                failures += 1;
                eprintln!(
                    "[{}/{}] ERR  {}: {}",
                    idx + 1,
                    items.len(),
                    src.display(),
                    e
                );
            }
        }
    }
    if failures > 0 {
        bail!("{failures} dataset(s) failed during GDAL clip.");
    }
    Ok(())
}

pub fn gdal_merge(gdal: &GdalEnvironment, input: &Path, out: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    let items = collect_inputs(input, false);
    if items.len() < 2 {
        bail!("Need at least 2 vector datasets to merge.");
    }
    let in_refs: Vec<&Path> = items.iter().map(|(p, _)| p.as_path()).collect();
    check_safe_gdal_output(out, &in_refs)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let overwrite = crate::engine::overwrite_enabled();
    for (idx, (src, _)) in items.iter().enumerate() {
        let mut args = Vec::new();
        if idx == 0 && overwrite {
            args.push("-overwrite".into());
        } else if idx > 0 {
            args.push("-update".into());
            args.push("-append".into());
        }
        args.push(out.display().to_string());
        args.push(src.display().to_string());
        run_cmd(gdal, ogr2ogr, &args)?;
    }
    println!("OK Merged {} dataset(s) -> {}", items.len(), out.display());
    Ok(())
}

pub fn gdal_pack_gpkg(gdal: &GdalEnvironment, input: &Path, out: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    let items = collect_inputs(input, false);
    if items.is_empty() {
        println!("{}", "No vector datasets found to pack.".yellow());
        return Ok(());
    }
    let out_gpkg = if out
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("gpkg"))
        .unwrap_or(false)
    {
        out.to_path_buf()
    } else {
        out.with_extension("gpkg")
    };
    let in_refs: Vec<&Path> = items.iter().map(|(p, _)| p.as_path()).collect();
    check_safe_gdal_output(&out_gpkg, &in_refs)?;
    if let Some(parent) = out_gpkg.parent() {
        fs::create_dir_all(parent)?;
    }
    let overwrite = crate::engine::overwrite_enabled();
    for (idx, (src, _)) in items.iter().enumerate() {
        let mut args = vec!["-f".into(), "GPKG".into()];
        if idx == 0 && overwrite {
            args.push("-overwrite".into());
        } else if idx > 0 {
            args.push("-update".into());
        }
        args.push(out_gpkg.display().to_string());
        args.push(src.display().to_string());
        run_cmd(gdal, ogr2ogr, &args)?;
        println!(
            "[{}/{}] OK   {} -> {}",
            idx + 1,
            items.len(),
            src.display(),
            out_gpkg.display()
        );
    }
    Ok(())
}

pub fn gdal_unpack(gdal: &GdalEnvironment, input: &Path, out_dir: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    fs::create_dir_all(out_dir)?;
    let mut args = vec!["-f".into(), "ESRI Shapefile".into()];
    if crate::engine::overwrite_enabled() {
        args.push("-overwrite".into());
    }
    args.push(out_dir.display().to_string());
    args.push(input.display().to_string());
    run_cmd(gdal, ogr2ogr, &args)?;
    println!("OK Unpacked {} -> {}", input.display(), out_dir.display());
    Ok(())
}

pub fn gdal_mosaic(gdal: &GdalEnvironment, input_dir: &Path, out: &Path) -> Result<()> {
    let Some(warp) = &gdal.gdalwarp else {
        bail!("gdalwarp not found in GDAL environment.");
    };
    let rasters: Vec<PathBuf> = collect_inputs(input_dir, true)
        .into_iter()
        .filter(|(_, is_r)| *is_r)
        .map(|(p, _)| p)
        .collect();
    if rasters.is_empty() {
        bail!("No raster files found in {}", input_dir.display());
    }
    let in_refs: Vec<&Path> = rasters.iter().map(|p| p.as_path()).collect();
    check_safe_gdal_output(out, &in_refs)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut args = Vec::new();
    if crate::engine::overwrite_enabled() {
        args.push("-overwrite".to_string());
    }
    for r in &rasters {
        args.push(r.display().to_string());
    }
    args.push(out.display().to_string());
    run_cmd(gdal, warp, &args)?;
    println!(
        "OK Mosaicked {} raster(s) -> {}",
        rasters.len(),
        out.display()
    );
    Ok(())
}

pub fn gdal_terrain(
    gdal: &GdalEnvironment,
    dem: &Path,
    out_dir: &Path,
    slope: bool,
    aspect: bool,
    hillshade: bool,
) -> Result<()> {
    let Some(gdaldem) = &gdal.gdaldem else {
        bail!("gdaldem not found in GDAL environment.");
    };
    fs::create_dir_all(out_dir)?;
    let stem = dem.file_stem().and_then(|s| s.to_str()).unwrap_or("dem");

    if slope {
        let dst = out_dir.join(format!("{stem}_slope.tif"));
        check_safe_gdal_output(&dst, &[dem])?;
        run_cmd(
            gdal,
            gdaldem,
            &[
                "slope".into(),
                dem.display().to_string(),
                dst.display().to_string(),
            ],
        )?;
        println!("[OK] Slope     -> {}", dst.display());
    }
    if aspect {
        let dst = out_dir.join(format!("{stem}_aspect.tif"));
        check_safe_gdal_output(&dst, &[dem])?;
        run_cmd(
            gdal,
            gdaldem,
            &[
                "aspect".into(),
                dem.display().to_string(),
                dst.display().to_string(),
            ],
        )?;
        println!("[OK] Aspect    -> {}", dst.display());
    }
    if hillshade {
        let dst = out_dir.join(format!("{stem}_hillshade.tif"));
        check_safe_gdal_output(&dst, &[dem])?;
        run_cmd(
            gdal,
            gdaldem,
            &[
                "hillshade".into(),
                dem.display().to_string(),
                dst.display().to_string(),
            ],
        )?;
        println!("[OK] Hillshade -> {}", dst.display());
    }
    Ok(())
}

pub fn gdal_xy2shp(
    gdal: &GdalEnvironment,
    input_csv: &Path,
    x_field: &str,
    y_field: &str,
    epsg: u32,
    out: &Path,
) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    check_safe_gdal_output(out, &[input_csv])?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let driver = match out
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("shp")
        .to_ascii_lowercase()
        .as_str()
    {
        "gpkg" => "GPKG",
        "geojson" => "GeoJSON",
        "fgb" => "FlatGeobuf",
        "kml" => "KML",
        _ => "ESRI Shapefile",
    };

    let mut args = vec!["-f".into(), driver.into()];
    if crate::engine::overwrite_enabled() {
        args.push("-overwrite".into());
    }
    args.extend([
        "-a_srs".into(),
        format!("EPSG:{epsg}"),
        "-oo".into(),
        format!("X_POSSIBLE_NAMES={x_field}"),
        "-oo".into(),
        format!("Y_POSSIBLE_NAMES={y_field}"),
        "-oo".into(),
        "AUTODETECT_TYPE=YES".into(),
        out.display().to_string(),
        input_csv.display().to_string(),
    ]);
    run_cmd(gdal, ogr2ogr, &args)?;
    println!(
        "OK Created point dataset from CSV ({x_field}, {y_field}, EPSG:{epsg}) -> {}",
        out.display()
    );
    Ok(())
}

pub fn gdal_convert(gdal: &GdalEnvironment, input: &Path, to_fmt: &str, out: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    let (driver, ext) = match to_fmt.to_ascii_lowercase().as_str() {
        "shp" | "shapefile" => ("ESRI Shapefile", "shp"),
        "gpkg" | "geopackage" => ("GPKG", "gpkg"),
        "geojson" => ("GeoJSON", "geojson"),
        "kml" => ("KML", "kml"),
        "fgb" | "flatgeobuf" => ("FlatGeobuf", "fgb"),
        other => bail!("Unsupported format '{other}'. Use shp, gpkg, geojson, kml, or fgb."),
    };

    let items = collect_inputs(input, false);
    if items.is_empty() {
        println!("{}", "No vector datasets found to convert.".yellow());
        return Ok(());
    }
    let is_batch = items.len() > 1 || out.is_dir() || out.extension().is_none();
    if is_batch {
        fs::create_dir_all(out)?;
    } else if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let overwrite = crate::engine::overwrite_enabled();
    let mut failures = 0usize;
    for (idx, (src, _)) in items.iter().enumerate() {
        let dst = if is_batch {
            let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("layer");
            out.join(format!("{stem}.{ext}"))
        } else {
            out.to_path_buf()
        };
        let res = check_safe_gdal_output(&dst, &[src.as_path()]).and_then(|()| {
            let mut args = vec!["-f".into(), driver.into()];
            if overwrite {
                args.push("-overwrite".into());
            }
            args.push(dst.display().to_string());
            args.push(src.display().to_string());
            run_cmd(gdal, ogr2ogr, &args)
        });
        match res {
            Ok(()) => println!(
                "[{}/{}] OK   {} -> {}",
                idx + 1,
                items.len(),
                src.display(),
                dst.display()
            ),
            Err(e) => {
                failures += 1;
                eprintln!(
                    "[{}/{}] ERR  {}: {}",
                    idx + 1,
                    items.len(),
                    src.display(),
                    e
                );
            }
        }
    }
    if failures > 0 {
        bail!("{failures} dataset(s) failed during conversion.");
    }
    Ok(())
}
