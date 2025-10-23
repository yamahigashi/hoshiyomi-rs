# Tasks

- [x] Introduce a TOML configuration loader that searches default paths and optional `--config-path` override.
- [x] Merge configuration sources with priority: CLI flags > environment variables > config file > built-in defaults; expose source info for validation errors.
- [x] Update `Config` construction and related modules to accept the merged settings without breaking existing callers/tests.
- [x] Add unit/integration tests covering precedence, malformed files, and missing required fields.
- [x] Document the configuration file format, default locations, and override order in README/usage help.
- [x] Run formatting, clippy, tests, and `openspec validate add-config-file-loading --strict` before requesting review.
