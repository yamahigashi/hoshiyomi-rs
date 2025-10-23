# Proposal: Refine Adaptive Polling with EMA

## Why
The current adaptive polling strategy averages the gaps across the five most recent star events. This makes scheduling hypersensitive to short-term spikes and ignores longer behavioral trends. It also leaves zero- or single-star users oscillating around the default interval without formal guardrails. These drawbacks increase rate-limit pressure and delay detection of sustained activity changes.

## What Changes
- Replace the fixed "last five gaps" heuristic with an exponential moving average (EMA) of inter-star intervals, so that recent activity influences scheduling while historic cadence still dampens spikes.
- Define how to bootstrap and update the EMA, including bounds tied to existing min/max interval settings.
- Codify fallback handling for users with fewer than three recorded star events so they poll on a conservative cadence until sufficient data exists.
- Align activity-tier labels with the EMA-derived interval to keep the web UI filters accurate.

## Impact
- Implementation will adjust the interval recomputation logic in `db.rs`, introduce persisted EMA state (or recompute from ordered history), and update activity-tier mapping to use the new interval.
- Polling will become smoother, reducing unnecessary GitHub API calls after transient bursts while remaining responsive to real behavior changes.
- Test suite must grow to cover EMA math, bootstrap rules, and edge cases (0/1/2 events).
