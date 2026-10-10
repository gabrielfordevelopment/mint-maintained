# MINT Maintained agent guidance

## Project and scope

- MINT Maintained is an independently maintained Deep Rock Galactic mod manager and integrator, continuing Trumank's MINT and Wasserkleber's mintfixed. Preserve their attribution and existing licenses.
- The canonical repository is `https://github.com/gabrielfordevelopment/mint-maintained`. Historical upstream repositories are references, not publication or push targets.
- This is a native Rust desktop application using egui/eframe and Tokio, with CLI commands for integration, profiles, launching, and linting. It supports Steam and Microsoft Store installations, local files, HTTP downloads, and mod.io.
- Core features include mod profiles and groups, enable/disable controls, version selection, priorities, sorting, dependency resolution, caching, linting, installation, and self-updates.
- The application is branded MINT Maintained; the Cargo package and executable remain `mint`. Renaming the repository does not authorize changing executable names, saved-data locations, or formats.
- Use `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/config.toml`, and `.github/workflows/` as the source of truth for dependencies, toolchains, commands, and CI. Do not import web application tooling from CompareCode.

## Code ownership

| Location | Responsibility |
| --- | --- |
| `src/main.rs`, `src/lib.rs` | CLI, startup, application directories, shared application helpers |
| `src/gui/mod.rs`, `src/gui/message.rs` | egui application shell, asynchronous operation dispatch and results |
| `src/gui/mod_list.rs`, `sorting.rs`, `settings.rs`, `lint.rs` | Mod rows, display ordering, settings/provider dialogs, lint selection and reports |
| `src/gui/editing.rs`, `groups.rs`, `src/state/edits.rs` | Deletion confirmation, shared-group management, validated profile/group operations |
| `src/gui/drag_drop.rs`, `src/gui/log_window.rs` | Drag targets and feedback; bounded asynchronous log reader and native log viewport |
| `src/gui/icons.rs`, `assets/icons/` | Shared SVG controls, display-scale-aware texture caching, icon sources and licenses |
| `src/gui/named_combobox.rs`, `src/gui/tests.rs` | Profile/group controls and GUI interaction/sorting regression tests |
| `src/state/`, `src/state/settings.rs` | Settings types, profiles, groups, versioned serialization, migrations, JSON persistence |
| `src/windows_startup.rs` | Windows parent-console attachment while preserving redirected CLI streams |
| `src/providers/` | Local/HTTP/mod.io providers, metadata and blob caches, dependency resolution and downloads |
| `src/integrate.rs`, `src/mod_lints/` | Game pak integration, hook installation/uninstallation, mod validation |
| `mint_lib/` | Shared mod/game contracts, installation discovery, logging, update URLs and build metadata |
| `hook/`, `hook_resolvers/` | Windows proxy DLL, Unreal runtime hooks, game-memory signature resolution |
| `tests/`, `test_assets/` | Integration tests and their fixtures |
| `.github/workflows/`, `release.toml`, `flake.nix` | CI, release packaging, cargo-release configuration, Nix builds |

## Working agreements

- Reply in the user's language. Write code, comments, instructions, technical documentation, branch names, commit messages, and PR text in English.
- Inspect the affected implementation, callers, tests, and integration points before editing. Extend the existing owner instead of duplicating behavior in another layer.
- Preserve unrelated changes and user-created commits. Keep changes within the requested scope; do not mix UI polish with unrelated dependency upgrades or cache refactors.
- Treat tests as behavior contracts. Add meaningful regression coverage for logic fixes; do not weaken tests to hide production failures.
- Keep UTF-8 text, preferably without BOM. Update this guidance or its linked skill when a change makes them inaccurate. Keep one root `AGENTS.md`; put reusable workflows in `.agents/skills/`.
- Resolve questions from code, tests, and existing decisions first. Ask only about material unresolved behavior, data loss, external effects, or scope; use a stated reversible assumption for minor choices. Do not reopen decisions without new evidence.
- Read the affected architecture contract before changing its behavior, and update it in the same task when necessary: [persistence](docs/architecture/persistence.md), [desktop UI and background work](docs/architecture/desktop-ui.md), [game integration and hook](docs/architecture/integration.md).

## Behavior and compatibility

### Desktop UI and sorting

