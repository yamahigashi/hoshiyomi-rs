# web-feed-ui Specification

## Purpose
TBD - created by archiving change update-web-feed-experience. Update Purpose after archive.
## Requirements
### Requirement: Interactive Star List Controls
- The served HTML page MUST present the most recent items based on fetch time, ensuring newly ingested stars appear first regardless of their original `starred_at` value.

#### Scenario: Items ordered by fetch time
- **GIVEN** multiple star events with different `starred_at` values but identical fetch timestamps
- **WHEN** the dashboard loads
- **THEN** the events are ordered by `fetched_at` descending, so the last-ingested items appear first

### Requirement: Star Metadata Visibility
- Each star event in the web UI MUST show repository context along with the **fetch timestamp** so readers know when the data was ingested.

#### Scenario: Fetch timestamp is displayed
- **GIVEN** a stored star event includes a `fetched_at` timestamp
- **WHEN** the event is rendered in the web UI
- **THEN** the item shows the fetch timestamp (for example, “Fetched at 2025-10-23T04:15:00Z”) alongside existing metadata

### Requirement: Star Data API for Web UI
The server MUST expose a JSON endpoint that supplies the UI with recent star events including the additional metadata.

#### Scenario: API responds with cached feed data
- **GIVEN** the database already contains recent star events
- **WHEN** a client requests `GET /api/stars`
- **THEN** the response is HTTP 200 with `application/json`, containing an array of events limited by the configured feed length, each including login, repo_full_name, repo_html_url, repo_description, repo_language (nullable), repo_topics (list of strings), and starred_at (RFC3339)
- **AND** each event includes `user_activity_tier` (string) so the client can filter without recomputing tiers

