## Why
Users serving hoshiyomi behind reverse proxies (e.g., `proxy_pass /hoshiyomi/`) cannot reach `/feed.xml`, `/api/stars`, or the dashboard because routes and frontend fetches assume a root path. Links break once a prefix is introduced, so the service is unusable in common nginx/apache setups.

## What Changes
- Add a configurable HTTP path prefix so `/`, `/feed.xml`, `/api/*` all honor the prefix while keeping the empty prefix as the default.
- Optionally honour proxy-provided `X-Forwarded-Prefix` (when present/valid) so deployments behind ingress controllers don’t need to hard-code the prefix.
- Make the HTML dashboard and API/RSS links prefix-aware so asset loading, fetch calls, and shared URLs remain valid when proxied.
- Document the new option (CLI/env/config) and extend tests to cover prefixed and default routing.

## Impact
- Affected specs: `github-following-stars-rss`, `web-feed-ui`
- Affected code: server routing, config parsing, frontend fetch URLs/templates, HTTP tests/docs
