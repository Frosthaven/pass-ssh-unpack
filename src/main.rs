mod cli;
mod config;
mod error;
mod platform;
mod progress;
mod proton_pass;
mod rclone;
mod ssh;

use anyhow::Result;
use clap::Parser;

use cli::Args;
use config::Config;
use error::ErrorCollector;
use proton_pass::ProtonPass;
use rclone::RcloneEntry;
use ssh::SshManager;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let mut errors = ErrorCollector::new();
    let dry_run = args.dry_run;

    // Load or create config
    let config_path = args.config.clone().unwrap_or_else(Config::default_path);
    let mut config = Config::load_or_create(&args.config)?;

    // Apply CLI overrides to config
    if let Some(ref output_dir) = args.output_dir {
        config.ssh_output_dir = output_dir.to_string_lossy().to_string();
    }
    if let Some(sync_public_key) = args.sync_public_key {
        config.sync_public_key = sync_public_key;
    }
    if let Some(ref password_path) = args.rclone_password_path {
        config.rclone.password_path = password_path.clone();
    }
    if args.always_encrypt {
        config.rclone.always_encrypt = true;
    }

    // Determine which operations to run
    // --ssh: only SSH, --rclone: only rclone, neither: both
    let do_ssh = !args.rclone; // SSH unless --rclone only
    let do_rclone = !args.ssh && config.rclone.enabled; // rclone unless --ssh only

    // Helper for logging
    let log = |msg: &str| {
        if !args.quiet {
            println!("{}", msg);
        }
    };

    // Check for missing config options and warn user
    if config_path.exists() {
        let missing = config::check_missing_options(&config_path);
        if !missing.is_empty() && !args.quiet {
            eprintln!(
                "Warning: Your config is missing new options: {}",
                missing.join(", ")
            );
            eprintln!(
                "  Consider regenerating with: rm {:?} && pass-ssh-unpack",
                config_path
            );
            eprintln!();
        }
    }

    if dry_run {
        log("[DRY RUN] No changes will be made");
        log("");
    }

    // Check dependencies
    check_dependencies()?;

    // Handle purge mode
    if args.purge {
        return handle_purge(&config, dry_run, args.quiet, do_ssh, do_rclone);
    }

    if do_ssh {
        log("Extracting SSH keys from Proton Pass...");
    } else {
        log("Syncing rclone remotes only...");
    }
    log("");

    // Get current hostname for machine-specific filtering
    let current_hostname = platform::get_hostname();

    // Setup SSH manager
    let ssh_output_dir = config.expanded_ssh_output_dir();
    let mut ssh_manager =
        SshManager::new(&ssh_output_dir, args.full, dry_run, config.sync_public_key)?;

    // Get vaults to process
    let proton_pass = ProtonPass::new();
    let spinner = if !args.quiet {
        Some(progress::spinner("Loading vaults..."))
    } else {
        None
    };
    let all_vaults = proton_pass.list_vaults()?;
    if let Some(sp) = spinner {
        sp.finish_and_clear();
    }

    // Apply vault filters (CLI overrides config defaults)
    let vault_patterns = if args.vault.is_empty() {
        &config.default_vaults
    } else {
        &args.vault
    };

    let vaults_to_process = filter_by_patterns(&all_vaults, vault_patterns);

    if vaults_to_process.is_empty() && !vault_patterns.is_empty() {
        log("Warning: No vaults matched the specified patterns");
    }

    // Apply item filters (CLI overrides config defaults)
    let item_patterns = if args.item.is_empty() {
        &config.default_items
    } else {
        &args.item
    };

    // Collect rclone entries for later sync
    let mut rclone_entries: Vec<RcloneEntry> = Vec::new();

    // Process each vault with progress bar (only if doing SSH)
    if do_ssh {
        let vault_pb = if !args.quiet && !vaults_to_process.is_empty() {
            Some(progress::vault_progress_bar(vaults_to_process.len() as u64))
        } else {
            None
        };

        // Helper for logging that works with progress bar
        let pb_log = |msg: &str| {
            if !args.quiet {
                if let Some(ref pb) = vault_pb {
                    pb.println(msg);
                } else {
                    println!("{}", msg);
                }
            }
        };

        for (i, vault) in vaults_to_process.iter().enumerate() {
            pb_log(&format!("[{}]", vault));

            let items = match proton_pass.list_ssh_keys(vault) {
                Ok(items) => items,
                Err(e) => {
                    errors.add(&format!("Failed to list SSH keys in vault '{}'", vault), e);
                    pb_log("  (error listing keys)");
                    pb_log("");
                    if let Some(ref pb) = vault_pb {
                        pb.set_position(i as u64 + 1);
                    }
                    continue;
                }
            };

            if items.is_empty() {
                pb_log("  (no SSH keys)");
                pb_log("");
                if let Some(ref pb) = vault_pb {
                    pb.set_position(i as u64 + 1);
                }
                continue;
            }

            for item in items {
                // Filter by item patterns
                if !matches_any_pattern(&item.title, item_patterns) {
                    continue;
                }

                // Check machine-specific suffix
                if let Some(suffix) = item.title.split('/').last() {
                    if item.title.contains('/') {
                        let suffix_lower = suffix.to_lowercase();
                        if suffix_lower != current_hostname.to_lowercase() {
                            pb_log(&format!(
                                "  Skipping: {} (not for this machine)",
                                item.title
                            ));
                            continue;
                        }
                    }
                }

                pb_log(&format!("  Processing: {}", item.title));

                // Extract and process the SSH key
                match ssh_manager.process_item(&proton_pass, vault, &item, &pb_log) {
                    Ok(entry) => {
                        if let Some(rclone_entry) = entry {
                            rclone_entries.push(rclone_entry);
                        }
                    }
                    Err(e) => {
                        errors.add(&format!("Failed to process '{}'", item.title), e);
                    }
                }
            }

            pb_log("");
            if let Some(ref pb) = vault_pb {
                pb.set_position(i as u64 + 1);
            }
        }

        if let Some(pb) = vault_pb {
            pb.finish_and_clear();
        }

        // Generate SSH config
        log("Generating SSH config...");
        let (primary_count, alias_count, pruned_count) = ssh_manager.write_config()?;

        log("");
        log(&format!(
            "Done! Generated config has {} hosts and {} aliases.",
            primary_count, alias_count
        ));
        if pruned_count > 0 {
            log(&format!("Pruned {} orphaned entries.", pruned_count));
        }
        log(&format!(
            "SSH config written to: {}",
            ssh_manager.config_path().display()
        ));
    }

    // Sync rclone remotes
    if do_rclone {
        if let Err(e) =
            rclone::sync_remotes(&rclone_entries, &config, args.full, dry_run, args.quiet)
        {
            errors.add("Rclone sync", e);
        }
    }

    // Report any collected errors
    errors.report();

    if errors.has_errors() {
        std::process::exit(1);
    }

    Ok(())
}

