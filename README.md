# pass-ssh-unpack

Extract SSH keys from Proton Pass to local files and generate SSH config.

## Features

- **Cross-platform**: Works on Linux, macOS, and Windows
- **Automatic SSH config generation**: Creates host entries with aliases
- **Machine-specific keys**: Filter keys by hostname suffix (e.g., `github/my-laptop`)
- **Incremental updates**: Only processes changed items by default
- **Rclone integration**: Automatically creates SFTP remotes for each SSH host
- **Wildcard filtering**: Filter vaults and items using glob patterns

## Requirements

- [Proton Pass CLI](https://protonpass.github.io/pass-cli/) (`pass-cli`)
- OpenSSH (`ssh-keygen`)
- [rclone](https://rclone.org/) (optional, for SFTP remote sync)

## Installation

### From source

```bash
git clone https://github.com/Frosthaven/pass-ssh-unpack.git
cd pass-ssh-unpack
cargo build --release
# Binary will be at ./target/release/pass-ssh-unpack
```

### Add to PATH

```bash
# Linux/macOS
sudo cp target/release/pass-ssh-unpack /usr/local/bin/

# Or symlink
ln -s "$(pwd)/target/release/pass-ssh-unpack" ~/.local/bin/pass-ssh-unpack
```

## Usage

```bash
# Extract all SSH keys from all vaults
pass-ssh-unpack

# Extract keys from specific vault(s)
pass-ssh-unpack --vault Personal
pass-ssh-unpack --vault "Work*"    # Wildcard matching

# Extract specific items
pass-ssh-unpack --item "github/*"
pass-ssh-unpack --vault Personal --item "github/*"

# Full regeneration (clear and rebuild)
pass-ssh-unpack --full

# Skip rclone sync
pass-ssh-unpack --no-rclone

# Remove all managed SSH keys and rclone remotes
pass-ssh-unpack --purge

# Quiet mode (suppress output)
pass-ssh-unpack --quiet

# Dry run (show what would be done)
pass-ssh-unpack --dry-run
```

### CLI Options

| Option | Short | Description |
|--------|-------|-------------|
| `--vault <PATTERN>` | `-v` | Vault(s) to process (repeatable, supports wildcards) |
| `--item <PATTERN>` | `-i` | Item title pattern(s) (repeatable, supports wildcards) |
| `--full` | `-f` | Full regeneration (clear config first) |
| `--dry-run` | | Show what would be done without making changes |
| `--quiet` | `-q` | Suppress output |
| `--no-rclone` | | Skip rclone remote sync |
| `--purge` | | Remove all managed SSH keys and rclone remotes |
| `--config <PATH>` | `-c` | Custom config file path |
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show version |

## Configuration

On first run, a default config file is created at `~/.config/pass-ssh-unpack/config.toml`:

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
# Optional if RCLONE_CONFIG_PASS is already set in your environment
password_path = ""
```

### Sync Public Key Options

The `sync_public_key` option controls when generated public keys are synced back to Proton Pass:

| Value | Description |
|-------|-------------|
| `"never"` | Never update public keys in Proton Pass |
| `"if_empty"` | Only update if the public key field is empty (default) |
| `"always"` | Always overwrite the public key in Proton Pass |

### Rclone Config Password

If your rclone config is encrypted, the password can be provided in two ways:

1. **Environment variable**: Set `RCLONE_CONFIG_PASS` in your shell profile
2. **Proton Pass**: Set `password_path` in the config to fetch it from Proton Pass

If both are available, the `password_path` value takes precedence.

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
2. **List vaults**: Gets all vaults (or filtered by `--vault`)
3. **Extract keys**: For each SSH key item:
   - Writes private key to `~/.ssh/proton-pass/<vault>/<item>`
   - Generates public key using `ssh-keygen`
   - Saves public key back to Proton Pass if missing
4. **Generate SSH config**: Creates `~/.ssh/proton-pass/config` with host entries
5. **Sync rclone** (optional): Creates SFTP remotes named after the first alias

### SSH Config Integration

Add this line to your `~/.ssh/config`:

```
Include ~/.ssh/proton-pass/config
```

## Rclone Remote Naming

Rclone remotes are created with the following convention:

- **Primary remote**: Named after the first alias (or item title if no aliases)
  - Type: `sftp`
  - Host: The actual hostname/IP from the `Host` field
- **Additional aliases**: Created as `alias` type remotes pointing to the primary

Example:
```ini
[my-server]
type = sftp
host = 192.168.1.100
user = admin
key_file = ~/.ssh/proton-pass/Personal/my-server
description = managed by pass-ssh-unpack

[server-alias]
type = alias
remote = my-server:
description = managed by pass-ssh-unpack
```

## License

MIT License - see [LICENSE](LICENSE) for details.
