use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_db_path")]
    pub db_path: String,

    pub claude_api_key: Option<String>,
    pub raindrop_token: Option<String>,

    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_minutes: u32,

    #[serde(default)]
    pub default_tags: Vec<String>,
}

fn default_db_path() -> String {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rss-reader");
    std::fs::create_dir_all(&data_dir).ok();
    data_dir.join("feeds.db").to_string_lossy().to_string()
}

fn default_refresh_interval() -> u32 {
    30
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            claude_api_key: None,
            raindrop_token: None,
            refresh_interval_minutes: default_refresh_interval(),
            default_tags: vec!["rss".to_string()],
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| AppError::Config(e.to_string()))?;
        std::fs::write(config_path, content)?;
        Ok(())
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rss-reader")
            .join("config.toml")
    }
}
