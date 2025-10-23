## MODIFIED Requirements
### Requirement: Star Data API for Web UI
- The JSON response SHALL include `fetched_at` (RFC3339) and `ingest_sequence` (monotonic integer) alongside existing fields so clients can detect recency without re-computing.
- The endpoint SHALL emit an `ETag` derived from the newest `fetched_at` plus total item count and honour `If-None-Match` by returning `304 Not Modified` when nothing changed.

#### Scenario: Conditional fetch avoids re-download
- **GIVEN** the previous response included `ETag: W/"2025-10-23T05:00:00Z@50"`
- **WHEN** the UI calls `GET /api/stars` with `If-None-Match: W/"2025-10-23T05:00:00Z@50"`
- **THEN** the server returns `304 Not Modified` with no body and the client keeps its cached list.

## ADDED Requirements
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
