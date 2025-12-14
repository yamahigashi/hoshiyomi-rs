# Proposal: Plan Frontend Startup Latency Improvements

## Why
Readers open the dashboard to quickly see what the GitHub accounts they follow just starred, yet the current `index.html` experience still stalls on first load. The UI shows "Loading…" while `/api/stars`, `/api/options`, and `/api/status` run sequentially; no data appears until the network returns. If the backend is still populating (common after a cold start) or `/api/stars` is slow, the page remains empty and controls stay unresponsive. We already optimized scrolling and pagination, but we have not addressed the “time to first useful render” problem, so the app fails its primary mission of keeping star activity immediately visible.

## What Changes
This plan focuses on removing the blank screen period by:
- Surfacing the most recent cached dataset instantly (warm cache) and clearly marking it as stale until the network refresh lands.
- Running the bootstrap requests in parallel with prioritisation (stars first, status/options secondary) and updating each region independently so controls become interactive sooner.
- Deferring non-critical work (virtualization measurements, preset wiring) until after the first paint and instrumenting the JS bundle so we can prove load-time improvements.

The result should cut perceived load time to <200 ms on a warm visit and give readers actionable feedback even if `/api/stars` is slow or temporarily offline.
