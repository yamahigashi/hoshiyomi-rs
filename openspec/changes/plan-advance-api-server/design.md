# Design: API Server Expansion

## Problem Statement
The current backend exposes a minimal `GET /api/stars` endpoint that dumps the most recent events without honoring the filters and pagination rules the frontend applies client-side. As the dataset grows, downloading and processing every star in the browser introduces latency, duplicates logic, and blocks external consumers from reusing the service. Additionally, operators and the UI lack an HTTP-visible status summary for the polling pipeline or derived filter options.

## Proposed Architecture
1. **Query Abstraction Layer**
   - Add `src/query.rs` (or extend `db.rs`) with functions like `fetch_stars(QueryParams)` that translate filter structs into parameterized SQL.
   - Compose SQL in stages: base SELECT + dynamic WHERE clauses (search, language, activity, user mode) + ORDER BY (ingest sequence vs alphabetical) + LIMIT/OFFSET.
   - Introduce covering indexes (e.g., `(fetched_at DESC)`, `(repo_language, fetched_at DESC)`) if benchmarks show >15% regression.
2. **API Envelope & Caching**
   - Standardize on response shape `{ meta: { page, page_size, total, has_next, has_prev, etag }, items: [...] }`.
   - Compute `etag` as a weak hash of `(filter tuple, newest_fetched_at, total)` ensuring two distinct filter sets never collide.
   - Emit `Last-Modified` based on newest `fetched_at` within the filtered subset.
3. **Status & Options Endpoints**
   - `/api/status`: Combine `users.next_check_at`, scheduler heartbeat (`last_poll_started/ended`), and last error message.
   - `/api/options`: Query distinct languages, activity tiers, and user handles plus counts for quick-filter chips.
   - Both endpoints share a lightweight caching layer (e.g., compute fresh data per request, then add short `Cache-Control: max-age=30` for downstream caches).
4. **Shared DTOs & Validation**
   - Define request parser (`QueryParams`) using `serde` + `validator` to enforce bounds.
  - Serialize responses with `serde_json`, ensuring `starred_at`/`fetched_at` remain RFC3339.
   - Provide integration tests under `tests/api_server.rs` using `warp::test` harness.

## Data Flow
```
HTTP Request -> warp filter parser -> QueryParams -> query::build_sql(QueryParams)
             -> rusqlite prepared statement -> StarRecord list -> map to DTO
             -> ResponseEnvelope -> warp reply
```

## Open Questions
- Do we need rate-limit headers mirrored onto `/api/status`? (Default: yes, if GitHub response exposes them.)
- Should `/api/options` include topic aggregates or defer to future change?
- How do we represent scheduler errors (single string vs structured list)? Proposal: latest error only to keep response light.

## Testing Strategy
- Unit tests for SQL builder verifying generated WHERE/ORDER BY clauses.
- Integration tests for `GET /api/stars` covering:
  - default view, search filter, pinned user, excluded user, language filter, pagination boundaries.
  - conditional request returning 304 when ETag matches.
- Integration tests for `/api/status` ensuring timestamps match simulated scheduler state and stale polls mark `is_stale = true` after configurable threshold.
- Snapshot test for `/api/options` verifying language counts order and user display names.

## Rollout Considerations
- Ship backend changes behind a `?preferServerFiltering=1` query flag consumed by the frontend during staged rollout.
- Maintain legacy array response for one release by negotiating via `Accept: application/vnd.hoshiyomi.v2+json`; switch frontend once stable.
- Monitor logs for query latency and adjust indexes or caching accordingly.
