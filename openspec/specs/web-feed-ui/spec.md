# web-feed-ui Specification

## Purpose
TBD - created by archiving change update-web-feed-experience. Update Purpose after archive.
## Requirements
### Requirement: Interactive Star List Controls
- The served HTML page MUST present the most recent items based on fetch time, ensuring newly ingested stars appear first regardless of their original `starred_at` value.

#### Scenario: Items ordered by fetch time
- **GIVEN** multiple star events with different `starred_at` values but identical fetch timestamps
- **WHEN** the dashboard loads
- **THEN** the events are ordered by `fetched_at` descending, so the last-ingested items appear first

### Requirement: Star Metadata Visibility
- Each star event in the web UI MUST show repository context along with the **fetch timestamp** so readers know when the data was ingested.

#### Scenario: Fetch timestamp is displayed
- **GIVEN** a stored star event includes a `fetched_at` timestamp
- **WHEN** the event is rendered in the web UI
- **THEN** the item shows the fetch timestamp (for example, “Fetched at 2025-10-23T04:15:00Z”) alongside existing metadata

### Requirement: Star Data API for Web UI
- The JSON response SHALL include `fetched_at` (RFC3339) and `ingest_sequence` (monotonic integer) alongside existing fields so clients can detect recency without re-computing.
- The endpoint SHALL emit an `ETag` derived from the newest `fetched_at` plus total item count and honour `If-None-Match` by returning `304 Not Modified` when nothing changed.

#### Scenario: Conditional fetch avoids re-download
- **GIVEN** the previous response included `ETag: W/"2025-10-23T05:00:00Z@50"`
- **WHEN** the UI calls `GET /api/stars` with `If-None-Match: W/"2025-10-23T05:00:00Z@50"`
- **THEN** the server returns `304 Not Modified` with no body and the client keeps its cached list.

### Requirement: Freshness Indicators for New Stars
- The web UI SHALL track the latest `fetched_at` the reader has acknowledged and visually distinguish any newer star entries until they are marked as seen.
- The UI SHALL display a “Last synced” timestamp and surface errors when background refresh fails, providing a manual retry affordance.

#### Scenario: Highlight new star since last visit
- **GIVEN** the most recent acknowledged `fetched_at` is `2025-10-23T05:00:00Z`
- **AND** the API returns an item with `fetched_at = 2025-10-23T05:07:00Z`
- **WHEN** the dashboard renders
- **THEN** that item is labeled as new until the reader clears the indicator, and the status line shows the sync time.

### Requirement: Persisted and Shareable Filters
- The web UI SHALL persist active search, language, activity, sort, density, page, page size, and user-selection settings between sessions using local storage.
- The web UI SHALL reflect the same settings in the page URL so copying the link restores the view for another reader.

#### Scenario: Deep link restores filter state
- **GIVEN** a user shares `/index.html?q=rust&lang=Rust&activity=high&sort=alpha&page=2&pageSize=25`
- **WHEN** another reader opens that URL
- **THEN** the dashboard loads with search “rust”, language `Rust`, activity `high`, alphabetical sort, page 2, page size 25, and the persisted state updates accordingly.

### Requirement: Paginated Star List Navigation
- The dashboard SHALL provide pagination controls (including page indicators and next/previous actions) so readers can browse large star lists without degraded performance.
- Pagination state SHALL reset to the first page when filters, sort order, or selected/excluded users change to avoid empty views.

#### Scenario: Navigating between pages
- **GIVEN** there are more than 25 items and the default page size is 25
- **WHEN** the reader activates “Next page” via mouse or keyboard shortcut
- **THEN** the dashboard updates the list to show items 26–50, updates the page indicator, and preserves other filters.

### Requirement: User Pin and Exclude Controls
- Clicking a star entry’s user handle (`#star-user`) SHALL cycle between three states: show only that user’s stars, exclude that user, and clear the selection.
- The UI SHALL expose matching controls accessible via keyboard and screen readers, and reflect the selection in URL/local storage state.

#### Scenario: Pinning and excluding a user
- **GIVEN** the dashboard currently shows stars from multiple users
- **WHEN** the reader activates the user handle for `alice`
- **THEN** only Alice’s stars remain visible and the URL reflects the pinned user
- **AND WHEN** the reader activates the handle again
- **THEN** Alice’s stars are hidden from the list while others remain visible until the filter is cleared.

### Requirement: Responsive Layout and Accessibility Enhancements
- The dashboard SHALL support a two-column card layout on viewports ≥1024px while retaining a single column on smaller screens, controlled by a density toggle.
- The dashboard SHALL expose keyboard shortcuts, skip links, and focus indicators that meet WCAG 2.2 AA contrast guidance, respecting `prefers-reduced-motion` preferences.

#### Scenario: Keyboard-only navigation remains functional
- **GIVEN** a reader uses only the keyboard on a desktop viewport ≥1024px
- **WHEN** they press `Tab`, `Shift+Tab`, `/`, and `?`
- **THEN** focus cycles through interactive controls in logical order, `/` focuses the search input, `?` opens the shortcuts help, and the grid layout adapts without breaking accessibility semantics.

### Requirement: Saved View Presets
- The web UI SHALL let readers save up to five named presets that capture the full filter/sort/page-size/density/user selection state and persist them in local storage.
- Presets SHALL render as buttons or chips beneath the controls, expose keyboard shortcuts (Alt+1…Alt+5), and update the URL/query parameters when applied so the view is shareable.
- Readers SHALL be able to rename or delete presets, and the shortcut modal plus ARIA labels SHALL describe the interactions for accessibility.

#### Scenario: Jumping between saved views
- **GIVEN** a reader saves two presets: “Rust” (`q=rust`, `lang=Rust`) and “High activity” (`activity=high`)
- **WHEN** they press `Alt+1` or click the “Rust” preset
- **THEN** the dashboard restores that filter combination, resets pagination to page 1, updates the URL to include the relevant query params, and stores it as the most recent state.

### Requirement: Virtualized Star Rendering
- The star list renderer SHALL virtualize rows whenever more than 500 items are available or the page size exceeds 50, ensuring no more than ~40 DOM cards exist at once while the scroll position remains accurate.
- Virtualization SHALL preserve keyboard navigation and screen-reader order by keeping focused elements mounted and announcing when additional rows load.
- The UI SHALL expose a progressive-loading indicator (e.g., “Loading more stars…”) while new windows mount so readers know the UI is working.

#### Scenario: Large history remains responsive
- **GIVEN** the database returns 2,000 star events
- **WHEN** the reader scrolls through the list with virtualization enabled
- **THEN** only a small window of cards is in the DOM, scrolling stays smooth (no noticeable jank), focus order remains sequential, and an inline status announces when more rows are fetched/rendered.

### Requirement: Server-Driven Pagination
- The dashboard SHALL request `/api/stars` with the current filters plus `page` and `page_size` parameters, using the API-provided `meta.total`, `meta.has_next`, and `meta.has_prev` to enable/disable pagination controls.
- Navigating to a page that is not already in the client cache SHALL trigger a fresh API call, update the URL/query parameters, and announce the new page via an `aria-live` status.
- The client SHALL cache at least the current, previous, and next pages so that going back/forward within that window does not require a refetch unless filters change.

#### Scenario: Fetching older pages
- **GIVEN** `/api/stars?page=1&page_size=25` returns `has_next=true`
- **WHEN** the reader clicks “Next”
- **THEN** the UI issues `/api/stars?page=2&page_size=25` (with the existing filters), disables pagination controls while loading, updates the list with the new results, and re-enables/announces the controls based on the returned metadata.

