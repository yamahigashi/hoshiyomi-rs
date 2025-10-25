## MODIFIED Requirements
### Requirement: Star Data API for Web UI
- The dashboard client SHALL send `/api/stars` requests that mirror its active filters (`q`, `language`, `activity`, `user_mode`, `user`, `sort`, `page`, `page_size`) and rely on the server to perform filtering, sorting, and pagination instead of trimming data browser-side.
- The client SHALL bind pagination controls to the `meta` envelope returned by the API (`page`, `page_size`, `total`, `has_next`, `has_prev`, `etag`, `last_modified`) and reuse the weak `etag` via `If-None-Match` so filtered views avoid re-downloading unchanged payloads.

#### Scenario: Filtered view updates purely via API metadata
- **GIVEN** the reader filters by language `Rust`, activity `high`, pins user `alice`, and switches to alphabetical sort on page 2 with page size 25
- **WHEN** the UI issues `/api/stars?language=Rust&activity=high&user_mode=pin&user=alice&sort=alpha&page=2&page_size=25`
- **THEN** the response populates exactly 25 rows for that view, the UI sets its pagination indicator from `meta.page`, enables “Prev” because `meta.has_prev=true`, disables “Next” when `meta.has_next=false`, and caches the response using `meta.etag` for subsequent conditional GETs.

### Requirement: UX-Friendly API Envelopes
- The frontend SHALL receive responses that always include the normalised query echo plus pagination metadata even when no items match, enabling empty-state rendering without guessing.
- `meta` SHALL expose `total`, `has_prev`, `has_next`, `page`, and `page_size` values consistent with the server-selected slice so keyboard navigation and skip links stay accurate; the UI SHALL surface zero-result states using this data instead of recomputing totals client-side.

#### Scenario: Empty result preserves navigation context
- **GIVEN** a reader applies filters that yield no matching stars (e.g., `language=Zig`, `activity=high`)
- **WHEN** the UI receives the `/api/stars` response
- **THEN** `items` is an empty array while `meta.page` echoes the requested page, `meta.total=0`, `meta.has_prev=false`, `meta.has_next=false`, and the UI renders a zero-state message with the same pagination controls (disabled) for consistency.
