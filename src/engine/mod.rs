pub mod gdal;

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::Builder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineBackend {
    Auto,
    Arcgis,
    Arcmap,
    Gdal,
}

const ARCPY_RUNNER_SCRIPT: &str = include_str!("../../scripts/arcpy_runner.py");
const ARCMAP_RUNNER_SCRIPT: &str = include_str!("../../scripts/arcmap_runner.py");

#[derive(Debug, Clone, Serialize)]
pub struct ArcGisEnvironment {
    pub arcgis_pro_propy: Option<PathBuf>,
    pub arcmap_python: Option<PathBuf>,
    pub active_source: Option<String>,
}

/// Auto-detects ArcGIS Pro (`propy.bat`) and ArcGIS Desktop 10.x (`python.exe`) installations.
pub fn detect_arcgis(override_propy: Option<&Path>) -> ArcGisEnvironment {
    let mut active_source = None;

    let arcgis_pro_propy = if let Some(custom) = override_propy {
        if custom.exists() {
            active_source = Some("CLI --propy flag".to_string());
            Some(custom.to_path_buf())
        } else {
            None
        }
    } else if let Ok(env_path) = env::var("GEOKIT_PROPY") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            active_source = Some("GEOKIT_PROPY env var".to_string());
            Some(p)
        } else {
            None
        }
    } else {
        find_default_propy().inspect(|_| {
            active_source = Some("Auto-detected ArcGIS Pro".to_string());
        })
    };

    let arcmap_python = find_default_arcmap_python();

    ArcGisEnvironment {
        arcgis_pro_propy,
        arcmap_python,
        active_source,
    }
}

fn find_default_propy() -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from(r"C:\Program Files\ArcGIS\Pro\bin\Python\Scripts\propy.bat"),
        PathBuf::from(r"C:\Program Files\ArcGIS Pro\ArcGISPro\bin\Python\Scripts\propy.bat"),
    ];

    if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data).join(r"Programs\ArcGIS\Pro\bin\Python\Scripts\propy.bat"),
        );
    }

    candidates.into_iter().find(|p| p.exists())
}

fn find_default_arcmap_python() -> Option<PathBuf> {
    let versions = ["10.8", "10.7", "10.6", "10.5", "10.4", "10.3"];
    for ver in versions {
        let p = PathBuf::from(format!(r"C:\Python27\ArcGIS{ver}\python.exe"));
        if p.exists() {
            return Some(p);
        }
        let p64 = PathBuf::from(format!(r"C:\Python27\ArcGISx64{ver}\python.exe"));
        if p64.exists() {
            return Some(p64);
        }
    }
    None
}

/// Returns true if `path` is an `.mxd` file OR a directory containing `.mxd` files (and no `.aprx`).
pub fn is_mxd_target(path: &Path) -> bool {
    if path.is_file() {
        return path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("mxd"))
            .unwrap_or(false);
    }
    if path.is_dir() {
        let mut has_mxd = false;
        let mut has_aprx = false;
        for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("mxd") {
                    has_mxd = true;
                } else if ext.eq_ignore_ascii_case("aprx") {
                    has_aprx = true;
                }
            }
        }
        return has_mxd && !has_aprx;
    }
    false
}

/// Resolves a path to an absolute string without Windows `\\?\` UNC prefix (which ArcPy rejects).
pub fn abs_string(p: &Path) -> String {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| {
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(p))
                    .unwrap_or_else(|_| p.to_path_buf())
            }
        })
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string()
}

/// Runs the embedded `arcpy_runner.py` script in a single ArcGIS Pro Python session
/// with the given JSON payload.
pub fn run_arcpy_job<T: Serialize>(override_propy: Option<&Path>, payload: &T) -> Result<()> {
    let env_info = detect_arcgis(override_propy);
    let Some(propy) = env_info.arcgis_pro_propy else {
        bail!(
            "ArcGIS Pro (propy.bat) was not found.\n\
             Hint: Install ArcGIS Pro or pass `--propy <path-to-propy.bat>` / set `GEOKIT_PROPY`.\n\
             Run `geokit doctor` to inspect your environment."
        );
    };

    let script_file = Builder::new()
        .prefix("geokit_arcpy_")
        .suffix(".py")
        .tempfile()
        .context("Failed to create temporary arcpy runner script")?;
    std::fs::write(script_file.path(), ARCPY_RUNNER_SCRIPT)
        .context("Failed to write temporary arcpy runner script")?;

    let payload_file = Builder::new()
        .prefix("geokit_payload_")
        .suffix(".json")
        .tempfile()
        .context("Failed to create temporary payload file")?;
    let json_str = serde_json::to_string_pretty(payload)?;
    std::fs::write(payload_file.path(), json_str)
        .context("Failed to write temporary payload file")?;

    let status = if cfg!(target_os = "windows")
        && propy
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("bat") || e.eq_ignore_ascii_case("cmd"))
            .unwrap_or(false)
    {
        Command::new("cmd")
            .arg("/C")
            .arg(&propy)
            .arg(script_file.path())
            .arg(payload_file.path())
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to invoke {}", propy.display()))?
    } else {
        Command::new(&propy)
            .arg(script_file.path())
            .arg(payload_file.path())
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to invoke {}", propy.display()))?
    };

    if !status.success() {
        bail!(
            "ArcGIS geoprocessing exited with status code {:?}",
            status.code()
        );
    }

    Ok(())
}

/// Runs the embedded `arcmap_runner.py` script in an ArcMap 10.x (Python 2.7) session.
pub fn run_arcmap_job<T: Serialize>(
    override_arcmap_python: Option<&Path>,
    payload: &T,
) -> Result<()> {
    let py = if let Some(custom) = override_arcmap_python {
        if custom.exists() {
            Some(custom.to_path_buf())
        } else {
            None
        }
    } else if let Ok(env_path) = env::var("GEOKIT_ARCMAP_PYTHON") {
        let p = PathBuf::from(env_path);
        p.exists().then_some(p)
    } else {
        find_default_arcmap_python()
    };

    let Some(python_exe) = py else {
        bail!(
            "ArcMap 10.x Python (python.exe) was not found.\n\
             Hint: Install ArcGIS Desktop 10.x or set `GEOKIT_ARCMAP_PYTHON`.\n\
             Run `geokit doctor` to inspect your environment."
        );
    };

    let script_file = Builder::new()
        .prefix("geokit_arcmap_")
        .suffix(".py")
        .tempfile()
        .context("Failed to create temporary arcmap runner script")?;
    std::fs::write(script_file.path(), ARCMAP_RUNNER_SCRIPT)
        .context("Failed to write temporary arcmap runner script")?;

    let payload_file = Builder::new()
        .prefix("geokit_payload_")
        .suffix(".json")
        .tempfile()
        .context("Failed to create temporary payload file")?;
    let json_str = serde_json::to_string_pretty(payload)?;
    std::fs::write(payload_file.path(), json_str)
        .context("Failed to write temporary payload file")?;

    let status = Command::new(&python_exe)
        .arg(script_file.path())
        .arg(payload_file.path())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to invoke {}", python_exe.display()))?;

    if !status.success() {
        bail!(
            "ArcMap geoprocessing exited with status code {:?}",
            status.code()
        );
    }

    Ok(())
}
