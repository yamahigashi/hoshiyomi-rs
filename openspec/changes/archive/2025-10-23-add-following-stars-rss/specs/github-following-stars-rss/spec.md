## ADDED Requirements

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
- The system SHALL compute a polling interval per user based on recent star activity, bounded between 10 minutes and 7 days.
- The system SHALL recalculate the interval whenever new star events are observed, shortening the interval for active users and lengthening it for dormant ones.
- The system SHALL skip users whose `next_check_at` is in the future and only enqueue those due for polling.

#### Scenario: Highly Active User
1. Given user `carol` starred five repositories within the last 24 hours
2. When the system recalculates her polling interval
3. Then it SHALL set `fetch_interval_minutes` to no greater than 30 minutes and schedule `next_check_at = now + interval`.

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
