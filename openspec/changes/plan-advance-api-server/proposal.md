# Proposal: Plan API Server Expansion

## Overview
Shape the next iteration of the Hoshiyomi backend so the web frontend and any future clients can rely on a cohesive API server instead of a thin static HTML bridge. The plan introduces filter-aware JSON endpoints, surfaced scheduler status, and shared server-side pagination so consumers do not need to replicate business logic in the browser.

## Current Challenges
- `/api/stars` streams the entire star corpus, leaving the browser to filter, sort, and paginate in-memory; slow networks or large datasets degrade UX and duplicate query rules.
- Operators lack an HTTP surface for scheduler health, last poll timestamps, or rate-limit state, making it hard to surface errors without reading logs.
- Conditional caching is limited to a single global ETag, so any filter change triggers a full download and prevents reuse by other clients.
- There is no documented contract for eventual API consumers (automations, alternative UIs), forcing them to mirror internal SQLite queries.

## Goals
1. Define a structured API surface that handles filtering, sorting, and pagination on the server, returning compact responses with metadata and caching primitives.
2. Expose lightweight service health endpoints so the frontend can present poll status, stale data warnings, and upcoming refresh times without bespoke hacks.
3. Align backend and frontend contracts through shared DTOs and validation so future features add rules in one place.
4. Keep the plan within existing technology choices (warp, rusqlite, Tokio) and GitHub rate-limit guidance from the research brief.

## Non-Goals
- Replacing SQLite or the ingestion scheduler.
- Introducing client authentication or multi-tenant accounts.
- Shipping GraphQL or streaming transports (SSE/WebSockets) in this iteration.
- Changing RSS output format beyond reusing new query helpers for feed building.

## Phased Approach
### Phase 1 – Queryable Star API
- Introduce a `GET /api/stars` contract that accepts query parameters for search, language, activity tier, user pin/exclude state, sort order, and pagination (page/page_size) with bounds from the spec (≤100 items).
- Return a JSON envelope containing `items`, `page`, `page_size`, `total`, `has_next`, `has_prev`, and an `etag` keyed by filters so conditional requests remain effective even when the view changes.
- Build query execution atop a dedicated repository module that composes SQL fragments safely and reuses existing indexes.

### Phase 2 – Service Metadata Endpoints
- Add a `GET /api/status` endpoint that reports the most recent poll timestamps, next scheduled poll per activity tier bucket, and last error (if any).
- Add a `GET /api/options` endpoint delivering derived filter options (languages, tiers, known users) so clients stop calculating these lists on every fetch.
- Ensure both endpoints carry caching headers and respect the scheduler’s adaptive intervals documented in the spec.

### Phase 3 – Contract Hardening & Tooling
- Define shared DTO structs (e.g., `StarListResponse`, `StatusSummary`) and validate them with integration tests covering default, filtered, and edge-case requests.
- Document the call patterns in README/specs and provide sample `curl` invocations; include a fixture JSON in `tests/` that the frontend can reuse during local development.
- Extend logging to trace API queries and rate-limit headers without exposing secrets, so ops can correlate frontend issues with backend workload.

## Risks & Mitigations
- **SQL complexity**: Dynamic filters can lead to brittle queries. Mitigate by using parameterized builders and unit tests for each combination.
- **Cache fragmentation**: Filter-specific ETags increase cache keys. Keep the key small (hash filter tuple) and document retention expectations.
- **Scope creep**: Additional endpoints (mutations, acknowledgements) may surface once clients exist. Constrain this change to read-only telemetry and document follow-up ideas.

## Alignment with Focus Areas
- **GitHub Data Collection**: Reuses the existing polling cadence and metadata, exposing it safely through status endpoints.
- **Storage Expectations**: Operates on the current SQLite schema; only adds read queries and optional covering indexes if measurements demand it.
- **Scheduling Guidance**: Surfaces scheduler-derived `next_check_at` so the frontend honours adaptive polling instead of guessing refresh windows.
- **RSS Output Requirements**: Feed generation can leverage the new repository module to stay consistent with API ordering.
- **When Choosing Libraries**: Continues with `warp`, `reqwest`, `tokio`, and `rusqlite`; no new heavy dependencies required.
- **Competitive Landscape Notes**: Differentiates by offering a reusable API layer rather than a single dashboard, making it easier to script personal automations compared to IFTTT/Huginn.

## Success Metrics
- Frontend requests shrink by ≥60% compared to downloading the entire star list thanks to pagination metadata.
- `/api/status` reflects poll freshness within 10 seconds of each completed cycle.
- End-to-end integration tests cover at least three filter combinations and pass without manual SQLite fixtures.
- Documentation includes API reference tables so external consumers can reproduce the frontend features without inspecting source code.