fn check_dependencies() -> Result<()> {
    use anyhow::bail;

    if which::which("pass-cli").is_err() {
        bail!("pass-cli not found. Install Proton Pass CLI first.");
    }

    // Check if logged in
    let output = std::process::Command::new("pass-cli")
        .arg("info")
        .output()?;

    if !output.status.success() {
        eprintln!("Not logged into Proton Pass. Launching login...");
        eprintln!();

        // Try to login interactively
        let login_status = std::process::Command::new("pass-cli")
            .arg("login")
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?;

        if !login_status.success() {
            bail!("Failed to login to Proton Pass. Please run 'pass-cli login' manually.");
        }

        eprintln!();
    }

    if which::which("ssh-keygen").is_err() {
        bail!("ssh-keygen not found. Install OpenSSH first.");
    }

    Ok(())
}

fn handle_purge(
    config: &Config,
    dry_run: bool,
    quiet: bool,
    do_ssh: bool,
    do_rclone: bool,
) -> Result<()> {
    if !quiet {
        println!("Purging managed resources...");
    }

    // Delete SSH keys folder
    if do_ssh {
        let ssh_dir = config.expanded_ssh_output_dir();
        if ssh_dir.exists() {
            if dry_run {
                if !quiet {
                    println!("  Would remove {}", ssh_dir.display());
                }
            } else {
                std::fs::remove_dir_all(&ssh_dir)?;
                if !quiet {
                    println!("  Removed {}", ssh_dir.display());
                }
            }
        } else if !quiet {
            println!("  {} does not exist", ssh_dir.display());
        }
    }

    // Delete managed rclone remotes
    if do_rclone {
        rclone::purge_managed_remotes(config, dry_run, quiet)?;
    }

    if !quiet {
        println!("Done.");
    }
    Ok(())
}

fn filter_by_patterns(items: &[String], patterns: &[String]) -> Vec<String> {
    if patterns.is_empty() {
        return items.to_vec();
    }

    items
        .iter()
        .filter(|item| matches_any_pattern(item, patterns))
        .cloned()
        .collect()
}

fn matches_any_pattern(item: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return true;
    }

    for pattern in patterns {
        if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
            if glob_pattern.matches(item) {
                return true;
            }
        }
    }

    false
}
