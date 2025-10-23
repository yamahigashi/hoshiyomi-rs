# Design Notes

## Runtime Topology
- **Mode selection**: The CLI gains a `serve` subcommand. When invoked, it
  1. Performs an initial poll using existing logic to ensure data is fresh.
  2. Starts two async tasks:
     - **Poller**: runs on a configurable interval, calling the existing polling routine.
     - **HTTP server**: Warp app exposing `/feed.xml` and `/`.
- Tasks share an `Arc<Config>` plus a lightweight cache channel if needed (e.g., to broadcast newly generated feed XML).

## HTTP Handlers
- `/feed.xml`: Generates RSS via existing `feed::build_feed`, sets `Content-Type: application/rss+xml`, and returns the XML body.
- `/`: Generates HTML with a simple template (hard-coded string builder) listing recent events. Optionally add relative timestamps via chrono formatting.
- Consider adding `Cache-Control: no-store` to ensure freshness.

## Polling Loop
- Reuse `run_once(config)` that:
  - Ensures DB schema is initialized.
  - Fetches followings (if stale) and polls due users.
  - Does **not** print RSS.
- Poller task calls `run_once` followed by sleep (`tokio::time::interval`). On error, log and retry next interval.

## Configuration Additions
- `serve` subcommand arguments:
  - `--bind` (default `127.0.0.1`)
  - `--port` (default `8080`)
  - `--refresh-minutes` (default `15`)
- Reuse existing database path, concurrency, intervals, etc.

## HTML Rendering
- Create helper `render_html(events: &[StarFeedRow]) -> String` that outputs minimal markup with a table or list.
- Include last build time and number of events.

## Graceful Shutdown
- Use `tokio::signal::ctrl_c()` to stop both tasks and shut down Warp via `warp::Server::graceful_shutdown`.

## Dependencies
- Add `warp` as the web framework and `mime` for content types if needed. Warp runs on Tokio so integration is straightforward.
