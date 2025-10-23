# Proposal: Plan Frontend Experience Enhancements

## Overview
Define the next wave of improvements for the single-page dashboard served at `/`, focusing on faster comprehension of new stars, richer personalization, and stronger accessibility. The plan keeps the implementation lightweight (no build tooling or frameworks) while upgrading the existing HTML+JS bundle.

## Current Challenges
- Readers have no indication of what changed since their last visit; the list always looks the same even after a fresh poll.
- Filter combinations reset on reload and cannot be shared, making repeat workflows tedious for heavy users.
- The layout is optimized for a single-column scroll; on wide screens it wastes space, and on keyboards it lacks obvious focus cues.
- The UI polls once on load. Operators must refresh the entire page to see new items, with no visibility into when the data became stale or if the poll failed.
- Long feeds require extensive scrolling; there is no pagination or quick way to focus on a single follower’s activity.

## Goals
1. Surface recency cues (highlighting new stars, staleness indicators) so users immediately know what’s new.
2. Persist and share filter configurations (query, language, activity level, sort, pagination state) through local storage and deep links.
3. Modernize layout and accessibility with responsive sections, keyboard navigation, skip links, and reduced-motion friendly animations.
4. Add light-touch background refresh with error handling that respects GitHub rate guidance by reusing cached data and conditional headers.

## Non-Goals
- Moving the UI to a SPA framework or adding a build step.
- Multi-tenant personalization or authenticated user accounts.
- Real-time WebSocket updates; polling remains the mechanism.
- Reworking the RSS feed or server architecture beyond the endpoints already shipped.

## Phased Approach
### Phase 1 – Template Separation & Freshness
- Extract the HTML/CSS/JS bundle from `feed.rs` into a dedicated `frontend/` directory, wiring build-time inclusion (`include_str!`) so assets remain self-contained while editable per file.
- Extend the `GET /api/stars` response with `fetched_at` metadata (documented in spec) and return strong ETags so the front-end can issue conditional fetches without excess load.
- Add client-side polling every N minutes (configurable in JavaScript) with a visible “Last synced” timer, error banner, and retry button.
- Highlight cards fetched since the last visit using `localStorage` (or session) to store the most recent `fetched_at` the reader acknowledged.

### Phase 2 – Navigation & Personalization
- Persist active filters and sort order in the URL query string (`?q=&lang=&activity=&sort=`) so links can be bookmarked or shared.
- Provide quick-filter chips for top languages and activity tiers so readers can switch views with a single tap.
- Add compact list density toggle and two-column layout for screens ≥1024px while maintaining a single column on mobile.
- Introduce client-side pagination with configurable page size and in-page navigation controls so large feeds remain manageable.
- Support clicking a star-card user handle (`#star-user`) to pin or exclude that user from the current view.

### Phase 3 – Accessibility & Performance Polish
- Introduce keyboard shortcuts (e.g., `/` focuses search, `l` cycles languages) and visible focus states aligned with WCAG 2.2 AA contrast ratios.
- Add a “Skip to results” link, ARIA live-region refinements, and reduced-motion preferences for highlight animations.
- Lazy render topic chips beyond the first five per repository to keep DOM light for large feeds.

## Risks & Mitigations
- **Increased API polling**: Background refreshes could hit GitHub more often. Mitigate by honoring `ETag`/`If-None-Match` and backing off when responses repeat.
- **State complexity**: Persisting filters across sessions introduces edge cases. Document state transitions and centralize in a small state manager module.
- **Layout regressions**: Responsive changes risk breaking existing readers. Add Storybook-style snapshots (via static HTML fixtures) and responsive tests in CI where feasible.
- **Asset management drift**: Splitting templates into `frontend/` risks mismatched versions if not wired into the build. Use `build.rs` to fingerprint assets, enforce CI checks ensuring generated bundle matches committed source.

## Alignment with Focus Areas
- **GitHub Data Collection**: Reuses existing star payloads and conditional headers; no new API endpoints beyond returning stored `fetched_at` and ETags.
- **Storage Expectations**: Relies on current SQLite schema; only clarifies that `fetched_at` is part of the UI contract—no new tables.
- **Scheduling Guidance**: Background refresh respects adaptive polling intervals by caching responses and avoiding server-triggered fetch bursts.
- **RSS Output Requirements**: RSS remains source of truth; UI continues to read from the same dataset and mirrors RSS ordering semantics.
- **When Choosing Libraries**: Continues with vanilla TypeScript-free bundle; if a helper library is needed (e.g., `htm`), ensure it is lightweight and justified.
- **Competitive Landscape Notes**: Differentiates from GitHub’s default feeds by providing freshness cues, saved filters, and accessibility polish unavailable in IFTTT/Huginn workflows.

## Success Metrics
- “New since last visit” indicator appears within 1 second of data load and can be dismissed.
- Saved links reproduce filter state with no more than one extra fetch.
- Lighthouse accessibility score ≥90 on both desktop and mobile snapshots.
- Background refresh runs at default 5-minute cadence without increasing GitHub API error rates in logs.
