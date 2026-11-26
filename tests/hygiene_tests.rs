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
