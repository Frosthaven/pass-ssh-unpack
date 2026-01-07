use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default rclone password path in Proton Pass (fallback when not configured)
pub const DEFAULT_RCLONE_PASSWORD_PATH: &str = "pass://Personal/rclone/password";

/// When to sync public keys back to Proton Pass
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SyncPublicKey {
    /// Never sync public keys
    Never,
    /// Only sync if the public key field is empty (default)
    #[default]
    IfEmpty,
    /// Always overwrite the public key
    Always,
}

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

# When to sync generated public keys back to Proton Pass
# Options: "never", "if_empty" (default), "always"
#   never    - Never update public keys in Proton Pass
#   if_empty - Only update if the public key field is empty (default)
#   always   - Always overwrite the public key in Proton Pass
sync_public_key = "if_empty"

[rclone]
# Enable rclone SFTP remote sync
# Default: true
enabled = true

# Path in Proton Pass to rclone config password (if encrypted)
# This is optional if RCLONE_CONFIG_PASS is already set in your environment.
# If both are set, this value takes precedence.
# Leave empty to rely on environment variable or unencrypted config.
# Example: "pass://Personal/rclone/password"
# Default: ""
password_path = ""

# Always ensure rclone config is encrypted after operations
# If true and a password is available (via password_path or RCLONE_CONFIG_PASS),
# the rclone config will be re-encrypted even if it wasn't encrypted before.
# Default: false
always_encrypt = false
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
    pub sync_public_key: SyncPublicKey,

    #[serde(default)]
    pub rclone: RcloneConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RcloneConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_rclone_password_path")]
    pub password_path: String,

    #[serde(default)]
    pub always_encrypt: bool,
}

fn default_ssh_output_dir() -> String {
    "~/.ssh/proton-pass".to_string()
}

fn default_true() -> bool {
    true
}

fn default_rclone_password_path() -> String {
    DEFAULT_RCLONE_PASSWORD_PATH.to_string()
}

impl Default for RcloneConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            password_path: default_rclone_password_path(),
            always_encrypt: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ssh_output_dir: default_ssh_output_dir(),
            default_vaults: Vec::new(),
            default_items: Vec::new(),
            sync_public_key: SyncPublicKey::default(),
            rclone: RcloneConfig::default(),
        }
    }
}

impl Config {
    /// Get the default config file path
    /// Always uses ~/.config for consistency across platforms
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".config")
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

/// Known top-level config keys (for detecting missing options)
const KNOWN_KEYS: &[&str] = &[
    "ssh_output_dir",
    "default_vaults",
    "default_items",
    "sync_public_key",
    "rclone",
];

/// Known rclone section keys
const KNOWN_RCLONE_KEYS: &[&str] = &["enabled", "password_path", "always_encrypt"];

/// Check for missing config options and return a list of missing keys
pub fn check_missing_options(path: &std::path::Path) -> Vec<String> {
    let mut missing = Vec::new();

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return missing, // Can't read file, skip check
    };

    let table: toml::Table = match content.parse() {
        Ok(t) => t,
        Err(_) => return missing, // Can't parse, skip check
    };

    // Check top-level keys
    for key in KNOWN_KEYS {
        if !table.contains_key(*key) {
            missing.push(key.to_string());
        }
    }

    // Check rclone section keys
    if let Some(toml::Value::Table(rclone)) = table.get("rclone") {
        for key in KNOWN_RCLONE_KEYS {
            if !rclone.contains_key(*key) {
                missing.push(format!("rclone.{}", key));
            }
        }
    }

    missing
}

/// Expand ~ to home directory
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}
