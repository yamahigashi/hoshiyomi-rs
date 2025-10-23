# github-auth Specification

## Purpose
TBD - created by archiving change add-github-oauth. Update Purpose after archive.
## Requirements
### Requirement: GitHub OAuth Device Flow Authentication
- The CLI MUST provide `auth login`, `auth status`, and `auth logout` subcommands for managing GitHub OAuth credentials.
- `auth login` MUST initiate the GitHub device authorization flow, display the verification URI and user code, poll for completion, and persist the issued access token with scopes and expiry metadata.
- `auth status` MUST report whether a valid OAuth token is stored (including its scopes and expiration) or indicate that re-authentication is required.
- `auth logout` MUST revoke local credentials by deleting stored tokens and falling back to legacy PAT authentication.
- The polling pipeline MUST prefer the stored OAuth token when present and valid, only using the legacy PAT when no OAuth token exists.

#### Scenario: Successful OAuth Login
1. Given no OAuth token is stored
2. When the user runs `starchaser auth login` and approves the device flow in a browser
3. Then the CLI stores the access token with metadata and reports success

#### Scenario: OAuth Status Reporting
1. Given a valid OAuth token is stored with known scopes
2. When the user runs `starchaser auth status`
3. Then the CLI prints the token scopes and expiration timestamp, indicating it is ready for use

#### Scenario: Fallback to PAT
1. Given OAuth credentials are missing or expired
2. When the poller builds the GitHub client
3. Then it SHALL use the configured PAT (if supplied) and log a notice suggesting `auth login`

#### Scenario: Logout Clears Token
1. Given a stored OAuth token
2. When the user runs `starchaser auth logout`
3. Then the token is removed from storage and subsequent runs require PAT or a fresh OAuth login

