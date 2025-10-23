# Tasks

- [ ] Add configuration fields for GitHub OAuth client id/secret and default to bundled public client when appropriate.
- [ ] Implement `auth login/logout/status` CLI subcommands using GitHub device flow; persist tokens in SQLite with created/expires timestamps.
- [ ] Update polling pipeline to prefer stored OAuth tokens, falling back to `--github-token` when absent; ensure revocation clears cached credentials.
- [ ] Add automated coverage for successful login/logout flows (mocking GitHub endpoints) and token selection precedence.
- [ ] Document OAuth usage in README (setup, scopes, troubleshooting) and note PAT fallback.
- [ ] Run formatting, clippy, tests, and `openspec validate add-github-oauth --strict` before requesting review.
