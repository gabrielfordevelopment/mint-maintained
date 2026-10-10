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

The Groups window leads with name entry and creation; advanced shared-group operations are collapsed by default. [drag_drop.rs](../../src/gui/drag_drop.rs) handles moves between the profile and groups, group-member ordering and profile ordering in Manual mode. Root and group-child rows share the same drag path. Group headers accept mods, insertion lines show exact positions, and the drag hint identifies shared targets. Apply only on release over a valid target against the captured data snapshot. Escape, external drops and stale snapshots must not mutate data; groups cannot nest. Row scopes and switch IDs follow entry identity rather than visual position so reordering does not inherit another mod's switch animation.

The UI disables editing while conflicting background work or a confirmation is active. Group mutations are validated atomically in memory before using the existing save path; a save failure remains visible and retains the in-memory change for recovery. No deletion here removes a mod file or cache blob. Mod removal uses the title "Remove mod?" and the "Remove" action. Center the confirmation title and actions with clear spacing while keeping the surrounding list compact. Dropdowns and name-entry popups close on outside presses as well as Escape.

## Windows startup and diagnostics

Group expansion belongs to [group_view.rs](../../src/gui/group_view.rs), separate from saved mod data. Newly created/attached groups and successful drop destinations open immediately. Hovering a mod over a closed group for 500 ms expands it temporarily; the entire visible group is the hover region. Leaving or cancelling restores the prior collapsed state, while a successful drop keeps it open. Already-open groups stay open.

Group headers show the total member count beside the name, including disabled mods. The name, arrow and remaining header width toggle expansion on left click and open the shared context menu on right click. This target excludes the separate drag, delete and enable controls and the member rows.

[group_colors.rs](../../src/gui/group_colors.rs) owns the 15-color theme-aware palette and the 5-by-3 color-dot picker in the group-header context menu. A group's header and left indentation gutter share its color; member rows retain normal alternating backgrounds. The selected dot has a ring and checkmark, with named hover/focus support. Color is a saved property of the shared group, so every referring profile displays the same choice. Gray is the default for old groups. Apply palette changes through the validated edit/save path without changing row geometry or drag targets.

Text inputs use [inputs.rs](../../src/gui/inputs.rs) for a shared theme-aware border, preserving native focus indication and compact geometry. On Windows, [native_popup.rs](../../src/gui/native_popup.rs) dismisses popups on non-client mouse presses (including the title bar) and focus loss through a window subclass removed on destruction. Other platforms use egui focus/outside-press handling.

The live viewer formats each timestamp once as `[YYYY-MM-DD HH:MM:SS]` in UTC and labels the toolbar `Time: UTC`; raw log files retain their original format. Timestamp formatting runs in the asynchronous reader and preserves message bodies and continuation lines. The viewer defaults to INFO/WARN/ERROR; Show debug includes DEBUG/TRACE and their continuation lines. Unknown text is preserved. Cached display text wraps to the viewport width, WARN/ERROR use theme-aware text colors, and Copy visible log copies the filtered, redacted view. Filtering never removes entries from the file. Completed profile/group edits log readable operation sentences with the affected item and outcome after the save attempt. Cancelled and unchanged operations do not emit success events; rejected edits and unsaved in-memory changes are distinguished. Provider secrets are redacted before edit descriptions are logged.

The Windows executable uses the GUI subsystem, so Explorer launch does not allocate a console. [windows_startup.rs](../../src/windows_startup.rs) attaches to an existing parent console for CLI use and preserves redirected input/output/error handles. CLI help, errors, exit codes and output redirection must remain testable. The caller's terminal is never hidden or closed. Settings exposes Open log for the current `mint.log` and Live log for a separate native viewport owned by [log_window.rs](../../src/gui/log_window.rs). Its cancellable async worker refreshes the last 128 KiB every 500 ms and redacts configured provider secrets before display/copy. The viewer supports pause, follow and always-on-top; closing it stops the reader without exiting the main app. No file IO runs in the rendering callback. Existing selectable error details remain available.

Tracked background tasks report unexpected panics through their request ID. Completion/failure clears only the matching operation; failed lint reports stop showing the running spinner. Closing a report window does not abandon an active worker. Integration cancellation retains its busy handle until the writer finishes; see the integration contract for staged replacement and locking.

Partial imports preserve input order and keep errors available for retry; they are not permission for installation to ignore failures. Search highlighting maps lowercase matches back to original UTF-8 character boundaries, including characters whose lowercase representation changes length.

## UI invariants

List rows use one logical pixel of vertical inner padding above and below their controls, with no horizontal padding. Apply the same padding to standalone rows, group headers and group members; keep control sizes unchanged.

The compact row geometry and existing control placement are intentional. A visual correction does not authorize a layout redesign, larger rows, or a table header. Reuse the icon/control owner before adding another implementation. In both themes, add buttons are blue with white icons; delete icons and confirmation actions are red with white foregrounds. Hover and pressed states use darker fills. Preserve disabled styling, theme selection, focus and keyboard behavior.

Sorting derives display indices without rewriting manual order. Group boundaries stay in place; individual runs and group children are sorted independently. Row actions must address stored entries, not sorted positions. Missing metadata is valid, including adjacent entries where one or both have none.

## Test boundaries

[src/gui/tests.rs](../../src/gui/tests.rs) contains the existing `TestApp` harness, egui event injection, AccessKit/control lookup, sorting checks, and layout regressions. Extend it for behavior it can observe before introducing another UI test framework.

The harness's placeholder `.pak` files represent UI entries, not valid integration payloads. Use the real pak fixtures under [test_assets/lints](../../test_assets/lints) for the lint behavior they cover; do not equate either fixture set with an in-game integration test.

[GUI editing tests](../../src/gui/tests/editing.rs) cover confirmation, Shift, cancellation, stale targets and shared-group deletion. [State editing tests](../../src/state/edits/tests.rs) cover atomic failure, references, ordering and reload. [Windows startup tests](../../tests/windows_startup.rs) verify the executable subsystem and redirected CLI behavior. Test group menus and the native log-opening action in an isolated visible session as well.

For a visible preview, choose a task-owned `--appdata` directory outside the repository and inspect the selected game path before performing actions. A fresh config can auto-discover the installed game: separate app data does not isolate game writes. For a UI-only demo, set `drg_pak_path` to null in the disposable config and avoid Install, Uninstall, Launch, and self-update actions.

Check the affected controls in light/dark themes, hover/pressed/selected/disabled states, representative display scaling, small/resized windows, long names, empty/populated groups, and missing metadata. Capture actual application states only when useful evidence is needed. Record which checks were automated, manually exercised, or unavailable; native window behavior and injected game hooks are not proven by headless egui tests.