- Preserve the compact layout, row heights, control order, and placement unless a layout change is requested. Do not add a table header or enlarge controls as incidental polish.
- Reuse the SVG controls in `src/gui/icons.rs`; keep vector sources in `assets/icons/`. Avoid Unicode stand-ins for existing icons and unnecessary texture regeneration each frame.
- Add/delete icons use the normal theme foreground; green/red feedback belongs to hover and pressed states. Theme selection uses the standard egui selection highlight. Preserve button padding, footer alignment, keyboard interaction, and focus feedback.
- Sorting must preserve saved manual order and group boundaries. Sort individual runs and group children within their own scope; use stored indices and group identity for row actions rather than sorted display positions.
- Missing metadata is a supported state, including adjacent mods where one or both lack metadata. Comparators must remain deterministic and consistent. Cover these cases when changing sorting.
- Preserve the persisted interpretation of sorting settings, including the existing `is_ascending` field, unless an explicit behavior change includes compatibility handling.
- Keep network, downloads, and integration work outside the UI frame. Preserve request-ID checks that prevent stale asynchronous results from replacing current state.
- Route destructive profile/list/group actions through `src/state/edits.rs` and the shared GUI confirmation path. Normal clicks confirm; Shift at the originating click bypasses confirmation. Cancel/Escape must preserve data, and stale confirmations must not act on changed lists.
- Groups are shared across profiles. Removing a group row detaches only that profile reference; deleting a shared group removes it and its references globally. Ungroup copies members into that profile in order and preserves their effective enabled state. Show affected profiles before global/shared deletion; never delete downloaded files as part of list editing.
- Dragging is available in Manual sorting. Commit moves only on a valid drop, validate the captured source state, preserve switch identities across reorder, and show group/insert feedback. Escape and drops outside the list must leave data unchanged; groups cannot be nested.
- Dropdowns and name-entry popups close on outside clicks. Keep the live log in its own native viewport, read a bounded tail asynchronously, redact provider secrets, and stop its reader when the window closes.

### Saved data and providers

- Existing data may use legacy `drg-mod-integration` or newer `mint` directories. Keep directory fallback behavior and the `--appdata` override compatible.
- Settings, profiles/groups, provider metadata, and cached blobs have distinct owners and formats. Inspect `src/state/` and `src/providers/cache.rs` before changing their serialization or initialization.
- Opening the app can write data: initialization saves the provider cache, and `ConfigWrapper` saves on drop. Investigate failures using copies or an isolated data directory, not the user's live files.
- Never silently replace malformed data or an unsupported schema with an empty default. Preserve recoverable files and use explicit migration/error handling when changing persistence.
- Preserve version tags, legacy profile migration, provider type tags, and numeric map-key handling. Cache deserialization workarounds must not be removed without representative old/new-format tests.
- Preserve pinned/latest-version behavior, dependency resolution, offline cache use, and ordering where resolution/fetch results determine mod load order.
- Keep the mod.io filtered-list endpoint workarounds unless verified API behavior justifies changing them. Never expose OAuth tokens or real provider configuration in logs, fixtures, screenshots, or commits.

### Game integration and updates

- Integration and uninstall write into the selected game installation. Use fixtures for ordinary tests; exercise a real installation only within the user's authorized testing scope.
- Changes to pak contents, serialized metadata in `mint_lib`, and readers in `hook` must stay compatible. Consider Steam and Microsoft Store paths and game-version-dependent signatures.
- Do not equate passing host tests with successful in-game testing. Report hook/runtime testing separately and name any untested platform.
- Keep update URLs derived from this repository's metadata. Release archive names and executable paths must match `mint_lib/src/update.rs` and `src/gui/message.rs` expectations.
- The application version is `[workspace.package].version` in `Cargo.toml`; all workspace packages inherit it. Update it only during authorized release preparation, refresh the workspace entries in `Cargo.lock` without upgrading dependencies, and update `CHANGELOG.md` on a task branch through a PR to `develop`. See the [release procedure](.agents/skills/mint-maintained-git-workflow/SKILL.md); the manual workflow reads the committed version and never bumps it.
- Preserve existing license notices and provenance. For new dependencies, verify necessity, license, enabled features, and relevant transitive risks. Keep `Cargo.lock` and affected notices consistent; avoid unrelated lockfile churn.
- `THIRD_PARTY_NOTICES.md` and the icon license files are already embedded in Settings. Update those owners instead of adding a competing notice system.

## Development and validation

