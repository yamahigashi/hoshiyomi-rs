# Design: API Server Expansion

## Problem Statement
The current backend exposes a minimal `GET /api/stars` endpoint that dumps the most recent events without honoring the filters and pagination rules the frontend applies client-side. As the dataset grows, downloading and processing every star in the browser introduces latency, duplicates logic, and blocks external consumers from reusing the service. Additionally, operators and the UI lack an HTTP-visible status summary for the polling pipeline or derived filter options.

## Proposed Architecture
1. **Query Abstraction Layer**
   - Add `src/query.rs` (or extend `db.rs`) with `QueryParams`/`QueryFilters` structs capturing `q`, `language`, `activity`, `user_mode`, `user`, `sort`, `page`, and `page_size`, plus a `NormalizedQueryKey` helper used for cache keys.
   - Compose SQL in stages: base `SELECT` from `stars` joined with `users` + dynamic `WHERE` clauses for search (tokenized `LIKE` across repo name/description/topics), language, activity tier, and user pin/exclude + `ORDER BY` (default `fetched_at DESC`, optional alphabetical by `repo_full_name`) + `LIMIT/OFFSET` derived from pagination.
   - Issue a paired `COUNT(*)` query (or use `COUNT(*) OVER()`) to produce the total row count without re-running filters; encapsulate this logic in a repository function that returns `(Vec<StarRecord>, PaginationMeta)`.
   - Introduce covering indexes (e.g., `(fetched_at DESC)`, `(repo_language, fetched_at DESC)`, `(user_id, fetched_at DESC)`) only if smoke tests or benchmarks show >15 % regression relative to today.
   - Build deterministic unit tests around the SQL builder by writing generated SQL strings to snapshots and executing them against a temporary SQLite database created via `tempfile::NamedTempFile`.
2. **API Envelope & Caching**
   - Standardize on response shape `{ meta: { page, page_size, total, has_next, has_prev, etag, last_modified }, items: [...] }` with DTOs `StarListResponse` and `StarListItem` deriving `Serialize`.
   - Compute `etag` as a weak hash of `(normalized filter tuple, newest_fetched_at, total)` using `sha1`/`xxhash` and format as `W/"stars-<hash>-<total>"`.
   - Emit `Last-Modified` based on newest `fetched_at` within the filtered subset and ship cache headers `Cache-Control: private, max-age=0` + `Vary: Accept, Accept-Encoding`.
   - Surface the same meta struct inside the RSS builder to keep ordering consistent across feed and API without duplicating SQL.
3. **Status & Options Endpoints**
   - `/api/status`: Combine `users.next_check_at`, scheduler heartbeat (`last_poll_started/ended`) persisted via a new `poll_runs` table or in-memory atomic updated by the scheduler, capture the latest rate-limit headers from `GitHubClient`, and shape a DTO `{ last_poll_started, last_poll_finished, is_stale, next_check_at, last_error, rate_limit_remaining, rate_limit_reset }`.
   - `/api/options`: Run aggregate queries (`SELECT repo_language, COUNT(*) ...`, `SELECT activity_tier, COUNT(*) ...`, `SELECT users.login, users.display_name, COUNT(*) ...`) with `GROUP BY` + deterministic sorting (desc count, then alpha) to build option lists.
   - Compute weak ETags for both endpoints using the payload bytes; `/api/status` uses the most recent poll timestamps/error tuple, while `/api/options` uses the aggregate hash. Cache headers: `status` → `Cache-Control: max-age=30, stale-while-revalidate=30`; `options` → `Cache-Control: max-age=300`.
   - Provide lightweight in-process caching by memoizing the latest payload + etag for the duration of a single request to avoid double-computing counts when reusing in other handlers.
4. **Shared DTOs & Validation**
   - Define request parser (`QueryParams`) using `serde` + `validator` (or manual checks) to enforce bounds and surface errors via RFC 7807 JSON problem responses.
   - Serialize responses with `serde_json`, ensuring `starred_at`/`fetched_at` remain RFC 3339 and `meta.last_modified` uses RFC 2822 as required by RSS tooling.
   - Provide integration tests under `tests/api_server.rs` using `warp::test` harness with seeded SQLite fixtures to exercise default, filtered, and error flows.

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
- Unit tests for SQL builder verifying generated WHERE/ORDER BY clauses, using deterministic fixtures and asserting both SQL text and executed results against temporary SQLite databases.
- Integration tests for `GET /api/stars` covering:
  - default view, search filter, pinned user, excluded user, language filter, pagination boundaries, and sort-by-alpha path.
  - validation failures for out-of-range `page`/`page_size` and unsupported `user_mode`.
  - conditional requests returning `304` when the `ETag` matches and updating `last_modified` when new items arrive.
- Integration tests for `/api/status` ensuring timestamps match simulated scheduler state, `is_stale` toggles once the oldest successful poll exceeds the configured threshold, and `503` is emitted before the first successful poll.
- Snapshot or structured assertion tests for `/api/options` verifying language and user counts ordering, `etag` stability, and conditional caching semantics.
- Extend the frontend-facing `tests/ui_contract.rs` (new) to deserialize the envelope JSON and validate it matches the documented contract for regression protection.

## Rollout Considerations
- Ship backend changes behind a `?preferServerFiltering=1` query flag consumed by the frontend during staged rollout.
- Maintain legacy array response for one release by negotiating via `Accept: application/vnd.hoshiyomi.v2+json`; switch frontend once stable.
- Monitor logs for query latency and adjust indexes or caching accordingly.
