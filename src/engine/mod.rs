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

pub fn geokit_home_dir() -> Option<PathBuf> {
    env::var("USERPROFILE")
        .or_else(|_| env::var("HOME"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|h| PathBuf::from(h).join(".geokit"))
}

fn generate_hex_token_64() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::time::UNIX_EPOCH;

    let mut out = String::with_capacity(64);
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    for i in 0..4u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(nanos.wrapping_add(i as u128));
        h.write_u64(pid.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(i));
        let v = h.finish();
        out.push_str(&format!("{v:016x}"));
    }
    out
}

/// Returns the per-user localhost auth token (`GEOKIT_AUTH_TOKEN` or `~/.geokit/token`),
/// creating `~/.geokit/token` automatically if it does not exist yet.
pub fn get_or_create_local_token() -> Option<String> {
    if let Ok(env_tok) = env::var("GEOKIT_AUTH_TOKEN") {
        let t = env_tok.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    let dir = geokit_home_dir()?;
    let tok_path = dir.join("token");
    if let Ok(existing) = std::fs::read_to_string(&tok_path) {
        let t = existing.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    let tok = generate_hex_token_64();
    if std::fs::write(&tok_path, format!("{tok}\n")).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tok_path, std::fs::Permissions::from_mode(0o600));
        }
        return Some(tok);
    }
    None
}

/// Translates common cryptic ArcPy / ArcMap / GDAL error codes into actionable hints.
pub fn translate_arcpy_hint(stderr: &str) -> Option<&'static str> {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("000210") {
        return Some("Cannot create output (ERROR 000210): check if the file is open/locked in ArcMap/ArcGIS Pro, or pass `--overwrite` if replacing an existing dataset.");
    }
    if lower.contains("000732") {
        return Some("Input dataset does not exist or is not supported (ERROR 000732): verify the file path and run `geokit audit` to check for missing shapefile sidecars.");
    }
    if lower.contains("000824")
        || lower.contains("spatial analyst") && lower.contains("not licensed")
    {
        return Some("Spatial Analyst extension is not licensed or unavailable (ERROR 000824): enable Spatial Analyst in ArcMap/ArcGIS Pro Extensions, or switch to `--engine gdal` where supported.");
    }
    if lower.contains("010474") {
        return Some("Overlapping/invalid RemapRange values (ERROR 010474): ensure reclass range boundaries are strictly ascending without inverted intervals.");
    }
    if lower.contains("000539") {
        return Some("Field calculation syntax or null-value error (ERROR 000539): inspect attribute values with `geokit gp values` and guard against NULL values.");
    }
    if lower.contains("000358") {
        return Some("Invalid SQL WHERE expression (ERROR 000358): check field quoting and string literals for Shapefile vs File Geodatabase syntax.");
    }
    if lower.contains("000865") || lower.contains("000258") {
        return Some("Output already exists or parameter path is invalid (ERROR 000865/000258): pass `--overwrite` explicitly or choose a new `--out` path.");
    }
    if lower.contains("000622") || lower.contains("out of memory") {
        return Some("Tool execution ran out of memory (ERROR 000622): use 64-bit ArcGIS Pro / ArcGISx64 Background Geoprocessing, clip to AOI first, or use `--engine gdal`.");
    }
    if lower.contains("no raster store is configurated") {
        return Some("Relative raster path rejected by ArcMap (ERROR 999999): pass an absolute path or update to GeoKit >= 2.6.1 which auto-normalizes paths.");
    }
    if lower.contains("999999") {
        return Some("Generic ArcPy internal error (ERROR 999999): inspect geometry validity (`geokit gp repair`), shorten output filename/path, and check `~/.geokit/logs/geokit.log`.");
    }
    None
}

/// Appends diagnostic details to `~/.geokit/logs/geokit.log` (auto-rotated at 2 MB).
pub fn append_diagnostic_log(context: &str, details: &str) {
    if details.trim().is_empty() {
        return;
    }
    let Some(home) = geokit_home_dir() else {
        return;
    };
    let log_dir = home.join("logs");
    if std::fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let log_path = log_dir.join("geokit.log");
    if let Ok(meta) = std::fs::metadata(&log_path) {
        if meta.len() > 2 * 1024 * 1024 {
            let _ = std::fs::rename(&log_path, log_dir.join("geokit.log.1"));
        }
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut fh) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = writeln!(fh, "=== [{ts}] {context} ===\n{details}\n");
    }
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

    let mut req_val = payload.clone();
    if let (Some(obj), Some(tok)) = (req_val.as_object_mut(), get_or_create_local_token()) {
        obj.entry("token").or_insert_with(|| json!(tok));
    }

    let line = match serde_json::to_string(&req_val) {
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
        Err(e) => {
            eprintln!(
                "[geokit] Warning: {label} daemon on port {port} interrupted ({e}); falling back to direct subprocess..."
            );
            append_diagnostic_log(&format!("daemon-fallback:{label}"), &e.to_string());
            return None;
        }
    };
    if let Some(out) = resp.get("stdout").and_then(|s| s.as_str()) {
        if !out.is_empty() {
            print!("{out}");
            let _ = std::io::stdout().flush();
        }
    }
    let err_str = resp.get("stderr").and_then(|s| s.as_str()).unwrap_or("");
    if !err_str.is_empty() {
        eprint!("{err_str}");
        let _ = std::io::stderr().flush();
    }
    let code = resp.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
    if code == 0 {
        Some(Ok(()))
    } else {
        append_diagnostic_log(&format!("daemon:{label}:exit-{code}"), err_str);
        if let Some(hint) = translate_arcpy_hint(err_str) {
            eprintln!("Hint: {hint}");
        }
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
    // Prefer 64-bit Background Geoprocessing Python over 32-bit for large raster/vector memory headroom
    for ver in versions {
        let p64 = PathBuf::from(format!(r"C:\Python27\ArcGISx64{ver}\python.exe"));
        if p64.exists() {
            return Some(p64);
        }
    }
    for ver in versions {
        let p = PathBuf::from(format!(r"C:\Python27\ArcGIS{ver}\python.exe"));
        if p.exists() {
            return Some(p);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_arcpy_hint_matches_common_codes() {
        assert!(
            translate_arcpy_hint("ERROR 000210: Cannot create output foo.shp")
                .unwrap()
                .contains("--overwrite")
        );
        assert!(
            translate_arcpy_hint("ERROR 000824: The tool is not licensed.")
                .unwrap()
                .contains("Spatial Analyst")
        );
        assert!(translate_arcpy_hint("ERROR 010474: Invalid remap range")
            .unwrap()
            .contains("RemapRange"));
        assert!(
            translate_arcpy_hint("ERROR 999999: No raster store is configurated")
                .unwrap()
                .contains("absolute path")
        );
        assert!(translate_arcpy_hint("OK Reclassified raster -> out.tif").is_none());
    }
}
