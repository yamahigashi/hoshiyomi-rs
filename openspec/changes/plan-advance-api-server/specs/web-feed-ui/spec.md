## MODIFIED Requirements
### Requirement: Star Data API for Web UI
- The UI client SHALL request `GET /api/stars` with query parameters mirroring its search, language, activity tier, user pin/exclude, sort, page, and page size controls, relying on the server to filter and paginate results.
- The UI SHALL handle the JSON envelope `{ items, page, page_size, total, has_next, has_prev, etag }`, updating pagination controls and reusing `etag` via `If-None-Match` to avoid downloading unchanged pages.

#### Scenario: Server-side pagination drives UI state
- **GIVEN** the reader selects language `Rust`, pins user `alice`, and navigates to page 3 with page size 25
- **WHEN** the UI calls `/api/stars?language=Rust&user_mode=pin&user=alice&page=3&page_size=25`
- **THEN** the response envelope populates the grid with 25 records, sets the page indicator to 3, and provides `has_prev=true`, `has_next` reflecting availability, enabling navigation without client-side filtering.

## ADDED Requirements
### Requirement: Display Polling Status from API
- The UI SHALL fetch `/api/status` after each star list refresh to display the last successful poll time, next scheduled refresh window, and any error message returned by the backend.
- If `/api/status` reports stale data (no success within the configurable interval) the UI SHALL surface a prominent warning and offer a manual retry action that reissues both `/api/stars` and `/api/status` requests.

#### Scenario: Surfacing stale backend state
- **GIVEN** `/api/status` responds with `last_poll_finished = 2025-10-23T08:00:00Z` and `is_stale = true`
- **WHEN** the UI processes the response
- **THEN** it displays a warning banner explaining polling is stale, highlights the last successful time, and exposes a retry button wired to refresh both status and star data.

### Requirement: Server-derived Filter Options
- The UI SHALL request `/api/options` to populate its language quick filters, activity tier dropdown, and user pin/exclude suggestions, caching the response locally for the session.
- When the reader changes star criteria (e.g., clears filters or new data arrives) and the ETag for `/api/options` changes, the UI SHALL refresh the cached filter lists accordingly.

#### Scenario: Refreshing derived filters after new data
- **GIVEN** `/api/options` previously returned languages `["Rust", "TypeScript"]` with ETag `W/"opts-123"`
- **AND** a subsequent request returns ETag `W/"opts-456"` with an additional language `"Go"`
- **WHEN** the UI detects the ETag change
- **THEN** it updates quick filter chips to include Go and keeps language selections in sync with the new data.
