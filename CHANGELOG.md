# Change Log

<!-- next-header -->

## [Unreleased] - ReleaseDate

- Add dismissible notifications with centered icons, a ten-second progress indicator that pauses on hover, and persistent error details.
- Add session undo/redo for profile and mod-list edits, with toolbar controls, keyboard shortcuts and an Undo action in editing notifications.
- Replace the default egui icon with the MINT Maintained M monogram in application windows and the Windows executable.
- Unify collapsible sections and selection dropdowns with animated SVG chevrons.

## [0.3.0] - 2026-10-10

The first MINT Maintained release continues the work of Trumank's MINT and
Wasserkleber's mintfixed, with reliability fixes and a more usable compact desktop UI.
Development, updates and downloads are now managed in
[MINT Maintained](https://github.com/gabrielfordevelopment/mint-maintained).

### Reliability fixes

- Preserve the previous mod bundle and hook on handled installation failures. Stage outputs before replacement, restore prior outputs if replacement fails, and prevent concurrent installation writers.
- Fix crashes involving malformed network retry headers, unwritable caches, missing group references and Unicode searches.
- Preserve legacy profile backups and unsupported cache files; report save and startup failures with copyable details and recovery options.
- Keep successful entries when independent cache updates or imports fail, preserve import/download order, and correct CLI mod-to-file pairing and dependency handling.
- Fix grouped-profile sorting and row actions, including adjacent mods with missing metadata. Failed lint operations no longer leave a permanent busy state.

### Profiles, groups and sorting

- Confirm mod, profile and group removal; hold Shift when clicking to skip confirmation. Removing entries keeps the mod files on disk.
- Create and manage shared groups from the UI. Drag mods into, out of and between groups, with insertion feedback and temporary expansion when hovering over a collapsed group.
- Customize groups with 15 colors, visible member counts and full-width clickable headers. Newly created groups open immediately.
- Keep standalone mods before groups in new and explicitly arranged profiles. Existing mixed layouts require confirmation before their load order changes.
- Replace sorting radio buttons with a dropdown and direction control. A successful reorder in a sorted view adopts the displayed order and returns to Manual.

### Desktop usability

- Retain the compact layout while adding SVG icons, clearer inputs, consistent add/delete buttons and improved light/dark theme styling.
- Fix moving switch controls during reordering and dismiss dropdowns on outside clicks, including Windows title-bar clicks.
- Add a separate live log window with readable UTC timestamps, operation messages, wrapped lines, warning/error colors and optional debug output.
- Open the Windows GUI without an extra console window while preserving CLI use.
- Bundle offline font support for Chinese, Japanese and Korean names, alongside Cyrillic, Greek and accented Latin. Fix false search misses when mod metadata is absent.

### Releases and validation

- Use one Cargo workspace version for the application, CLI and integration metadata, and direct update checks to this repository.
- Add manually triggered draft releases with versioned Windows/Linux ZIPs, updater-compatible aliases and bundled license notices.
- Expand regression coverage for data preservation, error recovery, groups, drag and drop, sorting and multilingual text. Windows and Linux CI run formatting, Clippy, tests and builds.
- Real-game and injected-hook execution still require release testing. Recovery from handled installation failures does not guarantee atomic replacement across power loss or forced termination.

### Inherited changes since 0.2.10

The following entries are retained from the upstream changelog and are not new
MINT Maintained features.

#### General

- Fix unintentionally linking to libssl on Linux. This used to prevent some users on various Linux
  distros from being able to launch mint at all.

#### User Interface

- Add light/dark mode toggle to settings menu
- Replace escape menu modding tab with new modding menu
- Show mint mods in public server list
- Show time since last action
- Make mod URL searchable for mods without cache data
- Implement load priority to no longer rely on implicit ordering
- Add mod list sorting
- Slightly improved error reporting; mint should now indicate the mod that caused a failure
- Various GUI improvements

#### Core Functionality

- Implement Asset Registry handling. This should be sufficient for most mods that were previously
  not usable with mint due to the lack of Asset Registry handling.
- Implement self-update
- Fix mod url resolution
- Fix mods sometimes integrating in incorrect order
- Add patch to fix gas clouds not exploding sometimes
- Some mod save file fixes for Windows store version

#### Internal Changes

- Significantly optimize cache updates (first update will still be a full update)
- Allow overriding appdata dir via CLI flag
- Rename cache and config directories from `drg-mod-integration` to `mint` and default to legacy
  if they exist
- Fix Windows console being full of garbage characters
- Disable LTO for dev builds
- Add nix flake
- Improved testing
- Various CI improvements
- Reduce release size by removing debug symbols
- Package Linux target with zip instead of tar
- Compress bundled mod pak

## [0.2.10] - 2023-08-18

- Many small improvements to the GUI
- Add simple in game UI to show local and remote integration version and active mods
- Add experimental mod linting to assist with common mod issues such as conflicts ([#55](https://github.com/trumank/mint/pull/55))
- Microsoft Store: Fix mods being unable to write custom save files ([#58](https://github.com/trumank/mint/issues/58))
- Fix `profile` CLI command not respecting mod's `enable` flag

## [0.2.9] - 2023-08-11

- Update `egui_dnd` which makes dragging and re-ordering mods significantly smoother
- Restore modding subsystem config upon uninstalling to prevent all mods getting enabled and kicking the user to sandbox
- Fix regression introduced by case sensitive path fix ([#36](https://github.com/trumank/mint/issues/36))

## [0.2.8] - 2023-08-05

- Fix `*.ushaderbytecode` files not being filtered out and causing crash on load
- Fix including same asset paths with different casings causing Unreal Engine to load neither ([#29](https://github.com/trumank/mint/issues/29))

<!-- next-url -->
[Unreleased]: https://github.com/gabrielfordevelopment/mint-maintained/compare/master...develop
[0.3.0]: https://github.com/gabrielfordevelopment/mint-maintained/releases/tag/v0.3.0
[0.2.10]: https://github.com/trumank/mint/compare/v0.2.9...v0.2.10
[0.2.9]: https://github.com/trumank/mint/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/trumank/mint/compare/v0.2.7...v0.2.8
