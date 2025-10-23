## MODIFIED Requirements
### Requirement: Star Metadata Visibility
- Each star event in the web UI MUST show repository context along with the **fetch timestamp** so readers know when the data was ingested.

#### Scenario: Fetch timestamp is displayed
- **GIVEN** a stored star event includes a `fetched_at` timestamp
- **WHEN** the event is rendered in the web UI
- **THEN** the item shows the fetch timestamp (for example, “Fetched at 2025-10-23T04:15:00Z”) alongside existing metadata

### Requirement: Interactive Star List Controls
- The served HTML page MUST present the most recent items based on fetch time, ensuring newly ingested stars appear first regardless of their original `starred_at` value.

#### Scenario: Items ordered by fetch time
- **GIVEN** multiple star events with different `starred_at` values but identical fetch timestamps
- **WHEN** the dashboard loads
- **THEN** the events are ordered by `fetched_at` descending, so the last-ingested items appear first
