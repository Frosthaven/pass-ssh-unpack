use anyhow::Result;
use std::path::Path;

/// Get the current hostname (lowercase)
pub fn get_hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_lowercase())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Set file permissions to be readable/writable only by owner (600 on Unix)
#[cfg(unix)]
pub fn set_private_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// Set file permissions on Windows using icacls
#[cfg(windows)]
pub fn set_private_permissions(path: &Path) -> Result<()> {
    use anyhow::Context;
    use std::process::Command;

    let path_str = path.to_string_lossy();

    // Get current username
    let username =
        std::env::var("USERNAME").with_context(|| "Failed to get USERNAME environment variable")?;

    // Remove inherited permissions and grant only current user full control
    let output = Command::new("icacls")
        .args([
            &*path_str,
            "/inheritance:r",
            "/grant:r",
            &format!("{}:F", username),
        ])
        .output()
        .with_context(|| "Failed to run icacls")?;

    if !output.status.success() {
        anyhow::bail!("icacls failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

/// Get the home directory path string for use in SSH config
/// Returns %d (SSH config placeholder for home directory)
pub fn ssh_home_placeholder() -> &'static str {
    "%d"
}
