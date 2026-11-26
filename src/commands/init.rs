use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::config::{starter_config_toml, CONFIG_FILE_NAME};

pub fn run(path: &Path, force: bool) -> Result<()> {
    if !path.is_dir() {
        bail!("Target directory does not exist: {}", path.display());
    }

    let target = path.join(CONFIG_FILE_NAME);
    if target.exists() && !force {
        bail!(
            "{} already exists. Use `--force` to overwrite.",
            target.display()
        );
    }

    fs::write(&target, starter_config_toml())
        .with_context(|| format!("Failed to write {}", target.display()))?;

    println!("{} Created {}", "[OK]".green().bold(), target.display());
    println!("Edit this file to customize folder names, default EPSG, DPI, or propy.bat path.");
    Ok(())
}
