## MODIFIED Requirements
### Requirement: Adapt Polling Frequency Per User
- The system SHALL compute each user's polling interval using an exponential moving average (EMA) of inter-star gaps, bounded between `min_interval_minutes` (≥10) and `max_interval_minutes` (≤10080).
- The EMA smoothing constant SHALL be α = 0.3, applied as `ema_next = clamp(α * gap_minutes + (1 - α) * ema_prev, min, max)` for every new star gap once the user has at least three recorded star events.
- Until a user accumulates three star events, the system SHALL use `default_interval_minutes` (clamped to the min/max bounds) and label the activity tier as `low`.
- When seeding the EMA (the first time a user transitions from fewer than three to three or more events), the system SHALL average all available gap minutes to produce `ema_prev` before applying the smoothing update.
- The stored polling interval and associated activity tier (high ≤60 minutes, medium ≤1440 minutes, low otherwise) SHALL reflect the latest EMA output.

#### Scenario: Fallback For Sparse History
1. Given user `eve` has fewer than three recorded star events
2. When the scheduler recomputes her polling interval
3. Then it SHALL set `fetch_interval_minutes` to the clamped default interval and record the activity tier as `low`.

#### Scenario: EMA Update After New Star
1. Given user `frank` already has three or more star events and an existing EMA value of 90 minutes
2. When a new star arrives 30 minutes after the previous one
3. Then the system SHALL compute `ema_next = clamp(0.3 * 30 + 0.7 * 90)` (result 72 minutes) and persist that interval for subsequent scheduling.

#### Scenario: Activity Tier Mirrors EMA
1. Given the EMA-derived interval for user `grace` is 45 minutes after the latest update
2. When the system stores the recomputed interval
3. Then it SHALL classify the user as `high` activity so the web UI can filter accordingly.
