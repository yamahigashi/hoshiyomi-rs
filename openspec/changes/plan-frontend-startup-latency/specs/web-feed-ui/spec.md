## ADDED Requirements
### Requirement: Warm Start Snapshot
- The dashboard SHALL persist the most recent `/api/stars` response (items + meta + timestamp) locally and render that snapshot within 200 ms on subsequent visits before issuing a network request.
- Cached renders SHALL display a "Cached" or "Updating…" badge plus the last fetched timestamp so readers know the view may be stale until the live refresh completes.
- The client SHALL discard snapshots older than 24 hours or tied to outdated filters to avoid presenting irrelevant data.

#### Scenario: Immediate cached render
- **GIVEN** the reader opened the dashboard earlier the same day and a cached `/api/stars` payload exists for the default filter set
- **WHEN** they reopen `index.html`
- **THEN** the persisted items appear within 200 ms, a "Cached • refreshing" badge is shown, and the UI swaps to the freshly fetched data once `/api/stars` completes.

### Requirement: Parallel Bootstrap Without Blocking Controls
- On startup the UI SHALL fire `/api/stars`, `/api/status`, and `/api/options` concurrently, updating each region as soon as its data arrives instead of waiting for all calls to finish.
- Bootstrap requests SHALL use `AbortController` (or equivalent) to cancel obsolete fetches, enforce a timeout fallback to cached data, and keep search/filter inputs interactive throughout the refresh.
- Non-critical scripts (modal wiring, virtualization measurement, metrics collection) SHALL defer until after the first paint so readers can type or navigate immediately.

#### Scenario: Stars slow, controls stay responsive
- **GIVEN** `/api/stars` takes 5 seconds to respond while `/api/options` and `/api/status` return quickly
- **WHEN** the dashboard loads
- **THEN** filter dropdowns populate from the completed calls, the cached star list remains visible with a "refreshing" badge, and the search input accepts text without lag until the fresh `/api/stars` data arrives or times out.
