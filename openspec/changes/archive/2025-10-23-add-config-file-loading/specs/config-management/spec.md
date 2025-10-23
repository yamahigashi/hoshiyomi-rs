## ADDED Requirements
### Requirement: File-Based Configuration Loading
- The CLI MUST support loading settings from a TOML configuration file located via `--config-path <path>` or default search paths (`./hoshiyomi.toml`, `$XDG_CONFIG_HOME/hoshiyomi/config.toml`, or platform equivalent).
- When a configuration file is present, the CLI MUST merge values with environment variables and command-line flags, with precedence: flags > env vars > config file > built-in defaults.
- The CLI MUST surface validation errors that identify the source of the offending value (file path, env var name, or flag) when possible.

#### Scenario: Config File With Overrides
1. Given `./hoshiyomi.toml` sets `feed_length = 50`
2. And the user runs `hoshiyomi --feed-length 25`
3. Then the effective feed length MUST be 25 and the CLI should report that the value came from the flag

#### Scenario: Missing Config File
1. Given no configuration file exists in any search path
2. When the user runs the CLI without `--config-path`
3. Then execution MUST proceed using environment variables and defaults without error

#### Scenario: Invalid Value Reports Source
1. Given the config file sets `min_interval_minutes = 0`
2. When the CLI validates settings
3. Then it MUST fail with an error citing the config file path and key that violated the constraint
