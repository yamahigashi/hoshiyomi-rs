# hoshiyomi · Following Stars RSS

hoshiyomi monitors the GitHub accounts you follow, stores every new star in SQLite, and republishes that activity as:
- **RSS (`/feed.xml`)** for any feed reader
- **Interactive web dashboard (`/`)** with filtering, search, and newest sorting options
- **JSON API (`/api/stars`)** that powers the UI or downstream tooling

The poller adapts to each user’s cadence using exponential moving averages, so highly active accounts refresh quickly while quieter ones back off to preserve rate limits.

## Quick Start
1. **Install prerequisites** (see below) and clone this repository.
2. **Export a GitHub token** with `read:user` + `public_repo` scopes: `export GITHUB_TOKEN=ghp_...`.
3. **Run a one-shot sync** to populate `following-stars.db` and emit RSS to stdout:
   ```bash
   cargo run --release -- \
     --github-token "$GITHUB_TOKEN" \
     --db-path ./following-stars.db \
     --feed-length 100
   ```
4. **Launch the server** for the dashboard and API:
   ```bash
   cargo run --release -- serve \
     --github-token "$GITHUB_TOKEN" \
     --db-path ./following-stars.db \
     --bind 127.0.0.1 \
     --port 8080 \
     --refresh-minutes 15
   ```
5. **Visit the endpoints**:
   - `http://127.0.0.1:8080/` — web dashboard (search, filters, newest sort switcher)
   - `http://127.0.0.1:8080/feed.xml` — RSS feed for your reader
   - `http://127.0.0.1:8080/api/stars` — JSON payload powering the UI

## Prerequisites
- Rust 1.78+ (edition 2021) and Cargo
- SQLite 3 (linked automatically via `rusqlite`)
- OpenSSL headers + `pkg-config` (e.g., `sudo apt install libssl-dev pkg-config`)
- GitHub personal access token with `read:user` and `public_repo`

> **Build tip:** If OpenSSL detection fails, set `OPENSSL_DIR=/usr/lib/ssl` (path varies by OS) or install the development package for your platform.

## Operating Modes
### Batch CLI (one-shot)
Use when you only need a fresh RSS export or want to run via CI/cron.
```bash
cargo run --release -- --github-token "$GITHUB_TOKEN" --db-path ./following-stars.db --feed-length 200
```
Outputs RSS to stdout, updates SQLite, then exits.

### Server Mode
Recommended for always-on dashboards and feed hosting.
- Performs an initial sync, then refreshes in the background (default 15 minutes).
- Dashboard features: search, language/activity filters, per-user pin/exclude, pagination, density toggle, keyboard shortcuts, and a pair of newest sort modes (by star time or fetch time).
- JSON API mirrors dashboard filters for external integrations.

### Automation / RSS-only Deployments
Keep the CLI output up to date via scheduled jobs when you do not need the dashboard running continuously.
- **systemd timer (user scope):** see [Operations & Automation](#operations--automation).
- **GitHub Actions or other CI:** run the batch command on a schedule and publish `feed.xml` as an artifact or to Pages.

## Configuration
Configuration values merge with the following precedence: **flags > environment variables > config file > built-in defaults**.

### Flags & Environment Variables
| Flag | Environment variable | Default |
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

### Config File (`hoshiyomi.toml`)
Search order: `./hoshiyomi.toml`, `$XDG_CONFIG_HOME/hoshiyomi/config.toml`, or a path passed to `--config`.
```toml
[github]
token = "ghp_..."

[app]
db_path = "./following-stars.db"
max_concurrency = 5
api_base_url = "https://api.github.com"
user_agent = "hoshiyomi"
timeout_secs = 30

[polling]
feed_length = 100
default_interval_minutes = 60
min_interval_minutes = 10
max_interval_minutes = 10080

[server]
enable = true
bind = "0.0.0.0"
port = 8080
refresh_minutes = 15
```
Validation errors identify the source (flag/env/file) so you can correct misconfigurations quickly.

## Operations & Automation
### systemd (user) timer
`~/.config/systemd/user/following-stars.service`
```
[Unit]
Description=Generate GitHub following stars RSS

[Service]
Type=oneshot
WorkingDirectory=/path/to/project
Environment=GITHUB_TOKEN=ghp_...
ExecStart=/usr/bin/env cargo run --release -- --db-path /path/to/following-stars.db --feed-length 200
```

`~/.config/systemd/user/following-stars.timer`
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
Enable with `systemctl --user enable --now following-stars.timer`.

### GitHub Actions (alternate scheduler)
```yaml
name: Refresh Feed
on:
  schedule:
    - cron: "*/30 * * * *"
  workflow_dispatch:
jobs:
  refresh:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: sudo apt-get install -y libssl-dev pkg-config sqlite3
      - run: cargo run --release -- --db-path ./following-stars.db --feed-length 200
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      - uses: actions/upload-artifact@v4
        with:
          name: feed
          path: following-stars.db
```
Adapt the final step to publish `feed.xml` or push to object storage as needed.

## Troubleshooting
| Issue | Symptoms | Suggested fix |
| --- | --- | --- |
| OpenSSL build failure | `openssl-sys` cannot find headers | Install `libssl-dev`/`openssl-devel`, set `OPENSSL_DIR`, or ensure `pkg-config` is on PATH |
| GitHub rate limiting | API responses with status 403 and `Retry-After` | Reduce concurrency, increase `refresh-minutes`, or wait for reset (the poller honours `Retry-After` automatically) |
| SQLite locked | `database is locked` during write | Run fewer concurrent pollers, increase polling interval, or move the DB onto faster storage |

## Contributor Guide
- **Tests & formatting:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (requires OpenSSL prerequisites).
- **Project layout:**
  - `src/` — application code (`server.rs`, `pipeline.rs`, etc.)
  - `frontend/` — dashboard assets bundled via `build.rs`
  - `openspec/` — specifications and change proposals
- **Workflow:** Propose behaviour changes by updating OpenSpec first (`openspec/changes/<id>/`), run `openspec validate <id> --strict`, then implement.

## License
MIT (see `LICENSE`).
