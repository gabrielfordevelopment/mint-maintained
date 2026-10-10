# Desktop UI and background operations

## Ownership and flow

- [src/gui/mod.rs](../../src/gui/mod.rs) owns the application shell, profile selector, startup recovery and footer. [mod_list.rs](../../src/gui/mod_list.rs) renders rows, [sorting.rs](../../src/gui/sorting.rs) owns display comparators, [settings.rs](../../src/gui/settings.rs) owns Settings/provider dialogs, and [lint.rs](../../src/gui/lint.rs) owns lint selection/reports.
- [src/gui/message.rs](../../src/gui/message.rs) dispatches asynchronous provider/download/integration operations and receives results. [RequestCounter](../../src/gui/request_counter.rs) identifies requests; matching IDs prevent stale results from applying to a newer operation.
- [src/gui/icons.rs](../../src/gui/icons.rs) owns icon controls and display-scale-aware SVG texture reuse. [named_combobox.rs](../../src/gui/named_combobox.rs) owns profile/group selection controls.
- [diagnostics.rs](../../src/gui/diagnostics.rs) formats error chains, redacts provider secrets and renders selectable/copyable details. The footer keeps a short status and opens a separate details window without changing row geometry.
- [src/state/mod.rs](../../src/state/mod.rs) owns durable settings and profile data. Render code consumes those owners rather than maintaining a second persisted representation.

Keep network and blocking integration outside the egui frame. When changing cancellation, profile switching, or result handling, inspect both dispatch and receive paths, including stale completion, failure, and progress updates. Preserve resolved mod/load ordering across asynchronous work.

## Profile and group editing

[editing.rs](../../src/gui/editing.rs) owns deletion confirmation and delegates to [state/edits.rs](../../src/state/edits.rs). Normal mod, duplicate, group-reference, shared-group and profile deletion requires confirmation. Shift at the originating click bypasses the dialog; Cancel initially has focus and Escape cancels. A prepared deletion captures the profile data and refuses to run if that data changed while the dialog was open. Never reinterpret a saved display index against a newly sorted or changed list.

[groups.rs](../../src/gui/groups.rs) exposes shared-group creation, attachment, renaming, membership and global deletion through the Groups button. Mod-name context menus move entries into/out of groups; group-header menus expose rename and ungroup. Row deletion detaches a group from one profile, while the manager's Delete group removes it globally with explicit affected-profile text. Display the sharing semantics; do not silently turn shared groups into profile-local copies. Ungroup is an explicit copy into the current profile, preserving order and effective enabled states, and leaves the shared definition available to other profiles.

The UI disables editing while conflicting background work or a confirmation is active. Group mutations are validated atomically in memory before using the existing save path; a save failure remains visible and retains the in-memory change for recovery. No deletion here removes a mod file or cache blob.

## Windows startup and diagnostics

The Windows executable uses the GUI subsystem, so Explorer launch does not allocate a console. [windows_startup.rs](../../src/windows_startup.rs) attaches to an existing parent console for CLI use and preserves redirected input/output/error handles. CLI help, errors, exit codes and output redirection must remain testable. The caller's terminal is never hidden or closed. Settings exposes Open log for the current `mint.log`; existing selectable error details remain available.

Tracked background tasks report unexpected panics through their request ID. Completion/failure clears only the matching operation; failed lint reports stop showing the running spinner. Closing a report window does not abandon an active worker. Integration cancellation retains its busy handle until the writer finishes; see the integration contract for staged replacement and locking.

Partial imports preserve input order and keep errors available for retry; they are not permission for installation to ignore failures. Search highlighting maps lowercase matches back to original UTF-8 character boundaries, including characters whose lowercase representation changes length.

## UI invariants

The compact row geometry and existing control placement are intentional. A visual correction does not authorize a layout redesign, larger rows, or a table header. Reuse the icon/control owner before adding another implementation. Preserve theme selection, neutral add/delete icon foregrounds, hover feedback, focus, and keyboard behavior.

Sorting derives display indices without rewriting manual order. Group boundaries stay in place; individual runs and group children are sorted independently. Row actions must address stored entries, not sorted positions. Missing metadata is valid, including adjacent entries where one or both have none.

## Test boundaries

[src/gui/tests.rs](../../src/gui/tests.rs) contains the existing `TestApp` harness, egui event injection, AccessKit/control lookup, sorting checks, and layout regressions. Extend it for behavior it can observe before introducing another UI test framework.

The harness's placeholder `.pak` files represent UI entries, not valid integration payloads. Use the real pak fixtures under [test_assets/lints](../../test_assets/lints) for the lint behavior they cover; do not equate either fixture set with an in-game integration test.

[GUI editing tests](../../src/gui/tests/editing.rs) cover confirmation, Shift, cancellation, stale targets and shared-group deletion. [State editing tests](../../src/state/edits/tests.rs) cover atomic failure, references, ordering and reload. [Windows startup tests](../../tests/windows_startup.rs) verify the executable subsystem and redirected CLI behavior. Test group menus and the native log-opening action in an isolated visible session as well.

For a visible preview, choose a task-owned `--appdata` directory outside the repository and inspect the selected game path before performing actions. A fresh config can auto-discover the installed game: separate app data does not isolate game writes. For a UI-only demo, set `drg_pak_path` to null in the disposable config and avoid Install, Uninstall, Launch, and self-update actions.

Check the affected controls in light/dark themes, hover/pressed/selected/disabled states, representative display scaling, small/resized windows, long names, empty/populated groups, and missing metadata. Capture actual application states only when useful evidence is needed. Record which checks were automated, manually exercised, or unavailable; native window behavior and injected game hooks are not proven by headless egui tests.
