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
    pub gdalinfo: Option<PathBuf>,
    pub gdal_translate: Option<PathBuf>,
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
            gdalinfo: find_on_path("gdalinfo"),
            gdal_translate: find_on_path("gdal_translate"),
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
                gdalinfo: bin
                    .join("gdalinfo.exe")
                    .is_file()
                    .then(|| bin.join("gdalinfo.exe")),
                gdal_translate: bin
                    .join("gdal_translate.exe")
                    .is_file()
                    .then(|| bin.join("gdal_translate.exe")),
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

    // Check OSGeo4W installations on Windows (C:\OSGeo4W\bin, C:\OSGeo4W64\bin)
    for osgeo_root in [PathBuf::from(r"C:\OSGeo4W"), PathBuf::from(r"C:\OSGeo4W64")] {
        let bin = osgeo_root.join("bin");
        if bin.join("ogr2ogr.exe").is_file() {
            let proj_data = osgeo_root.join("share").join("proj");
            let gdal_data = [
                osgeo_root.join("share").join("gdal"),
                osgeo_root
                    .join("apps")
                    .join("gdal")
                    .join("share")
                    .join("gdal"),
            ]
            .into_iter()
            .find(|p| p.is_dir());
            return GdalEnvironment {
                ogr2ogr: Some(bin.join("ogr2ogr.exe")),
                ogrinfo: bin
                    .join("ogrinfo.exe")
                    .is_file()
                    .then(|| bin.join("ogrinfo.exe")),
                gdalinfo: bin
                    .join("gdalinfo.exe")
                    .is_file()
                    .then(|| bin.join("gdalinfo.exe")),
                gdal_translate: bin
                    .join("gdal_translate.exe")
                    .is_file()
                    .then(|| bin.join("gdal_translate.exe")),
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
                source: Some(format!("OSGeo4W ({})", bin.display())),
            };
        }
    }

    // Check GDAL bundled inside ArcGIS Pro conda environment (arcgispro-py3\Library\bin)
    let mut pro_envs = vec![
        PathBuf::from(r"C:\Program Files\ArcGIS\Pro\bin\Python\envs\arcgispro-py3\Library"),
        PathBuf::from(
            r"C:\Program Files\ArcGIS Pro\ArcGISPro\bin\Python\envs\arcgispro-py3\Library",
        ),
    ];
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        pro_envs.push(
            PathBuf::from(local_app_data)
                .join(r"Programs\ArcGIS\Pro\bin\Python\envs\arcgispro-py3\Library"),
        );
    }
    for lib in pro_envs {
        let bin = lib.join("bin");
        if bin.join("ogr2ogr.exe").is_file() {
            let proj_data = lib.join("share").join("proj");
            let gdal_data = lib.join("share").join("gdal");
            return GdalEnvironment {
                ogr2ogr: Some(bin.join("ogr2ogr.exe")),
                ogrinfo: bin
                    .join("ogrinfo.exe")
                    .is_file()
                    .then(|| bin.join("ogrinfo.exe")),
                gdalinfo: bin
                    .join("gdalinfo.exe")
                    .is_file()
                    .then(|| bin.join("gdalinfo.exe")),
                gdal_translate: bin
                    .join("gdal_translate.exe")
                    .is_file()
                    .then(|| bin.join("gdal_translate.exe")),
                gdalwarp: bin
                    .join("gdalwarp.exe")
                    .is_file()
                    .then(|| bin.join("gdalwarp.exe")),
                gdaldem: bin
                    .join("gdaldem.exe")
                    .is_file()
                    .then(|| bin.join("gdaldem.exe")),
                proj_data: proj_data.is_dir().then_some(proj_data),
                gdal_data: gdal_data.is_dir().then_some(gdal_data),
                source: Some(format!("ArcGIS Pro GDAL ({})", bin.display())),
            };
        }
    }

    GdalEnvironment {
        ogr2ogr: None,
        ogrinfo: None,
        gdalinfo: None,
        gdal_translate: None,
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

fn geotiff_lzw_args(dst: &Path) -> Vec<String> {
    let ext = dst
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "tif" | "tiff") {
        vec![
            "-co".into(),
            "COMPRESS=LZW".into(),
            "-co".into(),
            "TILED=YES".into(),
        ]
    } else {
        Vec::new()
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
                        args.extend(geotiff_lzw_args(&dst));
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
                        args.extend(geotiff_lzw_args(&dst));
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
    args.extend(geotiff_lzw_args(out));
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
        let mut args = vec![
            "slope".into(),
            dem.display().to_string(),
            dst.display().to_string(),
        ];
        args.extend(geotiff_lzw_args(&dst));
        run_cmd(gdal, gdaldem, &args)?;
        println!("[OK] Slope     -> {}", dst.display());
    }
    if aspect {
        let dst = out_dir.join(format!("{stem}_aspect.tif"));
        check_safe_gdal_output(&dst, &[dem])?;
        let mut args = vec![
            "aspect".into(),
            dem.display().to_string(),
            dst.display().to_string(),
        ];
        args.extend(geotiff_lzw_args(&dst));
        run_cmd(gdal, gdaldem, &args)?;
        println!("[OK] Aspect    -> {}", dst.display());
    }
    if hillshade {
        let dst = out_dir.join(format!("{stem}_hillshade.tif"));
        check_safe_gdal_output(&dst, &[dem])?;
        let mut args = vec![
            "hillshade".into(),
            dem.display().to_string(),
            dst.display().to_string(),
        ];
        args.extend(geotiff_lzw_args(&dst));
        run_cmd(gdal, gdaldem, &args)?;
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
            // GPX files expose multiple OGR sub-layers (waypoints, tracks, routes).
            // Single-layer formats (GeoJSON, Shapefile, FlatGeobuf) need an explicit layer name.
            let src_ext = src
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if src_ext == "gpx" && matches!(ext, "geojson" | "shp" | "fgb") {
                if let Some(layer) = detect_primary_gpx_layer(gdal, src) {
                    args.push(layer);
                }
            }
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

fn driver_for_out(out: &Path) -> &'static str {
    match out
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("shp")
        .to_ascii_lowercase()
        .as_str()
    {
        "gpkg" => "GPKG",
        "geojson" | "json" => "GeoJSON",
        "fgb" => "FlatGeobuf",
        "kml" => "KML",
        "csv" => "CSV",
        _ => "ESRI Shapefile",
    }
}

pub fn gdal_select(
    gdal: &GdalEnvironment,
    input: &Path,
    where_clause: &str,
    out: &Path,
) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    check_safe_gdal_output(out, &[input])?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut args = vec!["-f".into(), driver_for_out(out).into()];
    if crate::engine::overwrite_enabled() {
        args.push("-overwrite".into());
    }
    args.extend([
        "-where".into(),
        where_clause.to_string(),
        out.display().to_string(),
        input.display().to_string(),
    ]);
    run_cmd(gdal, ogr2ogr, &args)?;
    println!("OK Selected features ({where_clause}) -> {}", out.display());
    Ok(())
}

