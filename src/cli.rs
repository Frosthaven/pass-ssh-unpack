use clap::Parser;
use std::path::PathBuf;

use crate::config::SyncPublicKey;

/// Extract SSH keys from Proton Pass to local files and generate SSH config
#[derive(Parser, Debug)]
#[command(name = "pass-ssh-unpack")]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Vault(s) to process (repeatable, supports wildcards)
    #[arg(short, long, action = clap::ArgAction::Append)]
    pub vault: Vec<String>,

    /// Item title pattern(s) to unpack (repeatable, supports wildcards)
    #[arg(short, long, action = clap::ArgAction::Append)]
    pub item: Vec<String>,

    /// Full regeneration (clear config first)
    #[arg(short, long)]
    pub full: bool,

    /// Suppress output
    #[arg(short, long)]
    pub quiet: bool,

    /// Only process SSH keys (skip rclone sync)
    #[arg(long, conflicts_with = "rclone")]
    pub ssh: bool,

    /// Only process rclone remotes (skip SSH key extraction)
    #[arg(long, conflicts_with = "ssh")]
    pub rclone: bool,

    /// Remove all managed SSH keys and rclone remotes, then exit
    #[arg(long)]
    pub purge: bool,

    /// Show what would be done without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Custom config file path
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Override SSH output directory (default: ~/.ssh/proton-pass)
    #[arg(short, long)]
    pub output_dir: Option<PathBuf>,

    /// Override when to sync public keys back to Proton Pass
    #[arg(long, value_enum)]
    pub sync_public_key: Option<SyncPublicKey>,

    /// Override path in Proton Pass to rclone config password
    #[arg(long)]
    pub rclone_password_path: Option<String>,

    /// Force rclone config encryption after operations
    #[arg(long)]
    pub always_encrypt: bool,
}
