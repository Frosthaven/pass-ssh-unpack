use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::progress;
use crate::proton_pass::ProtonPass;

/// Entry for creating rclone remotes
#[derive(Debug, Clone)]
pub struct RcloneEntry {
    pub remote_name: String,
    pub host: String,
    pub user: String,
    pub key_file: String,
    pub other_aliases: String,
    pub ssh: Option<String>,
    pub server_command: Option<String>,
}

/// In-memory rclone config that only writes to disk on finalize.
/// - Decrypts config into memory on creation
/// - All modifications happen in memory (no temp files)
/// - On finalize(): writes to disk and re-encrypts if needed
/// - On drop without finalize: original file is untouched (no changes made)
struct InMemoryConfig {
    /// The decrypted config content in memory
    content: String,
    /// Path to the actual rclone config
    original_path: PathBuf,
    /// Password to use for encryption (from env or config)
    password: Option<String>,
    /// Whether the original config was encrypted
    was_encrypted: bool,
    /// Whether to always encrypt (even if wasn't encrypted before)
    always_encrypt: bool,
    /// Whether any modifications were made to the config
    modified: bool,
    /// Whether finalize() was called successfully
    finalized: bool,
}

impl InMemoryConfig {
    /// Create a new in-memory config by decrypting the current rclone config.
    /// The password must already be set in RCLONE_CONFIG_PASS if config is encrypted.
    fn new(original_path: PathBuf, was_encrypted: bool, always_encrypt: bool) -> Result<Self> {
        // Capture the password (if any)
        let password = std::env::var("RCLONE_CONFIG_PASS").ok();

        // Export decrypted config to memory
        let output = Command::new("rclone")
            .args(["config", "show"])
            .output()
            .context("Failed to run rclone config show")?;

        if !output.status.success() {
            anyhow::bail!("Failed to decrypt rclone config");
        }

        let content = String::from_utf8_lossy(&output.stdout).into_owned();

        Ok(Self {
            content,
            original_path,
            password,
            was_encrypted,
            always_encrypt,
            modified: false,
            finalized: false,
        })
    }

    /// Get the current config content
    fn content(&self) -> &str {
        &self.content
    }

    /// Get mutable access to the config content
    fn content_mut(&mut self) -> &mut String {
        self.modified = true;
        &mut self.content
    }

    /// Determine if we should encrypt on finalize
    fn should_encrypt(&self) -> bool {
        // Encrypt if: (was encrypted) OR (always_encrypt AND we have a password)
        self.password.is_some() && (self.was_encrypted || self.always_encrypt)
    }

    /// Finalize: write config to disk and re-encrypt if needed.
    fn finalize(&mut self) -> Result<()> {
        if self.finalized {
            return Ok(());
        }

        if self.modified {
            // Write decrypted content to the config file
            fs::write(&self.original_path, &self.content)
                .context("Failed to write rclone config")?;

            // Re-encrypt if needed
            if self.should_encrypt() {
                if let Some(ref pass) = self.password {
                    Self::encrypt_config(pass, &self.original_path)?;
                }
            }
        }

        self.finalized = true;
        Ok(())
    }

    /// Encrypt the rclone config with the given password.
    fn encrypt_config(password: &str, config_path: &PathBuf) -> Result<()> {
        // We need to pass the password to rclone. Using stdin would be ideal
        // but rclone config encryption set doesn't support it well.
        // Use a pipe on Unix or a temporary approach that minimizes exposure.

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::process::Stdio;

            // Use process substitution via bash to avoid temp files
            let mut child = Command::new("rclone")
                .args([
                    "--config",
                    config_path.to_str().unwrap_or_default(),
                    "config",
                    "encryption",
                    "set",
                    "--password-command",
                    "cat",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .context("Failed to spawn rclone")?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(password.as_bytes())
                    .context("Failed to write password to rclone")?;
            }

            let output = child.wait_with_output()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to encrypt config: {}", stderr.trim());
            }
        }

        #[cfg(windows)]
        {
            // On Windows, we use echo via cmd - password briefly visible in process list
            // but no temp file on disk
            let output = Command::new("rclone")
                .args([
                    "--config",
                    config_path.to_str().unwrap_or_default(),
                    "config",
                    "encryption",
                    "set",
                    "--password-command",
                    &format!("cmd /c echo {}", password),
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .output()
                .context("Failed to run rclone config encryption")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to encrypt config: {}", stderr.trim());
            }
        }

        Ok(())
    }
}

