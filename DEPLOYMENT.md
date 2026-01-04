# Deployment Workflow

This project uses an automated release workflow with GitHub Actions and cargo-release.

## Overview

```
1. Develop on dev branch
2. Bump version with cargo-release
3. Push and create PR to main
4. Merge PR → draft release auto-created
5. Edit release notes on GitHub
6. Publish release → auto-publishes to crates.io
```

## Prerequisites

### Local tools

- [cargo-release](https://github.com/crate-ci/cargo-release): `cargo install cargo-release`

### GitHub secrets

Add `CARGO_REGISTRY_TOKEN` to your repository secrets:

1. Get a token from https://crates.io/settings/tokens
2. Go to repo `Settings > Secrets and variables > Actions`
3. Add new secret named `CARGO_REGISTRY_TOKEN`

## Step-by-step

### 1. Develop on dev

Make your changes on the `dev` branch as usual.

### 2. Bump version

When ready to release, bump the version:

```bash
# Preview what will happen (dry run)
cargo release patch

# Actually bump the version
cargo release patch --execute
```

Version bump options:
- `patch` - 0.1.0 → 0.1.1 (bug fixes)
- `minor` - 0.1.0 → 0.2.0 (new features)
- `major` - 0.1.0 → 1.0.0 (breaking changes)

This will:
- Update version in `Cargo.toml`
- Create a commit: `chore: release v0.1.1`

### 3. Push and create PR

```bash
git push origin dev
```

Then create a PR from `dev` → `main` on GitHub.

> **Note:** If your PR changes files in `src/`, `Cargo.toml`, or `Cargo.lock`, the version-check workflow will verify that the version has been bumped. The PR will fail if the version matches main.

### 4. Merge PR

Once the PR is approved and merged, the release workflow automatically:

1. Detects the new version in `Cargo.toml`
2. Creates a git tag (e.g., `v0.1.1`)
3. Builds binaries for all platforms:
   - linux-x64
   - linux-arm64
   - macos-x64
   - macos-arm64
   - windows-x64
4. Creates a **draft release** with the binaries attached

### 5. Edit release notes

Go to the [Releases page](https://github.com/Frosthaven/pass-ssh-unpack/releases) on GitHub.

Find your draft release and click "Edit". Add release notes describing:
- New features
- Bug fixes
- Breaking changes
- Upgrade instructions (if needed)

### 6. Publish release

Click "Publish release" to make it public.

This triggers the publish workflow which:
- Runs `cargo publish` to upload to crates.io

Users can now install with:
```bash
cargo install pass-ssh-unpack
```

## Workflows

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `version-check.yml` | PR to main | Ensures version is bumped for source changes |
| `release.yml` | Push to main | Creates tag, builds binaries, creates draft release |
| `publish.yml` | Release published | Publishes to crates.io |

## Fixing a draft release

If you find an issue before publishing:

1. Delete the draft release on GitHub
2. Delete the tag: `git push origin :refs/tags/v0.1.1`
3. Fix the issue on dev
4. Bump to the same version (or a new one)
5. Merge to main again

## Fixing a published release

If the release is already published to crates.io, you **must** bump to a new version. Crates.io does not allow overwriting published versions.

```bash
cargo release patch --execute  # 0.1.1 → 0.1.2
git push origin dev
# Create PR, merge, publish
```