pub fn gdal_export_table(gdal: &GdalEnvironment, input: &Path, out: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    check_safe_gdal_output(out, &[input])?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut args = vec!["-f".into(), "CSV".into()];
    if crate::engine::overwrite_enabled() {
        args.push("-overwrite".into());
    }
    args.extend([out.display().to_string(), input.display().to_string()]);
    run_cmd(gdal, ogr2ogr, &args)?;
    println!("OK Exported attribute table -> {}", out.display());
    Ok(())
}

pub fn gdal_flatten_2d(gdal: &GdalEnvironment, input: &Path, out: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    let items = collect_inputs(input, false);
    if items.is_empty() {
        println!("{}", "No vector datasets found to flatten.".yellow());
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
    for (idx, (src, _)) in items.iter().enumerate() {
        let dst = if is_batch {
            out.join(src.file_name().unwrap_or_default())
        } else {
            out.to_path_buf()
        };
        check_safe_gdal_output(&dst, &[src.as_path()])?;
        let mut args = vec!["-f".into(), driver_for_out(&dst).into()];
        if overwrite {
            args.push("-overwrite".into());
        }
        args.extend([
            "-dim".into(),
            "XY".into(),
            dst.display().to_string(),
            src.display().to_string(),
        ]);
        run_cmd(gdal, ogr2ogr, &args)?;
        println!(
            "[{}/{}] OK   Flattened 2D (Drop Z/M) {} -> {}",
            idx + 1,
            items.len(),
            src.display(),
            dst.display()
        );
    }
    Ok(())
}

pub fn gdal_define_crs(gdal: &GdalEnvironment, input: &Path, epsg: u32) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "shp" {
        bail!("GDAL `define-crs` in-place supports .shp files (use `gp project` or ArcGIS engine for other formats).");
    }
    let tmp = tempfile::tempdir()?;
    let tmp_shp = tmp.path().join("crs_probe.shp");
    run_cmd(
        gdal,
        ogr2ogr,
        &[
            "-f".into(),
            "ESRI Shapefile".into(),
            "-a_srs".into(),
            format!("EPSG:{epsg}"),
            "-limit".into(),
            "1".into(),
            tmp_shp.display().to_string(),
            input.display().to_string(),
        ],
    )?;
    let tmp_prj = tmp_shp.with_extension("prj");
    if !tmp_prj.is_file() {
        bail!("GDAL failed to generate .prj for EPSG:{epsg}.");
    }
    let dst_prj = input.with_extension("prj");
    fs::copy(&tmp_prj, &dst_prj)?;
    println!("OK Defined CRS EPSG:{epsg} -> {}", dst_prj.display());
    Ok(())
}

