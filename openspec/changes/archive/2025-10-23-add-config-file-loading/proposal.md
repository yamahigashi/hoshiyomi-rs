# Proposal: Support File-Based Configuration

## Why
Our CLI currently relies on command-line flags and environment variables for every run. That becomes unwieldy as options grow (GitHub OAuth, polling intervals, server settings). Operators want a declarative config file they can check into infrastructure repos, share across environments, or template for multiple instances. Adding file-based configuration reduces flag noise and makes deployments reproducible.

## What Changes
- Define a configuration file format (TOML) and default search order (`./starchaser.toml`, `~/.config/starchaser/config.toml`, custom path via `--config`).
- Load the config file before parsing CLI flags, merge values with environment variables and flags (flags win, then env, then file defaults).
- Validate the combined configuration (required GitHub auth, numeric ranges) and surface helpful errors referencing source (file/env/flag).
- Update documentation to show sample files and override precedence.

## Impact
- `Config::from_cli` will be refactored to read from disk, requiring a small configuration loader module.
- Tests need to cover precedence rules and failure cases (missing file, malformed TOML).
- Packaging/documentation must explain the new workflow so existing scripts are not surprised.
