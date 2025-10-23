## ADDED Requirements
### Requirement: Filterable Star Query API
- The server SHALL expose `GET /api/stars` that accepts query parameters for full-text search (`q`), language filter (`language`), activity tier filter (`activity`), user pin/exclude mode (`user_mode=pin|exclude|all`) plus associated `user` handles (single handle per request), sort order (`sort=newest|alpha`), page index (`page` ≥ 1, default 1), and page size (`page_size` ≤ 100, default 25).
- The endpoint SHALL parse unknown or out-of-range parameters as `400 Bad Request` responses with a JSON problem payload that enumerates validation errors for `page`, `page_size`, `sort`, `activity`, and `user_mode`.
- The response body SHALL be a JSON object with `items` (array of star events) and `meta` containing `page`, `page_size`, `total`, `has_next`, `has_prev`, `etag`, and `last_modified` (RFC 2822 string) so clients can paginate without recomputing totals.
- Each `items` entry SHALL include `login`, `repo_full_name`, `repo_html_url`, optional `repo_description`, optional `repo_language`, `repo_topics` (array), `starred_at` (RFC 3339), `fetched_at` (RFC 3339), optional `user_activity_tier`, and `ingest_sequence`.
- The endpoint SHALL generate filter-aware weak ETags keyed by the normalized query parameter tuple, the newest `fetched_at`, and the filtered `total` count. Matching `If-None-Match` requests MUST return `304 Not Modified` with no body while still sending `ETag`, `Cache-Control: private, max-age=0`, and `Last-Modified` headers.

#### Scenario: Querying filtered page
- **GIVEN** stored star events span multiple users, languages, and activity tiers
- **WHEN** a client requests `GET /api/stars?q=rust&language=Rust&activity=high&user_mode=pin&user=alice&page=2&page_size=25&sort=alpha`
- **THEN** the server returns status `200` with a JSON envelope whose `items` reflect only Alice’s Rust repositories matching the search, sorted alphabetically, and `meta.page=2`, `meta.page_size=25`, `meta.has_prev=true`, `meta.has_next` depending on remaining rows, while emitting filter-specific `ETag` and `Last-Modified` headers.

#### Scenario: Conditional request yields 304
- **GIVEN** the client previously retrieved `/api/stars?language=Rust&page=1` with `ETag: W/"stars-rust@abc"`
- **WHEN** it issues `GET /api/stars?language=Rust&page=1` with header `If-None-Match: W/"stars-rust@abc"`
- **THEN** the server responds `304 Not Modified` with an empty body and repeats `ETag: W/"stars-rust@abc"`, `Cache-Control: private, max-age=0`, and `Last-Modified` headers.

### Requirement: Service Status and Options Endpoints
- The server SHALL expose `GET /api/status` returning `last_poll_started`, `last_poll_finished`, `is_stale`, `next_check_at` grouped by activity tier (`high`, `medium`, `low`, `unknown`), `last_error` (nullable string), and `rate_limit_remaining`/`rate_limit_reset` when available from the most recent GitHub response.
- The server SHALL expose `GET /api/status` with `Cache-Control: max-age=30, stale-while-revalidate=30` and `Content-Type: application/json; charset=utf-8` headers, returning `503 Service Unavailable` with a JSON problem payload while `is_stale=true` and no successful polls have occurred since startup.
- The server SHALL expose `GET /api/options` returning derived filter helpers as `{ languages: [{ name, count }], activity_tiers: [{ tier, count }], users: [{ login, display_name, count }] }`, including `etag` and `last_modified` metadata mirroring the latest database change.
- `/api/options` SHALL provide `Cache-Control: max-age=300` and honour conditional requests via `If-None-Match` and `If-Modified-Since`, calculating the weak ETag from the options payload hash.

#### Scenario: Surface scheduler health
- **GIVEN** the polling task completed at `2025-10-23T09:00:00Z` and scheduled the next high-activity check for `2025-10-23T09:30:00Z`
- **WHEN** a client calls `GET /api/status`
- **THEN** the response body includes `"last_poll_finished":"2025-10-23T09:00:00Z"`, `"next_check_at":{"high":"2025-10-23T09:30:00Z"}`, an empty `last_error`, `is_stale=false`, and HTTP headers advertising `Cache-Control: max-age=30, stale-while-revalidate=30`.

#### Scenario: Options ETag prevents reload
- **GIVEN** `/api/options` previously responded with `ETag: W/"opts-123"`
- **WHEN** the client issues a follow-up request with `If-None-Match: W/"opts-123"` and no new data exists
- **THEN** the server responds `304 Not Modified` with headers `ETag: W/"opts-123"`, `Cache-Control: max-age=300`, and omits the response body.
