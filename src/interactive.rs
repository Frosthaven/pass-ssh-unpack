use anyhow::Result;
use inquire::{Confirm, MultiSelect, Select, Text};
use std::io::IsTerminal;

use crate::config::{Config, DEFAULT_RCLONE_PASSWORD_PATH};
use crate::progress;
use crate::proton_pass::ProtonPass;
use crate::teleport::Teleport;

/// Result of interactive mode - what action to take
pub enum InteractiveAction {
    /// Import from Teleport
    ImportTeleport {
        vault: String,
        item_pattern: Option<String>,
        scan_remotes: bool,
        dry_run: bool,
    },
    /// Export to local machine
    ExportLocal {
        mode: ExportMode,
        vaults: Vec<String>,
        item_pattern: Option<String>,
        full: bool,
        dry_run: bool,
    },
    /// Purge managed resources
    Purge { mode: PurgeMode, dry_run: bool },
    /// View status was shown, return to menu
    ViewedStatus,
    /// User cancelled or quit
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExportMode {
    SshOnly,
    RcloneOnly,
    Both,
}

impl std::fmt::Display for ExportMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportMode::SshOnly => write!(f, "SSH config only"),
            ExportMode::RcloneOnly => write!(f, "rclone remotes only"),
            ExportMode::Both => write!(f, "Both SSH and rclone"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PurgeMode {
    SshOnly,
    RcloneOnly,
    Both,
}

impl std::fmt::Display for PurgeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PurgeMode::SshOnly => write!(f, "SSH keys only"),
            PurgeMode::RcloneOnly => write!(f, "rclone remotes only"),
            PurgeMode::Both => write!(f, "Both SSH keys and rclone remotes"),
        }
    }
}

/// Check if we're running in an interactive terminal
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Run interactive mode and return the chosen action
pub fn run_interactive() -> Result<InteractiveAction> {
    println!();
    println!("  pass-ssh-unpack");
    println!("  ───────────────");
    println!();

    // Main action selection
    let actions = vec![
        "Export Proton Pass SSH to local machine",
        "Import Teleport nodes into Proton Pass",
        "View status",
        "Purge managed resources",
        "Quit",
    ];

    let action = match Select::new("What would you like to do?", actions).prompt() {
        Ok(choice) => choice,
        Err(
            inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted,
        ) => {
            return Ok(InteractiveAction::Cancelled);
        }
        Err(e) => return Err(e.into()),
    };

    match action {
        "Export Proton Pass SSH to local machine" => run_export_local(),
        "Import Teleport nodes into Proton Pass" => run_teleport_import(),
        "View status" => run_view_status(),
        "Purge managed resources" => run_purge(),
        "Quit" => Ok(InteractiveAction::Cancelled),
        _ => Ok(InteractiveAction::Cancelled),
    }
}

