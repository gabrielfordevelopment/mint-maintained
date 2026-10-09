# Desktop UI and background operations

## Ownership and flow

- [src/gui/mod.rs](../../src/gui/mod.rs) owns the egui application, profiles/groups, sorting, Settings, and footer.
- [src/gui/message.rs](../../src/gui/message.rs) dispatches asynchronous provider/download/integration operations and receives results. [RequestCounter](../../src/gui/request_counter.rs) identifies requests; matching IDs prevent stale results from applying to a newer operation.
- [src/gui/icons.rs](../../src/gui/icons.rs) owns icon controls and display-scale-aware SVG texture reuse. [named_combobox.rs](../../src/gui/named_combobox.rs) owns profile/group selection controls.
- [src/state/mod.rs](../../src/state/mod.rs) owns durable settings and profile data. Render code consumes those owners rather than maintaining a second persisted representation.

Keep network and blocking integration outside the egui frame. When changing cancellation, profile switching, or result handling, inspect both dispatch and receive paths, including stale completion, failure, and progress updates. Preserve resolved mod/load ordering across asynchronous work.

## UI invariants

The compact row geometry and existing control placement are intentional. A visual correction does not authorize a layout redesign, larger rows, or a table header. Reuse the icon/control owner before adding another implementation. Preserve theme selection, neutral add/delete icon foregrounds, hover feedback, focus, and keyboard behavior.

Sorting derives display indices without rewriting manual order. Group boundaries stay in place; individual runs and group children are sorted independently. Row actions must address stored entries, not sorted positions. Missing metadata is valid, including adjacent entries where one or both have none.

## Test boundaries

[src/gui/tests.rs](../../src/gui/tests.rs) contains the existing `TestApp` harness, egui event injection, AccessKit/control lookup, sorting checks, and layout regressions. Extend it for behavior it can observe before introducing another UI test framework.

The harness's placeholder `.pak` files represent UI entries, not valid integration payloads. Use the real pak fixtures under [test_assets/lints](../../test_assets/lints) for the lint behavior they cover; do not equate either fixture set with an in-game integration test.

For a visible preview, choose a task-owned `--appdata` directory outside the repository and inspect the selected game path before performing actions. A fresh config can auto-discover the installed game: separate app data does not isolate game writes. For a UI-only demo, set `drg_pak_path` to null in the disposable config and avoid Install, Uninstall, Launch, and self-update actions.

Check the affected controls in light/dark themes, hover/pressed/selected/disabled states, representative display scaling, small/resized windows, long names, empty/populated groups, and missing metadata. Capture actual application states only when useful evidence is needed. Record which checks were automated, manually exercised, or unavailable; native window behavior and injected game hooks are not proven by headless egui tests.
