# Design: Frontend Startup Latency Improvements

## Warm Cache Snapshot
- Persist the most recent `/api/stars` payload (items + meta + ETag + fetched_at timestamp) in `localStorage` or IndexedDB under a versioned key.
- On load, synchronously hydrate the cache (if <24 h old and matches the current filter signature) before any network call so the list renders within the first frame.
- Display a subtle "Cached • updating…" badge while a background refresh runs; clear it once fresh data arrives or highlight staleness if the cache exceeds a configured TTL.
- Store only the first page needed for the active presets to avoid ballooning storage; respect existing server-driven pagination cache to keep logic consistent.

## Parallel Bootstrap Pipeline
- Fire `/api/stars`, `/api/status`, and `/api/options` concurrently with `Promise.allSettled`, but render each region (list, status bar, quick filters) as soon as its promise resolves.
- Introduce a bootstrap scheduler that promotes `/api/stars` to the highest priority, cancels obsolete requests using `AbortController`, and falls back to the cached payload if the network call exceeds a timeout (e.g., 4 s) or returns 5xx.
- Keep the controls interactive by shipping a lean event-binding phase before heavier modules (virtualization, preset dialog, shortcut modal) are imported/run. Use `requestIdleCallback` (with a timeout fallback) to finish optional work without blocking typing/filtering.

## Instrumentation & Guardrails
- Add lightweight performance marks (NavigationStart → CachedRender, CachedRender → FreshRender) and log them (guarded behind `?debug=perf`) so we can verify the <200 ms cached render target.
- Track cache hit rate, bootstrap durations, and error states via counters surfaced in the dev console (future work could send them to `/api/metrics`).
- Document fallback order: cached payload → stale network response → empty state with actionable retry, ensuring the UI never regresses to a blank shell.
