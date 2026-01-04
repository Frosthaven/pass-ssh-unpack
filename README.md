# pass-ssh-unpack

> [!IMPORTANT]
> This tool is still in the PROTOTYPE phase. Expect breaking changes.
> It is not recommended for use in production environments.

A utility for unpacking proton's pass-cli ssh keys into usable ssh and rclone configurations. 

## Features

- **Cross-platform**: Works on Linux, macOS, and Windows
- **Automatic SSH config generation**: Creates host entries with aliases
- **Machine-specific keys**: Filter keys by hostname suffix (e.g., `github/my-laptop`)
- **Incremental updates**: Only processes changed items by default
- **Rclone integration**: Automatically creates SFTP remotes for each SSH host
- **Wildcard filtering**: Filter vaults and items using glob patterns
- **Progress indicators**: Visual feedback with spinners and progress bars
- **Encrypted rclone config**: Supports encrypted rclone configs with password from Proton Pass

## Requirements

- [Proton Pass CLI](https://protonpass.github.io/pass-cli/) (`pass-cli`)
- OpenSSH (`ssh-keygen`)
- [rclone](https://rclone.org/) (optional, for SFTP remote sync)

## Installation

### From [crates.io](https://crates.io/crates/pass-ssh-unpack)

```bash
cargo install pass-ssh-unpack
```

### From source

#### Clone Repository

```bash
git clone https://github.com/Frosthaven/pass-ssh-unpack.git
cd pass-ssh-unpack
cargo build --release
# Binary will be at ./target/release/pass-ssh-unpack
```

#### Add to PATH

```bash
# Linux/macOS
sudo cp target/release/pass-ssh-unpack /usr/local/bin/

# Or symlink
ln -s "$(pwd)/target/release/pass-ssh-unpack" ~/.local/bin/pass-ssh-unpack
```

## Usage

```bash
# Generates SSH key files, config, and rclone remotes from all vaults
pass-ssh-unpack

# ..from specific vault(s)
pass-ssh-unpack --vault Personal
pass-ssh-unpack --vault "Work*"    # Wildcard matching

# ..for specific items
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

# Quiet mode (suppress output)
pass-ssh-unpack --quiet

# Dry run (show what would be done)
pass-ssh-unpack --dry-run
```

### CLI Options

CLI options override corresponding config file settings.

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
| `--version` | `-V` | Show version |

## Configuration

On first run, a default config file is created at `~/.config/pass-ssh-unpack/config.toml`:

```toml
# pass-ssh-unpack configuration file
# This file is auto-generated on first run. All fields are optional.

# Directory where SSH keys and config are written
# Supports ~ for home directory
# Default: ~/.ssh/proton-pass
ssh_output_dir = "~/.ssh/proton-pass"

# Default vault filter(s) - applied when no --vault flag is given
# Supports wildcards: "Personal", "Work*", etc.
# Default: [] (all vaults)
default_vaults = []

# Default item filter(s) - applied when no --item flag is given
# Supports wildcards: "github/*", "*-prod", etc.
# Default: [] (all items)
default_items = []

# When to sync generated public keys back to Proton Pass
# Options: "never", "if_empty" (default), "always"
#   never    - Never update public keys in Proton Pass
#   if_empty - Only update if the public key field is empty (default)
#   always   - Always overwrite the public key in Proton Pass
sync_public_key = "if_empty"

[rclone]
# Enable rclone SFTP remote sync
# Default: true
enabled = true

# Path in Proton Pass to rclone config password (if encrypted)
# This is optional if RCLONE_CONFIG_PASS is already set in your environment.
# If both are set, this value takes precedence.
# Leave empty to rely on environment variable or unencrypted config.
# Example: "pass://Personal/rclone/password"
# Default: ""
password_path = ""

# Always ensure rclone config is encrypted after operations
# If true and a password is available (via password_path or RCLONE_CONFIG_PASS),
# the rclone config will be re-encrypted even if it wasn't encrypted before.
# Default: false
always_encrypt = false
```

## Proton Pass Item Structure

SSH key items in Proton Pass should have the following fields:

| Field | Required | Description |
|-------|----------|-------------|
| **Title** | Yes | Item name. Use `title/hostname` format for machine-specific keys |
| **Private Key** | Yes | The private key |
| **Host** | Yes | The SSH host (IP or hostname) |
| **Username** | No | SSH username |
| **Aliases** | No | Comma-separated host aliases |

### Machine-Specific Keys

If an item title contains a `/`, the part after the last `/` is treated as a hostname filter. The key will only be extracted on machines with a matching hostname.

Examples:
- `github/my-laptop` - Only extracted on machine with hostname `my-laptop`
- `work-server` - Extracted on all machines

## How It Works

1. **Authenticate**: Checks that you're logged into Proton Pass CLI
2. **Extract keys**: For each SSH key item:
   - Writes private key to `~/.ssh/proton-pass/<vault>/<item>`
   - Generates public key using `ssh-keygen`
   - Saves public key back to Proton Pass if missing and `sync-public-key` is
   enabled
4. **Generate SSH config**: Creates `~/.ssh/proton-pass/config` (or at your configured `ssh_output_dir`) with host entries that you can include in your own ssh config file.
5. **Sync rclone remotes**: Syncs SFTP remotes named after the first alias

### SSH Config Integration

Add this line to your `~/.ssh/config`:

```
Include ~/.ssh/proton-pass/config
```
_Note: If you used a custom `ssh_output_dir`, include config from there._

## License

MIT License - see [LICENSE](LICENSE) for details.
