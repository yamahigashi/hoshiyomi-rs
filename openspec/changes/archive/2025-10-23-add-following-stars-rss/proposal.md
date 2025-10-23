# Proposal: Following Stars RSS Feed

## Overview
We will build a Rust-based service that aggregates the repositories starred by accounts the authenticated user follows on GitHub and exposes the events through an RSS feed. The proposal codifies the research findings documented in `spec.pdf` and establishes the minimum behaviour, storage model, and runtime constraints required for implementation.

## Problem
Today there is no automated way in this codebase to surface the repositories starred by the user's followings. Existing third-party automations (e.g., IFTTT, Huginn) either lost support for star triggers or demand brittle external integrations. We need a first-party capability that respects GitHub rate limits, keeps historical star data, and publishes an RSS feed consumable by any reader.

## Goals
- Collect followings and their starred repositories using GitHub's REST API while staying inside primary and secondary rate limits.
- Persist star events and per-user metadata so we can detect deltas, avoid duplicates, and adapt polling frequency.
- Generate a standards-compliant RSS 2.0 feed ordered by the time each star was created.
- Provide scheduling rules so high-activity users refresh frequently without wasting API calls on dormant accounts.

## Non-Goals
- Delivering a web UI or hosted feed reader.
- Supporting non-GitHub sources or private organization policies beyond the authenticated user's token permissions.
- Building push notifications or webhooks; this scope is strictly pull-based polling.

## Background & Research Highlights
- GitHub REST endpoints `GET /user/following` and `GET /users/{login}/starred` (with `Accept: application/vnd.github.star+json`) return the needed data, including `starred_at` timestamps.
- Conditional requests (`If-None-Match`, `If-Modified-Since`) combined with cached `ETag` values prevent rate-limit usage when nothing changed.
- Throttled concurrency (e.g., semaphore limiting ~5 simultaneous requests) plus backoff informed by `X-RateLimit-Remaining` mitigates primary and burst rate limiting.
- SQLite plus a two-table layout (`users`, `stars`) provides a compact, queryable history that scales to thousands of users and tens of thousands of events.
- Adaptive polling based on historical activity (minimum ~10 minutes, maximum ~7 days) preserves freshness for active users while reducing wasted calls.
- RSS generation should rely on the `rss` crate (or `rss-gen` if required) to avoid brittle hand-crafted XML, and each item must include title, link, GUID, and `pubDate`.
- Comparable tools (GitHub Atom feeds, Astral, Star History) only partially solve the problem and reinforce the need for a bespoke implementation.

## Proposed Approach
1. Build a command-line worker that authenticates via personal access token and orchestrates polling cycles.
2. Implement a GitHub client wrapper using `reqwest` (or `octocrab` where beneficial) with rate-limit aware request scheduling.
3. Persist responses into SQLite through `rusqlite`, updating both star events and per-user polling metadata.
4. Calculate polling intervals dynamically based on recent star activity and queue only the users whose `next_check_at` is due.
5. Produce the RSS feed from stored events, sorted descending by `starred_at`, and expose it as a file or stdout artefact.

## Risks & Mitigations
- **Rate limit exhaustion**: Enforce concurrency limits, monitor headers, and honour `Retry-After` values.
- **Token expiry or misconfiguration**: Surface clear errors and halt polling to avoid repeated failures.
- **Data drift**: Use `starred_at` plus `repo_full_name` as a composite key to deduplicate events and avoid duplicate feed entries.
- **Scaling poll intervals**: Start with conservative bounds (10 minutes–7 days) and make them configurable for future tuning.

## Success Metrics
- Polling one thousand followed users completes within GitHub's authenticated hourly quota when there are no changes (via 304 responses).
- New star events appear in the RSS feed within the next scheduled polling window for the originating user.
- Feed consumers (e.g., FreshRSS) accept the generated RSS without validation errors.

## Open Questions
- How will authentication secrets be provided (environment variable, config file)? – Needs confirmation during implementation.
- Should we surface multiple feeds (e.g., per user vs. aggregate)? – Current scope assumes a single aggregate feed.