/// Check if rclone config is encrypted by looking at the file content
fn is_config_encrypted() -> bool {
    let config_path = match get_config_path() {
        Ok(p) => p,
        Err(_) => return false,
    };

    match fs::read_to_string(&config_path) {
        Ok(content) => content.contains("RCLONE_ENCRYPT_"),
        Err(_) => false,
    }
}

/// Get the rclone config file path
fn get_config_path() -> Result<PathBuf> {
    let output = Command::new("rclone")
        .args(["config", "file"])
        .output()
        .context("Failed to run rclone config file")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output is like "Configuration file is stored at:\n/path/to/rclone.conf\n"
    let path = stdout
        .lines()
        .find(|l| l.ends_with(".conf"))
        .unwrap_or("/home/user/.config/rclone/rclone.conf");

    Ok(PathBuf::from(path))
}

/// Sync rclone SFTP remotes based on extracted SSH keys
pub fn sync_remotes(
    entries: &[RcloneEntry],
    config: &Config,
    full_mode: bool,
    dry_run: bool,
    quiet: bool,
) -> Result<()> {
    // Skip if rclone not available
    if which::which("rclone").is_err() {
        return Ok(());
    }

    // Skip if no entries to process
    if entries.is_empty() {
        return Ok(());
    }

    if !quiet {
        println!();
        println!("Syncing rclone remotes...");
    }

    // Set rclone password: password_path -> env var
    if !config.rclone.password_path.is_empty() {
        let spinner = if !quiet {
            Some(progress::spinner("Loading rclone password..."))
        } else {
            None
        };

        let proton_pass = ProtonPass::new();
        match proton_pass.get_item_field(&config.rclone.password_path) {
            Ok(password) => {
                std::env::set_var("RCLONE_CONFIG_PASS", password);
                if let Some(sp) = spinner {
                    sp.finish_and_clear();
                }
            }
            Err(_) => {
                if let Some(sp) = spinner {
                    sp.finish_with_message("failed");
                }
                if !quiet {
                    println!("  (skipped - could not get rclone password)");
                }
                return Ok(());
            }
        }
    }

    // Determine if we should use in-memory config (encrypted or always_encrypt)
    let was_encrypted = is_config_encrypted();
    let has_password = std::env::var("RCLONE_CONFIG_PASS").is_ok();
    let always_encrypt = config.rclone.always_encrypt && !dry_run;
    let use_in_memory = was_encrypted || (always_encrypt && has_password);
    let original_config_path = get_config_path()?;

    // Load config into memory if needed
    let mut in_memory_config = if use_in_memory {
        let spinner_msg = if was_encrypted {
            "Decrypting rclone config..."
        } else {
            "Reading rclone config..."
        };
        let spinner = if !quiet {
            Some(progress::spinner(spinner_msg))
        } else {
            None
        };
        let cfg = InMemoryConfig::new(original_config_path.clone(), was_encrypted, always_encrypt)?;
        if let Some(sp) = spinner {
            sp.finish_and_clear();
        }
        Some(cfg)
    } else {
        None
    };

    // Get current config - parse from memory or use rclone
    let current_config = if let Some(ref cfg) = in_memory_config {
        parse_ini_config(cfg.content())
    } else {
        get_rclone_config(None)?
    };

    // Build list of desired remotes for comparison
    let mut desired_remotes: HashMap<String, DesiredRemote> = HashMap::new();
    for entry in entries {
        if entry.remote_name.is_empty() {
            continue;
        }

        // Primary SFTP remote
        desired_remotes.insert(
            entry.remote_name.clone(),
            DesiredRemote::Sftp {
                host: entry.host.clone(),
                user: entry.user.clone(),
                key_file: if entry.key_file.is_empty() {
                    None
                } else {
                    Some(entry.key_file.clone())
                },
                ssh: entry.ssh.clone(),
                server_command: entry.server_command.clone(),
            },
        );

        // Alias remotes
        if !entry.other_aliases.is_empty() {
            for alias_name in entry
                .other_aliases
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                if alias_name != entry.remote_name {
                    desired_remotes.insert(
                        alias_name.to_string(),
                        DesiredRemote::Alias {
                            target: entry.remote_name.clone(),
                        },
                    );
                }
            }
        }
    }

    // Determine what needs to be done
    let mut to_create: Vec<(String, DesiredRemote)> = Vec::new();
    let mut to_update: Vec<(String, DesiredRemote)> = Vec::new();
    let mut to_delete: Vec<String> = Vec::new();
    let mut unchanged: Vec<String> = Vec::new();
    let mut skipped_unmanaged: Vec<String> = Vec::new();

    // Check what needs creating/updating
    let mut desired_names: Vec<_> = desired_remotes.keys().collect();
    desired_names.sort();

    for name in desired_names {
        let desired = &desired_remotes[name];
        if let Some(existing) = current_config.get(name) {
            // Check if it's managed by us
            if existing.description.as_deref() != Some("managed by pass-ssh-unpack") {
                skipped_unmanaged.push(name.clone());
                continue;
            }

            // Check if it needs updating
            if remote_matches(existing, desired) {
                unchanged.push(name.clone());
            } else {
                to_update.push((name.clone(), desired.clone()));
            }
        } else {
            to_create.push((name.clone(), desired.clone()));
        }
    }

    // In full mode, delete managed remotes that aren't in desired set
    if full_mode {
        for (name, remote) in &current_config {
            if remote.description.as_deref() == Some("managed by pass-ssh-unpack")
                && !desired_remotes.contains_key(name)
            {
                to_delete.push(name.clone());
            }
        }
    }

    // Calculate totals for progress
    let total_ops = to_delete.len() + to_create.len() + to_update.len();

    if total_ops == 0 {
        if !quiet {
            println!("  {} remotes up to date, nothing to do.", unchanged.len());
        }
        return Ok(());
    }

    // For dry run, just show what would happen
    if dry_run {
        if !quiet {
            for name in &to_delete {
                println!("  Would delete: {}", name);
            }
            for (name, desired) in &to_create {
                match desired {
                    DesiredRemote::Sftp { .. } => println!("  Would create: {}", name),
                    DesiredRemote::Alias { target } => {
                        println!("  Would create alias: {} -> {}", name, target)
                    }
                }
            }
            for (name, _) in &to_update {
                println!("  Would update: {}", name);
            }

            let mut parts = Vec::new();
            if !to_create.is_empty() {
                parts.push(format!("{} to create", to_create.len()));
            }
            if !to_update.is_empty() {
                parts.push(format!("{} to update", to_update.len()));
            }
            if !to_delete.is_empty() {
                parts.push(format!("{} to delete", to_delete.len()));
            }
            if !unchanged.is_empty() {
                parts.push(format!("{} unchanged", unchanged.len()));
            }
            println!("  {}", parts.join(", "));
        }
        return Ok(());
    }

    // Show progress bar for operations
    let pb = if !quiet {
        Some(progress::rclone_progress_bar(total_ops as u64))
    } else {
        None
    };

    let mut completed = 0u64;
    let mut created_names: Vec<String> = Vec::new();
    let mut updated_names: Vec<String> = Vec::new();
    let mut deleted_names: Vec<String> = Vec::new();

    // Delete remotes
    for name in &to_delete {
        if let Some(ref bar) = pb {
            bar.set_message(format!("Deleting: {}", name));
        }
        if let Some(ref mut cfg) = in_memory_config {
            delete_remote_in_memory(cfg.content_mut(), name);
        } else {
            delete_remote_via_rclone(name)?;
        }
        deleted_names.push(name.clone());
        completed += 1;
        if let Some(ref bar) = pb {
            bar.set_position(completed);
        }
    }

    // Create new remotes
    for (name, desired) in &to_create {
        if let Some(ref bar) = pb {
            bar.set_message(format!("Creating: {}", name));
        }
        if let Some(ref mut cfg) = in_memory_config {
            create_remote_in_memory(cfg.content_mut(), name, desired);
        } else {
            create_remote_via_rclone(name, desired)?;
        }
        created_names.push(name.clone());
        completed += 1;
        if let Some(ref bar) = pb {
            bar.set_position(completed);
        }
    }

    // Update changed remotes
    for (name, desired) in &to_update {
        if let Some(ref bar) = pb {
            bar.set_message(format!("Updating: {}", name));
        }
        if let Some(ref mut cfg) = in_memory_config {
            delete_remote_in_memory(cfg.content_mut(), name);
            create_remote_in_memory(cfg.content_mut(), name, desired);
        } else {
            delete_remote_via_rclone(name)?;
            create_remote_via_rclone(name, desired)?;
        }
        updated_names.push(name.clone());
        completed += 1;
        if let Some(ref bar) = pb {
            bar.set_position(completed);
        }
    }

    if let Some(bar) = pb {
        bar.finish_and_clear();
    }

    // Finalize in-memory config (write to disk and re-encrypt)
    if let Some(ref mut cfg) = in_memory_config {
        let spinner_msg = if cfg.should_encrypt() {
            "Encrypting rclone config..."
        } else {
            "Saving rclone config..."
        };
        let spinner = if !quiet {
            Some(progress::spinner(spinner_msg))
        } else {
            None
        };
        cfg.finalize()?;
        if let Some(sp) = spinner {
            sp.finish_and_clear();
        }
    }

    // Summary
    if !quiet {
        // Show detailed lists of changes
        if !created_names.is_empty() {
            created_names.sort();
            for name in &created_names {
                println!("  + {}", name);
            }
        }
        if !updated_names.is_empty() {
            updated_names.sort();
            for name in &updated_names {
                println!("  ~ {}", name);
            }
        }
        if !deleted_names.is_empty() {
            deleted_names.sort();
            for name in &deleted_names {
                println!("  - {}", name);
            }
        }

        // Show counts summary
        let mut parts = Vec::new();
        if !created_names.is_empty() {
            parts.push(format!("{} created", created_names.len()));
        }
        if !updated_names.is_empty() {
            parts.push(format!("{} updated", updated_names.len()));
        }
        if !deleted_names.is_empty() {
            parts.push(format!("{} deleted", deleted_names.len()));
        }
        if !unchanged.is_empty() {
            parts.push(format!("{} unchanged", unchanged.len()));
        }
        if parts.is_empty() {
            println!("  No changes made.");
        } else {
            println!("  {}", parts.join(", "));
        }

        if !skipped_unmanaged.is_empty() {
            println!(
                "  Skipped {} (unmanaged conflicts).",
                skipped_unmanaged.len()
            );
        }
    }

    Ok(())
}

