## ADDED Requirements
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
