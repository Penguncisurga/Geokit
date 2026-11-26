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
            source: Some("System PATH".to_string()),
        };
    }

    // Check QGIS installations on Windows (C:\Program Files\QGIS *\bin)
    if let Ok(entries) = fs::read_dir(r"C:\Program Files") {
        let mut qgis_bins: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.to_ascii_lowercase().starts_with("qgis"))
                        .unwrap_or(false)
            })
            .map(|p| p.join("bin"))
            .filter(|b| b.join("ogr2ogr.exe").is_file())
            .collect();
        qgis_bins.sort();
        if let Some(bin) = qgis_bins.pop() {
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
                source: Some(format!("QGIS ({})", bin.display())),
            };
        }
    }

    GdalEnvironment {
        ogr2ogr: None,
        ogrinfo: None,
        gdalwarp: None,
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

fn run_cmd(bin: &Path, args: &[String]) -> Result<()> {
    let status = Command::new(bin)
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
        return run_cmd(ogrinfo, &["-so".into(), input.display().to_string()]);
    }
    for (p, _) in items {
        println!("{} {}", "Dataset:".bold().cyan(), p.display());
        run_cmd(
            ogrinfo,
            &["-al".into(), "-so".into(), p.display().to_string()],
        )?;
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
    let mut failures = 0usize;

    for (idx, (src, is_raster)) in items.iter().enumerate() {
        let dst = if is_batch {
            out.join(src.file_name().unwrap_or_default())
        } else {
            out.to_path_buf()
        };
        let res = if *is_raster {
            match &gdal.gdalwarp {
                Some(warp) => run_cmd(
                    warp,
                    &[
                        "-overwrite".into(),
                        "-t_srs".into(),
                        srs.clone(),
                        src.display().to_string(),
                        dst.display().to_string(),
                    ],
                ),
                None => bail!("gdalwarp not found for raster reprojection."),
            }
        } else {
            run_cmd(
                ogr2ogr,
                &[
                    "-t_srs".into(),
                    srs.clone(),
                    dst.display().to_string(),
                    src.display().to_string(),
                ],
            )
        };
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

    let mut failures = 0usize;
    for (idx, (src, is_raster)) in items.iter().enumerate() {
        let dst = if is_batch {
            out.join(src.file_name().unwrap_or_default())
        } else {
            out.to_path_buf()
        };
        let res = if *is_raster {
            match &gdal.gdalwarp {
                Some(warp) => run_cmd(
                    warp,
                    &[
                        "-overwrite".into(),
                        "-cutline".into(),
                        aoi.display().to_string(),
                        "-crop_to_cutline".into(),
                        src.display().to_string(),
                        dst.display().to_string(),
                    ],
                ),
                None => bail!("gdalwarp not found for raster clipping."),
            }
        } else {
            run_cmd(
                ogr2ogr,
                &[
                    "-clipsrc".into(),
                    aoi.display().to_string(),
                    dst.display().to_string(),
                    src.display().to_string(),
                ],
            )
        };
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
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    for (idx, (src, _)) in items.iter().enumerate() {
        let mut args = Vec::new();
        if idx > 0 {
            args.push("-update".into());
            args.push("-append".into());
        }
        args.push(out.display().to_string());
        args.push(src.display().to_string());
        run_cmd(ogr2ogr, &args)?;
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
    if let Some(parent) = out_gpkg.parent() {
        fs::create_dir_all(parent)?;
    }
    for (idx, (src, _)) in items.iter().enumerate() {
        let mut args = vec!["-f".into(), "GPKG".into()];
        if idx > 0 {
            args.push("-update".into());
        }
        args.push(out_gpkg.display().to_string());
        args.push(src.display().to_string());
        run_cmd(ogr2ogr, &args)?;
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
    run_cmd(
        ogr2ogr,
        &[
            "-f".into(),
            "ESRI Shapefile".into(),
            out_dir.display().to_string(),
            input.display().to_string(),
        ],
    )?;
    println!("OK Unpacked {} -> {}", input.display(), out_dir.display());
    Ok(())
}
