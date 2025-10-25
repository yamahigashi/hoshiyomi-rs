# Proposal: Plan Frontend Insight & Performance Refresh

## Overview
The dashboard already supports filtering and freshness cues, but it still behaves like a linear dump of `/api/stars`. Because the app’s primary mission is to keep readers up to date on what their followings star, we need to ensure the baseline experience (paging through that activity, preserving favorite views, keeping renders smooth) scales as history grows. This proposal defines the next wave of frontend improvements so the project’s delivery/observability focus area keeps up with the richer data we now ingest.

## Problems Observed
1. **One-off filter state** – `frontend/app.js` stores a single UI snapshot in `localStorage`, so hopping between “Rust-focused repos” and “High-activity users” requires manually re-tuning controls every time. Nothing maps to the spec’s “shareable, durable experiences” pillar.
2. **DOM thrash at scale** – `renderList` in `frontend/app.js` rebuilds the entire `<ul>` for every change. With a month of history (1k+ cards) the browser janks, undermining the feed-generation focus area because the UI cannot showcase everything we store.
3. **Pagination is client-only** – the current Next/Previous buttons simply slice the initially fetched array, so once `state.items` runs out the UI cannot reach older history even though `/api/stars` exposes `page`, `page_size`, `has_next`, and `has_prev`. This blocks readers from working through more than one chunk of data and wastes the backend’s pagination logic.

## Goals
- Let readers save and recall named view presets (plus deep links) to jump between workflows and share them with teammates.
- Keep interactions under 100 ms even when thousands of items exist by virtualizing the star list and streaming renders.
- Make pagination source-of-truth synced with `/api/stars`, including deep-linkable `page`/`page_size` parameters and back/next availability driven by API metadata.

## Non-Goals
- Replacing the existing `/api/stars` payload or adding server-side aggregation; the first iteration will compute metrics client-side.
- Recreating the dashboard with a component framework—this refresh extends the current vanilla JS stack.
- Designing mobile-only experiences beyond the responsive tweaks already in the spec (we will ensure new pieces remain responsive, but no mobile-specific nav overhaul).

## Success Measures
- Switching between at least three saved views (search + filters + pagination) takes <1 s and persists after reload/share.
- Pagination requests include `page`/`page_size` query params, only enable Next/Previous when the API reports `has_next`/`has_prev`, and successfully load >5 pages in manual QA without duplicating rows.
- Rendering 2k star items keeps main-thread blocks under 50 ms according to Lighthouse trace.
