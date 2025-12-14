## 1. Implementation
- [x] 1.1 Add a serve prefix option to config/CLI/env defaults (empty by default) and thread it into runtime state.
- [x] 1.2 Update warp route wiring to honor the prefix for `/`, `/feed.xml`, and `/api/*`, keeping existing behaviour when the prefix is empty; add/adjust integration tests for both cases.
- [x] 1.3 Honour `X-Forwarded-Prefix` (when present and well-formed) as the effective prefix per request, falling back to the configured prefix; include tests for header and non-header cases.
- [x] 1.4 Make the HTML dashboard and JS fetches prefix-aware (e.g., injected base path or relative URLs) so asset loading and API calls work behind a proxy; include snapshot updates if needed.
- [x] 1.5 Ensure any generated URLs/metadata (printed serve banner, shared links) reflect the prefix and document usage in README/config samples.

## 2. Validation
- [x] 2.1 `openspec validate add-serve-prefix-support --strict`
- [x] 2.2 `cargo fmt && cargo clippy --all-targets -- -D warnings`
- [x] 2.3 `cargo test` (including warp route tests covering prefixed paths)
