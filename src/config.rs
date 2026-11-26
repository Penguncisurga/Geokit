use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::gis::GisCategory;

pub const CONFIG_FILE_NAME: &str = "geokit.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeokitConfig {
    #[serde(default)]
    pub arcgis: ArcGisConfig,
    #[serde(default)]
    pub defaults: DefaultsConfig,
    #[serde(default)]
    pub folders: FolderConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArcGisConfig {
    pub propy: Option<PathBuf>,
    pub arcmap_python: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    pub engine: Option<String>,
    pub epsg: Option<u32>,
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    #[serde(default = "default_format")]
    pub layout_format: String,
}

fn default_dpi() -> u32 {
    300
}

fn default_format() -> String {
    "pdf".to_string()
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            engine: None,
            epsg: None,
            dpi: default_dpi(),
            layout_format: default_format(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderConfig {
    #[serde(default = "default_vector")]
    pub vector: String,
    #[serde(default = "default_raster")]
    pub raster: String,
    #[serde(default = "default_geodatabase")]
    pub geodatabase: String,
    #[serde(default = "default_projects")]
    pub projects: String,
    #[serde(default = "default_tabular")]
    pub tabular: String,
}

fn default_vector() -> String {
    "01_Vector".to_string()
}
fn default_raster() -> String {
    "02_Raster".to_string()
}
fn default_geodatabase() -> String {
    "03_Geodatabase".to_string()
}
fn default_projects() -> String {
    "04_Projects".to_string()
}
fn default_tabular() -> String {
    "05_Tabular".to_string()
}

impl Default for FolderConfig {
    fn default() -> Self {
        Self {
            vector: default_vector(),
            raster: default_raster(),
            geodatabase: default_geodatabase(),
            projects: default_projects(),
            tabular: default_tabular(),
        }
    }
}

impl FolderConfig {
    pub fn name_for(&self, category: GisCategory) -> &str {
        match category {
            GisCategory::Vector => &self.vector,
            GisCategory::Raster => &self.raster,
            GisCategory::Geodatabase => &self.geodatabase,
            GisCategory::Project => &self.projects,
            GisCategory::Tabular => &self.tabular,
        }
    }
}

/// Loads `geokit.toml` from `dir` (or its ancestors) if present; otherwise returns defaults.
pub fn load_config(start_dir: &Path) -> Result<(GeokitConfig, Option<PathBuf>)> {
    let dir = if start_dir.is_file() {
        start_dir.parent().unwrap_or(start_dir)
    } else {
        start_dir
    };

    for ancestor in dir.ancestors() {
        let candidate = ancestor.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            let content = fs::read_to_string(&candidate)
                .with_context(|| format!("Failed to read {}", candidate.display()))?;
            let cfg: GeokitConfig = toml::from_str(&content)
                .with_context(|| format!("Invalid TOML in {}", candidate.display()))?;
            return Ok((cfg, Some(candidate)));
        }
    }

    Ok((GeokitConfig::default(), None))
}

pub fn starter_config_toml() -> &'static str {
    r#"# geokit project configuration
# Place this file in your GIS project root.

[arcgis]
# Optional: override auto-detected ArcGIS Pro propy.bat or ArcMap 10.x python.exe path
# propy = "C:\\Program Files\\ArcGIS\\Pro\\bin\\Python\\Scripts\\propy.bat"
# arcmap_python = "C:\\Python27\\ArcGIS10.7\\python.exe"

[defaults]
# Optional: default engine backend ("auto", "arcgis", "arcmap", or "gdal")
# engine = "auto"
# Optional: default EPSG used by `geokit gp project` when --epsg is omitted
# Example: 32748 (UTM 48S), 32749 (UTM 49S), 4326 (WGS 84)
# epsg = 32748
dpi = 300
layout_format = "pdf"

[folders]
# Folder names used by `geokit organize`
vector = "01_Vector"
raster = "02_Raster"
geodatabase = "03_Geodatabase"
projects = "04_Projects"
tabular = "05_Tabular"
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_starter_config() {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join(CONFIG_FILE_NAME);
        fs::write(&cfg_path, starter_config_toml()).unwrap();

        let (cfg, found) = load_config(dir.path()).unwrap();
        assert_eq!(found, Some(cfg_path));
        assert_eq!(cfg.defaults.dpi, 300);
        assert_eq!(cfg.defaults.layout_format, "pdf");
        assert_eq!(cfg.folders.vector, "01_Vector");
    }
}