pub fn gdal_explode(gdal: &GdalEnvironment, input: &Path, out: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    check_safe_gdal_output(out, &[input])?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut args = vec!["-f".into(), driver_for_out(out).into()];
    if crate::engine::overwrite_enabled() {
        args.push("-overwrite".into());
    }
    args.extend([
        "-explodecollections".into(),
        out.display().to_string(),
        input.display().to_string(),
    ]);
    run_cmd(gdal, ogr2ogr, &args)?;
    println!("OK Exploded multipart -> singlepart: {}", out.display());
    Ok(())
}

pub fn gdal_split(gdal: &GdalEnvironment, input: &Path, field: &str, out_dir: &Path) -> Result<()> {
    let Some(ogr2ogr) = &gdal.ogr2ogr else {
        bail!("ogr2ogr not found in GDAL environment.");
    };
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let unique_vals: Vec<String> = if ext == "shp" {
        let recs = crate::gis::shp::read_dbf_records(&input.with_extension("dbf"))?;
        let mut set = std::collections::BTreeSet::new();
        for r in recs {
            if let Some((_, v)) = r.iter().find(|(k, _)| k.eq_ignore_ascii_case(field)) {
                let s = match v {
                    serde_json::Value::String(s) => s.trim().to_string(),
                    serde_json::Value::Null => continue,
                    other => other.to_string(),
                };
                if !s.is_empty() {
                    set.insert(s);
                }
            }
        }
        set.into_iter().collect()
    } else if ext == "geojson" || ext == "json" {
        let raw = fs::read_to_string(input)?;
        let val: serde_json::Value = serde_json::from_str(&raw)?;
        let mut set = std::collections::BTreeSet::new();
        if let Some(feats) = val.get("features").and_then(|v| v.as_array()) {
            for f in feats {
                if let Some(props) = f.get("properties").and_then(|p| p.as_object()) {
                    if let Some((_, v)) = props.iter().find(|(k, _)| k.eq_ignore_ascii_case(field))
                    {
                        let s = match v {
                            serde_json::Value::String(s) => s.trim().to_string(),
                            serde_json::Value::Null => continue,
                            other => other.to_string(),
                        };
                        if !s.is_empty() {
                            set.insert(s);
                        }
                    }
                }
            }
        }
        set.into_iter().collect()
    } else {
        bail!("GDAL `gp split` supports .shp and .geojson inputs (use ArcGIS/ArcMap engine for .gdb).");
    };

    if unique_vals.is_empty() {
        bail!(
            "No non-empty values found in field '{field}' of {}",
            input.display()
        );
    }
    fs::create_dir_all(out_dir)?;
    let overwrite = crate::engine::overwrite_enabled();
    for (idx, val) in unique_vals.iter().enumerate() {
        let safe = crate::gis::sanitize_gis_name(val);
        let dst = out_dir.join(format!("{safe}.shp"));
        check_safe_gdal_output(&dst, &[input])?;
        let escaped = val.replace('\'', "''");
        let where_q = format!("\"{field}\" = '{escaped}'");
        let mut args = vec!["-f".into(), "ESRI Shapefile".into()];
        if overwrite {
            args.push("-overwrite".into());
        }
        args.extend([
            "-where".into(),
            where_q,
            dst.display().to_string(),
            input.display().to_string(),
        ]);
        run_cmd(gdal, ogr2ogr, &args)?;
        println!(
            "[{}/{}] OK   {field}='{val}' -> {}",
            idx + 1,
            unique_vals.len(),
            dst.display()
        );
    }
    Ok(())
}

