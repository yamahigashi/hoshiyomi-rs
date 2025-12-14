## ADDED Requirements
### Requirement: Configurable Serve Prefix
- The server SHALL accept an optional HTTP path prefix (via config/CLI/env) that applies to all served routes (`/`, `/feed.xml`, `/api/*`), defaulting to no prefix.
- When a prefix is configured, the server SHALL expose routes only under that prefix and reflect the full prefixed URLs in startup/help output so operators can verify proxy settings.
- Route handlers SHALL retain their existing behaviour (status codes, headers, caching) while honoring the prefix; requests missing the configured prefix SHALL return `404 Not Found`.
- If header `X-Forwarded-Prefix` is present and well-formed, the server SHALL use it as the effective prefix for that request (after normalization) in preference to the configured prefix; malformed values SHALL be ignored in favor of the configured prefix.

#### Scenario: Proxy path prefix
1. Given the operator starts serve mode with prefix `/hoshi`
2. When a client performs `GET /hoshi/feed.xml` and `GET /hoshi/api/stars`
3. Then both requests succeed with their normal payloads and `GET /feed.xml` without the prefix returns `404`.

#### Scenario: Default root behaviour
1. Given serve mode is started without specifying a prefix
2. When a client performs `GET /feed.xml`, `GET /`, and `GET /api/status`
3. Then all respond under the root path with no additional segments required.

#### Scenario: Forwarded prefix overrides config
1. Given serve mode is started with configured prefix `/hoshi`
2. And an upstream proxy adds header `X-Forwarded-Prefix: /alt`
3. When a client performs `GET /alt/api/status` through that proxy
4. Then the server responds under `/alt/...` and ignores the configured `/hoshi` prefix for that request.
