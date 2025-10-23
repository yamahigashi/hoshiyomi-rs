# web-feed-ui Specification

## Purpose
TBD - created by archiving change update-web-feed-experience. Update Purpose after archive.
## Requirements
### Requirement: Interactive Star List Controls
The served HTML page MUST provide client-side controls that let users narrow and order recent star events without reloading the page.

#### Scenario: Text search narrows results
- **GIVEN** the web page has loaded recent star events
- **WHEN** the user types a query into the search box
- **THEN** only events whose repository name, owner, description, or starring user include the query (case-insensitive) remain visible

#### Scenario: Language filter limits entries
- **GIVEN** the controls include a language selector populated from the displayed events
- **WHEN** the user chooses a language value
- **THEN** only events whose repository language matches that value remain visible, and selecting “All” restores the full list

#### Scenario: Sort toggle reorders list
- **GIVEN** the page offers a toggle between “Newest” and “Alphabetical” orders
- **WHEN** the user switches the toggle
- **THEN** the rendered list re-sorts by starred timestamp descending for “Newest” and by repository full name for “Alphabetical”

#### Scenario: Activity tier filter segments users
- **GIVEN** the page displays a control that lists activity tiers derived from historical star frequency (e.g., High, Medium, Low)
- **WHEN** the user selects a tier
- **THEN** only events from accounts in that tier remain visible, and choosing the all-tier option restores the full list

### Requirement: Star Metadata Visibility
Each star event shown in the web UI MUST surface repository context that mirrors the stored dataset.

#### Scenario: Repository description is shown
- **GIVEN** a stored star event includes a repository description
- **WHEN** the event is rendered
- **THEN** the description text appears under the repository title (HTML-escaped)

#### Scenario: Language and topics are shown when available
- **GIVEN** a stored star event has a primary language or topics
- **WHEN** the event is rendered
- **THEN** the language label and up to 10 topics (as pill-style badges or comma-separated text) appear alongside the entry

#### Scenario: Activity tier label is shown
- **GIVEN** the starring user has an assigned activity tier
- **WHEN** the event is rendered
- **THEN** the activity tier label appears with the entry so the user can tell whether it represents a high- or low-frequency star source

### Requirement: Star Data API for Web UI
The server MUST expose a JSON endpoint that supplies the UI with recent star events including the additional metadata.

#### Scenario: API responds with cached feed data
- **GIVEN** the database already contains recent star events
- **WHEN** a client requests `GET /api/stars`
- **THEN** the response is HTTP 200 with `application/json`, containing an array of events limited by the configured feed length, each including login, repo_full_name, repo_html_url, repo_description, repo_language (nullable), repo_topics (list of strings), and starred_at (RFC3339)
- **AND** each event includes `user_activity_tier` (string) so the client can filter without recomputing tiers

