## ADDED Requirements
### Requirement: Prefix-Aware Dashboard Requests
- The served HTML/JS MUST determine the active server prefix (configured value, or `X-Forwarded-Prefix` when present) and prepend it to all API calls (`/api/stars`, `/api/status`, `/api/options`) and feed/dashboard links instead of hard-coding root-relative URLs.
- The dashboard assets (HTML, inline styles/scripts) MUST load without 404s when hosted under a subpath by using relative URLs or injected prefix metadata.

#### Scenario: UI served under prefix
1. Given the application is reverse-proxied at `/hoshi/`
2. When a reader loads `/hoshi/`
3. Then the page renders with styles and scripts intact and subsequent requests are sent to `/hoshi/api/stars` (and matching status/options routes), allowing filters and pagination to function normally.

#### Scenario: Forwarded prefix honoured by UI
1. Given an upstream proxy forwards requests to the app with header `X-Forwarded-Prefix: /alt`
2. When a reader loads `/alt/`
3. Then the served HTML/JS derives `/alt` as the base path and issues `/alt/api/stars` (and peers), succeeding even if the configured prefix differs.
