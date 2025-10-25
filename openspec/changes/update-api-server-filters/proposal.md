# Proposal: Expand API Server Filtering

## Summary
Elevate the HTTP server from a feed-only shim to a reusable API that powers the dashboard and future clients. The change unlocks cursorless pagination, server-side filtering, and richer status/options telemetry so `/` and `/api/*` scale with the number of stored stars instead of the `feed_length` cap.

## Background
- `GET /api/stars` currently returns at most `feed_length` records (default 50) because it reuses the RSS query helper. Pagination parameters exist in the route, but the handler slices an in-memory vector, so page 2+ immediately runs out of data.
- Filter query parameters (`q`, `language`, `activity`, `user_mode`, `user`) are ignored, forcing the frontend to simulate filtering client-side or leave controls disabled.
- `/api/options` and `/api/status` return stub payloads. The web UI cannot surface scheduler timing, list of languages, or user counts without duplicating SQL.
- Index HTML renders the same limited slice as `/api/stars`, so “APIサーバ化” is blocked until the backend can answer arbitrary slices of the dataset.

## Goals
1. Teach `/api/stars` to execute SQL that honours query params for search, language, activity tier, user pin/exclude, sort order, page, and page size (≤100), returning metadata needed for pagination.
2. Have `/api/options` derive languages, activity tiers, and per-user frequencies from the SQLite tables so the frontend and other clients can bootstrap filter state without scanning everything locally.
3. Populate `/api/status` with scheduler progress (last poll start/finish, per-tier `next_check_at`, rate-limit headroom, last error) so dashboards can show freshness and retry guidance.
4. Wire index HTML and RSS generation to the new query helpers so any future API consumers share the same contract.

## Non-Goals
- Changing the SQLite schema or ingestion pipeline mechanics (we will add read queries and optional indexes only if needed for performance).
- Introducing cursor-based pagination or streaming transports.
- Implementing authentication/authorization for the API.
- Reworking frontend UX beyond enabling the dormant controls.

## Approach
- Create a `db::StarQuery` builder that translates filter sets into parameterised SQL with total-count queries and filter-specific ETags (hash of params + newest `fetched_at` + total).
- Update `/api/stars` to call the builder, stream results directly from the database, and expose `meta` with total counts, navigation booleans, and `last_modified` derived from the newest record in the filtered set.
- Expand `/api/options` to aggregate languages, activity tiers, and user counts from SQLite, returning sorted lists plus an ETag/hash so clients can cache.
- Surface scheduler snapshots from existing tables/cron bookkeeping via `/api/status`, marking the payload as stale when the last-success delta exceeds the refresh interval.
- Reuse the new query layer inside HTML rendering (and optional static bundles) so `index.html` can request arbitrary pages via the API instead of embedding the limited slice.

## Validation
- Add integration tests covering `/api/stars` pagination, filter combos, and `304` caching semantics.
- Assert `/api/options` and `/api/status` return realistic data by seeding temporary databases in tests.
- Exercise the new query builder via unit tests that assert generated SQL fragments and parameter bindings for representative scenarios.
- Run `openspec validate update-api-server-filters --strict` before requesting review.
