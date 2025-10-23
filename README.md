# Following Stars RSS

A Rust CLI that collects the repositories starred by the GitHub accounts you follow and emits an RSS feed you can subscribe to with any reader.

## Usage

```bash
cargo run -- \
  --github-token <TOKEN> \
  --db-path ./following-stars.db \
  --max-concurrency 5 \
  --feed-length 100
```

You can also configure the CLI via environment variables:

| Flag | Env Var | Default |
| --- | --- | --- |
| `--github-token` | `GITHUB_TOKEN` | _required_ |
| `--db-path` | `FOLLOWING_RSS_DB_PATH` | `following-stars.db` |
| `--max-concurrency` | `FOLLOWING_RSS_MAX_CONCURRENCY` | `5` |
| `--feed-length` | `FOLLOWING_RSS_FEED_LENGTH` | `100` |
| `--default-interval-minutes` | `FOLLOWING_RSS_DEFAULT_INTERVAL_MINUTES` | `60` |
| `--min-interval-minutes` | `FOLLOWING_RSS_MIN_INTERVAL_MINUTES` | `10` |
| `--max-interval-minutes` | `FOLLOWING_RSS_MAX_INTERVAL_MINUTES` | `10080` |
| `--api-base-url` | `FOLLOWING_RSS_API_BASE` | `https://api.github.com` |
| `--user-agent` | `FOLLOWING_RSS_USER_AGENT` | `following-stars-rss` |
| `--timeout-secs` | `FOLLOWING_RSS_TIMEOUT_SECS` | `30` |
| `serve --bind` | `FOLLOWING_RSS_BIND` | `127.0.0.1` |
| `serve --port` | `FOLLOWING_RSS_PORT` | `8080` |
| `serve --refresh-minutes` | `FOLLOWING_RSS_REFRESH_MINUTES` | `15` |

The command writes the RSS XML to stdout and stores all state in the SQLite database. Re-run it periodically to refresh the feed.

## Server Mode

Run the CLI with the `serve` subcommand to host the feed as both `feed.xml` and a simple HTML index:

```bash
cargo run -- serve \
  --github-token <TOKEN> \
  --db-path ./following-stars.db \
  --bind 0.0.0.0 \
  --port 8080 \
  --refresh-minutes 15
```

The server performs an initial GitHub sync, then refreshes in the background every `refresh-minutes`. Access the endpoints at:

- `http://<bind>:<port>/feed.xml` — RSS output
- `http://<bind>:<port>/` — Interactive HTML dashboard with search, language and activity-tier filters, plus newest/alphabetical sorting
- `http://<bind>:<port>/api/stars` — JSON payload backing the dashboard (includes descriptions, language, topics, and cached activity tiers)

Use `Ctrl+C` (or send SIGINT) to shut the server down gracefully.

## Scheduling

The poller now adapts to each follower using an exponential moving average (α = 0.3) of recent star gaps. Accounts with fewer than three observed stars stay on the configured default interval (clamped between the global min/max) and are classified as `low` activity until more history accumulates. Highly active accounts naturally trend toward shorter intervals while long-dormant accounts stretch toward the maximum, keeping rate-limit usage predictable without manual tuning.

To refresh the feed every 15 minutes with systemd timers, create `~/.config/systemd/user/following-stars.service`:

```
[Unit]
Description=Generate GitHub following stars RSS

[Service]
Type=oneshot
WorkingDirectory=/path/to/project
Environment=GITHUB_TOKEN=ghp_...
ExecStart=/usr/bin/env cargo run --release -- --db-path /path/to/following-stars.db --feed-length 200
```

And the companion timer `~/.config/systemd/user/following-stars.timer`:

```
[Unit]
Description=Run following-stars-rss every 15 minutes

[Timer]
OnBootSec=1m
OnUnitActiveSec=15m
Persistent=true

[Install]
WantedBy=timers.target
```

Enable with:

```bash
systemctl --user enable --now following-stars.timer
```

Alternatively, use cron:

```cron
*/15 * * * * cd /path/to/project && GITHUB_TOKEN=ghp_... cargo run --release -- --db-path /path/to/following-stars.db >> /path/to/feed.xml
```

Ensure the GitHub token has the `read:user` and `public_repo` scopes to access followings and starred repositories.