fn run_cmd_capture(gdal: &GdalEnvironment, bin: &Path, args: &[String]) -> Result<String> {
    let mut cmd = Command::new(bin);
    if let Some(proj) = &gdal.proj_data {
        cmd.env("PROJ_DATA", proj).env("PROJ_LIB", proj);
    }
    if let Some(gd) = &gdal.gdal_data {
        cmd.env("GDAL_DATA", gd);
    }
    let output = cmd
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Failed to execute {}", bin.display()))?;
    if !output.status.success() {
        bail!(
            "{} exited with status {:?}: {}",
            bin.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn detect_primary_gpx_layer(gdal: &GdalEnvironment, gpx_path: &Path) -> Option<String> {
    let ogrinfo = gdal.ogrinfo.as_ref()?;
    for candidate in ["tracks", "waypoints", "routes", "track_points"] {
        if let Ok(out) = run_cmd_capture(
            gdal,
            ogrinfo,
            &[
                "-so".into(),
                gpx_path.display().to_string(),
                candidate.into(),
            ],
        ) {
            for line in out.lines() {
                if let Some(rest) = line.trim().strip_prefix("Feature Count:") {
                    if rest.trim().parse::<usize>().unwrap_or(0) > 0 {
                        return Some(candidate.to_string());
                    }
                }
            }
        }
    }
    Some("waypoints".to_string())
}

pub fn gdal_raster_align(
    gdal: &GdalEnvironment,
    inputs: &[PathBuf],
    reference: &Path,
    out: &Path,
    resampling: &str,
) -> Result<()> {
    let Some(warp) = &gdal.gdalwarp else {
        bail!("gdalwarp not found in GDAL environment.");
    };
    let Some(gdalinfo) = &gdal.gdalinfo else {
        bail!("gdalinfo not found in GDAL environment.");
    };
    let info_raw = run_cmd_capture(
        gdal,
        gdalinfo,
        &["-json".into(), reference.display().to_string()],
    )?;
    let info: serde_json::Value =
        serde_json::from_str(&info_raw).context("Failed to parse gdalinfo -json output")?;

    let wkt = info
        .get("coordinateSystem")
        .and_then(|c| c.get("wkt"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!("Reference raster has no CRS WKT: {}", reference.display())
        })?;
    let gt = info
        .get("geoTransform")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Reference raster missing geoTransform"))?;
    let px_x = gt.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0).abs();
    let px_y = gt.get(5).and_then(|v| v.as_f64()).unwrap_or(0.0).abs();
    if px_x <= 0.0 || px_y <= 0.0 {
        bail!("Invalid reference raster pixel size ({px_x}, {px_y}).");
    }

    let corners = info
        .get("cornerCoordinates")
        .ok_or_else(|| anyhow::anyhow!("Reference raster missing cornerCoordinates"))?;
    let ll = corners.get("lowerLeft").and_then(|v| v.as_array());
    let ur = corners.get("upperRight").and_then(|v| v.as_array());
    let (Some(ll), Some(ur)) = (ll, ur) else {
        bail!("Reference raster missing lowerLeft/upperRight extent.");
    };
    let xmin = ll[0].as_f64().unwrap_or(0.0);
    let ymin = ll[1].as_f64().unwrap_or(0.0);
    let xmax = ur[0].as_f64().unwrap_or(0.0);
    let ymax = ur[1].as_f64().unwrap_or(0.0);

    let gdal_resamp = match resampling.to_ascii_lowercase().as_str() {
        "nearest" | "near" => "near",
        "cubic" => "cubic",
        "average" => "average",
        "mode" => "mode",
        _ => "bilinear",
    };

    // Write WKT to a temp .prj file so Windows command-line length/quoting is never an issue
    let tmp_dir = tempfile::tempdir()?;
    let wkt_file = tmp_dir.path().join("ref_srs.prj");
    fs::write(&wkt_file, wkt)?;

    let is_batch = inputs.len() > 1 || out.is_dir() || out.extension().is_none();
    if is_batch {
        fs::create_dir_all(out)?;
    } else if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let overwrite = crate::engine::overwrite_enabled();
    for (idx, src) in inputs.iter().enumerate() {
        let dst = if is_batch {
            let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("raster");
            out.join(format!("{stem}_aligned.tif"))
        } else {
            out.to_path_buf()
        };
        check_safe_gdal_output(&dst, &[src.as_path(), reference])?;
        let mut args = Vec::new();
        if overwrite {
            args.push("-overwrite".into());
        }
        args.extend(geotiff_lzw_args(&dst));
        args.extend([
            "-t_srs".into(),
            wkt_file.display().to_string(),
            "-tr".into(),
            format!("{px_x}"),
            format!("{px_y}"),
            "-te".into(),
            format!("{xmin}"),
            format!("{ymin}"),
            format!("{xmax}"),
            format!("{ymax}"),
            "-r".into(),
            gdal_resamp.into(),
            src.display().to_string(),
            dst.display().to_string(),
        ]);
        run_cmd(gdal, warp, &args)?;
        println!(
            "[{}/{}] OK   Aligned {} -> {} ({:.4}x{:.4}, {})",
            idx + 1,
            inputs.len(),
            src.display(),
            dst.display(),
            px_x,
            px_y,
            gdal_resamp
        );
    }
    Ok(())
}

pub fn gdal_raster_georef(
    gdal: &GdalEnvironment,
    input: &Path,
    gcps: &[crate::gis::gcp::GcpPoint],
    epsg: u32,
    order: u8,
    out: &Path,
) -> Result<()> {
    let Some(translate) = &gdal.gdal_translate else {
        bail!("gdal_translate not found in GDAL environment.");
    };
    let Some(warp) = &gdal.gdalwarp else {
        bail!("gdalwarp not found in GDAL environment.");
    };
    check_safe_gdal_output(out, &[input])?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let tmp_dir = tempfile::tempdir()?;
    let gcp_tif = tmp_dir.path().join("geokit_gcp_stage.tif");

    let mut t_args = vec![
        "-of".into(),
        "GTiff".into(),
        "-a_srs".into(),
        format!("EPSG:{epsg}"),
    ];
    for g in gcps {
        t_args.extend([
            "-gcp".into(),
            format!("{}", g.pixel_x),
            format!("{}", g.pixel_y),
            format!("{}", g.map_x),
            format!("{}", g.map_y),
        ]);
    }
    t_args.extend([input.display().to_string(), gcp_tif.display().to_string()]);
    run_cmd(gdal, translate, &t_args)?;

    let mut w_args = Vec::new();
    if crate::engine::overwrite_enabled() {
        w_args.push("-overwrite".into());
    }
    w_args.extend(geotiff_lzw_args(out));
    w_args.extend([
        "-t_srs".into(),
        format!("EPSG:{epsg}"),
        "-order".into(),
        order.clamp(1, 3).to_string(),
        "-r".into(),
        "bilinear".into(),
        gcp_tif.display().to_string(),
        out.display().to_string(),
    ]);
    run_cmd(gdal, warp, &w_args)?;
    println!(
        "OK Georeferenced {} ({} GCPs, order={}, EPSG:{}) -> {}",
        input.display(),
        gcps.len(),
        order.clamp(1, 3),
        epsg,
        out.display()
    );
    Ok(())
}
