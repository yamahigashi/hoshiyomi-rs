# Proposal: Serve Following Stars Feed via Web

## Overview
We will extend the existing CLI to optionally run a lightweight HTTP server (using Warp) that serves:
- `feed.xml`: the current RSS feed generated from stored star events
- `index.html`: a minimal HTML page that renders recent activity using the stored data

The server will reuse the existing polling pipeline so that HTTP clients can consume the feed and a human-friendly view without running the CLI manually.

## Problem
Today the tool only prints feed XML to stdout, leaving users to manage file hosting separately. To preview or publish the feed, they must redirect output and run additional tooling. A built-in HTTP server removes that friction and enables always-on hosting in one process.

## Goals
- Add an optional `serve` mode that starts Warp and serves both `feed.xml` and `index.html` endpoints.
- Reuse existing SQLite-backed data and feed generation logic to respond quickly without refetching from GitHub on every request.
- Offer configuration for bind address/port and refresh cadence (how often the server regenerates RSS from the database).
- Ensure the server continues polling GitHub (as today) so content stays fresh while the process runs.

## Non-Goals
- Building a full web UI beyond a lightweight HTML view.
- Supporting templating or customization beyond basic styling.
- Implementing push notifications or websockets.

## Proposed Approach
1. Introduce a new subcommand or flag (e.g., `--serve`) that runs the server loop instead of emitting RSS once.
2. Share existing polling logic by refactoring the runner so it can be invoked on a schedule within the same async runtime.
3. Implement Warp routes:
   - `GET /feed.xml` returns the RSS XML (recomputing from DB).
   - `GET /` (or `/index.html`) serves HTML built using stored events (possibly via Handlebars-lite string formatting).
4. Add background tasks: one for polling GitHub on an interval, another for serving HTTP requests.
5. Provide graceful shutdown (Ctrl+C) and logging.

## Risks & Mitigations
- **Concurrency overlap between polling and HTTP access**: Wrap DB operations in spawn_blocking (already done) and ensure read/write transactions avoid conflicts.
- **Increased resource usage**: Keep the server minimal, reuse existing feed generation code.
- **Warp dependency footprint**: Acceptable trade-off; choose minimal features.

## Open Questions
- Should HTML include pagination or just the latest N events? (Assume same item count as RSS for now.)
- What default port should we choose? (Tentatively 8080.)

