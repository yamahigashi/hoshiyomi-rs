# Proposal: Upgrade Warp to 0.4

## Why
Warp 0.3 is approaching end-of-life and lacks newer hyper/tokio integrations and security patches. Upgrading to Warp 0.4 keeps the project aligned with current ecosystem APIs, benefits from maintained dependencies (including hyper 1.x), and unlocks performance and security fixes.

## What Changes
- Bump the `warp` crate from 0.3 to 0.4 in `Cargo.toml`, along with any transitive dependency adjustments (e.g., `hyper`, `tower`, `headers`).
- Adapt server code to the updated Warp 0.4 APIs (notably response builders, filter trait changes, or rejection handling).
- Update integration/unit tests to compile against the new APIs.
- Document the minimum supported Rust version if Warp 0.4 raises it.

## Impact
- Slight refactors in `src/server.rs` to align with new filter combinators/replies.
- Potential need to update feature flags or HTTP types imported from Warp/hyper.
- Regression testing of the HTTP handlers (`/feed.xml`, `/`, `/api/stars`) is required to ensure behaviour remains unchanged.
