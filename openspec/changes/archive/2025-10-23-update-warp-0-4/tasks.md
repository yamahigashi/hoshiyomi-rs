# Tasks

- [x] Bump `warp` to 0.4 in `Cargo.toml` and regenerate `Cargo.lock`.
- [x] Update server routes/handlers to satisfy the Warp 0.4 API (filters, replies, rejections).
- [x] Adjust tests (unit + integration) to compile and pass with the new API.
- [x] Verify minimal supported Rust version alignment and note any changes (no change detected).
- [x] Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`, and `openspec validate update-warp-0-4 --strict`.