fn run_teleport_import() -> Result<InteractiveAction> {
    println!();

    // Check if tsh is installed
    if which::which("tsh").is_err() {
        println!("tsh not found. Install Teleport CLI first.");
        return Ok(InteractiveAction::Cancelled);
    }

    // Check if logged into Teleport
    let spinner = progress::spinner("Checking Teleport login...");
    let teleport = Teleport::new();
    let status = teleport.get_status();
    spinner.finish_and_clear();

    if status.is_err() {
        println!("Not logged into Teleport. Run 'tsh login' first.");
        return Ok(InteractiveAction::Cancelled);
    }

    // Fetch available vaults
    let proton_pass = ProtonPass::new();
    let available_vaults = proton_pass.list_vaults().unwrap_or_default();

    // Ask for vault selection
    let vault = if available_vaults.is_empty() {
        // Fall back to text input if no vaults found
        match Text::new("Vault name to import into:")
            .with_help_message("Could not fetch vaults. Items will be created in this vault.")
            .prompt()
        {
            Ok(v) if v.trim().is_empty() => {
                println!("Vault name is required.");
                return Ok(InteractiveAction::Cancelled);
            }
            Ok(v) => v.trim().to_string(),
            Err(
                inquire::InquireError::OperationCanceled
                | inquire::InquireError::OperationInterrupted,
            ) => {
                return Ok(InteractiveAction::Cancelled);
            }
            Err(e) => return Err(e.into()),
        }
    } else {
        // Build options: existing vaults + "Create new vault..."
        const CREATE_NEW: &str = "+ Create new vault...";
        let mut options: Vec<&str> = available_vaults.iter().map(|s| s.as_str()).collect();
        options.push(CREATE_NEW);

        let selection = match Select::new("Select vault to import into:", options)
            .with_help_message("Select an existing vault or create a new one.")
            .prompt()
        {
            Ok(s) => s,
            Err(
                inquire::InquireError::OperationCanceled
                | inquire::InquireError::OperationInterrupted,
            ) => {
                return Ok(InteractiveAction::Cancelled);
            }
            Err(e) => return Err(e.into()),
        };

        if selection == CREATE_NEW {
            // Ask for new vault name
            match Text::new("New vault name:")
                .with_help_message("This vault will be created if it doesn't exist.")
                .prompt()
            {
                Ok(v) if v.trim().is_empty() => {
                    println!("Vault name is required.");
                    return Ok(InteractiveAction::Cancelled);
                }
                Ok(v) => v.trim().to_string(),
                Err(
                    inquire::InquireError::OperationCanceled
                    | inquire::InquireError::OperationInterrupted,
                ) => {
                    return Ok(InteractiveAction::Cancelled);
                }
                Err(e) => return Err(e.into()),
            }
        } else {
            selection.to_string()
        }
    };

    // Ask for item pattern filter
    let item_pattern = match Text::new("Node filter pattern (optional):")
        .with_help_message("Supports wildcards: 'prod-*', '*-server', etc. Leave empty for all.")
        .prompt()
    {
        Ok(p) if p.trim().is_empty() => None,
        Ok(p) => Some(p.trim().to_string()),
        Err(
            inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted,
        ) => {
            return Ok(InteractiveAction::Cancelled);
        }
        Err(e) => return Err(e.into()),
    };

    // Ask about scanning
    let scan_remotes = match Confirm::new("Scan each server to detect sftp-server path?")
        .with_default(true)
        .with_help_message("Slower but more accurate. Skip to use default path.")
        .prompt()
    {
        Ok(v) => v,
        Err(
            inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted,
        ) => {
            return Ok(InteractiveAction::Cancelled);
        }
        Err(e) => return Err(e.into()),
    };

    // Ask about dry run
    let dry_run = ask_dry_run()?;
    if dry_run.is_none() {
        return Ok(InteractiveAction::Cancelled);
    }
    let dry_run = dry_run.unwrap();

    // Build and show confirmation summary
    let nodes_str = item_pattern.as_deref().unwrap_or("all nodes");
    let scan_str = if scan_remotes { "Yes" } else { "No" };
    let dry_run_str = if dry_run { "Yes" } else { "No" };

    let summary = [
        "Action:  Import Teleport nodes".to_string(),
        format!("Vault:   {}", vault),
        format!("Nodes:   {}", nodes_str),
        format!("Scan:    {}", scan_str),
        format!("Dry run: {}", dry_run_str),
    ];
    let summary_refs: Vec<&str> = summary.iter().map(|s| s.as_str()).collect();

    if !confirm_summary(&summary_refs)? {
        return Ok(InteractiveAction::Cancelled);
    }

    Ok(InteractiveAction::ImportTeleport {
        vault,
        item_pattern,
        scan_remotes,
        dry_run,
    })
}

