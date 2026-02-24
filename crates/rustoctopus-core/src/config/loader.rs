//! Configuration file loading and saving utilities.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::schema::Config;

/// Returns the default config file path: `~/.rustoctopus/config.json`.
pub fn default_config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".rustoctopus").join("config.json")
}

/// Loads configuration from a JSON file.
///
/// If `path` is `Some`, loads from that path. Otherwise loads from
/// [`default_config_path`]. If the file does not exist, returns a
/// default [`Config`].
pub fn load_config(path: Option<&Path>) -> Result<Config> {
    let config_path = match path {
        Some(p) => p.to_path_buf(),
        None => default_config_path(),
    };

    if !config_path.exists() {
        return Ok(Config::default());
    }

    let contents = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let config: Config = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;

    Ok(config)
}

/// Saves configuration to a JSON file with pretty formatting.
///
/// If `path` is `Some`, saves to that path. Otherwise saves to
/// [`default_config_path`]. Parent directories are created if needed.
pub fn save_config(config: &Config, path: Option<&Path>) -> Result<()> {
    let config_path = match path {
        Some(p) => p.to_path_buf(),
        None => default_config_path(),
    };

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(config)
        .context("Failed to serialize config to JSON")?;

    std::fs::write(&config_path, json)
        .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

    Ok(())
}
