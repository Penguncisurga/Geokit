pub mod gdal;

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use serde::Serialize;
use serde_json::{json, Value};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tempfile::Builder;

static OVERWRITE_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_overwrite(enabled: bool) {
    OVERWRITE_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn overwrite_enabled() -> bool {
    OVERWRITE_ENABLED.load(Ordering::Relaxed)
        || env::var("GEOKIT_OVERWRITE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

fn enrich_payload<T: Serialize>(payload: &T) -> Result<Value> {
    let mut val = serde_json::to_value(payload)?;
    if let Some(obj) = val.as_object_mut() {
        obj.entry("overwrite")
            .or_insert_with(|| Value::Bool(overwrite_enabled()));
    }
    Ok(val)
}

pub const DEFAULT_ARCMAP_DAEMON_PORT: u16 = 9878;
pub const DEFAULT_ARCPY_DAEMON_PORT: u16 = 9879;

pub fn arcmap_daemon_port() -> u16 {
    env::var("GEOKIT_ARCMAP_DAEMON_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ARCMAP_DAEMON_PORT)
}

pub fn arcpy_daemon_port() -> u16 {
    env::var("GEOKIT_ARCPY_DAEMON_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_ARCPY_DAEMON_PORT)
}

fn daemon_disabled() -> bool {
    env::var("GEOKIT_NO_DAEMON")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn daemon_send(
    port: u16,
    payload: &Value,
    read_timeout: Option<Duration>,
) -> Option<Result<Value>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(80)).ok()?;
    let _ = stream.set_read_timeout(read_timeout);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let line = match serde_json::to_string(payload) {
        Ok(s) => s,
        Err(e) => {
            return Some(Err(anyhow::anyhow!(
                "Failed to serialize daemon payload: {e}"
            )))
        }
    };
    if let Err(e) = writeln!(stream, "{line}").and_then(|_| stream.flush()) {
        return Some(Err(anyhow::anyhow!(
            "Failed to write to geokit daemon on port {port}: {e}"
        )));
    }

    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    if let Err(e) = reader.read_line(&mut resp_line) {
        return Some(Err(anyhow::anyhow!(
            "Failed to read response from geokit daemon on port {port}: {e}"
        )));
    }
    if resp_line.trim().is_empty() {
        return Some(Err(anyhow::anyhow!(
            "Empty response from geokit daemon on port {port}"
        )));
    }
    match serde_json::from_str::<Value>(resp_line.trim()) {
        Ok(v) => Some(Ok(v)),
        Err(e) => Some(Err(anyhow::anyhow!(
            "Invalid JSON from geokit daemon on port {port}: {e}"
        ))),
    }
}

pub fn ping_daemon(port: u16) -> Option<Value> {
    match daemon_send(
        port,
        &json!({ "action": "__ping__" }),
        Some(Duration::from_secs(2)),
    ) {
        Some(Ok(v)) if v.get("ok").and_then(|b| b.as_bool()).unwrap_or(false) => Some(v),
        _ => None,
    }
}

pub fn stop_daemon(port: u16) -> bool {
    matches!(
        daemon_send(
            port,
            &json!({ "action": "__shutdown__" }),
            Some(Duration::from_secs(3))
        ),
        Some(Ok(_))
    )
}

fn try_run_via_daemon<T: Serialize>(port: u16, label: &str, payload: &T) -> Option<Result<()>> {
    if daemon_disabled() {
        return None;
    }
    let val = match enrich_payload(payload) {
        Ok(v) => v,
        Err(_) => return None,
    };
    let resp = match daemon_send(port, &val, None)? {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    if let Some(out) = resp.get("stdout").and_then(|s| s.as_str()) {
        if !out.is_empty() {
            print!("{out}");
            let _ = std::io::stdout().flush();
        }
    }
    if let Some(err) = resp.get("stderr").and_then(|s| s.as_str()) {
        if !err.is_empty() {
            eprint!("{err}");
            let _ = std::io::stderr().flush();
        }
    }
    let code = resp.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
    if code == 0 {
        Some(Ok(()))
    } else {
        Some(Err(anyhow::anyhow!(
            "{label} hot-worker exited with status code {code}"
        )))
    }
}

fn resolve_pro_python_exe(propy: &Path) -> Option<PathBuf> {
    if let Some(scripts_dir) = propy.parent() {
        if let Some(python_dir) = scripts_dir.parent() {
            let direct = python_dir
                .join("envs")
                .join("arcgispro-py3")
                .join("python.exe");
            if direct.is_file() {
                return Some(direct);
            }
        }
    }
    None
}

pub fn start_daemon(
    backend: ResolvedBackend,
    override_propy: Option<&Path>,
    override_arcmap: Option<&Path>,
    idle_minutes: u32,
) -> Result<Value> {
    let (port, script_name, script_src, engine_label) = match backend {
        ResolvedBackend::Arcmap => (
            arcmap_daemon_port(),
            "geokit_daemon_arcmap.py",
            ARCMAP_RUNNER_SCRIPT,
            "ArcMap 10.x",
        ),
        ResolvedBackend::Arcgis => (
            arcpy_daemon_port(),
            "geokit_daemon_arcgis.py",
            ARCPY_RUNNER_SCRIPT,
            "ArcGIS Pro",
        ),
        ResolvedBackend::Gdal => {
            bail!("GDAL starts in <0.1s natively and does not need a warm daemon. Pass `--engine arcmap` or `--engine arcgis`.");
        }
    };

    if let Some(existing) = ping_daemon(port) {
        return Ok(existing);
    }

    let script_path = std::env::temp_dir().join(script_name);
    std::fs::write(&script_path, script_src)
        .with_context(|| format!("Failed to write {}", script_path.display()))?;

    let mut cmd = match backend {
        ResolvedBackend::Arcmap => {
            let py = if let Some(custom) = override_arcmap {
                custom.exists().then(|| custom.to_path_buf())
            } else if let Ok(env_path) = env::var("GEOKIT_ARCMAP_PYTHON") {
                let p = PathBuf::from(env_path);
                p.exists().then_some(p)
            } else {
                find_default_arcmap_python()
            };
            let Some(python_exe) = py else {
                bail!("ArcMap 10.x Python (python.exe) was not found.");
            };
            let mut c = Command::new(python_exe);
            c.arg(&script_path)
                .arg("--serve")
                .arg(port.to_string())
                .arg("--idle-minutes")
                .arg(idle_minutes.to_string());
            c
        }
        ResolvedBackend::Arcgis => {
            let env_info = detect_arcgis(override_propy);
            let Some(propy) = env_info.arcgis_pro_propy else {
                bail!("ArcGIS Pro (propy.bat) was not found.");
            };
            if let Some(py_exe) = resolve_pro_python_exe(&propy) {
                let mut c = Command::new(py_exe);
                c.arg(&script_path)
                    .arg("--serve")
                    .arg(port.to_string())
                    .arg("--idle-minutes")
                    .arg(idle_minutes.to_string());
                c
            } else {
                let mut c = Command::new("cmd");
                c.arg("/C")
                    .arg(&propy)
                    .arg(&script_path)
                    .arg("--serve")
                    .arg(port.to_string())
                    .arg("--idle-minutes")
                    .arg(idle_minutes.to_string());
                c
            }
        }
        ResolvedBackend::Gdal => unreachable!(),
    };

    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW (0x08000000) | DETACHED_PROCESS (0x00000008)
        cmd.creation_flags(0x08000000 | 0x00000008);
    }

    let started = Instant::now();
    cmd.spawn()
        .with_context(|| format!("Failed to spawn {engine_label} warm worker daemon"))?;

    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        if let Some(mut info) = ping_daemon(port) {
            if let Some(obj) = info.as_object_mut() {
                obj.insert(
                    "warmup_sec".into(),
                    json!((started.elapsed().as_secs_f64() * 10.0).round() / 10.0),
                );
            }
            return Ok(info);
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    bail!(
        "{engine_label} daemon did not respond on 127.0.0.1:{port} within 40s. Check your ArcGIS license/installation with `geokit doctor`."
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineBackend {
    Auto,
    Arcgis,
    Arcmap,
    Gdal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedBackend {
    Arcgis,
    Arcmap,
    Gdal,
}

/// Resolves `--engine auto` with priority: ArcGIS Pro -> ArcMap 10.x -> GDAL.
pub fn resolve_backend(
    backend: EngineBackend,
    override_propy: Option<&Path>,
    override_arcmap: Option<&Path>,
) -> ResolvedBackend {
    match backend {
        EngineBackend::Arcgis => ResolvedBackend::Arcgis,
        EngineBackend::Arcmap => ResolvedBackend::Arcmap,
        EngineBackend::Gdal => ResolvedBackend::Gdal,
        EngineBackend::Auto => {
            let env_info = detect_arcgis(override_propy);
            if env_info.arcgis_pro_propy.is_some() {
                ResolvedBackend::Arcgis
            } else if override_arcmap.map(|p| p.exists()).unwrap_or(false)
                || env_info.arcmap_python.is_some()
            {
                ResolvedBackend::Arcmap
            } else if gdal::detect_gdal().is_available() {
                ResolvedBackend::Gdal
            } else {
                ResolvedBackend::Arcgis
            }
        }
    }
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
    if let Some(res) = try_run_via_daemon(arcpy_daemon_port(), "ArcGIS", payload) {
        return res;
    }

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
    let json_str = serde_json::to_string_pretty(&enrich_payload(payload)?)?;
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
    if let Some(res) = try_run_via_daemon(arcmap_daemon_port(), "ArcMap", payload) {
        return res;
    }

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
    let json_str = serde_json::to_string_pretty(&enrich_payload(payload)?)?;
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
