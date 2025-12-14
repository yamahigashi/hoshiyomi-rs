# Tasks
1. [ ] Baseline current startup timings (cold/warm) by instrumenting `performance.now()` around `loadPage()` and recording the blank-screen duration.
2. [ ] Implement the warm-cache snapshot (persist last `/api/stars` response + meta/etag/timestamp) and render it within 200 ms on load, including UI affordances that mark cached data as updating.
3. [ ] Add the parallel bootstrap scheduler (simultaneous `/api/stars`, `/api/status`, `/api/options` with AbortController + timeout fallbacks) and ensure each region updates independently without blocking controls.
4. [ ] Defer non-critical modules (virtualization measurements, preset dialog wiring, shortcut modal) behind `requestIdleCallback`/`setTimeout` so typing/searching stays responsive during refresh.
5. [ ] Add instrumentation hooks (performance marks + optional console metrics) and update developer docs with guidance on reading them.
6. [ ] Update `tests/frontend_snapshot.html`, add/adjust JS tests to cover cache hydration + stale indicators, and run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
