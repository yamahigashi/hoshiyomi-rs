# Proposal: Add GitHub OAuth Device Flow Support

## Why
Today the tool requires a pre-generated personal access token (PAT). That raises friction for non-technical users, pushes them toward long-lived PATs with broad scopes, and is incompatible with organisations that block PAT creation. Enabling GitHub's OAuth device flow would let users authenticate with standard GitHub approvals, revoke tokens centrally, and rely on short-lived credentials.

## What Changes
- Introduce an optional `auth login` CLI flow that registers a GitHub OAuth application (client id/secret configurable) and exchanges a device code for an access token.
- Persist the issued OAuth access token (and metadata) locally, reusing it for future polling runs in place of the PAT.
- Expose helper commands to view status (`auth status`) and revoke credentials (`auth logout`).
- Keep PAT support as a fallback, automatically preferring the newer OAuth credentials when present.

## Impact
- Requires securely storing token material (likely in SQLite alongside existing state) and handling token expiration/refresh rules.
- CLI UX expands with new subcommands and prompts; documentation must clarify OAuth vs PAT usage.
- Implementation must call GitHub's OAuth device endpoints, poll for completion, and surface errors appropriately.
