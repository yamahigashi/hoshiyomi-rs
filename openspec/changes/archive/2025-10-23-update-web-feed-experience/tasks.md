# Tasks

- [x] Update database schema and models to capture repository language, topics, and cached user activity tiers for stored star events, including a migration/backfill path.
- [x] Compute and persist activity tiers (e.g., high/medium/low) from historical star cadence so they stay in sync with scheduling logic.
- [x] Extend GitHub ingestion to request and persist language/topics while preserving rate-limit safeguards.
- [x] Expose a JSON endpoint (e.g., `GET /api/stars`) returning recent star events with the new metadata, sharing the existing data access layer.
- [x] Rebuild the served HTML into an interactive single-page view with search, language, activity-tier filters, and sort controls powered by client-side rendering.
- [x] Add automated coverage (unit/component/integration tests) for the API endpoint and UI data formatting logic.
- [x] Refresh documentation (README/server section) with instructions for the enhanced web app features.
- [x] Run `cargo fmt`, `cargo clippy`, `cargo test`, and `openspec validate update-web-feed-experience --strict` before requesting review.
