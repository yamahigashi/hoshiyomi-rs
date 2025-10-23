## ADDED Requirements

### Requirement: Serve Feed via HTTP
- The system SHALL expose an optional server mode that listens on a configurable host and port, defaulting to `127.0.0.1:8080`.
- When server mode is active, the system SHALL respond to `GET /feed.xml` with the latest RSS feed XML using `Content-Type: application/rss+xml`.
- When server mode is active, the system SHALL respond to `GET /` with an HTML page summarizing recent star events (user, repository link, description, and timestamp).

#### Scenario: Requesting Feed XML
1. Given the server is running with current data in SQLite
2. When a client performs `GET /feed.xml`
3. Then the server returns status `200`, `Content-Type: application/rss+xml`, and the body equals the RSS currently built from stored events.

#### Scenario: Requesting HTML Index
1. Given the server is running with at least one stored star event
2. When a client performs `GET /`
3. Then the server returns status `200`, `Content-Type: text/html`, and the body contains the starring user's login, repository name, and a link to the repo.

### Requirement: Background Polling in Server Mode
- The server mode SHALL refresh GitHub data on a configurable interval (default 15 minutes) using the existing polling pipeline.
- The server mode SHALL perform an initial refresh before accepting HTTP requests to avoid serving stale data.
- The server mode SHALL log polling errors and retry on the next scheduled interval without crashing the server.

#### Scenario: Initial Refresh Before Serving
1. Given the database is empty
2. When the server starts in serve mode
3. Then it performs a polling cycle before the HTTP routes begin responding, ensuring `/feed.xml` returns a valid (possibly empty) RSS feed.

### Requirement: Graceful Shutdown
- The server mode SHALL listen for termination signals (Ctrl+C) and shut down both the HTTP server and polling task cleanly.
- During shutdown, the server SHALL stop accepting new requests and finalize in-flight polling before exiting.

#### Scenario: Signal Handling
1. Given the server is running and polling on an interval
2. When the process receives an interrupt signal
3. Then the HTTP server stops accepting requests and the process exits without panicking.
