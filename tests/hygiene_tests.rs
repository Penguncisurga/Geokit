use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn geokit_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_geokit"))
}

#[test]
fn test_audit_organize_sanitize_clean_workflow() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create a messy shapefile bundle (with spaces & uppercase)
    fs::write(root.join("01 Batas Desa.shp"), b"shp").unwrap();
    fs::write(root.join("01 Batas Desa.shx"), b"shx").unwrap();
    fs::write(root.join("01 Batas Desa.dbf"), b"dbf").unwrap();
    fs::write(root.join("01 Batas Desa.prj"), b"prj").unwrap();
    fs::write(root.join("01 Batas Desa.shp.xml"), b"xml").unwrap();

    // Create a raster + sidecar
    fs::write(root.join("DEM Wilayah.tif"), b"tif").unwrap();
    fs::write(root.join("DEM Wilayah.tfw"), b"tfw").unwrap();

    // Create a File Geodatabase folder with an internal .lock file
    let gdb = root.join("Data Proyek.gdb");
    fs::create_dir(&gdb).unwrap();
    fs::write(gdb.join("a00000001.gdbtable"), b"table").unwrap();
    fs::write(gdb.join("a00000001.lock"), b"lock").unwrap();

    // 1. Audit JSON
    let audit_out = geokit_bin()
        .args(["audit", root.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(audit_out.status.success());
    let audit_json: serde_json::Value = serde_json::from_slice(&audit_out.stdout).unwrap();
    assert_eq!(audit_json["datasets"].as_array().unwrap().len(), 3);
    assert_eq!(audit_json["lock_files"].as_array().unwrap().len(), 1);

    // 2. Sanitize (actual run)
    let san_out = geokit_bin()
        .args(["sanitize", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(san_out.status.success());
    assert!(root.join("l_01_batas_desa.shp").exists());
    assert!(root.join("l_01_batas_desa.dbf").exists());
    assert!(root.join("l_01_batas_desa.shp.xml").exists());
    assert!(root.join("dem_wilayah.tif").exists());
    assert!(root.join("dem_wilayah.tfw").exists());
    assert!(root.join("data_proyek.gdb").is_dir());

    // 3. Organize
    let org_out = geokit_bin()
        .args(["organize", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(org_out.status.success());
    assert!(root.join("01_Vector").join("l_01_batas_desa.shp").exists());
    assert!(root.join("01_Vector").join("l_01_batas_desa.dbf").exists());
    assert!(root.join("02_Raster").join("dem_wilayah.tif").exists());
    assert!(root.join("02_Raster").join("dem_wilayah.tfw").exists());
    assert!(root.join("03_Geodatabase").join("data_proyek.gdb").is_dir());

    // 4. Clean: default Safe Mode protects .gdb internal .lock files!
    let gdb_lock = root
        .join("03_Geodatabase")
        .join("data_proyek.gdb")
        .join("a00000001.lock");
    let loose_tmp = root.join("scratch.tmp");
    fs::write(&loose_tmp, b"tmp").unwrap();

    let clean_safe = geokit_bin()
        .args(["clean", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(clean_safe.status.success());
    assert!(!loose_tmp.exists(), "loose .tmp must be cleaned");
    assert!(
        gdb_lock.exists(),
        "default clean must NOT delete .lock inside .gdb without --include-gdb-locks"
    );

    // Explicit --include-gdb-locks removes .gdb lock while preserving .gdbtable
    let clean_gdb = geokit_bin()
        .args(["clean", root.to_str().unwrap(), "--include-gdb-locks"])
        .output()
        .unwrap();
    assert!(clean_gdb.status.success());
    assert!(!gdb_lock.exists());
    // Ensure gdb internal table was untouched
    assert!(root
        .join("03_Geodatabase")
        .join("data_proyek.gdb")
        .join("a00000001.gdbtable")
        .exists());
}

#[test]
fn test_init_and_custom_config_organize() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let init_out = geokit_bin()
        .args(["init", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(init_out.status.success());
    let cfg_path = root.join("geokit.toml");
    assert!(cfg_path.is_file());

    fs::write(
        &cfg_path,
        r#"
[folders]
vector = "SHP_Data"
raster = "IMG_Data"
"#,
    )
    .unwrap();

    fs::write(root.join("sungai.shp"), b"shp").unwrap();
    fs::write(root.join("sungai.dbf"), b"dbf").unwrap();
    fs::write(root.join("dem.tif"), b"tif").unwrap();

    let org_out = geokit_bin()
        .args(["organize", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(org_out.status.success());
    assert!(root.join("SHP_Data").join("sungai.shp").exists());
    assert!(root.join("SHP_Data").join("sungai.dbf").exists());
    assert!(root.join("IMG_Data").join("dem.tif").exists());
}

#[test]
fn test_mcp_server_stdio_handshake_and_tools() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = geokit_bin()
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{{}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"geokit_doctor","arguments":{{}}}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":4,"method":"prompts/list","params":{{}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":5,"method":"prompts/get","params":{{"name":"peta_kerawanan","arguments":{{"folder":"D:\\Proyek"}}}}}}"#
        )
        .unwrap();
    });

    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(output.status.success());

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 5);

    let init_resp: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "geokit");

    let list_resp: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    let tools = list_resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 20);

    let call_resp: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(call_resp["result"]["isError"], false);

    let prompts_resp: serde_json::Value = serde_json::from_str(lines[3]).unwrap();
    let prompts = prompts_resp["result"]["prompts"].as_array().unwrap();
    assert_eq!(prompts.len(), 3);

    let prompt_get: serde_json::Value = serde_json::from_str(lines[4]).unwrap();
    let prompt_text = prompt_get["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(prompt_text.contains("D:\\Proyek"));
    assert!(prompt_text.contains("classify"));
}

#[test]
fn test_organize_ignores_plain_json_but_moves_geojson() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    fs::write(root.join("package.json"), b"{}").unwrap();
    fs::write(root.join("batas.geojson"), b"{}").unwrap();

    let org_out = geokit_bin()
        .args(["organize", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(org_out.status.success());

    // Plain .json must remain untouched in root
    assert!(root.join("package.json").exists());
    assert!(!root.join("01_Vector").join("package.json").exists());
    // .geojson must be organized into 01_Vector
    assert!(root.join("01_Vector").join("batas.geojson").exists());
}

#[test]
fn test_toolbox_create_and_pipeline_run() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // 1. Generate a ModelBuilder-ready Python Toolbox (.pyt)
    let pyt = root.join("DAS_Tools.pyt");
    let tb_out = geokit_bin()
        .args([
            "toolbox",
            "create",
            "GeoKit_DAS",
            "--out",
            pyt.to_str().unwrap(),
            "--template",
            "das",
        ])
        .output()
        .unwrap();
    assert!(tb_out.status.success());
    let pyt_content = fs::read_to_string(&pyt).unwrap();
    assert!(pyt_content.contains("class Toolbox"));
    assert!(pyt_content.contains("DelineateWatershedTool"));

    // 2. Generate and dry-run + execute a declarative pipeline.toml
    let pipe = root.join("pipeline.toml");
    let init_out = geokit_bin()
        .args(["pipeline", "init", "--out", pipe.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(init_out.status.success());
    assert!(pipe.is_file());

    let dry_out = geokit_bin()
        .args(["pipeline", "run", pipe.to_str().unwrap(), "--dry-run"])
        .output()
        .unwrap();
    assert!(dry_out.status.success());

    let exec_out = geokit_bin()
        .current_dir(root)
        .args(["pipeline", "run", pipe.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(exec_out.status.success());
}

#[test]
fn test_report_markdown_html_and_json() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    fs::write(root.join("batas_desa.shp"), b"shp").unwrap();
    fs::write(root.join("batas_desa.shx"), b"shx").unwrap();
    fs::write(root.join("batas_desa.dbf"), b"dbf").unwrap();
    fs::write(root.join("batas_desa.prj"), b"prj").unwrap();
    fs::write(root.join("dem_kendal.tif"), b"tif").unwrap();

    let md_path = root.join("inventory.md");
    let html_path = root.join("inventory.html");
    let json_path = root.join("inventory.json");

    let md_out = geokit_bin()
        .args([
            "report",
            root.to_str().unwrap(),
            "--out",
            md_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(md_out.status.success());
    let md = fs::read_to_string(&md_path).unwrap();
    assert!(md.contains("batas_desa.shp"));
    assert!(md.contains("dem_kendal.tif"));

    let html_out = geokit_bin()
        .args([
            "report",
            root.to_str().unwrap(),
            "--out",
            html_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(html_out.status.success());
    let html = fs::read_to_string(&html_path).unwrap();
    assert!(html.contains("<html"));
    assert!(html.contains("batas_desa.shp"));

    let json_out = geokit_bin()
        .args([
            "report",
            root.to_str().unwrap(),
            "--out",
            json_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(json_out.status.success());
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&json_path).unwrap()).unwrap();
    assert!(parsed["datasets"].as_array().unwrap().len() >= 2);
}

#[test]
fn test_gp_summary_and_webmap_native() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let geojson_path = root.join("kerawanan.geojson");
    let sample = r#"{
      "type": "FeatureCollection",
      "features": [
        {
          "type": "Feature",
          "properties": { "KELAS": "Rendah", "AREA_HA": 120.5 },
          "geometry": { "type": "Polygon", "coordinates": [[[110.1,-7.0],[110.2,-7.0],[110.2,-6.9],[110.1,-6.9],[110.1,-7.0]]] }
        },
        {
          "type": "Feature",
          "properties": { "KELAS": "Tinggi", "AREA_HA": 279.5 },
          "geometry": { "type": "Polygon", "coordinates": [[[110.2,-7.0],[110.3,-7.0],[110.3,-6.9],[110.2,-6.9],[110.2,-7.0]]] }
        }
      ]
    }"#;
    fs::write(&geojson_path, sample).unwrap();

    let md_out = root.join("rekap.md");
    let sum_res = geokit_bin()
        .args([
            "gp",
            "summary",
            geojson_path.to_str().unwrap(),
            "--group-by",
            "KELAS",
            "--out",
            md_out.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        sum_res.status.success(),
        "summary stderr: {}",
        String::from_utf8_lossy(&sum_res.stderr)
    );
    let sum_json: serde_json::Value =
        serde_json::from_slice(&sum_res.stdout).expect("valid summary JSON");
    assert_eq!(sum_json["total_features"], 2);
    assert_eq!(sum_json["total_area"], 400.0);
    let md_text = fs::read_to_string(&md_out).unwrap();
    assert!(md_text.contains("Rendah"));
    assert!(md_text.contains("Tinggi"));
    assert!(md_text.contains("400.00"));

    let html_out = root.join("peta.html");
    let wm_res = geokit_bin()
        .args([
            "webmap",
            geojson_path.to_str().unwrap(),
            "--field",
            "KELAS",
            "--title",
            "Uji WebMap",
            "--out",
            html_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        wm_res.status.success(),
        "webmap stderr: {}",
        String::from_utf8_lossy(&wm_res.stderr)
    );
    let html_text = fs::read_to_string(&html_out).unwrap();
    assert!(html_text.contains("leaflet"));
    assert!(html_text.contains("Uji WebMap"));
    assert!(html_text.contains("Tinggi"));
}

#[test]
fn test_install_pro_bridge_extracts_script_and_pyt() {
    let dir = tempdir().unwrap();
    let out_dir = dir.path().join("pro_bridge");

    let res = geokit_bin()
        .args([
            "live",
            "install-pro-bridge",
            "--out",
            out_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "install-pro-bridge stderr: {}",
        String::from_utf8_lossy(&res.stderr)
    );

    let py_path = out_dir.join("geokit_pro_bridge.py");
    let pyt_path = out_dir.join("GeoKit_Live_Bridge.pyt");
    assert!(py_path.is_file(), "geokit_pro_bridge.py missing");
    assert!(pyt_path.is_file(), "GeoKit_Live_Bridge.pyt missing");

    let py_src = fs::read_to_string(&py_path).unwrap();
    assert!(py_src.contains("def start_bridge("));
    assert!(py_src.contains("ArcGISProject"));

    let pyt_src = fs::read_to_string(&pyt_path).unwrap();
    assert!(pyt_src.contains("StartGeoKitBridge"));
    assert!(pyt_src.contains("StopGeoKitBridge"));
}

#[test]
fn test_daemon_status_json_offline() {
    let res = geokit_bin()
        .args(["daemon", "status", "--json"])
        .env("GEOKIT_ARCMAP_DAEMON_PORT", "19878")
        .env("GEOKIT_ARCPY_DAEMON_PORT", "19879")
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "daemon status stderr: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&res.stdout).expect("valid daemon status JSON");
    assert_eq!(parsed["arcmap"]["port"], 19878);
    assert_eq!(parsed["arcmap"]["running"], false);
    assert_eq!(parsed["arcgis"]["port"], 19879);
    assert_eq!(parsed["arcgis"]["running"], false);
}

#[test]
fn test_webmap_and_gp_summary_escape_xss_payloads() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let geojson_path = root.join("xss.geojson");
    let sample = r#"{
      "type": "FeatureCollection",
      "features": [
        {
          "type": "Feature",
          "properties": {
            "KELAS": "</script><img src=x onerror=alert(1)>",
            "NOTE": "<b>evil</b>",
            "AREA_HA": 42.5
          },
          "geometry": {
            "type": "Polygon",
            "coordinates": [[[110,-7],[111,-7],[111,-6],[110,-6],[110,-7]]]
          }
        }
      ]
    }"#;
    fs::write(&geojson_path, sample).unwrap();

    // 1. WebMap HTML escaping & script-tag breakout guard
    let wm_out = root.join("xss_map.html");
    let wm_res = geokit_bin()
        .args([
            "webmap",
            geojson_path.to_str().unwrap(),
            "--field",
            "KELAS",
            "--title",
            "Test </script><script>alert(2)</script>",
            "--out",
            wm_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        wm_res.status.success(),
        "webmap stderr: {}",
        String::from_utf8_lossy(&wm_res.stderr)
    );
    let wm_html = fs::read_to_string(&wm_out).unwrap();
    assert!(
        !wm_html.contains("</script><script>alert(2)"),
        "raw title script breakout must be escaped"
    );
    assert!(
        !wm_html.contains("</script><img src=x"),
        "raw JSON </script> breakout must be escaped"
    );
    assert!(wm_html.contains("<\\/script>"));
    assert!(wm_html.contains("function escHtml("));

    // 2. gp summary HTML escaping
    let sum_out = root.join("xss_summary.html");
    let sum_res = geokit_bin()
        .args([
            "gp",
            "summary",
            geojson_path.to_str().unwrap(),
            "--group-by",
            "KELAS",
            "--out",
            sum_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        sum_res.status.success(),
        "gp summary stderr: {}",
        String::from_utf8_lossy(&sum_res.stderr)
    );
    let sum_html = fs::read_to_string(&sum_out).unwrap();
    assert!(
        !sum_html.contains("<img src=x onerror=alert(1)>"),
        "raw HTML payload must be escaped in gp summary HTML"
    );
    assert!(sum_html.contains("&lt;img src=x onerror=alert(1)&gt;"));
}

#[test]
fn test_fetch_osm_safe_mode_refuses_overwrite_without_flag() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let existing = root.join("osm_road.geojson");
    fs::write(&existing, r#"{"sentinel":"keep-me"}"#).unwrap();

    let res = geokit_bin()
        .args([
            "fetch",
            "osm",
            "--bbox",
            "110.1,-7.05,110.2,-6.95",
            "--layers",
            "road",
            "--out",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !res.status.success(),
        "Safe Mode must refuse to overwrite existing osm_road.geojson without --overwrite"
    );
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(
        stderr.contains("Output already exists"),
        "unexpected stderr: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(&existing).unwrap(),
        r#"{"sentinel":"keep-me"}"#
    );
}

#[test]
fn test_clean_blocks_protected_root_and_workspace_confinement() {
    let root_path = if cfg!(windows) { r"C:\" } else { "/" };
    let res_root = geokit_bin().args(["clean", root_path]).output().unwrap();
    assert!(!res_root.status.success());
    let stderr_root = String::from_utf8_lossy(&res_root.stderr);
    assert!(
        stderr_root.contains("filesystem root"),
        "expected root block error, got: {stderr_root}"
    );

    let allowed = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let res_outside = geokit_bin()
        .args(["clean", outside.path().to_str().unwrap()])
        .env("GEOKIT_WORKSPACE_ROOT", allowed.path())
        .output()
        .unwrap();
    assert!(!res_outside.status.success());
    let stderr_out = String::from_utf8_lossy(&res_outside.stderr);
    assert!(
        stderr_out.contains("GEOKIT_WORKSPACE_ROOT"),
        "expected confinement error, got: {stderr_out}"
    );
}

#[test]
fn test_fetch_bbox_validation_and_dem_tile_cap() {
    let dir = tempdir().unwrap();
    let out_s = dir.path().to_str().unwrap();

    // Malformed token must fail (never silently skipped!)
    let bad_tok = geokit_bin()
        .args(["fetch", "osm", "--bbox", "110,bad,111,-6", "--out", out_s])
        .output()
        .unwrap();
    assert!(!bad_tok.status.success());

    // Inverted min/max must fail
    let inv = geokit_bin()
        .args(["fetch", "osm", "--bbox", "111,-6,110,-7", "--out", out_s])
        .output()
        .unwrap();
    assert!(!inv.status.success());

    // Out-of-WGS84-range must fail
    let oor = geokit_bin()
        .args(["fetch", "osm", "--bbox", "200,-7,201,-6", "--out", out_s])
        .output()
        .unwrap();
    assert!(!oor.status.success());

    // Huge DEM bbox exceeding --max-tiles must fail fast offline
    let huge = geokit_bin()
        .args([
            "fetch",
            "dem",
            "--bbox",
            "100,-10,120,5",
            "--max-tiles",
            "4",
            "--out",
            out_s,
        ])
        .output()
        .unwrap();
    assert!(!huge.status.success());
    let stderr = String::from_utf8_lossy(&huge.stderr);
    assert!(
        stderr.contains("exceeding --max-tiles"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn test_mcp_readonly_mode_blocks_mutating_and_allows_readonly() {
    use std::io::Write;
    use std::process::Stdio;

    let dir = tempdir().unwrap();
    let root_json = serde_json::to_string(dir.path().to_str().unwrap()).unwrap();

    let mut child = geokit_bin()
        .arg("mcp")
        .env("GEOKIT_MCP_READONLY", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"geokit_doctor","arguments":{{}}}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"geokit_clean","arguments":{{"path":{root_json}}}}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"geokit_clean","arguments":{{"path":{root_json},"dry_run":true}}}}}}"#
        )
        .unwrap();
    });

    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(output.status.success());

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 3);

    let r1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(r1["result"]["isError"], false);

    let r2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(r2["result"]["isError"], true);
    assert!(r2["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("GEOKIT_MCP_READONLY"));

    let r3: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(r3["result"]["isError"], false);
}

#[test]
fn test_doctor_json_reports_version_and_bridges() {
    let res = geokit_bin().args(["doctor", "--json"]).output().unwrap();
    assert!(res.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&res.stdout).expect("valid doctor JSON");
    assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));
    assert!(parsed["bridges"]["live_bridge_port"].as_u64().is_some());
    assert!(parsed["bridges"]["arcmap_daemon_port"].as_u64().is_some());
    assert!(parsed["bridges"]["arcgis_daemon_port"].as_u64().is_some());
}

#[test]
fn test_mcp_tools_schema_contract_integrity() {
    use std::collections::HashSet;
    use std::io::Write;
    use std::process::Stdio;

    let mut child = geokit_bin()
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{{}}}}"#
        )
        .unwrap();
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(output.status.success());

    let line = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    let tools = resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 20);

    let mut seen_names = HashSet::new();
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name");
        assert!(
            seen_names.insert(name.to_string()),
            "duplicate MCP tool: {name}"
        );
        assert!(
            !tool["description"].as_str().unwrap_or("").is_empty(),
            "empty description on {name}"
        );
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "invalid schema type on {name}");
        let props = schema["properties"].as_object().expect("properties object");
        if let Some(req) = schema.get("required").and_then(|v| v.as_array()) {
            for r in req {
                let rk = r.as_str().unwrap();
                assert!(
                    props.contains_key(rk),
                    "MCP tool {name} requires '{rk}' which is missing from properties"
                );
            }
        }
    }
}

