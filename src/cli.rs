use clap::Parser;
use std::path::PathBuf;

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

    /// Skip rclone remote sync
    #[arg(long)]
    pub no_rclone: bool,

    /// Remove all managed SSH keys and rclone remotes, then exit
    #[arg(long)]
    pub purge: bool,

    /// Show what would be done without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Custom config file path
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}