- Use the repository's nightly Rust toolchain. Cargo artifact dependencies require the existing `bindeps` configuration and the Windows hook target, including when building the Linux loader.
- On Windows, use an x64 Visual Studio Developer PowerShell with the MSVC C++ tools and Windows SDK. If detection picks an incomplete installation, initialize a working Build Tools installation explicitly.
- Linux CI installs `libgtk-3-dev`, `gcc-mingw-w64`, and `pkg-config`; consult the workflow before changing platform setup.
- For a development preview, use a dedicated directory outside the repository, for example `cargo run --locked -- --appdata ../mint-maintained-dev-data`. Do not add a real game path or credentials merely to populate a UI demo.
- A fresh config can automatically discover the real game even with `--appdata`. For UI-only demos, clear the isolated config's game path and avoid installation, uninstall, launch, and self-update actions. Report headless, visible-desktop, and in-game validation separately.
- For production Rust, dependency, or build-configuration changes, run the CI checks before a requested commit or PR:

  ```text
  cargo fmt -- --check
  cargo clippy --locked --all-features --all-targets -- -D warnings
  cargo test --locked -p mint -p mint_lib
  cargo build --locked
  git diff --check
  ```

- While iterating, run relevant tests first. For test-only changes, run formatting, the affected tests, and Clippy. For documentation/instruction-only changes, check structure, local links, skill frontmatter, whitespace, and the final diff; no application build is required.
- The CI host-test scope is `mint` and `mint_lib`. Do not substitute `cargo test --workspace` without reviewing the injected hook's runtime assumptions.
- For UI changes, inspect both themes, normal/hover/selected/disabled states, footer and Settings sizing, long names, groups, and missing metadata. Use the existing egui interaction tests where appropriate; say explicitly if manual interaction was unavailable.
- Run workflow validation for CI changes and platform-specific checks where available. Report failed, skipped, or unavailable checks accurately; never claim prior-task results as current validation.
- For agent guidance, architecture documents, or their checker, run `python scripts/check_agent_guidance.py` and `python -m unittest discover -s scripts/tests`. The `Agent guidance` CI job enforces structural validity; content still requires review against the implementation.

## Git authority and skill routing

- Use [mint-maintained-git-workflow](.agents/skills/mint-maintained-git-workflow/SKILL.md) for branching, staging, committing, pushing, PR preparation, merging, and releases.
- Use [mint-maintained-data-migration](.agents/skills/mint-maintained-data-migration/SKILL.md) for persisted settings, profiles, cache formats, migrations, and compatible readers/writers.
- Use [mint-maintained-desktop-testing](.agents/skills/mint-maintained-desktop-testing/SKILL.md) for native GUI tests, visible previews, demo data, interaction evidence, and session cleanup.
- Use [mint-maintained-code-review](.agents/skills/mint-maintained-code-review/SKILL.md) for requested code/diff/PR reviews and pre-merge assessments.
- Use [mint-maintained-agent-files](.agents/skills/mint-maintained-agent-files/SKILL.md) for changing or auditing instruction files, skills, architecture contracts, and their routing.
- `develop` is the repository's default branch and the default target for all delivery and PRs. `master` remains the release branch (the branch the user may call `main`). Target `master` only when the user explicitly requests that release-branch destination for the current task; a general request to commit, push, or merge means delivery through a PR to `develop`. Never create or rename a branch to `main` based on that wording. These instructions do not themselves create branches or configure GitHub protection.
- Interpret `dev`, `develop`, and `development` as the repository's actual `develop` branch. Do not create separate branches for these aliases.
- Follow the skill's setup guidance if `develop` is missing locally or remotely. Keep `master` under its existing name; do not substitute it for the integration branch.
- Creating/switching branches, committing, pushing, opening PRs, merging, changing repository settings, and publishing releases require authorization for the relevant action. Existing authorization in the conversation remains valid; do not ask for it again.
- Never commit directly to, merge locally into, or push directly to `develop` or `master`. All changes, including version bumps, documentation, and agent instructions, enter these branches through GitHub PRs. Check the current branch before every commit; if it is a shared branch, move the task changes to a task branch first. Do not rewrite history, change Git identity/signing, or alter remotes without an explicit request. Never push to a historical upstream repository.
- Use task-based branch prefixes such as `feature/`, `fix/`, and `chore/` from the Git skill's allowed list; never use `codex/` or other agent/tool prefixes.
- Use English Conventional Commit subjects of at most 70 characters, including the type and optional scope. Do not write commit descriptions or explanatory bodies; retain only the required DCO sign-off trailer. Keep PR titles short, clear, and at most 70 characters. Follow the Git skill and preserve user-approved PR wording and user-added screenshots when updating a PR.
