# Project Context

## Purpose
hoshiyomi converts the public repositories starred by the GitHub accounts you follow into first-class artifacts you can consume anywhere, and its primary mission is to keep you informed about that followings-star stream above all else. The tool keeps a durable SQLite history of star events, adapts polling cadence per user, and republishes the activity as an RSS feed plus an interactive web dashboard and JSON API for downstream automations. The six focus areas from the current research spec are: GitHub following discovery, rate-limit-aware star retrieval, durable storage, adaptive scheduling, feed generation, and observability/server delivery.

## Tech Stack
- Rust 2024 edition with Tokio for async orchestration
- `reqwest` + `serde` for GitHub REST API access (`application/vnd.github.star+json`)
- `rusqlite` (WAL mode) for local persistence; schema managed in `src/db.rs`
- `rss` crate for RSS 2.0 serialization and HTML templating via `feed.rs`
- `warp` HTTP server with pre-rendered frontend assets bundled at compile time via `build.rs`
- Integration tests with `httpmock`, `tempfile`, and frontend HTML snapshot tests under `tests/`

## Project Conventions

### Code Style
- Enforce `cargo fmt` (rustfmt defaults) and `cargo clippy --all-targets -- -D warnings` as pre-submit gates.
- Prefer explicit structs and typed builders over ad-hoc maps; propagate errors with `anyhow::Result`.
- Keep modules focused: `config`, `db`, `feed`, `github`, `pipeline`, `server`; avoid cyclic dependencies.
- Log actionable events with structured context (`println!/eprintln!` currently; future work will adopt tracing).

### Architecture Patterns
- Entry point `src/main.rs` delegates to `Config::from_cli()` then either one-shot generation or server mode.
- `pipeline` orchestrates the full fetch cycle: discover followings, select due users, fetch stars concurrently under a Tokio semaphore, and persist outcomes.
- `github::GitHubClient` centralizes conditional requests, pagination stops, rate-limit handling, and typed responses.
- `db` module wraps all SQLite work in blocking tasks, applying transactions per user batch and recalculating EMA-based schedules.
- `feed` builds deterministic RSS XML and server-rendered HTML from the latest events, ensuring GUID stability.
- `server` composes `warp` routes (`/`, `/feed.xml`, `/api/stars`) with a background refresh task kicked off before serving traffic.

### Testing Strategy
- Require `cargo test`, covering:
  - Integration happy-paths exercising config parsing, polling, feed generation (`tests/integration.rs`).
  - HTTP contract replay via `httpmock` to assert GitHub edge cases (304, rate limit, pagination).
  - Frontend smoke/snapshot validation in `tests/frontend_snapshot.rs` after `build.rs` bundles assets.
- Manual verification: run one-shot CLI against a fixture token prior to releasing; capture RSS output diffs.
- Pending gaps: add deterministic clock injection for scheduler unit tests (tracked in OpenSpec change backlog).

### Git Workflow
- Use standard Git feature branches; keep `main` deploy-ready.
- Capture behavioural changes through OpenSpec proposals (`openspec/changes/<change-id>/`) before implementation.
- Squash or minimal commits are acceptable if the change links back to its spec `change-id`.
- Run `openspec validate --strict`, `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` before opening PRs.

## Domain Context
- GitHub followings are polled via REST endpoints: `GET /user/following` (paginated) and `GET /users/{login}/starred` with the star media type to expose `starred_at`.
- Conditional headers (`If-None-Match`, `If-Modified-Since`) and stored `ETag`/`Last-Modified` values prevent wasted calls; a 304 should still bump `last_fetched_at` and schedule the next poll.
- Star events persist all display metadata (repo description, language, topics) so the feed/UI does not need live API calls.
- Adaptive intervals rely on EMA over inter-star gaps; new users default to configurable fallback intervals until enough history accrues.
- RSS consumers expect RFC822 timestamps and stable GUIDs; feed ordering is strictly by `starred_at`.

## Important Constraints
- GitHub REST v3 rate limits: respect `X-RateLimit-Remaining`, `X-RateLimit-Reset`, and `Retry-After`; cap concurrency (default 5) and defer users when throttled.
- Authentication requires a PAT with `read:user` and `public_repo`; fail-fast on 401/403 to prompt operator action.
- SQLite runs in WAL mode; all DB writes occur inside blocking tasks and transactions to avoid cross-thread panics.
- Server mode must perform an initial sync before serving HTTP to guarantee `/feed.xml` and `/` are never empty placeholders.
- Spec mandates six focus areas remain in sync: followings discovery, star retrieval, storage, scheduling, feed generation, and delivery/observability.
- Future compatibility: keep code compatible with GitHub Enterprise by honouring configurable API base URLs and user agents.

## External Dependencies
- GitHub REST API (`https://api.github.com`) for followings and starred repositories.
- SQLite 3 (bundled via `rusqlite` with optional system libraries) for persistent storage.
- OpenSSL (TLS) and `pkg-config` toolchain required by `reqwest` during build.
- RSS readers/WebSub consumers ingesting `feed.xml`; dashboard assets compiled via `frontend/` build process.
- System schedulers (cron, systemd timers, GitHub Actions) are expected to run the CLI in environments without the server component.
