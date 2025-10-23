# Proposal: Enhance Web Feed UI

## Overview
Evolve the existing `--serve` web experience from a static RSS preview into a lightweight, single-user dashboard that makes it easy to explore starred repositories. The upgraded page will support in-browser search, filtering, and sorting of recent star events, while surfacing richer repository details such as description, primary language, and topics.

## Motivation
The current HTML page mirrors the RSS feed: a static, chronologically ordered list without affordances to sift through activity. As the number of followed accounts grows, users need faster ways to find specific repositories, focus on languages they care about, or distinguish between high-volume star authors and occasional users. Exposing more metadata directly in the UI aligns the experience with what RSS readers already display, reducing the need to click into GitHub for basic context.

## Goals
- Provide interactive controls on the served HTML page for text search, language filtering, switching between chronological and alphabetical ordering, and filtering by user activity bands (e.g., high/medium/low frequency star authors).
- Display additional metadata for each star event (repository description, primary language, topics, starred timestamp) in an easy-to-scan layout.
- Keep the implementation single-user and lightweight—no authentication, no persistent sessions, minimal client-side scripting.
- Maintain parity with RSS content: the same star records power both the RSS feed and the HTML view.

## Non-Goals
- Multi-user access control or personalization per follower.
- Visual redesign beyond a clean list with basic styling.
- Replacing the RSS endpoint or adding Atom output (those remain untouched).
- Real-time push updates; polling cadence stays managed server-side.

## High-Level Approach
1. Extend the GitHub ingest pipeline and SQLite schema to capture repository language, topics, and per-user activity bands (derived from historical star cadence) so the UI can render them without extra API calls.
2. Introduce a small JSON endpoint (e.g., `GET /api/stars`) that returns the recent star events for client-side filtering; reuse the same data accessor as the RSS builder.
3. Replace the static HTML renderer with a template that delivers the base layout, search/filter controls (including activity-level filters), and a bundled script handling in-memory filtering, sorting, and rendering.
4. Ensure the web page remains performant by limiting the payload (respecting current feed length configuration) and by reusing cached data instead of triggering GitHub requests per interaction.

## Risks & Mitigations
- **Schema migration complexity**: Adding new columns requires backfilling existing rows. We'll write a lightweight migration that defaults language/topics to `NULL` and lets new polling runs populate them.
- **Client-side payload size**: Including topics may inflate the JSON response. We'll cap topics to a reasonable limit per repo (e.g., first 10) and rely on gzip via Warp defaults.
- **API compatibility**: Fetching topics requires preview headers. We'll gate feature detection so the topic list gracefully degrades if GitHub omits data.

## Alignment with Focus Areas
- **GitHub Data Collection**: Extends stored metadata while honoring conditional requests and rate limits; no additional endpoints beyond existing starred calls.
- **Storage Expectations**: Adds normalized fields to the `stars` table, keeping the DB authoritative for both RSS and HTML rendering.
- **RSS Output Requirements**: RSS stays authoritative; the new UI reads from the same dataset, ensuring consistency across channels.
- **Scheduling Guidance**: Polling cadence logic stays intact; interactive UI uses cached data, so no new pressure on scheduling.

## Open Questions
- None at this time; requirements are scoped to single-user interactive exploration of the existing feed data.
