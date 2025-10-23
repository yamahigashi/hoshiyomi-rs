# Tasks

- [x] Scaffold a new Rust binary crate (or augment existing crate) with Tokio, reqwest, rusqlite, serde, and rss dependencies.
- [x] Implement configuration loading for GitHub token, database path, concurrency, and feed size (env vars + CLI flags).
- [x] Create SQLite migrations/tables for `users` and `stars`, including unique constraints and timestamp handling.
- [x] Build a GitHub client wrapper that handles pagination, conditional headers, and parses `starred_at` fields.
- [x] Add rate-limit aware request scheduling (Tokio semaphore, backoff on `Retry-After`, stop on auth errors).
- [x] Persist fetched followings and star events, ensuring deduplication by user/repo/timestamp.
- [x] Implement adaptive polling interval calculations and storage of `next_check_at` per user.
- [x] Generate RSS output from stored events and expose it via file or stdout with deterministic GUIDs.
- [x] Write integration or contract tests with mocked GitHub responses covering rate limit handling and RSS output shape.
- [x] Document usage and deployment guidance (cron/systemd) in README or docs.