/// Purge all managed rclone remotes
pub fn purge_managed_remotes(config: &Config, dry_run: bool, quiet: bool) -> Result<()> {
    // Skip if rclone not available
    if which::which("rclone").is_err() {
        if !quiet {
            println!("  (rclone not installed)");
        }
        return Ok(());
    }

    // Set rclone password
    if !config.rclone.password_path.is_empty() {
        let proton_pass = ProtonPass::new();
        if let Ok(password) = proton_pass.get_item_field(&config.rclone.password_path) {
            std::env::set_var("RCLONE_CONFIG_PASS", password);
        } else {
            if !quiet {
                println!("  (skipped rclone - could not get password)");
            }
            return Ok(());
        }
    }

    // Determine if we should use in-memory config
    let was_encrypted = is_config_encrypted();
    let has_password = std::env::var("RCLONE_CONFIG_PASS").is_ok();
    let always_encrypt = config.rclone.always_encrypt && !dry_run;
    let use_in_memory = was_encrypted || (always_encrypt && has_password);
    let original_config_path = get_config_path()?;

    // Load config into memory if needed (for reading current state)
    let mut in_memory_config = if use_in_memory && !dry_run {
        let spinner_msg = if was_encrypted {
            "Decrypting rclone config..."
        } else {
            "Reading rclone config..."
        };
        let spinner = if !quiet {
            Some(progress::spinner(spinner_msg))
        } else {
            None
        };
        let cfg = InMemoryConfig::new(original_config_path.clone(), was_encrypted, always_encrypt)?;
        if let Some(sp) = spinner {
            sp.finish_and_clear();
        }
        Some(cfg)
    } else {
        None
    };

    // Get current config
    let current_config = if let Some(ref cfg) = in_memory_config {
        parse_ini_config(cfg.content())
    } else {
        get_rclone_config(None)?
    };

    let managed_remotes: Vec<String> = current_config
        .iter()
        .filter(|(_, remote)| remote.description.as_deref() == Some("managed by pass-ssh-unpack"))
        .map(|(name, _)| name.clone())
        .collect();

    if managed_remotes.is_empty() {
        if !quiet {
            println!("  No managed rclone remotes found");
        }
        return Ok(());
    }

    if dry_run {
        if !quiet {
            for name in &managed_remotes {
                println!("  Would remove: {}", name);
            }
            println!("  Would remove {} rclone remotes", managed_remotes.len());
        }
        return Ok(());
    }

    let pb = if !quiet {
        Some(progress::rclone_progress_bar(managed_remotes.len() as u64))
    } else {
        None
    };

    for (i, name) in managed_remotes.iter().enumerate() {
        if let Some(ref bar) = pb {
            bar.set_message(format!("Deleting: {}", name));
            bar.set_position(i as u64 + 1);
        }
        if let Some(ref mut cfg) = in_memory_config {
            delete_remote_in_memory(cfg.content_mut(), name);
        } else {
            delete_remote_via_rclone(name)?;
        }
    }

    if let Some(bar) = pb {
        bar.finish_and_clear();
    }

    // Finalize in-memory config (write to disk and re-encrypt)
    if let Some(ref mut cfg) = in_memory_config {
        let spinner_msg = if cfg.should_encrypt() {
            "Encrypting rclone config..."
        } else {
            "Saving rclone config..."
        };
        let spinner = if !quiet {
            Some(progress::spinner(spinner_msg))
        } else {
            None
        };
        cfg.finalize()?;
        if let Some(sp) = spinner {
            sp.finish_and_clear();
        }
    }

    if !quiet {
        println!("  Removed {} rclone remotes", managed_remotes.len());
    }

    Ok(())
}

