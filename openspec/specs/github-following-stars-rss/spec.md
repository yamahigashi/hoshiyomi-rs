# github-following-stars-rss Specification

## Purpose
TBD - created by archiving change add-following-stars-rss. Update Purpose after archive.
## Requirements
### Requirement: Discover Followed Accounts
- The system SHALL authenticate to GitHub using a personal access token and fetch the complete list of followings via `GET /user/following`, handling pagination until all users are collected.
- The system SHALL cache each user's `login`, numeric `id`, and the timestamp of their most recent known star event.

#### Scenario: Fetching Multiple Pages of Followings
1. Given the authenticated user follows more than 100 accounts
2. When the system issues successive `GET /user/following` requests with page parameters
3. Then every followed account is stored with its `login`, `id`, and `last_starred_at` remains null until star data is retrieved.

### Requirement: Retrieve Star Events with Rate-Limit Safety
- The system SHALL call `GET /users/{login}/starred` with header `Accept: application/vnd.github.star+json` and reuse cached `ETag` / `Last-Modified` values via conditional headers to avoid unnecessary rate consumption.
- The system SHALL cap concurrent outbound requests (default ≤5 in-flight) and monitor `X-RateLimit-Remaining`, `X-RateLimit-Reset`, and `Retry-After` headers, pausing polling when limits are exhausted.
- The system SHALL stop paginating for a user once it encounters a `starred_at` value already present in storage or when GitHub returns fewer than the requested items.

#### Scenario: Conditional Request Returns 304
1. Given the system previously stored an `ETag` for user `alice`
2. When it issues `GET /users/alice/starred` with `If-None-Match` and GitHub returns `304 Not Modified`
3. Then the system SHALL record the attempt time, leave star data unchanged, and schedule the next check based on the adaptive interval without consuming rate-limit quota.

### Requirement: Persist Star Data in SQLite
- The system SHALL store followings in a `users` table with columns `user_id`, `login`, `last_starred_at`, `last_fetched_at`, `etag`, `fetch_interval_minutes`, and `next_check_at`.
- The system SHALL store star events in a `stars` table with columns `id`, `user_id`, `repo_full_name`, `starred_at`, `fetched_at`, enforcing uniqueness on `(user_id, repo_full_name, starred_at)`.
- The system SHALL ensure inserts and updates occur within transactions to prevent partial writes during polling.

#### Scenario: Inserting New Star Events
1. Given a fresh API response for user `bob` containing a star on `rust-lang/rust` at `2025-10-17T03:00:00Z`
2. When the system processes the response
3. Then it SHALL upsert `bob` in `users`, insert the star event into `stars`, update `last_starred_at` to the new timestamp, and commit atomically.

### Requirement: Adapt Polling Frequency Per User
- The system SHALL compute each user's polling interval using an exponential moving average (EMA) of inter-star gaps, bounded between `min_interval_minutes` (≥10) and `max_interval_minutes` (≤10080).
- The EMA smoothing constant SHALL be α = 0.3, applied as `ema_next = clamp(α * gap_minutes + (1 - α) * ema_prev, min, max)` for every new star gap once the user has at least three recorded star events.
- Until a user accumulates three star events, the system SHALL use `default_interval_minutes` (clamped to the min/max bounds) and label the activity tier as `low`.
- When seeding the EMA (the first time a user transitions from fewer than three to three or more events), the system SHALL average all available gap minutes to produce `ema_prev` before applying the smoothing update.
- The stored polling interval and associated activity tier (high ≤60 minutes, medium ≤1440 minutes, low otherwise) SHALL reflect the latest EMA output.

#### Scenario: Fallback For Sparse History
1. Given user `eve` has fewer than three recorded star events
2. When the scheduler recomputes her polling interval
3. Then it SHALL set `fetch_interval_minutes` to the clamped default interval and record the activity tier as `low`.

