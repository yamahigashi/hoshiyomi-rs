## ADDED Requirements
### Requirement: Filter-aware Stars API
- The server SHALL execute `GET /api/stars` by querying SQLite with the caller's filters (`q` substring search across repo name/description, `language`, `activity` tier, `user_mode` + `user` for pin/exclude) and sort order (`newest` by `fetched_at` desc, `alpha` by `repo_full_name` asc).
- The endpoint SHALL honour `page` (≥1) and `page_size` (1–100) on the SQL query itself, returning only the requested slice plus total result count, `has_next`, `has_prev`, an RFC822 `last_modified` (newest `fetched_at` within the filtered set), and a weak `etag` derived from the normalized filter key + newest timestamp + total count.
- The endpoint SHALL support `If-None-Match` with the normalized ETag so cached views (including those with filters) receive `304 Not Modified` when no new stars matching that view arrived.

#### Scenario: Paginated filtered request only returns matching slice
1. **GIVEN** the database stores 120 stars including 60 Rust repos starred by user `alice` within the `high` activity tier
2. **WHEN** a client calls `/api/stars?language=Rust&user_mode=pin&user=alice&page=2&page_size=25&sort=alpha`
3. **THEN** the response includes exactly items 26–50 from the filtered result set, `meta.total=60`, `meta.has_prev=true`, `meta.has_next=true`, `meta.last_modified` equals the most recent `fetched_at` among those 60 records, and repeated requests with `If-None-Match` for that filter return `304` until the filtered data changes.

### Requirement: Derived Filter Options Endpoint
- The server SHALL implement `GET /api/options` that aggregates current languages, activity tiers, and star counts per user directly from SQLite, returning sorted lists with counts so clients can populate dropdowns without scanning all events locally.
- The response SHALL include `meta.etag` (hash of counts + updated_at) and honour `If-None-Match`, responding `304` when the derived sets are unchanged; `Cache-Control` SHALL be `public, max-age=300` to allow browser caching.

#### Scenario: Options reflect live aggregates
1. **GIVEN** the database contains stars from languages Rust (40) and Go (10) plus users `alice` (45 stars) and `bob` (5 stars)
2. **WHEN** a client requests `/api/options`
3. **THEN** the response lists languages `[{name:"Rust",count:40},{name:"Go",count:10}]`, users ordered by count, includes an ETag fingerprint, and a subsequent request with `If-None-Match` before data changes produces `304 Not Modified`.

### Requirement: Polling Status Endpoint
- The server SHALL expose `GET /api/status` with scheduler telemetry: `last_poll_started`, `last_poll_finished`, `is_stale` (true when `now - last_poll_finished` exceeds twice the configured refresh interval), `next_check_at` grouped by activity tier (high/medium/low/unknown), `last_error`, `rate_limit_remaining`, and `rate_limit_reset`.
- The endpoint SHALL set `Cache-Control: private, max-age=30, stale-while-revalidate=30`, reuse weak ETags, and return `304` when the serialized payload matches the client's `If-None-Match` header.

#### Scenario: Stale polling warning
1. **GIVEN** the last successful poll finished at `2025-10-24T10:00:00Z` and the server refresh interval is 30 minutes
2. **WHEN** a client requests `/api/status` at `2025-10-24T11:15:00Z`
3. **THEN** the response reports `last_poll_finished` accordingly, sets `is_stale=true`, surfaces next-check timestamps per tier, and returns the remaining rate-limit metadata so the UI can warn operators.
