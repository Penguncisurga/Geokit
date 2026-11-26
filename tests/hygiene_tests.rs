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

    {
        let stdin = child.stdin.as_mut().unwrap();
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
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 3);

    let init_resp: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "geokit");

    let list_resp: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    let tools = list_resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 10);

    let call_resp: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(call_resp["result"]["isError"], false);
}
