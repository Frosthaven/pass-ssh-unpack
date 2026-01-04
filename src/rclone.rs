use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

use crate::config::Config;
use crate::proton_pass::ProtonPass;

/// Entry for creating rclone remotes
#[derive(Debug, Clone)]
pub struct RcloneEntry {
    pub remote_name: String,
    pub host: String,
    pub user: String,
    pub key_file: String,
    pub other_aliases: String,
}

/// Sync rclone SFTP remotes based on extracted SSH keys
pub fn sync_remotes(
    entries: &[RcloneEntry],
    config: &Config,
    full_mode: bool,
    dry_run: bool,
    log: &impl Fn(&str),
) -> Result<()> {
    // Skip if rclone not available
    if which::which("rclone").is_err() {
        return Ok(());
    }

    // Skip if no entries to process
    if entries.is_empty() {
        return Ok(());
    }

    log("");
    log("Syncing rclone remotes...");

    // Set rclone password: password_path -> env var -> prompt (handled in get_rclone_config)
    if !config.rclone.password_path.is_empty() {
        // Try to get password from Proton Pass
        let proton_pass = ProtonPass::new();
        match proton_pass.get_item_field(&config.rclone.password_path, "password") {
            Ok(password) => {
                std::env::set_var("RCLONE_CONFIG_PASS", password);
            }
            Err(_) => {
                log("  (skipped - could not get rclone password)");
                return Ok(());
            }
        }
    }
    // If password_path is empty, check if RCLONE_CONFIG_PASS is already set
    // If not set, get_rclone_config() will prompt the user if needed

    // Get current config
    let mut current_config = get_rclone_config()?;

    // Full mode: delete all managed remotes first
    if full_mode {
        let managed_remotes: Vec<String> = current_config
            .iter()
            .filter(|(_, remote)| {
                remote.description.as_deref() == Some("managed by pass-ssh-unpack")
            })
            .map(|(name, _)| name.clone())
            .collect();

        for remote_name in &managed_remotes {
            if dry_run {
                log(&format!("  Would delete remote: {}", remote_name));
            } else {
                delete_remote(remote_name)?;
            }
        }

        // Refresh config after deletions (skip in dry run since nothing was deleted)
        if !dry_run {
            current_config = get_rclone_config()?;
        }
    }

    let mut created_count = 0;
    let mut skipped_count = 0;

    // Process each entry
    for entry in entries {
        if entry.remote_name.is_empty() {
            continue;
        }

        // Check if remote exists without our marker (unmanaged)
        if let Some(existing) = current_config.get(&entry.remote_name) {
            if existing.description.as_deref() != Some("managed by pass-ssh-unpack") {
                log(&format!(
                    "  Skipping {}: existing unmanaged remote",
                    entry.remote_name
                ));
                skipped_count += 1;
                continue;
            }
        }

        // Create/update primary SFTP remote (named after first alias, connects to host)
        if dry_run {
            if current_config.contains_key(&entry.remote_name) {
                log(&format!("  {} (exists)", entry.remote_name));
            } else {
                log(&format!(
                    "  Would create SFTP remote: {}",
                    entry.remote_name
                ));
            }
        } else if !entry.key_file.is_empty() {
            create_sftp_remote(
                &entry.remote_name,
                &entry.host,
                &entry.user,
                Some(&entry.key_file),
            )?;
        } else {
            create_sftp_remote(&entry.remote_name, &entry.host, &entry.user, None)?;
        }
        created_count += 1;

        // Create alias remotes for remaining aliases
        if !entry.other_aliases.is_empty() {
            for alias_name in entry
                .other_aliases
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                if alias_name == entry.remote_name {
                    continue;
                }

                // Check for unmanaged conflict
                if let Some(existing) = current_config.get(alias_name) {
                    if existing.description.as_deref() != Some("managed by pass-ssh-unpack") {
                        log(&format!(
                            "  Skipping alias {}: existing unmanaged remote",
                            alias_name
                        ));
                        skipped_count += 1;
                        continue;
                    }
                }

                if dry_run {
                    if current_config.contains_key(alias_name) {
                        log(&format!("  {} (exists)", alias_name));
                    } else {
                        log(&format!(
                            "  Would create alias remote: {} -> {}",
                            alias_name, entry.remote_name
                        ));
                    }
                } else {
                    create_alias_remote(alias_name, &entry.remote_name)?;
                }
                created_count += 1;
            }
        }
    }

    // Auto-prune: managed sftp remotes whose key_file doesn't exist (skip in dry run)
    let mut pruned_count = 0;

    if !dry_run {
        let updated_config = get_rclone_config()?;

        // Prune SFTP remotes with missing key files
        let home_dir = dirs::home_dir().unwrap_or_default();
        let sftp_to_prune: Vec<String> = updated_config
            .iter()
            .filter(|(_, remote)| {
                remote.remote_type == "sftp"
                    && remote.description.as_deref() == Some("managed by pass-ssh-unpack")
                    && remote
                        .key_file
                        .as_ref()
                        .map(|kf| {
                            let expanded = kf.replace("~", &home_dir.to_string_lossy());
                            !std::path::Path::new(&expanded).exists()
                        })
                        .unwrap_or(false)
            })
            .map(|(name, _)| name.clone())
            .collect();

        for remote_name in &sftp_to_prune {
            delete_remote(remote_name)?;
            pruned_count += 1;
        }

        // Prune alias remotes whose target was deleted
        let updated_config = get_rclone_config()?;
        let alias_to_prune: Vec<String> = updated_config
            .iter()
            .filter(|(_, remote)| {
                remote.remote_type == "alias"
                    && remote.description.as_deref() == Some("managed by pass-ssh-unpack")
                    && remote
                        .remote
                        .as_ref()
                        .map(|r| {
                            let target = r.trim_end_matches(':');
                            !updated_config.contains_key(target)
                        })
                        .unwrap_or(false)
            })
            .map(|(name, _)| name.clone())
            .collect();

        for remote_name in alias_to_prune {
            delete_remote(&remote_name)?;
            pruned_count += 1;
        }
    }

    if dry_run {
        log(&format!("  Would sync {} remotes.", created_count));
    } else {
        log(&format!("  Synced {} remotes.", created_count));
    }

    if skipped_count > 0 {
        log(&format!(
            "  Skipped {} (unmanaged conflicts).",
            skipped_count
        ));
    }
    if pruned_count > 0 {
        log(&format!("  Pruned {} orphaned remotes.", pruned_count));
    }

    Ok(())
}

