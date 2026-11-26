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

    // 4. Clean lock files
    let clean_out = geokit_bin()
        .args(["clean", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(clean_out.status.success());
    assert!(!root
        .join("03_Geodatabase")
        .join("data_proyek.gdb")
        .join("a00000001.lock")
        .exists());
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
    assert_eq!(tools.len(), 18);

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
