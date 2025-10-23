# Design Notes

- Remain framework-free: continue using a self-invoking module written in ES2020, split into logical helpers during build (bundled via `include_str!` if growth demands). Introduce a tiny state module to own persisted filters, pagination, active dataset, and highlight metadata. This module exposes `loadState()`, `applyFilters()`, and `persistState()` to keep concerns separated from DOM rendering.
- State persistence: mirror filters (`search`, `language`, `activity`, `sort`, `density`, `page`, `pageSize`) into both `localStorage` (for default) and query params (for shareable links). When parsing incoming URLs, validate entries against server-provided filter options before applying.
- Keyboard shortcuts: bind using a simple keymap table; ensure shortcuts are no-ops when focus is in an input to avoid conflicts. Provide help tooltip listing shortcuts.

## Asset Bundling
- Move existing inline template into `frontend/index.html`, `frontend/css/main.css`, `frontend/js/app.js`, and optional partials (e.g., `frontend/js/state.js`).
- Add `build.rs` to concatenate/minify (optional) and emit a single `generated/index.html` plus hashed asset metadata into `OUT_DIR`. Rust code loads assets via `include_str!(concat!(env!("OUT_DIR"), "/index.html"))` to keep a single-binary deploy.
- In development builds, gate an environment variable (e.g., `HOSHIYOMI_DEV_ASSETS`) that serves files directly from the filesystem to avoid rebuilding on every edit.
- Ensure CI runs a check (`cargo run --bin check-assets`) that regenerates the bundle and verifies no diff to prevent stale embedded content.

## Data & HTTP Contract
- Extend `GET /api/stars` payload schema with `fetched_at` (RFC3339) and `ingest_sequence` (monotonic integer derived from `rowid`) so the client can sort new items without ambiguity. Both fields already exist server-side; expose them explicitly.
- Server should emit `ETag` based on the latest fetched timestamp + count (e.g., `W/"<last_fetched>@<count>"`) and honour `If-None-Match` by returning `304` with empty body when nothing changed. This keeps background refresh cheap.
- Introduce optional `Last-Modified` header mirroring the newest `fetched_at` for browser caches.

## Background Refresh Loop
- Client timer defaults to 5 minutes (configurable via data attribute). Loop performs `fetch('/api/stars', { headers: { 'If-None-Match': etag } })`. If 304, only update “Last checked” timestamp; if 200, re-render and update stored etag/new item markers.
- Detect failures: on network error or ≥500 response, surface alert banner with exponential backoff (e.g., 1→2→4→8 minutes) while allowing manual retry.
- Highlight behaviour: store `lastAcknowledgedFetchedAt` in `localStorage`; cards with `fetched_at` greater than this value receive a “New” badge until the reader dismisses or toggles a “Mark all seen” control.

## Layout & Accessibility
- Responsive grid: at ≥1024px, render cards in two columns using CSS grid with equal heights; below that, stay single column. Density toggle switches between card spacing variables.
- Accessibility: add skip link, ensure ARIA live region only announces counts when they change, and provide `prefers-reduced-motion` media queries to disable highlight transitions. Focus outlines must meet 3:1 contrast. Provide text alternatives for icons.
- Shortcut discoverability: include a dialog listing controls triggered by `?` key. Respect `Escape` to close overlays. Add shortcuts for pagination (e.g., `[`/`]` for prev/next page) and user selection toggles.

## Pagination & User Selection
- Pagination lives purely on the client: compute visible slice based on `state.page` and `state.pageSize`. Provide page indicators, previous/next buttons, and an optional page-size dropdown (default 25). Ensure ARIA attributes announce page changes.
- Clicking `#star-user` toggles a selection pill; the first click pins that user (show only their stars), a second click cycles to “exclude,” and a third clears the filter. Provide explicit UI chips to manage selections for mouse-free control.
- When a user filter is active, update URL params (`user`, `exclude`) and ensure pagination resets to the first page to avoid empty views.

## Testing & Tooling
- Add unit coverage for state manager (URL⇆state round trips, highlight persistence) via `wasm-bindgen-test` or headless DOM tests using `wasm-pack test` alternative (consider `wasm-bindgen-test` or `jest` via `node`).
- Build static HTML fixtures representing desktop/mobile variants. Use `cargo test` to invoke `insta` snapshots (or alternative) comparing rendered DOM for regression.
- Instrument logging: emit `console.info` summaries for refresh results in development; gate behind `data-debug` attribute so production remains quiet.