fn run_export_local() -> Result<InteractiveAction> {
    println!();

    // Ask what to export
    let modes = vec![
        ExportMode::Both,
        ExportMode::SshOnly,
        ExportMode::RcloneOnly,
    ];

    let mode = match Select::new("What to generate?", modes).prompt() {
        Ok(m) => m,
        Err(
            inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted,
        ) => {
            return Ok(InteractiveAction::Cancelled);
        }
        Err(e) => return Err(e.into()),
    };

    // Fetch available vaults
    let proton_pass = ProtonPass::new();
    let available_vaults = proton_pass.list_vaults().unwrap_or_default();

    // Ask for vault selection (multi-select if vaults available, fall back to text)
    let vaults = if available_vaults.is_empty() {
        // Fall back to text input if no vaults found
        match Text::new("Vault filter pattern (optional):")
            .with_help_message(
                "Could not fetch vaults. Supports wildcards: 'Personal', 'Work*', etc.",
            )
            .prompt()
        {
            Ok(p) if p.trim().is_empty() => vec![],
            Ok(p) => vec![p.trim().to_string()],
            Err(
                inquire::InquireError::OperationCanceled
                | inquire::InquireError::OperationInterrupted,
            ) => {
                return Ok(InteractiveAction::Cancelled);
            }
            Err(e) => return Err(e.into()),
        }
    } else {
        match MultiSelect::new("Select vaults to export from:", available_vaults)
            .with_help_message("Space to select, Enter to confirm. Leave empty for all vaults.")
            .prompt()
        {
            Ok(v) => v,
            Err(
                inquire::InquireError::OperationCanceled
                | inquire::InquireError::OperationInterrupted,
            ) => {
                return Ok(InteractiveAction::Cancelled);
            }
            Err(e) => return Err(e.into()),
        }
    };

    // Ask for item pattern
    let item_pattern = match Text::new("Item filter pattern (optional):")
        .with_help_message("Supports wildcards: 'github/*', '*-prod', etc. Leave empty for all.")
        .prompt()
    {
        Ok(p) if p.trim().is_empty() => None,
        Ok(p) => Some(p.trim().to_string()),
        Err(
            inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted,
        ) => {
            return Ok(InteractiveAction::Cancelled);
        }
        Err(e) => return Err(e.into()),
    };

    // Ask about full regeneration
    let full = match Confirm::new("Full regeneration? (clear existing config first)")
        .with_default(false)
        .with_help_message("Use this to remove stale entries from previous runs.")
        .prompt()
    {
        Ok(v) => v,
        Err(
            inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted,
        ) => {
            return Ok(InteractiveAction::Cancelled);
        }
        Err(e) => return Err(e.into()),
    };

    // Ask about dry run
    let dry_run = ask_dry_run()?;
    if dry_run.is_none() {
        return Ok(InteractiveAction::Cancelled);
    }
    let dry_run = dry_run.unwrap();

    // Build and show confirmation summary
    let vaults_str = if vaults.is_empty() {
        "all vaults".to_string()
    } else {
        vaults.join(", ")
    };
    let items_str = item_pattern.as_deref().unwrap_or("all items");
    let mode_str = format!("{}", mode);
    let full_str = if full { "Yes" } else { "No" };
    let dry_run_str = if dry_run { "Yes" } else { "No" };

    let summary = [
        "Action:     Export to local machine".to_string(),
        format!("Vaults:     {}", vaults_str),
        format!("Items:      {}", items_str),
        format!("Mode:       {}", mode_str),
        format!("Full regen: {}", full_str),
        format!("Dry run:    {}", dry_run_str),
    ];
    let summary_refs: Vec<&str> = summary.iter().map(|s| s.as_str()).collect();

    if !confirm_summary(&summary_refs)? {
        return Ok(InteractiveAction::Cancelled);
    }

    Ok(InteractiveAction::ExportLocal {
        mode,
        vaults,
        item_pattern,
        full,
        dry_run,
    })
}

