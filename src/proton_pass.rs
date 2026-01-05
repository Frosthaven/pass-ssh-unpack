use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Command;

/// Interface to Proton Pass CLI
pub struct ProtonPass;

#[derive(Debug, Deserialize)]
pub struct VaultListResponse {
    pub vaults: Vec<Vault>,
}

#[derive(Debug, Deserialize)]
pub struct Vault {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ItemListResponse {
    pub items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub content: ItemContent,
}

#[derive(Debug, Deserialize)]
pub struct ItemContent {
    pub title: String,
    pub content: ItemData,
    #[serde(default)]
    pub extra_fields: Vec<ExtraField>,
}

#[derive(Debug, Deserialize)]
pub struct ItemData {
    #[serde(rename = "SshKey")]
    pub ssh_key: Option<SshKeyData>,
}

#[derive(Debug, Deserialize)]
pub struct SshKeyData {
    pub private_key: Option<String>,
    pub public_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExtraField {
    pub name: String,
    pub content: FieldContent,
}

#[derive(Debug, Deserialize)]
pub struct FieldContent {
    #[serde(rename = "Text")]
    pub text: Option<String>,
}

/// Simplified SSH item for processing
#[derive(Debug)]
pub struct SshItem {
    pub title: String,
    pub private_key: Option<String>,
    pub public_key: Option<String>,
    pub host: Option<String>,
    pub username: Option<String>,
    pub aliases: Option<String>,
    pub command: Option<String>,
}

impl ProtonPass {
    pub fn new() -> Self {
        Self
    }

    /// List all vault names
    pub fn list_vaults(&self) -> Result<Vec<String>> {
        let output = Command::new("pass-cli")
            .args(["vault", "list", "--output", "json"])
            .output()
            .context("Failed to execute pass-cli vault list")?;

        if !output.status.success() {
            anyhow::bail!(
                "pass-cli vault list failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let response: VaultListResponse = serde_json::from_slice(&output.stdout)
            .context("Failed to parse vault list response")?;

        Ok(response.vaults.into_iter().map(|v| v.name).collect())
    }

    /// List SSH key items in a vault
    pub fn list_ssh_keys(&self, vault: &str) -> Result<Vec<SshItem>> {
        let output = Command::new("pass-cli")
            .args([
                "item",
                "list",
                vault,
                "--filter-type",
                "ssh-key",
                "--output",
                "json",
            ])
            .output()
            .context("Failed to execute pass-cli item list")?;

        // Empty vault or no SSH keys returns non-zero or empty output
        if !output.status.success() || output.stdout.is_empty() {
            return Ok(Vec::new());
        }

        let response: ItemListResponse =
            serde_json::from_slice(&output.stdout).context("Failed to parse item list response")?;

        let items = response
            .items
            .into_iter()
            .map(|item| {
                let ssh_key = item.content.content.ssh_key;
                let (private_key, public_key) = ssh_key
                    .map(|k| (k.private_key, k.public_key))
                    .unwrap_or((None, None));

                let host = Self::get_field(&item.content.extra_fields, "Host");
                let username = Self::get_field(&item.content.extra_fields, "Username");
                let aliases = Self::get_field(&item.content.extra_fields, "Aliases");
                let command = Self::get_field(&item.content.extra_fields, "Command");

                SshItem {
                    title: item.content.title,
                    private_key,
                    public_key,
                    host,
                    username,
                    aliases,
                    command,
                }
            })
            .collect();

        Ok(items)
    }

    /// Get a field value from a pass URI (e.g., pass://Vault/Item/password)
    pub fn get_item_field(&self, path: &str) -> Result<String> {
        let output = Command::new("pass-cli")
            .args(["item", "view", path])
            .output()
            .context("Failed to execute pass-cli item view")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to get value from '{}': {}",
                path,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Update an item field (for saving generated public key)
    pub fn update_item_field(
        &self,
        vault: &str,
        title: &str,
        field: &str,
        value: &str,
    ) -> Result<()> {
        let field_arg = format!("{}={}", field, value);
        let output = Command::new("pass-cli")
            .args([
                "item",
                "update",
                "--vault-name",
                vault,
                "--item-title",
                title,
                "--field",
                &field_arg,
            ])
            .output()
            .context("Failed to execute pass-cli item update")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to update field '{}': {}",
                field,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    fn get_field(fields: &[ExtraField], name: &str) -> Option<String> {
        fields
            .iter()
            .find(|f| f.name == name)
            .and_then(|f| f.content.text.clone())
            .filter(|s| !s.is_empty())
    }
}

impl Default for ProtonPass {
    fn default() -> Self {
        Self::new()
    }
}
