# Design: File-Based Configuration

## Format
- Use TOML for readability and compatibility (`toml` crate for parsing).
- Top-level tables mirror existing CLI options (`github`, `polling`, `server`, `auth`). Example structure:
  ```toml
  [github]
  oauth_client_id = "..."
  token = "..."

  [polling]
  feed_length = 100
  min_interval_minutes = 10

  [server]
  enable = true
  bind = "0.0.0.0"
  port = 8080
  refresh_minutes = 15
  ```

## Discovery Order
1. Path specified via `--config <file>` if present.
2. `./starchaser.toml` (current working directory).
3. `$XDG_CONFIG_HOME/starchaser/config.toml` or `~/.config/starchaser/config.toml` fallback on Unix; use `%AppData%` path on Windows.
4. If no file found, skip gracefully.

## Merge Strategy
- Load file into an intermediate struct with `Option<T>` fields.
- Environment variables (existing ones) override file values.
- CLI flags override both.
- Provide helper to report the origin of each field for better error messages.

## Validation
- After merging, reuse existing validation (non-empty GitHub credential, interval ranges) but augment errors to mention which layers supplied the conflicting value.
- Ensure confidential fields (token) can still come from env/flags even if absent in file.

## Compatibility
- CLI flags remain backward-compatible; users not providing a file experience no change.
- Document that `--config` is optional and may be combined with overrides.

## Open Questions
- Do we need nested profiles (e.g., `[profile.staging]`)? Not initially—keep MVP simple.
- Should we watch for config changes at runtime? Out of scope; read once at startup.