#### Scenario: EMA Update After New Star
1. Given user `frank` already has three or more star events and an existing EMA value of 90 minutes
2. When a new star arrives 30 minutes after the previous one
3. Then the system SHALL compute `ema_next = clamp(0.3 * 30 + 0.7 * 90)` (result 72 minutes) and persist that interval for subsequent scheduling.

#### Scenario: Activity Tier Mirrors EMA
1. Given the EMA-derived interval for user `grace` is 45 minutes after the latest update
2. When the system stores the recomputed interval
3. Then it SHALL classify the user as `high` activity so the web UI can filter accordingly.

### Requirement: Produce RSS Feed Output
- The system SHALL generate an RSS 2.0 feed using the `rss` crate (or equivalent) containing the most recent star events sorted by `starred_at` descending.
- Each RSS item SHALL include a title `{username} starred {repo_full_name}`, link to the repository HTML page, GUID `github-star://{username}/{repo_full_name}/{starred_at}` (non-permalink), description with repository summary (when available), and `pubDate` matching the star timestamp.
- The RSS channel SHALL include title `GitHub Followings Stars`, link `https://github.com`, description summarising the aggregation, and `lastBuildDate` set to feed generation time.

#### Scenario: Feed Generation After New Star
1. Given a new star event from user `dana` stored at `2025-10-18T04:15:00Z`
2. When the system renders the RSS feed
3. Then the resulting XML SHALL place Dana's event first, populate the required item fields, and set `lastBuildDate` to the rendering timestamp.

### Requirement: Handle Errors Transparently
- The system SHALL abort polling and surface actionable errors when GitHub returns authentication failures (401/403).
- The system SHALL retry transient network failures up to three times with exponential backoff before deferring the user to the next polling window.
- The system SHALL record failures with timestamps so operators can diagnose chronic issues.

#### Scenario: Hitting Rate Limit
1. Given the system receives a `403` response with `Retry-After: 60`
2. When processing the response
3. Then it SHALL pause additional calls for at least 60 seconds, mark affected users for retry after the pause, and log the event for review.

### Requirement: Serve Feed via HTTP
- The system SHALL expose an optional server mode that listens on a configurable host and port, defaulting to `127.0.0.1:8080`.
- When server mode is active, the system SHALL respond to `GET /feed.xml` with the latest RSS feed XML using `Content-Type: application/rss+xml`.
- When server mode is active, the system SHALL respond to `GET /` with an HTML page summarizing recent star events (user, repository link, description, and timestamp).

#### Scenario: Requesting Feed XML
1. Given the server is running with current data in SQLite
2. When a client performs `GET /feed.xml`
3. Then the server returns status `200`, `Content-Type: application/rss+xml`, and the body equals the RSS currently built from stored events.

#### Scenario: Requesting HTML Index
1. Given the server is running with at least one stored star event
2. When a client performs `GET /`
3. Then the server returns status `200`, `Content-Type: text/html`, and the body contains the starring user's login, repository name, and a link to the repo.

### Requirement: Background Polling in Server Mode
- The server mode SHALL refresh GitHub data on a configurable interval (default 15 minutes) using the existing polling pipeline.
- The server mode SHALL perform an initial refresh before accepting HTTP requests to avoid serving stale data.
- The server mode SHALL log polling errors and retry on the next scheduled interval without crashing the server.

#### Scenario: Initial Refresh Before Serving
1. Given the database is empty
2. When the server starts in serve mode
3. Then it performs a polling cycle before the HTTP routes begin responding, ensuring `/feed.xml` returns a valid (possibly empty) RSS feed.

### Requirement: Graceful Shutdown
- The server mode SHALL listen for termination signals (Ctrl+C) and shut down both the HTTP server and polling task cleanly.
- During shutdown, the server SHALL stop accepting new requests and finalize in-flight polling before exiting.

#### Scenario: Signal Handling
1. Given the server is running and polling on an interval
2. When the process receives an interrupt signal
3. Then the HTTP server stops accepting requests and the process exits without panicking.

