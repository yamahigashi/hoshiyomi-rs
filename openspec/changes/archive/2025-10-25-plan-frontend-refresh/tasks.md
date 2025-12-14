# Tasks
1. [x] Audit the current dashboard (summary strip, filters, render path) and capture screenshots/perf traces that show the lack of insights, preset switching, and large-list slowdown.
2. [x] Update `web-feed-ui` specs with the new insight panel, preset management, and virtualized list requirements plus scenarios that tie back to the six focus areas.
3. [x] Draft a lightweight design describing the insight pipeline, preset storage/URL contract, and virtualization approach (windowing thresholds, accessibility fallbacks).
4. [x] Run `openspec validate plan-frontend-refresh --strict` and fix any issues before requesting review.
5. [x] Implementation plan: break the change into the following execution tracks and validate each with reviewers before coding.
   - **Saved view presets**
     1. [x] Implement preset CRUD helpers (load/save/delete up to five entries) backed by a new `starchaser:viewPresets` key and synchronize with URL parameters via `syncUrl()`.
     2. [x] Add UI affordances: “Save current view” button + modal/dialog, preset chips with Alt+1…Alt+5 shortcuts (update shortcut modal copy) and contextual aria-labels.
     3. [x] Persist last-used preset id to restore the correct state after reload and ensure deep links keep working when the preset layer is bypassed.
   - **Virtualized list renderer**
     4. [x] Refactor `renderList` into a virtualization helper that maintains a fixed window (~40 cards) with spacer divs to preserve scroll height; gate activation behind `state.items.length > 500` or `state.pageSize > 50`.
     5. [x] Guarantee focus/ARIA order by pinning the focused node until the user leaves it and announcing window changes via `aria-live`.
     6. [ ] Add perf regression coverage (Lighthouse or custom timer) plus integration test that scrolls through 2k mock items to ensure smoothness and progressive-loading messaging.
   - **Server-driven pagination**
     7. [x] Refactor `fetchStars` to accept filter/pagination parameters, call `/api/stars?page=X&page_size=Y` (plus other filters), and ingest both `items` and the returned `meta` block for control state.
     8. [x] Implement a small page cache (current ±1) so returning to a recent page avoids redundant fetches while still invalidating whenever filters/sorts/presets change.
     9. [x] Wire pagination controls, shortcuts, and URL sync to the API metadata (`has_next`, `has_prev`, `total`), announce page loads via `aria-live`, and add coverage that clicking “Next” requests new pages instead of just slicing the initial payload.
   - **Validation + polish**
    10. [x] Update `tests/frontend_snapshot.html` and any JS unit tests to cover the new UI elements, presets, virtualization indicators, and pagination messaging.
    11. [ ] Verify `cargo test`, `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and frontend build snapshots pass locally; capture before/after screenshots for docs.
