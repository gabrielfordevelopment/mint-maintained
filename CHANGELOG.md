# Change Log

<!-- next-header -->

## [Unreleased] - ReleaseDate

- Fix temporary executables left behind by Windows application updates. Cleanup waits for the app to exit and retries temporary file locks, while failed or interrupted replacements retain the previous executable.

## [0.5.2] - 2026-10-11

- Add a Show notifications setting, enabled by default, to hide all snackbars while keeping footer status and error details available. The choice is saved across restarts.

## [0.5.1] - 2026-10-11

### Application update fixes

- Fix Windows self-updates failing while replacing the running application. Retry temporary file locks and retain a recovery copy if replacement fails.
- Install the exact release shown in the update dialog and reject incomplete or invalid downloads before replacing the application.
- Keep update dialogs within the window, with scrollable release notes and visible action buttons. Dismiss with Close, Escape or Remind me later, and cancel downloads before installation begins.
- Check for application updates and reopen postponed updates from Settings. Show clear errors when checking or installing fails.
- Preserve the selected data directory when restarting after an update, and allow restarting later.

## [0.5.0] - 2026-10-11

### Mod updates and installation status

- Check for updates for one mod.io mod from its context menu, the current profile, or all cached mods. Checks refresh version information without installing files or changing pinned versions.
- Space mod.io requests and respect numeric server retry delays, including waits longer than 30 seconds. Show the remaining wait and allow cancellation while keeping completed cache updates.
- Show per-mod failures with readable reasons, expandable technical details and copyable reports. Retry failed checks or resume remaining checks without repeating successful ones.
- Indicate when a pinned version differs from the latest known version, or when the selected version still needs to be installed. Use Install mods to apply changes to the game.
- Track installed mod versions and effective profile changes, including load order and local file size/modification time. Older installations show an unknown tracking state until the next successful installation.

### Desktop usability

- Separate footer status and action buttons into two compact rows with consistent spacing and more room below the buttons. Keep colored status feedback and notifications above the footer.
- Rename Lint mods to Check mod files and explain that it checks enabled mods for file conflicts and packaging problems. Distinguish unselected checks, clean reports and findings; a clean report does not guarantee in-game compatibility.
- Place groups before standalone mods in new and explicitly arranged profiles, including imports and drag-and-drop. Existing layouts keep their saved load order until Arrange groups is confirmed; the arrangement supports Undo/Redo.

## [0.4.0] - 2026-10-10

- Add dismissible notifications with centered icons, a ten-second progress indicator that pauses on hover, and persistent error details.
- Add session undo/redo for the last 50 profile and mod-list edits, including shared groups, removals, imports and mod settings. Use toolbar controls, Ctrl+Z, Ctrl+Y / Ctrl+Shift+Z, or Undo in editing notifications. Game files and cache operations are excluded, and history resets when the app closes.
- Explain which active mod links were copied, excluding disabled mods and groups. Keep moves, toggles and successful Undo/Redo silent.
- Open confirmation dialogs without a focused button; Enter alone does nothing, while Tab navigation and Escape remain available.
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
[0.5.2]: https://github.com/gabrielfordevelopment/mint-maintained/releases/tag/v0.5.2
[0.5.1]: https://github.com/gabrielfordevelopment/mint-maintained/releases/tag/v0.5.1
[0.5.0]: https://github.com/gabrielfordevelopment/mint-maintained/releases/tag/v0.5.0
[0.4.0]: https://github.com/gabrielfordevelopment/mint-maintained/releases/tag/v0.4.0
[0.3.0]: https://github.com/gabrielfordevelopment/mint-maintained/releases/tag/v0.3.0
[0.2.10]: https://github.com/trumank/mint/compare/v0.2.9...v0.2.10
[0.2.9]: https://github.com/trumank/mint/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/trumank/mint/compare/v0.2.7...v0.2.8
