# Proposal: Order Web Index by Fetch Timestamp

## Why
The current web index sorts by the `starred_at` timestamp. When followers star repositories at high cadence, we can fetch multiple users in one polling cycle. A newly fetched star from a slow cadence user might carry an older `starred_at` value, causing it to appear far down the list—or fall out of the configured feed window—even though it was just discovered. Displaying items by fetch time better reflects freshness from the reader’s perspective and avoids newly ingested items being buried.

## What Changes
- Persist the star fetch timestamp and expose it through the query powering the HTML page and JSON API.
- Update sorting for the web UI (and JSON endpoint consumers) to use fetch time descending, while keeping RSS output unaffected (still keyed by `starred_at`).
- Render the fetch timestamp (possibly in a tooltip or secondary text) so readers understand the difference between star time and discovery time.

## Impact
- Database query adjustments in `recent_events_for_feed` and related structs.
- Additional field(s) in the web JSON response and HTML renderer.
- Client-side sorting change in the dashboard to rely on the new field.
