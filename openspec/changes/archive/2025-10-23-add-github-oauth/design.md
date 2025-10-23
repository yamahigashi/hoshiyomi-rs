# Design: GitHub OAuth Device Flow Integration

## Overview
We will layer a device-code OAuth flow on top of the existing PAT-based authentication. Users can run `hoshiyomi auth login` to initiate GitHub's device authorization, approve via browser, and have the CLI capture the issued access token. The main polling pipeline will consume whichever credential is present—OAuth token first, PAT fallback.

## Components
### OAuth Client Configuration
- Add `oauth_client_id` / `oauth_client_secret` to `Config`. Provide defaults via environment variables so deployments can supply their own values. For local use, bundle GitHub's public demo client unless policy requires custom registration.

### CLI Subcommands
- `auth login`: initiates device flow (`POST /login/device/code`), prints verification URL/code, polls `POST /login/oauth/access_token` until success or timeout, and stores the token.
- `auth status`: inspects stored token metadata (issued at, scopes) and prints whether OAuth credentials are available/valid.
- `auth logout`: deletes cached OAuth credentials.

### Persistence
- Extend SQLite with `auth_tokens` (id INTEGER PK, provider TEXT, access_token TEXT, scopes TEXT, created_at, expires_at). Store tokens encrypted at rest if we can rely on `ring` or similar; otherwise, store plain text with clear documentation about filesystem protections.
- Update migration logic to create the new table and backfill `provider='github-oauth'` rows as needed.

### Polling Integration
- When building the HTTP client, check for a valid token in `auth_tokens`. If found and not expired, inject `Authorization: Bearer <token>`. If absent or expired, fall back to PAT (and log suggestion to run `auth login`).
- Implement token expiry handling. GitHub OAuth tokens issued via device flow do not include refresh tokens by default; to rotate, prompt the user to re-run `auth login` when the token fails or `expires_at` passes.

### Error Handling & UX
- Surface clear messages during login (e.g., "Waiting for approval..."), handle denial/timeouts, and exit with actionable hints.
- Ensure logout wipes both DB row and any cached in-memory credential.

### Documentation & Security
- Update README with OAuth instructions, required scopes (`read:user`, `repo` if private repos needed), and mention PAT fallback.
- Warn users to treat SQLite file as sensitive since it now stores OAuth tokens.

## Alternatives Considered
- **Browser-based OAuth redirect**: More complex because CLI would need localhost callback handling. Device flow is simpler and avoids networking constraints.
- **Dependency on `gh` CLI**: Could outsource auth to `gh auth status`, but adds external dependency and divergent UX.

## Open Questions
- Should we support refresh tokens (requires GitHub App integration)? For now, keep manual re-login.
- Do we need token encryption? Evaluate feasibility; otherwise rely on filesystem permissions and document risk.