#[derive(Debug, Clone)]
enum DesiredRemote {
    Sftp {
        host: String,
        user: String,
        key_file: Option<String>,
        ssh: Option<String>,
        server_command: Option<String>,
    },
    Alias {
        target: String,
    },
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
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    ssh: Option<String>,
    #[serde(default)]
    server_command: Option<String>,
}

/// Check if existing remote matches desired config
fn remote_matches(existing: &RcloneRemote, desired: &DesiredRemote) -> bool {
    match desired {
        DesiredRemote::Sftp {
            host,
            user,
            key_file,
            ssh,
            server_command,
        } => {
            existing.remote_type == "sftp"
                && existing.host.as_deref() == Some(host)
                && existing.user.as_deref() == Some(user)
                && existing.key_file.as_ref().map(|s| s.as_str())
                    == key_file.as_ref().map(|s| s.as_str())
                && existing.ssh.as_ref().map(|s| s.as_str()) == ssh.as_ref().map(|s| s.as_str())
                && existing.server_command.as_ref().map(|s| s.as_str())
                    == server_command.as_ref().map(|s| s.as_str())
        }
        DesiredRemote::Alias { target } => {
            existing.remote_type == "alias"
                && existing
                    .remote
                    .as_ref()
                    .map(|r| r.trim_end_matches(':') == target)
                    .unwrap_or(false)
        }
    }
}

