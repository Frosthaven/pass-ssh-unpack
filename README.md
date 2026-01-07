# pass-ssh-unpack

> [!IMPORTANT]
> This tool is still in the PROTOTYPE phase. Expect breaking changes.
> It is not recommended for use in production environments.

A utility for unpacking SSH keys and generating SSH/rclone configurations from Proton Pass and Teleport.

## Guides

| Guide | Description |
|-------|-------------|
| [Proton Pass](docs/proton-pass.md) | Extract SSH keys from Proton Pass to local files |
| [Teleport](docs/teleport.md) | Import Teleport nodes for rclone access |

## Features

- **Cross-platform**: Works on Linux, macOS, and Windows
- **Automatic SSH config generation**: Creates host entries with aliases
- **Machine-specific keys**: Filter keys by hostname suffix (e.g., `github/my-laptop`)
- **Incremental updates**: Only processes changed items by default
- **Rclone integration**: Automatically creates SFTP remotes for each SSH host
- **Wildcard filtering**: Filter vaults and items using glob patterns
- **Progress indicators**: Visual feedback with spinners and progress bars
- **Encrypted rclone config**: Supports encrypted rclone configs with password from Proton Pass
- **Teleport support**: Import Teleport nodes as rclone-compatible items

## Requirements

- [Proton Pass CLI](https://protonpass.github.io/pass-cli/) (`pass-cli`)
- OpenSSH (`ssh-keygen`)
- [rclone](https://rclone.org/) (optional, for SFTP remote sync)
- [Teleport CLI](https://goteleport.com/docs/connect-your-client/tsh/) (`tsh`) (optional, for `--from-tsh`)

## Installation

### From [crates.io](https://crates.io/crates/pass-ssh-unpack)

```bash
cargo install pass-ssh-unpack
```

### From source

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

## Quick Start

### Proton Pass

Extract SSH keys and generate configs:

```bash
# From all vaults
pass-ssh-unpack

# From specific vault
pass-ssh-unpack --vault Personal

# Preview changes
pass-ssh-unpack --dry-run
```

See the [Proton Pass Guide](docs/proton-pass.md) for full documentation.

### Teleport

Import Teleport nodes for rclone access:

```bash
# Import nodes to a vault
pass-ssh-unpack --from-tsh --vault "Teleport Servers"

# Then generate rclone remotes
pass-ssh-unpack --rclone
```

See the [Teleport Guide](docs/teleport.md) for full documentation.

## License

MIT License - see [LICENSE](LICENSE) for details.
