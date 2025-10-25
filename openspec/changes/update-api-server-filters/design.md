# Design: Filter-aware API Server

## Overview
We refactor the read surface so every consumer (HTML, RSS, JSON API) pulls data from one query module that understands filters, pagination, and cache signals. This removes the tight coupling between `feed_length` and API responses and gives us a single place to optimise SQLite access.

## Components
### StarQuery Builder
- Lives under `db::star_query` and exposes two async helpers:
  - `fetch(params: StarQueryParams) -> StarQueryResult` returning rows + total + newest timestamps.
  - `options_snapshot() -> OptionsSnapshot` for languages/activity/users.
- Accepts normalized parameters (search text, language, activity tier, pin/exclude + user list, sort order, page, page size).
- Generates SQL with optional WHERE clauses and ORDER BY derived from sort. Pagination uses `LIMIT ? OFFSET ?` with validated bounds.
- Total count runs as a separate query sharing the same filters to avoid windowing errors.
- Computes `etag_seed = sha256(normalized_params + newest_fetched_at + total)` so clients can reuse caches even when filters change.

### Status Telemetry
- Scheduler already records `last_poll_started`, `last_poll_finished`, `last_error`, `rate_limit_remaining`, `rate_limit_reset`, and `next_check_at` per activity tier. Expose a lightweight struct that `/api/status` serializes directly.
- `is_stale` derives from `(now - last_poll_finished) > refresh_interval * 2`. This matches the “warn readers before staleness exceeds one cycle” expectation.

### Options Aggregation
- SQL view aggregates languages, activity tiers, and users from `stars` and `users` tables with `COUNT(*)` metrics. We can cache results in memory for a few seconds if profiling shows repeated hits, but spec only requires deterministic ordering plus caching headers.

### Routing Changes
- `/api/stars` loads directly from `StarQuery`, returning the struct as JSON. `index.html` fetches `/api/stars` instead of embedding data so pagination/filtering all come from the API.
- `/api/options` and `/api/status` respond with real structs and proper caching headers (`private, max-age=0` for sensitive data; `public, max-age=300` for options).

## Data Flow
1. Client requests `/api/stars` with query params → warp handler normalises params and calls `StarQuery::fetch`.
2. Builder executes parameterised SQL, returning rows + totals + newest timestamp.
3. Handler maps rows to DTOs, computes `etag`, and sets cache headers before replying.
4. `/api/options` and `/api/status` follow similar path, using their own queries/structs but the same caching helper to keep HTTP semantics aligned.

## Frontend Usability Considerations
- **Deterministic envelopes:** Every response mirrors the request parameters (normalised) inside the `meta` block so the UI can reconcile optimistic navigation with the data that actually shipped.
- **Empty-state friendliness:** Even when no stars match a filter, the API returns an empty `items` array plus `meta.total=0`, `has_prev=false`, `has_next=false`, and the original `page` value so the UI can show a zero-results card without guessing.
- **Quick filter hydration:** `/api/options` includes both machine identifiers (language slug, login) and human-safe labels plus counts. This lets the frontend show badges and sort chips without running additional queries.
- **Latency masking:** Cache headers (ETag + short-lived max-age) allow the browser to reuse filtered views and option lists, keeping pagination snappy while slow database queries are avoided when nothing changed.
- **Error surfacing:** `/api/status` payloads are shaped for direct rendering (ISO strings, boolean `is_stale`, optional `last_error`). The frontend can drop the JSON straight into banners/tooltips without extra parsing logic.

## Risks
- SQL builder complexity: mitigate with unit tests for `WHERE`/`ORDER BY` permutations.
- Performance: ensure indexes exist on `repo_language`, `fetched_at`, `user_id`, `activity_tier`. Add indexes only if measurements show slow queries.
- Cache correctness: hashed ETags must include all filter inputs; we will reuse the normalized key from query params to avoid omissions.
