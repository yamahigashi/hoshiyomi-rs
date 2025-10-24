## ADDED Requirements
### Requirement: README Overview
- The repository README MUST open with a project summary that states the primary outputs (RSS, web dashboard, JSON API) and highlights differentiating features (adaptive polling, newest sorting options, filtering capabilities).

#### Scenario: First-time visitor understands scope
- **GIVEN** someone new to the project opens the README
- **WHEN** they read the introduction
- **THEN** they learn the tool emits RSS, serves a dashboard/API, and adapts polling behaviour without scrolling past the first section.

### Requirement: Quick Start Workflow
- The README MUST include a quick-start checklist that covers prerequisites, initial one-shot sync, and launching server mode with example commands.
- The quick-start MUST reference URL endpoints a reader can visit after launching the server.

#### Scenario: Running locally in minutes
- **GIVEN** a reader who has the prerequisites installed
- **WHEN** they follow the quick-start steps
- **THEN** they run one command to sync data, one command to start the server, and know which URLs to open for the dashboard and RSS feed.

### Requirement: Prerequisites and Installation Notes
- The README MUST enumerate required tooling (Rust toolchain, SQLite, OpenSSL headers, pkg-config) and GitHub token scopes.
- The README MUST offer at least one remediation tip for OpenSSL build issues (e.g., installing `libssl-dev` or setting `OPENSSL_DIR`).

#### Scenario: Build failure guidance
- **GIVEN** a reader hits an OpenSSL-related build error
- **WHEN** they review the prerequisites section
- **THEN** they see a suggested package to install or environment variable to set to proceed.

### Requirement: Mode Guides
- The README MUST provide dedicated subsections for (a) batch CLI runs, (b) server mode, and (c) RSS-only/automation usage, each summarising when to use the mode and pointing to representative commands.
- The server mode subsection MUST mention dashboard capabilities including filtering and newest sorting options.

#### Scenario: Choosing the right mode
- **GIVEN** an operator is deciding how to deploy the project
- **WHEN** they read the mode guide
- **THEN** they can pick the batch CLI, server, or automation approach that matches their needs and find the relevant commands.

### Requirement: Configuration Reference
- The README MUST explain configuration precedence (flags > env vars > config > defaults) and include a sample `hoshiyomi.toml` snippet highlighting key fields.

#### Scenario: Using config files
- **GIVEN** a reader wants to avoid long command lines
- **WHEN** they consult the configuration section
- **THEN** they see the precedence rules and a TOML example they can copy.

### Requirement: Operations and Scheduling
- The README MUST describe at least one automation approach (e.g., systemd timer) for recurring syncs and mention an alternative scheduling option (such as GitHub Actions or another platform).

#### Scenario: Automating refreshes
- **GIVEN** an operator wants unattended updates
- **WHEN** they read the operations section
- **THEN** they find a ready-to-use automation template and learn about at least one alternative.

### Requirement: Troubleshooting
- The README MUST contain a troubleshooting section that covers common issues, including OpenSSL build failures, GitHub rate limiting, and SQLite locking conflicts, with suggested remedies.

#### Scenario: Diagnosing issues
- **GIVEN** the system hits a rate limit or database lock
- **WHEN** the operator checks the troubleshooting section
- **THEN** they see actionable steps to resolve or mitigate the problem.

### Requirement: Contributor Guidance
- The README MUST outline developer-facing information: how to run tests/formatting, a brief directory layout overview, and a reminder to follow the OpenSpec workflow when contributing changes.

#### Scenario: New contributor onboarding
- **GIVEN** a developer wants to submit a patch
- **WHEN** they read the contributor guidance
- **THEN** they learn the basic commands to verify changes and understand the expectation to update OpenSpec artifacts.
