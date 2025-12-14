## Context
The HTTP server assumes root-mounted routes (`/`, `/feed.xml`, `/api/*`) and the bundled frontend fetches `"/api/stars"` directly. When hoshiyomi is reverse-proxied under a subpath (e.g., `/hoshiyomi/`), all links and API calls break because neither the warp filters nor the UI know about the prefix.

## Goals / Non-Goals
- Goals: allow operators to configure a path prefix; ensure server routing, printed links, and frontend fetches/assets honor that prefix while keeping the empty prefix working unchanged.
- Non-goals: altering RSS item/channel target URLs (they still point to GitHub repos), changing authentication, or introducing host/base URL rewriting beyond the prefix segment.

## Decisions
- **Prefix input + normalization**: add a `serve_prefix` option (CLI/env/config). Normalize to a canonical string with a single leading slash and no trailing slash, and store both the string form and a vector of path segments for wiring warp routes.
- **Route composition**: wrap existing warp routes in a reusable `with_prefix` helper that applies the effective prefix segments before the route-specific filter, so `/feed.xml`, `/`, `/api/stars`, `/api/status`, and `/api/options` all live under the prefix when set and remain root-mounted when empty.
- **Forwarded prefix support**: accept `X-Forwarded-Prefix` when present; normalize and validate it, then treat it as the effective prefix for that request. If absent or invalid, fall back to the configured prefix. Keep the surface small to avoid header spoofing (`X-Forwarded-Prefix` only, no auto-detection of other headers).
- **Frontend awareness**: inject the effective prefix into the served HTML (e.g., data attribute or inline global). JS builds API URLs by joining the prefix with `api/stars|status|options` and uses relative navigation for feed links so the dashboard works whether mounted at `/` or `/foo/bar/`.
- **Operator feedback**: include the prefix in startup logs/help text and document proxy examples so operators can verify the effective URLs.

## Risks / Trade-offs
- **Mis-normalized prefixes** (e.g., double slashes) could lead to 404s; mitigated by canonicalization and explicit tests for prefixed/unprefixed routes.
- **Frontend regressions** if the prefix injection is missed; mitigated by snapshot/integration coverage that asserts the UI hits prefixed endpoints.

## Open Questions
- Should we also bind both prefixed and root routes when a prefix is set for transition? Current plan is prefixed-only to avoid ambiguity; confirm during review.
