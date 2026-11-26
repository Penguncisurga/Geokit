use anyhow::{bail, Result};
use serde::Serialize;
use std::path::Path;

use crate::config::GeokitConfig;
use crate::engine::{
    abs_string, resolve_backend, run_arcmap_job, run_arcpy_job, EngineBackend, ResolvedBackend,
};

#[derive(Debug)]
pub struct QaOptions {
    pub expected_epsg: Option<u32>,
    pub check_nulls: Option<String>,
    pub check_overlaps: bool,
    pub json: bool,
}

#[derive(Debug, Serialize)]
struct QaPayload {
    action: String,
    input: String,
    expected_epsg: Option<u32>,
    check_nulls: Option<String>,
    check_overlaps: bool,
    json: bool,
}

pub fn run(
    input: &Path,
    opts: QaOptions,
    override_propy: Option<&Path>,
    cfg: &GeokitConfig,
    backend: EngineBackend,
) -> Result<()> {
    if !input.exists() {
        bail!("Input path does not exist: {}", input.display());
    }

    let resolved = resolve_backend(backend, override_propy, cfg.arcgis.arcmap_python.as_deref());
    let target_epsg = opts.expected_epsg.or(cfg.defaults.epsg);

    let payload = QaPayload {
        action: "qa_check".into(),
        input: abs_string(input),
        expected_epsg: target_epsg,
        check_nulls: opts.check_nulls,
        check_overlaps: opts.check_overlaps,
        json: opts.json,
    };

    match resolved {
        ResolvedBackend::Arcmap => run_arcmap_job(cfg.arcgis.arcmap_python.as_deref(), &payload),
        ResolvedBackend::Arcgis => run_arcpy_job(override_propy, &payload),
        ResolvedBackend::Gdal => bail!(
            "`geokit qa` requires ArcGIS Pro or ArcMap 10.x (`--engine arcgis` or `--engine arcmap`)."
        ),
    }
}
