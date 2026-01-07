# Proton Pass Guide

[Back to README](../README.md) | [Teleport Guide](teleport.md)

---

Extract SSH keys from Proton Pass to local files and generate SSH config and rclone remotes.

## Requirements

- [Proton Pass CLI](https://protonpass.github.io/pass-cli/) (`pass-cli`)
- OpenSSH (`ssh-keygen`)
- [rclone](https://rclone.org/) (optional, for SFTP remote sync)

## Usage

```bash
# Extract SSH keys and generate config from all vaults
pass-ssh-unpack

# From specific vault(s)
pass-ssh-unpack --vault Personal
pass-ssh-unpack --vault "Work*"    # Wildcard matching

# For specific items
pass-ssh-unpack --item "github/*"
pass-ssh-unpack --vault Personal --item "github/*"

# Full regeneration (clear and rebuild)
pass-ssh-unpack --full

# Only process SSH keys (skip rclone)
pass-ssh-unpack --ssh

# Only process rclone remotes (skip SSH)
pass-ssh-unpack --rclone

# Remove all managed SSH key files, config, and rclone remotes
pass-ssh-unpack --purge

# Preview changes
pass-ssh-unpack --dry-run
```

## CLI Options

| Option | Short | Description |
|--------|-------|-------------|
| `--vault <PATTERN>` | `-v` | Vault(s) to process (repeatable, supports wildcards) |
| `--item <PATTERN>` | `-i` | Item title pattern(s) (repeatable, supports wildcards) |
| `--full` | `-f` | Full regeneration (clear config first) |
| `--dry-run` | | Show what would be done without making changes |
| `--quiet` | `-q` | Suppress output |
| `--ssh` | | Only process SSH keys (skip rclone sync) |
| `--rclone` | | Only process rclone remotes (skip SSH extraction) |
| `--purge` | | Remove all managed SSH keys and rclone remotes |
| `--config <PATH>` | `-c` | Custom config file path |
| `--output-dir <PATH>` | `-o` | Override SSH output directory |
| `--sync-public-key <MODE>` | | Override public key sync mode (never/if-empty/always) |
| `--rclone-password-path <PATH>` | | Override rclone password path in Proton Pass |
| `--always-encrypt` | | Force rclone config encryption after operations |
| `--help` | `-h` | Show help |

## Proton Pass Item Structure

SSH key items in Proton Pass should have the following fields:

| Field | Required | Description |
|-------|----------|-------------|
| **Title** | Yes | Item name. Use `title/hostname` format for machine-specific keys |
| **Private Key** | Yes | The private key |
| **Host** | Yes | The SSH host (IP or hostname) |
| **Username** | No | SSH username |
| **Aliases** | No | Comma-separated host aliases |
| **Jump** | No | Jump host for SSH config (`ProxyJump` directive) |
| **SSH** | No | Custom SSH binary/command for rclone (`ssh` option) |
| **Server Command** | No | SFTP server command for rclone (`server_command` option) |

### Jump Hosts and Custom SSH Commands

**Jump** is used for SSH config's `ProxyJump` directive - specify just the jump host:
- Example: `Jump = bastion.example.com`
- Generated SSH config: `ProxyJump bastion.example.com`
- This field only affects SSH config, not rclone.

**SSH** is used for rclone's `ssh` option - specify the full SSH command:
- Example: `SSH = ssh -J bastion.example.com`
- Generated rclone config: `ssh = ssh -J bastion.example.com`
- This field only affects rclone, not SSH config.

**Server Command** is used for rclone's `server_command` option - specify the SFTP server path:
- Example: `Server Command = /usr/lib/openssh/sftp-server`
- Generated rclone config: `server_command = /usr/lib/openssh/sftp-server`
- This is useful when using custom SSH commands that don't support the SFTP subsystem.

### Machine-Specific Keys

If an item title contains a `/`, the part after the last `/` is treated as a hostname filter. The key will only be extracted on machines with a matching hostname (case-insensitive).

Examples:
- `github/my-laptop` - Only extracted on machine with hostname `my-laptop`
- `work-server` - Extracted on all machines

#### macOS Hostname Detection

On macOS, the tool uses the **LocalHostName** (Bonjour name) rather than the dynamic DHCP hostname. To check or set it:

```bash
# Check
scutil --get LocalHostName

# Set
sudo scutil --set LocalHostName my-laptop
```

## How It Works

1. **Authenticate**: Checks that you're logged into Proton Pass CLI
2. **Extract keys**: For each SSH key item:
   - Writes private key to `~/.ssh/proton-pass/<vault>/<item>`
   - Generates public key using `ssh-keygen`
   - Saves public key back to Proton Pass if missing and `sync-public-key` is enabled
3. **Generate SSH config**: Creates `~/.ssh/proton-pass/config` with host entries
4. **Sync rclone remotes**: Creates SFTP remotes named after the first alias

### SSH Config Integration

Add this line to your `~/.ssh/config`:

```
Include ~/.ssh/proton-pass/config
```

## Configuration

On first run, a config file is created at `~/.config/pass-ssh-unpack/config.toml`:

```toml
# Directory where SSH keys and config are written
ssh_output_dir = "~/.ssh/proton-pass"

# Default vault filter(s) - applied when no --vault flag is given
default_vaults = []

# Default item filter(s) - applied when no --item flag is given
default_items = []

# When to sync generated public keys back to Proton Pass
# Options: "never", "if_empty" (default), "always"
sync_public_key = "if_empty"

[rclone]
# Enable rclone SFTP remote sync
enabled = true

# Path in Proton Pass to rclone config password (if encrypted)
# Example: "pass://Personal/rclone/password"
password_path = ""

# Always ensure rclone config is encrypted after operations
always_encrypt = false
```