#[test]
fn test_golden_gdal_project_safe_mode_and_roundtrip() {
    // Only run end-to-end GDAL round-trip when GDAL/ogr2ogr is available (e.g. CI Ubuntu & local QGIS)
    let doc = geokit_bin().args(["doctor", "--json"]).output().unwrap();
    assert!(doc.status.success());
    let doc_json: serde_json::Value = serde_json::from_slice(&doc.stdout).unwrap();
    if doc_json["gdal"]["ogr2ogr"].is_null() {
        return;
    }

    let dir = tempdir().unwrap();
    let root = dir.path();
    let src_gj = root.join("golden_square.geojson");
    let sample = r#"{
      "type": "FeatureCollection",
      "features": [
        {
          "type": "Feature",
          "properties": { "KELAS": "Sedang", "AREA_HA": 100.0 },
          "geometry": {
            "type": "Polygon",
            "coordinates": [[[110.20, -7.00], [110.21, -7.00], [110.21, -6.99], [110.20, -6.99], [110.20, -7.00]]]
          }
        }
      ]
    }"#;
    fs::write(&src_gj, sample).unwrap();

    // 1. Reproject WGS84 -> UTM 49S (EPSG:32749) into Shapefile via GDAL
    let utm_shp = root.join("golden_utm49s.shp");
    let proj_ok = geokit_bin()
        .args([
            "--engine",
            "gdal",
            "gp",
            "project",
            src_gj.to_str().unwrap(),
            "--epsg",
            "32749",
            "--out",
            utm_shp.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        proj_ok.status.success(),
        "GDAL project failed: {}",
        String::from_utf8_lossy(&proj_ok.stderr)
    );
    assert!(utm_shp.is_file());
    assert!(root.join("golden_utm49s.prj").is_file());

    // 2. Safe Mode: re-running WITHOUT --overwrite MUST fail cleanly
    let proj_dup = geokit_bin()
        .args([
            "--engine",
            "gdal",
            "gp",
            "project",
            src_gj.to_str().unwrap(),
            "--epsg",
            "32749",
            "--out",
            utm_shp.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !proj_dup.status.success(),
        "GDAL Safe Mode must refuse overwriting existing shapefile without --overwrite"
    );

    // 3. Re-running WITH --overwrite succeeds
    let proj_ow = geokit_bin()
        .args([
            "--overwrite",
            "--engine",
            "gdal",
            "gp",
            "project",
            src_gj.to_str().unwrap(),
            "--epsg",
            "32749",
            "--out",
            utm_shp.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        proj_ow.status.success(),
        "GDAL --overwrite failed: {}",
        String::from_utf8_lossy(&proj_ow.stderr)
    );

    // 4. Round-trip SHP -> GeoJSON via GDAL and verify native gp summary preserves attributes
    let rt_gj = root.join("roundtrip.geojson");
    let conv_ok = geokit_bin()
        .args([
            "--engine",
            "gdal",
            "gp",
            "convert",
            utm_shp.to_str().unwrap(),
            "--to",
            "geojson",
            "--out",
            rt_gj.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        conv_ok.status.success(),
        "GDAL convert failed: {}",
        String::from_utf8_lossy(&conv_ok.stderr)
    );
    let sum = geokit_bin()
        .args([
            "gp",
            "summary",
            rt_gj.to_str().unwrap(),
            "--group-by",
            "KELAS",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(sum.status.success());
    let sum_json: serde_json::Value = serde_json::from_slice(&sum.stdout).unwrap();
    assert_eq!(sum_json["total_features"], 1);
    assert_eq!(sum_json["total_area"], 100.0);
}

#[test]
fn test_webmap_klhk_23_class_preset_embedded() {
    let dir = tempdir().unwrap();
    let gj = dir.path().join("tutupan.geojson");
    fs::write(
        &gj,
        r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"TUTUPAN":"Hutan Lahan Kering Primer","AREA_HA":50.0},"geometry":{"type":"Polygon","coordinates":[[[110.1,-7.0],[110.2,-7.0],[110.2,-6.9],[110.1,-6.9],[110.1,-7.0]]]}}]}"#,
    )
    .unwrap();
    let html_out = dir.path().join("klhk_map.html");
    let res = geokit_bin()
        .args([
            "webmap",
            gj.to_str().unwrap(),
            "--field",
            "TUTUPAN",
            "--preset",
            "klhk",
            "--out",
            html_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(res.status.success());
    let html = fs::read_to_string(&html_out).unwrap();
    assert!(html.contains("KLHK_RULES"));
    assert!(html.contains("#00441B"));
    assert!(html.contains("hutan lahan kering primer"));
}

#[test]
fn test_gp_export_table_native_csv_and_safe_mode() {
    let dir = tempdir().unwrap();
    let gj = dir.path().join("desa.geojson");
    fs::write(
        &gj,
        r#"{
          "type": "FeatureCollection",
          "features": [
            {
              "type": "Feature",
              "properties": { "DESA": "Mekar, Jaya", "AREA_HA": 125.5 },
              "geometry": { "type": "Point", "coordinates": [110.2, -7.0] }
            },
            {
              "type": "Feature",
              "properties": { "DESA": "Sari \"Utara\"", "AREA_HA": 80.0 },
              "geometry": { "type": "Point", "coordinates": [110.3, -7.0] }
            }
          ]
        }"#,
    )
    .unwrap();

    let csv_out = dir.path().join("desa.csv");
    let res = geokit_bin()
        .args([
            "gp",
            "export-table",
            gj.to_str().unwrap(),
            "--out",
            csv_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "export-table stderr: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    let csv = fs::read_to_string(&csv_out).unwrap();
    assert!(csv.contains("\"Mekar, Jaya\""));
    assert!(csv.contains("\"Sari \"\"Utara\"\"\""));
    assert!(csv.contains("125.5"));

    // Safe Mode must refuse overwrite without --overwrite
    let dup = geokit_bin()
        .args([
            "gp",
            "export-table",
            gj.to_str().unwrap(),
            "--out",
            csv_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!dup.status.success());
}

#[test]
fn test_gdal_select_flatten_explode_and_split() {
    let doc = geokit_bin().args(["doctor", "--json"]).output().unwrap();
    let doc_json: serde_json::Value = serde_json::from_slice(&doc.stdout).unwrap();
    if doc_json["gdal"]["ogr2ogr"].is_null() {
        return;
    }

    let dir = tempdir().unwrap();
    let root = dir.path();
    let src_gj = root.join("wilayah.geojson");
    fs::write(
        &src_gj,
        r#"{
          "type": "FeatureCollection",
          "features": [
            {
              "type": "Feature",
              "properties": { "WILAYAH": "Kota A", "SKOR": 5 },
              "geometry": {
                "type": "MultiPolygon",
                "coordinates": [
                  [[[110.10, -7.00, 12.0], [110.12, -7.00, 12.0], [110.12, -6.98, 12.0], [110.10, -6.98, 12.0], [110.10, -7.00, 12.0]]],
                  [[[110.15, -7.00, 15.0], [110.17, -7.00, 15.0], [110.17, -6.98, 15.0], [110.15, -6.98, 15.0], [110.15, -7.00, 15.0]]]
                ]
              }
            },
            {
              "type": "Feature",
              "properties": { "WILAYAH": "Kota B", "SKOR": 2 },
              "geometry": {
                "type": "Polygon",
                "coordinates": [[[110.20, -7.00, 5.0], [110.22, -7.00, 5.0], [110.22, -6.98, 5.0], [110.20, -6.98, 5.0], [110.20, -7.00, 5.0]]]
              }
            }
          ]
        }"#,
    )
    .unwrap();

    // 1. Select by SQL WHERE
    let sel_out = root.join("kota_a_only.geojson");
    let r_sel = geokit_bin()
        .args([
            "--engine",
            "gdal",
            "gp",
            "select",
            src_gj.to_str().unwrap(),
            "--where",
            "SKOR >= 4",
            "--out",
            sel_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        r_sel.status.success(),
        "select stderr: {}",
        String::from_utf8_lossy(&r_sel.stderr)
    );
    let sel_val: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&sel_out).unwrap()).unwrap();
    assert_eq!(sel_val["features"].as_array().unwrap().len(), 1);

    // 2. Flatten 2D (Drop Z)
    let flat_out = root.join("flat_2d.geojson");
    let r_flat = geokit_bin()
        .args([
            "--engine",
            "gdal",
            "gp",
            "flatten-2d",
            src_gj.to_str().unwrap(),
            "--out",
            flat_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        r_flat.status.success(),
        "flatten-2d stderr: {}",
        String::from_utf8_lossy(&r_flat.stderr)
    );
    let flat_val: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&flat_out).unwrap()).unwrap();
    let first_pt = &flat_val["features"][0]["geometry"]["coordinates"][0][0][0];
    assert_eq!(
        first_pt.as_array().unwrap().len(),
        2,
        "Z coordinate must be stripped"
    );

    // 3. Explode MultiPolygon -> Singlepart (2 original features -> 3 singlepart polygons!)
    let exp_out = root.join("exploded.geojson");
    let r_exp = geokit_bin()
        .args([
            "--engine",
            "gdal",
            "gp",
            "explode",
            flat_out.to_str().unwrap(),
            "--out",
            exp_out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        r_exp.status.success(),
        "explode stderr: {}",
        String::from_utf8_lossy(&r_exp.stderr)
    );
    let exp_val: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&exp_out).unwrap()).unwrap();
    assert_eq!(exp_val["features"].as_array().unwrap().len(), 3);

    // 4. Split by WILAYAH -> kota_a.shp + kota_b.shp
    let split_dir = root.join("by_wilayah");
    let r_split = geokit_bin()
        .args([
            "--engine",
            "gdal",
            "gp",
            "split",
            flat_out.to_str().unwrap(),
            "--field",
            "WILAYAH",
            "--out",
            split_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        r_split.status.success(),
        "split stderr: {}",
        String::from_utf8_lossy(&r_split.stderr)
    );
    assert!(split_dir.join("kota_a.shp").is_file());
    assert!(split_dir.join("kota_b.shp").is_file());
}

#[test]
fn test_gdal_xy2shp_dms_and_gpx_convert() {
    let doc = geokit_bin().args(["doctor", "--json"]).output().unwrap();
    let doc_json: serde_json::Value = serde_json::from_slice(&doc.stdout).unwrap();
    if doc_json["gdal"]["ogr2ogr"].is_null() {
        return;
    }

    let dir = tempdir().unwrap();
    let root = dir.path();

    // 1. CSV with DMS coordinates (English + Indonesian LS/BT) -> GeoJSON via xy2shp
    let csv_path = root.join("stasiun_dms.csv");
    fs::write(
        &csv_path,
        "NAMA,LON,LAT,CH_MM\n\
         Stasiun A,\"110°24'30\"\"E\",\"7°00'12\"\"S\",2450\n\
         Stasiun B,\"110° 25' 12\"\" BT\",\"6° 58' 30\"\" LS\",2100\n",
    )
    .unwrap();

    let pts_gj = root.join("stasiun.geojson");
    let r_xy = geokit_bin()
        .args([
            "--engine",
            "gdal",
            "gp",
            "xy2shp",
            csv_path.to_str().unwrap(),
            "--x-field",
            "LON",
            "--y-field",
            "LAT",
            "--epsg",
            "4326",
            "--out",
            pts_gj.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        r_xy.status.success(),
        "xy2shp DMS stderr: {}",
        String::from_utf8_lossy(&r_xy.stderr)
    );
    let gj_val: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pts_gj).unwrap()).unwrap();
    let feats = gj_val["features"].as_array().unwrap();
    assert_eq!(feats.len(), 2);
    let c0 = feats[0]["geometry"]["coordinates"].as_array().unwrap();
    assert!((c0[0].as_f64().unwrap() - 110.4083333).abs() < 1e-4);
    assert!((c0[1].as_f64().unwrap() - (-7.0033333)).abs() < 1e-4);
    let c1 = feats[1]["geometry"]["coordinates"].as_array().unwrap();
    assert!((c1[0].as_f64().unwrap() - 110.42).abs() < 1e-4);
    assert!((c1[1].as_f64().unwrap() - (-6.975)).abs() < 1e-4);

    // 2. GPX waypoints -> GeoJSON via `gp convert`
    let gpx_path = root.join("survey_gps.gpx");
    fs::write(
        &gpx_path,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="GeoKitTest" xmlns="http://www.topografix.com/GPX/1/1">
  <wpt lat="-7.0123" lon="110.4150"><name>BM-01</name></wpt>
  <wpt lat="-7.0250" lon="110.4320"><name>BM-02</name></wpt>
</gpx>"#,
    )
    .unwrap();

    let gpx_gj = root.join("survey_gps.geojson");
    let r_gpx = geokit_bin()
        .args([
            "--engine",
            "gdal",
            "gp",
            "convert",
            gpx_path.to_str().unwrap(),
            "--to",
            "geojson",
            "--out",
            gpx_gj.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        r_gpx.status.success(),
        "gpx convert stderr: {}",
        String::from_utf8_lossy(&r_gpx.stderr)
    );
    let gpx_val: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&gpx_gj).unwrap()).unwrap();
    assert_eq!(gpx_val["features"].as_array().unwrap().len(), 2);
}
