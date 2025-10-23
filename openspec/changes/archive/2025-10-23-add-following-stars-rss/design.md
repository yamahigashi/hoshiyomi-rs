# Design Notes

## Component Overview
- **Scheduler CLI**: Entry point that loads configuration, opens the SQLite database, and orchestrates polling runs at a configurable cadence.
- **GitHub Client**: `reqwest` (or `octocrab`) wrapper that signs requests with a personal access token, sets conditional headers, parses pagination, and exposes typed responses containing `starred_at` timestamps.
- **Rate-Limit Controller**: Tokio semaphore (default 5 permits) around outbound calls plus shared state that inspects `X-RateLimit-Remaining`, `X-RateLimit-Reset`, and `Retry-After` headers to delay additional work when limits approach zero.
- **Persistence Layer**: `rusqlite`-backed repository that maintains `users` and `stars` tables, applies UPSERT semantics, and records polling metadata (`last_fetched_at`, `etag`, `next_check_at`).
- **Feed Builder**: Uses the `rss` crate to create a channel (`GitHub Followings Stars`) and convert stored events into RSS items with consistent GUIDs and RFC822 `pubDate` values.

## Data Flow
1. Scheduler queries for users whose `next_check_at <= now` ordered by priority (e.g., oldest first).
2. For each selected user, acquire a semaphore permit and issue `GET /users/{login}/starred` with cached `ETag` / `If-Modified-Since` headers.
3. If GitHub returns 304, update `last_fetched_at` and reschedule via adaptive interval logic. If 200, ingest JSON items until encountering a `starred_at` older than the most recent entry, then stop pagination.
4. Store new events in `stars`, update the corresponding `users` record (`last_starred_at`, `etag`, `next_check_at`), and release the semaphore permit.
5. After polling, select the latest N events (default 100, configurable) ordered by `starred_at DESC` and pass them to the feed builder to emit XML.

## Schema Draft
```sql
CREATE TABLE users (
  user_id INTEGER PRIMARY KEY,
  login TEXT NOT NULL UNIQUE,
  last_starred_at TEXT,
  last_fetched_at TEXT,
  etag TEXT,
  fetch_interval_minutes INTEGER NOT NULL,
  next_check_at TEXT NOT NULL
);

CREATE TABLE stars (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
  repo_full_name TEXT NOT NULL,
  starred_at TEXT NOT NULL,
  fetched_at TEXT NOT NULL,
  UNIQUE(user_id, repo_full_name, starred_at)
);
```
- Store timestamps in ISO8601 strings to avoid timezone ambiguity.

## Adaptive Scheduling
- Start with a baseline interval (e.g., 60 minutes) when no history exists.
- After each fetch, compute the mean time between the last 5 star events (minimum window of 1). Map the average to an interval bounded between 10 minutes and 7 days.
- If a user has no activity for 90 days, progressively extend towards the maximum interval but still re-check weekly.
- On 304 responses, reuse the prior interval; on new events, re-evaluate immediately.

## Error Handling
- For 401/403 responses, mark the run as failed and stop further calls to prevent lockouts.
- For transient network errors, retry with jittered exponential backoff up to a cap (e.g., 3 attempts) before deferring the user to the next window.
- Persist failures to a `poll_errors` log table (optional future enhancement) to inform observability.

## Feed Construction Details
- Channel metadata: `title="GitHub Followings Stars"`, `link="https://github.com"`, `description` summarising the aggregation.
- Item title format: `"{username} starred {repo_full_name}"`.
- GUID format: `"github-star://{username}/{repo_full_name}/{starred_at}"` with `isPermaLink=false`.
- `description` should include repository description and a link to the starring user profile when available.
- Support optional Atom output later by swapping the builder layer (out of scope for MVP but design keeps separation).

## Configuration & Secrets
- Accept the GitHub token via environment variable (`GITHUB_TOKEN`) or config file flag; fail fast if missing.
- Expose command-line flags for maximum concurrency, feed size, and database path to facilitate unattended runs (cron/systemd timers).

## Tooling & Libraries
- `reqwest` + Tokio for HTTP.
- `serde` / `serde_json` for decoding API responses.
- `rusqlite` (`SQLx` optional upgrade) for storage.
- `rss` crate for feed output; consider `quick-xml` if performance tuning is needed later.
