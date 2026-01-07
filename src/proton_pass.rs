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
    #[serde(rename = "Custom")]
    pub custom: Option<CustomData>,
}

#[derive(Debug, Deserialize)]
pub struct SshKeyData {
    pub private_key: Option<String>,
    pub public_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CustomData {
    #[serde(default)]
    pub sections: Vec<CustomSection>,
}

#[derive(Debug, Deserialize)]
pub struct CustomSection {
    pub section_name: String,
    #[serde(default)]
    pub section_fields: Vec<SectionField>,
}

#[derive(Debug, Deserialize)]
pub struct SectionField {
    pub name: String,
    pub content: FieldContent,
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
    pub ssh: Option<String>,
    pub server_command: Option<String>,
    pub jump: Option<String>,
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

        Ok(response
            .vaults
            .into_iter()
            .map(|v| v.name)
            .filter(|name| name != "Trash")
            .collect())
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
                "--filter-state",
                "active",
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
                let ssh = Self::get_field(&item.content.extra_fields, "SSH");
                let server_command = Self::get_field(&item.content.extra_fields, "Server Command");
                let jump = Self::get_field(&item.content.extra_fields, "Jump");

                SshItem {
                    title: item.content.title,
                    private_key,
                    public_key,
                    host,
                    username,
                    aliases,
                    ssh,
                    server_command,
                    jump,
                }
            })
            .collect();

        Ok(items)
    }

    /// List custom items with "Teleport Rclone Config" section in a vault
    pub fn list_teleport_items(&self, vault: &str) -> Result<Vec<SshItem>> {
        let output = Command::new("pass-cli")
            .args([
                "item",
                "list",
                vault,
                "--filter-type",
                "custom",
                "--filter-state",
                "active",
                "--output",
                "json",
            ])
            .output()
            .context("Failed to execute pass-cli item list")?;

        // Empty vault or no custom items returns non-zero or empty output
        if !output.status.success() || output.stdout.is_empty() {
            return Ok(Vec::new());
        }

        let response: ItemListResponse =
            serde_json::from_slice(&output.stdout).context("Failed to parse item list response")?;

        let items = response
            .items
            .into_iter()
            .filter_map(|item| {
                // Check if this is a Teleport item by looking for the section
                let custom = item.content.content.custom?;
                let teleport_section = custom
                    .sections
                    .iter()
                    .find(|s| s.section_name == "Teleport Rclone Config")?;

                // Extract fields from the section
                let ssh = Self::get_section_field(&teleport_section.section_fields, "SSH");
                let server_command =
                    Self::get_section_field(&teleport_section.section_fields, "Server Command");

                // Only include if we have at least SSH or Server Command
                if ssh.is_none() && server_command.is_none() {
                    return None;
                }

                Some(SshItem {
                    title: item.content.title,
                    private_key: None,
                    public_key: None,
                    host: None,
                    username: None,
                    aliases: None,
                    ssh,
                    server_command,
                    jump: None,
                })
            })
            .collect();

        Ok(items)
    }

    /// List all processable items in a vault (SSH keys + Teleport custom items)
    pub fn list_all_items(&self, vault: &str) -> Result<Vec<SshItem>> {
        let mut items = self.list_ssh_keys(vault)?;
        items.extend(self.list_teleport_items(vault)?);
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

    /// Check if a vault exists by name
    pub fn vault_exists(&self, name: &str) -> Result<bool> {
        let vaults = self.list_vaults()?;
        Ok(vaults.iter().any(|v| v == name))
    }

    /// List all active item titles in a vault (any type)
    pub fn list_item_titles(&self, vault: &str) -> Result<Vec<String>> {
        let output = Command::new("pass-cli")
            .args([
                "item",
                "list",
                vault,
                "--filter-state",
                "active",
                "--output",
                "json",
            ])
            .output()
            .context("Failed to execute pass-cli item list")?;

        // Empty vault returns non-zero or empty output
        if !output.status.success() || output.stdout.is_empty() {
            return Ok(Vec::new());
        }

        let response: ItemListResponse =
            serde_json::from_slice(&output.stdout).context("Failed to parse item list response")?;

        Ok(response
            .items
            .into_iter()
            .map(|item| item.content.title)
            .collect())
    }

    /// Create a new vault
    pub fn create_vault(&self, name: &str) -> Result<()> {
        let output = Command::new("pass-cli")
            .args(["vault", "create", "--name", name])
            .output()
            .context("Failed to execute pass-cli vault create")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to create vault '{}': {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Create a custom item for Teleport with SSH and Server Command fields
    pub fn create_tsh_item(
        &self,
        vault: &str,
        title: &str,
        ssh_command: &str,
        server_command: &str,
    ) -> Result<()> {
        use std::io::Write;

        // Build the JSON template
        let template = serde_json::json!({
            "title": title,
            "note": "",
            "sections": [
                {
                    "section_name": "Teleport Rclone Config",
                    "fields": [
                        {
                            "field_name": "SSH",
                            "field_type": "text",
                            "value": ssh_command
                        },
                        {
                            "field_name": "Server Command",
                            "field_type": "text",
                            "value": server_command
                        }
                    ]
                }
            ]
        });

        // Write template to a temp file
        let mut temp_file =
            tempfile::NamedTempFile::new().context("Failed to create temp file for template")?;
        temp_file
            .write_all(template.to_string().as_bytes())
            .context("Failed to write template to temp file")?;

        // Create custom item from template
        let output = Command::new("pass-cli")
            .args([
                "item",
                "create",
                "custom",
                "--vault-name",
                vault,
                "--from-template",
                temp_file.path().to_str().unwrap(),
            ])
            .output()
            .context("Failed to create custom item")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to create item '{}': {}",
                title,
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

    fn get_section_field(fields: &[SectionField], name: &str) -> Option<String> {
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
