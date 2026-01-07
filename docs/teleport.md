# Teleport Integration

[Back to README](../README.md) | [Proton Pass Guide](proton-pass.md)

---

Import Teleport nodes as rclone-compatible items in Proton Pass. This allows you to use rclone with Teleport-authenticated SSH connections.

## Requirements

- [Teleport CLI](https://goteleport.com/docs/connect-your-client/tsh/) (`tsh`)
- [Proton Pass CLI](https://protonpass.github.io/pass-cli/) (`pass-cli`)
- Active `tsh` login session (`tsh login`)

## Usage

```bash
# Import all Teleport nodes to a vault
pass-ssh-unpack --from-tsh --vault "Teleport Servers"

# Import only matching nodes
pass-ssh-unpack --from-tsh --vault "Teleport Servers" --item "prod-*"

# Preview what would be imported
pass-ssh-unpack --from-tsh --vault "Teleport Servers" --dry-run

# Skip remote scanning (use default sftp-server path)
pass-ssh-unpack --from-tsh --vault "Teleport Servers" --no-scan
```

## CLI Options

| Option | Short | Description |
|--------|-------|-------------|
| `--from-tsh` | | Import SSH entries from Teleport (required) |
| `--vault <NAME>` | `-v` | Target vault for imported items (required) |
| `--item <PATTERN>` | `-i` | Filter nodes by pattern (repeatable, supports wildcards) |
| `--dry-run` | | Show what would be done without making changes |
| `--no-scan` | | Skip scanning remotes for sftp-server path (use default) |
| `--quiet` | `-q` | Suppress output |
| `--help` | `-h` | Show help |

## How It Works

1. **Connects to Teleport**: Reads your active `tsh` session
2. **Lists nodes**: Fetches available nodes from your Teleport cluster
3. **Detects SFTP path**: SSHs into each node to find the sftp-server binary (unless `--no-scan`)
4. **Creates items**: Adds custom items to the specified Proton Pass vault

Each item contains a "Teleport Rclone Config" section with:
- **SSH**: `tsh ssh --proxy=<proxy> <hostname>` (used by rclone as the SSH command)
- **Server Command**: SFTP subsystem path (e.g., `/usr/libexec/openssh/sftp-server`)

## Generated rclone Remote

After importing, run `pass-ssh-unpack --rclone` to generate rclone remotes:

```ini
[my-server]
type = sftp
ssh = tsh ssh --proxy=teleport.example.com my-server
server_command = /usr/libexec/openssh/sftp-server
pass_ssh_unpack = true
```

You can then use rclone normally:

```bash
rclone ls my-server:/path/to/files
rclone sync my-server:/data ./local-backup
```

## Notes

- Items that already exist in the vault are skipped to preserve user customizations
- The vault is created automatically if it doesn't exist
- No SSH keys are stored since Teleport handles authentication via `tsh`
- The `--no-scan` flag uses `/usr/lib/openssh/sftp-server` as the default path
