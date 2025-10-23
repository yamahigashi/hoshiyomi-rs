# Tasks

- [x] Update interval recomputation logic to use an exponential moving average (EMA) of inter-star gaps, honoring configured min/max bounds.
- [x] Bootstrap EMA safely when a user reaches 2 and 3 observed events, falling back to the default interval beforehand.
- [x] Persist or derive the EMA so subsequent polls reuse the smoothed value without replaying whole history.
- [x] Adjust activity-tier classification to rely on the EMA-derived interval and keep thresholds documented.
- [x] Add focused tests that cover: zero/one/two-event fallback, EMA convergence after a burst, and tier mapping edge cases.
- [x] Migrate stored data if new columns or defaults are required, including backfill of EMA/tier for existing users.
- [x] Update README/spec or developer docs to describe the refined adaptive polling behavior.
- [x] Run formatting, clippy, tests, and `openspec validate update-adaptive-polling --strict` before requesting review.
