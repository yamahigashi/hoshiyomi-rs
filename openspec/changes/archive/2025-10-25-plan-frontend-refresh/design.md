# Design: Frontend Insight & Performance Refresh

## Saved View Presets
- Keep the existing `UI_STORAGE_KEY` snapshot for “last session” continuity.
- Introduce a new `starchaser:viewPresets` array in `localStorage`, each entry containing name + parameters (`search`, `language`, `activity`, `sort`, `pageSize`, `density`, `userMode`, `userValue`).
- UI affordances:
  - “Save current view” button near the controls opens a small dialog (native `<dialog>` or inline form) to name the preset.
  - Render preset chips below the controls; clicking applies the stored snapshot, updates URL params, and sets `state.page = 1`.
  - Provide edit/delete actions per preset so the list stays manageable (cap at 5 entries to avoid clutter).
- Deep links: when a preset is applied, the URL already reflects parameters via `syncUrl()`. Allow copying the link; optionally expose a share icon that copies to clipboard.
- Keyboard support: number presets and allow quick recall via `Alt+1`..`Alt+5` (mirrors existing shortcut modal). Update the modal copy accordingly.

## Virtualized Star List
- Replace the naive `renderList(pageItems)` DOM rebuild with a windowed renderer:
  - Keep pagination logic but also support “infinite scroll” inside a page by only materializing ~40 cards at a time (configurable window of 2× page size).
  - Use a wrapper div with fixed-height spacers (simpler) or `IntersectionObserver` to recycle list items as the user scrolls; prefer the spacer approach for determinism.
- Performance targets: virtualization activates automatically when `state.items.length > 500` or when the chosen page size exceeds 50.
- Accessibility fallbacks:
  - Ensure focusable elements remain in DOM even when recycled by providing offscreen buffer or by only virtualizing non-focused rows.
  - Provide a `prefers-reduced-motion` check to disable animated placeholder transitions.
- Testing: add a JS unit (or integration) test that seeds 2k mock items and verifies scroll rendering stays under 60 frames.

## Server-Driven Pagination
- Treat `/api/stars` as the primary cursor source. Every fetch call should include the current filters, `page`, and `page_size` derived from `state`, and ingest both the returned `items` and `meta` payload (`total`, `has_next`, `has_prev`, `page`, `page_size`).
- Replace the purely client-side `state.page` adjustments with a `loadPage(deltaOrNumber)` helper that:
  - Checks `meta.has_next/has_prev` before enabling navigation controls.
  - Triggers a new fetch when the target page has not been cached yet, reusing ETag/`If-None-Match` for that parameter set.
  - Keeps a small client cache (e.g., most recent 2–3 pages) so returning to the previous page does not require a refetch unless filters changed.
- URL/state sync: continue to push `?page=` and `?pageSize=` params, but reset to `1` whenever filters/sorts/users/presets change to avoid requesting empty pages.
- Accessibility: announce page loads (`aria-live` status near pagination controls) and disable navigation buttons while the new page fetch is in flight.

## Copy & Visual Alignment
- Reuse existing typography/token system in `frontend/styles.css`. Insight chips and preset buttons inherit `.summary-chip` styles to avoid a brand-new palette.
- Keep the layout responsive by stacking the insight panel when `max-width < 720px` and hiding the sparkline when space is insufficient (but keep alt text).

This design keeps the implementation within the current vanilla JS + static asset pipeline while unlocking the requested UX gains.
