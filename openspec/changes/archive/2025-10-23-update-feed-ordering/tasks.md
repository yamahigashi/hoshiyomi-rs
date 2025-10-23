# Tasks

- [ ] Ensure star fetch timestamps are persisted and exposed from the database query used by the web UI.
- [ ] Update the HTML view and JSON endpoint to sort by fetch time descending and include a formatted fetch timestamp field.
- [ ] Adjust client-side rendering (filter/sort state) to use the new fetch timestamp and display it in the item metadata.
- [ ] Re-run automated tests or add coverage verifying the new sort order.
- [ ] Update documentation (README/web section) to mention fetch-time ordering.
- [ ] Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`, and `openspec validate update-feed-ordering --strict`.
