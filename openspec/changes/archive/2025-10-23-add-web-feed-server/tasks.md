# Tasks

- [x] Refactor existing run logic so polling can be invoked on-demand and/or scheduled without printing feed to stdout each time.
- [x] Add configuration options for server mode (serve flag/subcommand, bind address, port, refresh interval).
- [x] Integrate Warp, defining routes for `/feed.xml` and `/` (HTML) with appropriate content types and caching headers.
- [x] Implement HTML rendering that lists recent star events with user, repo link, description, and time.
- [x] Ensure server mode runs background polling at the configured interval and updates the database.
- [x] Add integration tests (or component tests) for HTTP handlers using Warp test utilities.
- [x] Update README with instructions for server mode.
- [x] Run `cargo fmt` / `cargo test` and validate OpenSpec change.
