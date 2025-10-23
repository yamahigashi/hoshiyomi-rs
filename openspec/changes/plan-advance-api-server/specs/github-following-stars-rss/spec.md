## ADDED Requirements
### Requirement: Filterable Star Query API
- The server SHALL expose `GET /api/stars` that accepts query parameters for full-text search (`q`), language filter (`language`), activity tier filter (`activity`), user pin/exclude (`user_mode`, `user`), sort order (`sort=newest|alpha`), page index (`page` ≥1), and page size (`page_size` ≤100, default 25).
- The endpoint SHALL respond with a JSON envelope containing `items` (array of star events), `page`, `page_size`, `total`, `has_next`, `has_prev`, and `etag` plus `last_modified` metadata so clients can paginate without recomputing totals.
- The endpoint SHALL generate filter-aware weak ETags keyed by the newest `fetched_at`, total items for that filter set, and a hash of normalized query parameters; matching `If-None-Match` requests MUST return `304 Not Modified` with no body.

#### Scenario: Querying filtered page
- **GIVEN** stored star events span multiple users, languages, and activity tiers
- **WHEN** a client requests `GET /api/stars?q=rust&language=Rust&activity=high&user_mode=pin&user=alice&page=2&page_size=25&sort=alpha`
- **THEN** the server returns status `200` with a JSON envelope whose `items` reflect only Alice’s Rust repositories matching the search, sorted alphabetically, and `page=2`, `page_size=25`, `has_prev=true`, `has_next` depending on remaining rows, along with filter-specific `etag` and `Last-Modified` headers.

### Requirement: Service Status and Options Endpoints
- The server SHALL expose `GET /api/status` returning the last poll start/end timestamps, the earliest upcoming `next_check_at` per activity tier, and the most recent polling error message (or `null`).
- The server SHALL expose `GET /api/options` returning derived filter helpers including distinct languages with counts, distinct activity tiers in use, and known user handles with star counts, computed from stored data.
- Both endpoints SHALL include `Cache-Control: max-age=30`, mirror the latest scheduler heartbeat timestamps, and return `503 Service Unavailable` if the polling pipeline has not completed at least one successful run since startup.

#### Scenario: Surface scheduler health
- **GIVEN** the polling task completed at `2025-10-23T09:00:00Z` and scheduled the next high-activity check for `2025-10-23T09:30:00Z`
- **WHEN** a client calls `GET /api/status`
- **THEN** the response body includes `"last_poll_finished":"2025-10-23T09:00:00Z"`, a `next_checks.high` entry of `"2025-10-23T09:30:00Z"`, an empty `last_error`, and HTTP headers advertising `Cache-Control: max-age=30`.