fn create_remote_in_memory(content: &mut String, name: &str, desired: &DesiredRemote) {
    // Remove existing section if present
    *content = remove_ini_section(content, name);

    // Build new section
    let section = match desired {
        DesiredRemote::Sftp {
            host,
            user,
            key_file,
            ssh,
            server_command,
        } => {
            let mut s = format!(
                "[{}]\ntype = sftp\nhost = {}\nuser = {}\n",
                name, host, user
            );
            if let Some(kf) = key_file {
                s.push_str(&format!("key_file = {}\n", kf));
            } else {
                s.push_str("ask_password = true\n");
            }
            if let Some(cmd) = ssh {
                s.push_str(&format!("ssh = {}\n", cmd));
            }
            if let Some(cmd) = server_command {
                s.push_str(&format!("server_command = {}\n", cmd));
            }
            s.push_str("description = managed by pass-ssh-unpack\n");
            s
        }
        DesiredRemote::Alias { target } => {
            format!(
                "[{}]\ntype = alias\nremote = {}:\ndescription = managed by pass-ssh-unpack\n",
                name, target
            )
        }
    };

    // Append new section
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&section);
}

fn create_remote_via_rclone(name: &str, desired: &DesiredRemote) -> Result<()> {
    let mut cmd = Command::new("rclone");

    match desired {
        DesiredRemote::Sftp {
            host,
            user,
            key_file,
            ssh,
            server_command,
        } => {
            cmd.args(["config", "create", name, "sftp"]);
            cmd.arg(format!("host={}", host));
            cmd.arg(format!("user={}", user));

            if let Some(kf) = key_file {
                cmd.arg(format!("key_file={}", kf));
            } else {
                cmd.arg("ask_password=true");
            }

            if let Some(ssh_cmd) = ssh {
                cmd.arg(format!("ssh={}", ssh_cmd));
            }

            if let Some(srv_cmd) = server_command {
                cmd.arg(format!("server_command={}", srv_cmd));
            }

            cmd.arg("description=managed by pass-ssh-unpack");
        }
        DesiredRemote::Alias { target } => {
            cmd.args([
                "config",
                "create",
                name,
                "alias",
                &format!("remote={}:", target),
                "description=managed by pass-ssh-unpack",
            ]);
        }
    }

    cmd.output().context("Failed to create rclone remote")?;
    Ok(())
}

