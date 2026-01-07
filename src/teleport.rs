use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;
use url::Url;

/// Interface to Teleport CLI (tsh)
pub struct Teleport;

#[derive(Debug, Deserialize)]
pub struct TeleportStatusResponse {
    pub active: Option<TeleportActive>,
}

#[derive(Debug, Deserialize)]
pub struct TeleportActive {
    pub profile_url: String,
    pub username: String,
    pub cluster: String,
}

#[derive(Debug, Deserialize)]
struct TeleportNode {
    spec: TeleportNodeSpec,
}

#[derive(Debug, Deserialize)]
struct TeleportNodeSpec {
    hostname: String,
}

impl Teleport {
    pub fn new() -> Self {
        Self
    }

    /// Check if tsh is logged in and return status info.
    /// Returns an error if not logged in.
    pub fn get_status(&self) -> Result<TeleportActive> {
        self.try_get_status()?
            .ok_or_else(|| anyhow::anyhow!("Not logged into Teleport. Run 'tsh login' first."))
    }

    /// Try to get status without prompting for login
    fn try_get_status(&self) -> Result<Option<TeleportActive>> {
        let output = Command::new("tsh")
            .args(["status", "--format=json"])
            .output()
            .context("Failed to execute tsh status")?;

        if !output.status.success() {
            return Ok(None);
        }

        let response: TeleportStatusResponse =
            serde_json::from_slice(&output.stdout).context("Failed to parse tsh status output")?;

        Ok(response.active)
    }

    /// Extract proxy address from profile_url
    /// - "https://teleport.thedragon.dev:443" -> "teleport.thedragon.dev"
    /// - "https://proxy.example.com:3080" -> "proxy.example.com:3080"
    pub fn get_proxy(&self, status: &TeleportActive) -> Result<String> {
        let url =
            Url::parse(&status.profile_url).context("Failed to parse Teleport profile URL")?;

        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("No host in Teleport profile URL"))?;

        let port = url.port().unwrap_or(443);

        if port == 443 {
            Ok(host.to_string())
        } else {
            Ok(format!("{}:{}", host, port))
        }
    }

    /// List all nodes via `tsh ls --format=json`
    pub fn list_nodes(&self) -> Result<Vec<String>> {
        let output = Command::new("tsh")
            .args(["ls", "--format=json"])
            .output()
            .context("Failed to execute tsh ls")?;

        if !output.status.success() {
            bail!("tsh ls failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let nodes: Vec<TeleportNode> =
            serde_json::from_slice(&output.stdout).context("Failed to parse tsh ls output")?;

        Ok(nodes.into_iter().map(|n| n.spec.hostname).collect())
    }

    /// Get SFTP subsystem path from remote node
    /// Searches the filesystem for sftp-server binary
    /// Returns the path (default: /usr/lib/openssh/sftp-server)
    pub fn get_subsystem(&self, hostname: &str) -> Result<String> {
        // Use find to locate sftp-server anywhere on the system
        let detect_script = r#"find /usr -name "sftp-server" -type f 2>/dev/null | head -1"#;

        let output = Command::new("tsh")
            .args(["ssh", hostname, detect_script])
            .output()
            .context("Failed to detect sftp-server on remote")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let path = stdout.trim();

        if path.is_empty() || !output.status.success() {
            // Fallback to common default
            Ok("/usr/lib/openssh/sftp-server".to_string())
        } else {
            Ok(path.to_string())
        }
    }
}

impl Default for Teleport {
    fn default() -> Self {
        Self::new()
    }
}
