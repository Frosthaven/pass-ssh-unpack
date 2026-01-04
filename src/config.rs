use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default configuration file content with comments
const DEFAULT_CONFIG: &str = r#"# pass-ssh-unpack configuration file
# This file is auto-generated on first run. All fields are optional.

# Directory where SSH keys and config are written
# Supports ~ for home directory
# Default: ~/.ssh/proton-pass
ssh_output_dir = "~/.ssh/proton-pass"

# Default vault filter(s) - applied when no --vault flag is given
# Supports wildcards: "Personal", "Work*", etc.
# Default: [] (all vaults)
default_vaults = []

# Default item filter(s) - applied when no --item flag is given
# Supports wildcards: "github/*", "*-prod", etc.
# Default: [] (all items)
default_items = []

[rclone]
# Enable rclone SFTP remote sync
# Default: true
enabled = true

# Path in Proton Pass to rclone config password (if encrypted)
# This is optional if RCLONE_CONFIG_PASS is already set in your environment.
# If both are set, this value takes precedence.
# Leave empty to rely on environment variable or unencrypted config.
# Example: "Personal/rclone-password" (vault/item format)
# Default: ""
password_path = ""
"#;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_ssh_output_dir")]
    pub ssh_output_dir: String,

    #[serde(default)]
    pub default_vaults: Vec<String>,

    #[serde(default)]
    pub default_items: Vec<String>,

    #[serde(default)]
    pub rclone: RcloneConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RcloneConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub password_path: String,
}

fn default_ssh_output_dir() -> String {
    "~/.ssh/proton-pass".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for RcloneConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            password_path: String::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ssh_output_dir: default_ssh_output_dir(),
            default_vaults: Vec::new(),
            default_items: Vec::new(),
            rclone: RcloneConfig::default(),
        }
    }
}

impl Config {
    /// Get the default config file path
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("pass-ssh-unpack")
            .join("config.toml")
    }

    /// Load config from file, or create default if it doesn't exist
    pub fn load_or_create(custom_path: &Option<PathBuf>) -> Result<Self> {
        let path = custom_path.clone().unwrap_or_else(Self::default_path);

        if path.exists() {
            Self::load(&path)
        } else {
            Self::create_default(&path)?;
            Ok(Self::default())
        }
    }

    /// Load config from a file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }

    /// Create default config file
    fn create_default(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        std::fs::write(path, DEFAULT_CONFIG)
            .with_context(|| format!("Failed to write default config: {}", path.display()))?;

        Ok(())
    }

    /// Expand ~ in ssh_output_dir to actual home directory
    pub fn expanded_ssh_output_dir(&self) -> PathBuf {
        expand_tilde(&self.ssh_output_dir)
    }
}

/// Expand ~ to home directory
pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}