fn delete_remote_in_memory(content: &mut String, name: &str) {
    *content = remove_ini_section(content, name);
}

fn delete_remote_via_rclone(name: &str) -> Result<()> {
    Command::new("rclone")
        .args(["config", "delete", name])
        .output()
        .context("Failed to delete rclone remote")?;
    Ok(())
}

/// Remove an INI section by name from content
fn remove_ini_section(content: &str, section_name: &str) -> String {
    let section_header = format!("[{}]", section_name);
    let mut result = String::new();
    let mut skip = false;

    for line in content.lines() {
        if line.starts_with('[') {
            skip = line == section_header;
        }
        if !skip {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

fn get_rclone_config(config_path: Option<&PathBuf>) -> Result<HashMap<String, RcloneRemote>> {
    let mut cmd = Command::new("rclone");

    if let Some(path) = config_path {
        cmd.arg("--config").arg(path);
    }

    cmd.args(["config", "dump"]);
    cmd.env("RCLONE_ASK_PASSWORD", "false");

    let output = cmd.output().context("Failed to run rclone config dump")?;

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

            let mut retry_cmd = Command::new("rclone");
            if let Some(path) = config_path {
                retry_cmd.arg("--config").arg(path);
            }
            retry_cmd.args(["config", "dump"]);

            let retry_output = retry_cmd
                .output()
                .context("Failed to run rclone config dump")?;

            if !retry_output.status.success() {
                let retry_stderr = String::from_utf8_lossy(&retry_output.stderr);
                if retry_stderr.contains("wrong password")
                    || retry_stderr.contains("unable to decrypt")
                {
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

        return Ok(HashMap::new());
    }

    if output.stdout.is_empty() {
        return Ok(HashMap::new());
    }

    let config: HashMap<String, RcloneRemote> =
        serde_json::from_slice(&output.stdout).unwrap_or_default();

    Ok(config)
}

/// Parse rclone INI config content into a HashMap of remotes
fn parse_ini_config(content: &str) -> HashMap<String, RcloneRemote> {
    let mut remotes = HashMap::new();
    let mut current_section: Option<String> = None;
    let mut current_fields: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with('[') && line.ends_with(']') {
            // Save previous section if any
            if let Some(ref section_name) = current_section {
                if let Some(remote) = fields_to_remote(&current_fields) {
                    remotes.insert(section_name.clone(), remote);
                }
            }

            // Start new section
            current_section = Some(line[1..line.len() - 1].to_string());
            current_fields.clear();
        } else if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim().to_string();
            let value = line[eq_pos + 1..].trim().to_string();
            current_fields.insert(key, value);
        }
    }

    // Save last section
    if let Some(ref section_name) = current_section {
        if let Some(remote) = fields_to_remote(&current_fields) {
            remotes.insert(section_name.clone(), remote);
        }
    }

    remotes
}

/// Convert INI fields to RcloneRemote
fn fields_to_remote(fields: &HashMap<String, String>) -> Option<RcloneRemote> {
    let remote_type = fields.get("type")?.clone();
    Some(RcloneRemote {
        remote_type,
        description: fields.get("description").cloned(),
        key_file: fields.get("key_file").cloned(),
        remote: fields.get("remote").cloned(),
        host: fields.get("host").cloned(),
        user: fields.get("user").cloned(),
        ssh: fields.get("ssh").cloned(),
        server_command: fields.get("server_command").cloned(),
    })
}