/// Purge all managed rclone remotes
pub fn purge_managed_remotes(config: &Config, dry_run: bool, log: &impl Fn(&str)) -> Result<()> {
    // Skip if rclone not available
    if which::which("rclone").is_err() {
        log("  (rclone not installed)");
        return Ok(());
    }

    // Set rclone password: password_path -> env var -> prompt (handled in get_rclone_config)
    if !config.rclone.password_path.is_empty() {
        let proton_pass = ProtonPass::new();
        if let Ok(password) = proton_pass.get_item_field(&config.rclone.password_path, "password") {
            std::env::set_var("RCLONE_CONFIG_PASS", password);
        } else {
            log("  (skipped rclone - could not get password)");
            return Ok(());
        }
    }

    let current_config = get_rclone_config()?;

    let managed_remotes: Vec<String> = current_config
        .iter()
        .filter(|(_, remote)| remote.description.as_deref() == Some("managed by pass-ssh-unpack"))
        .map(|(name, _)| name.clone())
        .collect();

    let deleted_count = managed_remotes.len();

    for remote_name in &managed_remotes {
        if dry_run {
            log(&format!("  Would remove remote: {}", remote_name));
        } else {
            delete_remote(remote_name)?;
        }
    }

    if deleted_count > 0 {
        if dry_run {
            log(&format!("  Would remove {} rclone remotes", deleted_count));
        } else {
            log(&format!("  Removed {} rclone remotes", deleted_count));
        }
    } else {
        log("  No managed rclone remotes found");
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct RcloneRemote {
    #[serde(rename = "type")]
    remote_type: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    key_file: Option<String>,
    #[serde(default)]
    remote: Option<String>,
}

fn get_rclone_config() -> Result<HashMap<String, RcloneRemote>> {
    let output = Command::new("rclone")
        .args(["config", "dump"])
        .env("RCLONE_ASK_PASSWORD", "false")
        .output()
        .context("Failed to run rclone config dump")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check if this is an encrypted config without password
        if stderr.contains("unable to decrypt configuration")
            || stderr.contains("RCLONE_CONFIG_PASS")
        {
            // Prompt user for password
            eprint!("Rclone config password: ");
            let password = rpassword::read_password().context("Failed to read rclone password")?;

            if password.is_empty() {
                anyhow::bail!(
                    "No password provided. Set 'password_path' in your config file under [rclone] to avoid this prompt, e.g.:\n\
                     \n\
                     [rclone]\n\
                     password_path = \"pass://Personal/rclone/password\""
                );
            }

            // Set the password and retry
            std::env::set_var("RCLONE_CONFIG_PASS", &password);

            let retry_output = Command::new("rclone")
                .args(["config", "dump"])
                .output()
                .context("Failed to run rclone config dump")?;

            if !retry_output.status.success() {
                let retry_stderr = String::from_utf8_lossy(&retry_output.stderr);
                if retry_stderr.contains("wrong password")
                    || retry_stderr.contains("unable to decrypt")
                {
                    // Clear the bad password
                    std::env::remove_var("RCLONE_CONFIG_PASS");
                    anyhow::bail!("Incorrect rclone config password");
                }
                return Ok(HashMap::new());
            }

            if retry_output.stdout.is_empty() {
                return Ok(HashMap::new());
            }

            let config: HashMap<String, RcloneRemote> =
                serde_json::from_slice(&retry_output.stdout).unwrap_or_default();

            return Ok(config);
        }

        // Other errors - return empty config (might just be no config file)
        return Ok(HashMap::new());
    }

    if output.stdout.is_empty() {
        return Ok(HashMap::new());
    }

    let config: HashMap<String, RcloneRemote> =
        serde_json::from_slice(&output.stdout).unwrap_or_default();

    Ok(config)
}

fn create_sftp_remote(name: &str, host: &str, user: &str, key_file: Option<&str>) -> Result<()> {
    let mut args = vec![
        "config".to_string(),
        "create".to_string(),
        name.to_string(),
        "sftp".to_string(),
        format!("host={}", host),
        format!("user={}", user),
    ];

    if let Some(kf) = key_file {
        args.push(format!("key_file={}", kf));
    } else {
        args.push("ask_password=true".to_string());
    }

    args.push("description=managed by pass-ssh-unpack".to_string());

    Command::new("rclone")
        .args(&args)
        .output()
        .context("Failed to create rclone SFTP remote")?;

    Ok(())
}

fn create_alias_remote(name: &str, target: &str) -> Result<()> {
    Command::new("rclone")
        .args([
            "config",
            "create",
            name,
            "alias",
            &format!("remote={}:", target),
            "description=managed by pass-ssh-unpack",
        ])
        .output()
        .context("Failed to create rclone alias remote")?;

    Ok(())
}

fn delete_remote(name: &str) -> Result<()> {
    Command::new("rclone")
        .args(["config", "delete", name])
        .output()
        .context("Failed to delete rclone remote")?;

    Ok(())
}