fn ask_dry_run() -> Result<Option<bool>> {
    println!();
    match Confirm::new("Dry run? (preview changes without applying)")
        .with_default(false)
        .prompt()
    {
        Ok(v) => Ok(Some(v)),
        Err(
            inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted,
        ) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Show a summary and ask for final confirmation
fn confirm_summary(lines: &[&str]) -> Result<bool> {
    println!();
    println!("  Summary");
    println!("  ───────");
    for line in lines {
        println!("  {}", line);
    }
    println!();

    match Confirm::new("Proceed?").with_default(true).prompt() {
        Ok(v) => Ok(v),
        Err(
            inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted,
        ) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn run_purge() -> Result<InteractiveAction> {
    println!();

    // Ask what to purge
    let modes = vec![PurgeMode::Both, PurgeMode::SshOnly, PurgeMode::RcloneOnly];

    let mode = match Select::new("What to purge?", modes).prompt() {
        Ok(m) => m,
        Err(
            inquire::InquireError::OperationCanceled | inquire::InquireError::OperationInterrupted,
        ) => {
            return Ok(InteractiveAction::Cancelled);
        }
        Err(e) => return Err(e.into()),
    };

    // Ask about dry run first
    let dry_run = ask_dry_run()?;
    if dry_run.is_none() {
        return Ok(InteractiveAction::Cancelled);
    }
    let dry_run = dry_run.unwrap();

    // Build and show confirmation summary
    let mode_str = format!("{}", mode);
    let dry_run_str = if dry_run { "Yes" } else { "No" };

    let summary = [
        "Action:  Purge managed resources".to_string(),
        format!("Target:  {}", mode_str),
        format!("Dry run: {}", dry_run_str),
    ];
    let summary_refs: Vec<&str> = summary.iter().map(|s| s.as_str()).collect();

    if !confirm_summary(&summary_refs)? {
        return Ok(InteractiveAction::Cancelled);
    }

    // Confirm with "purge" typed out (unless dry run)
    if !dry_run {
        println!();
        let warning = match mode {
            PurgeMode::Both => {
                "This will DELETE all managed SSH keys and rclone remotes from your local machine."
            }
            PurgeMode::SshOnly => "This will DELETE all managed SSH keys from your local machine.",
            PurgeMode::RcloneOnly => {
                "This will DELETE all managed rclone remotes from your local machine."
            }
        };
        println!("  {}", warning);
        println!("  Proton Pass will NOT be modified.");
        println!();

        let confirmation = match Text::new("Type 'purge' to confirm:")
            .with_help_message("This action cannot be undone.")
            .prompt()
        {
            Ok(c) => c,
            Err(
                inquire::InquireError::OperationCanceled
                | inquire::InquireError::OperationInterrupted,
            ) => {
                return Ok(InteractiveAction::Cancelled);
            }
            Err(e) => return Err(e.into()),
        };

        if confirmation.trim().to_lowercase() != "purge" {
            println!("Purge cancelled.");
            return Ok(InteractiveAction::Cancelled);
        }
    }

    Ok(InteractiveAction::Purge { mode, dry_run })
}

fn run_view_status() -> Result<InteractiveAction> {
    println!();
    println!("  Status");
    println!("  ──────");
    println!();

    // Show version
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    println!("  Version:         v{}", VERSION);
    println!();

    // Load config
    let config = Config::load_or_create(&None).unwrap_or_default();
    let ssh_dir = config.expanded_ssh_output_dir();
    let config_path = Config::default_path();

    // Count SSH keys
    let ssh_key_count = if ssh_dir.exists() {
        std::fs::read_dir(&ssh_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name();
                        let name_str = name.to_string_lossy();
                        // Count files that look like private keys (no extension, not config)
                        !name_str.contains('.') && name_str != "config"
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    // Count SSH config hosts
    let ssh_config_path = ssh_dir.join("config");
    let ssh_host_count = if ssh_config_path.exists() {
        std::fs::read_to_string(&ssh_config_path)
            .map(|content| content.lines().filter(|l| l.starts_with("Host ")).count())
            .unwrap_or(0)
    } else {
        0
    };

    // Count rclone remotes (managed by us)
    // First, try to load rclone password if configured (or check if already in env)
    let mut rclone_password_available = std::env::var("RCLONE_CONFIG_PASS").is_ok();
    if !rclone_password_available {
        // Use configured password_path, or fall back to default
        let password_path = if config.rclone.password_path.is_empty() {
            DEFAULT_RCLONE_PASSWORD_PATH
        } else {
            &config.rclone.password_path
        };

        let spinner = progress::spinner("Loading rclone password...");
        let proton_pass = ProtonPass::new();
        if let Ok(password) = proton_pass.get_item_field(password_path) {
            std::env::set_var("RCLONE_CONFIG_PASS", password);
            rclone_password_available = true;
        }
        spinner.finish_and_clear();
    }

    // Count remotes (this decrypts the config internally via rclone)
    let spinner = progress::spinner("Decrypting rclone config...");
    let rclone_count = count_managed_rclone_remotes();
    spinner.finish_and_clear();
    let rclone_str = match rclone_count {
        Some(count) => count.to_string(),
        None => {
            if !rclone_password_available {
                "(encrypted)".to_string()
            } else {
                "(encrypted - wrong password?)".to_string()
            }
        }
    };

    // Display status
    println!("  SSH keys:        {}", ssh_key_count);
    println!("  SSH hosts:       {}", ssh_host_count);
    println!("  rclone remotes:  {}", rclone_str);
    println!();
    println!("  Locations:");
    println!("    SSH dir:       {}", ssh_dir.display());
    println!("    Config file:   {}", config_path.display());
    println!();

    // Return to menu after showing status
    Ok(InteractiveAction::ViewedStatus)
}

/// Count rclone remotes managed by pass-ssh-unpack
/// Returns None if config is encrypted and can't be read
fn count_managed_rclone_remotes() -> Option<usize> {
    // Use rclone config dump which outputs JSON (works with RCLONE_CONFIG_PASS env var)
    let output = std::process::Command::new("rclone")
        .args(["config", "dump"])
        .env("RCLONE_ASK_PASSWORD", "false")
        .output()
        .ok()?;

    if !output.status.success() {
        // Likely encrypted and no password available
        return None;
    }

    if output.stdout.is_empty() {
        return Some(0);
    }

    // Parse JSON output - it's a map of remote_name -> remote_config
    let config: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_slice(&output.stdout).ok()?;

    // Count remotes with description = "managed by pass-ssh-unpack"
    let count = config
        .values()
        .filter(|remote| {
            remote
                .get("description")
                .and_then(|d| d.as_str())
                .map(|d| d == "managed by pass-ssh-unpack")
                .unwrap_or(false)
        })
        .count();

    Some(count)
}
