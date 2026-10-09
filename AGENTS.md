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
| `src/gui/mod.rs`, `src/gui/message.rs` | egui application, sorting and row actions, asynchronous operation dispatch and results |
| `src/gui/icons.rs`, `assets/icons/` | Shared SVG controls, display-scale-aware texture caching, icon sources and licenses |
| `src/gui/named_combobox.rs`, `src/gui/tests.rs` | Profile/group controls and GUI interaction/sorting regression tests |
| `src/state/` | Settings, profiles, groups, versioned serialization, migrations, JSON persistence |
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

## Behavior and compatibility

### Desktop UI and sorting

- Preserve the compact layout, row heights, control order, and placement unless a layout change is requested. Do not add a table header or enlarge controls as incidental polish.
- Reuse the SVG controls in `src/gui/icons.rs`; keep vector sources in `assets/icons/`. Avoid Unicode stand-ins for existing icons and unnecessary texture regeneration each frame.
- Add/delete icons use the normal theme foreground; green/red feedback belongs to hover and pressed states. Theme selection uses the standard egui selection highlight. Preserve button padding, footer alignment, keyboard interaction, and focus feedback.
- Sorting must preserve saved manual order and group boundaries. Sort individual runs and group children within their own scope; use stored indices and group identity for row actions rather than sorted display positions.
- Missing metadata is a supported state, including adjacent mods where one or both lack metadata. Comparators must remain deterministic and consistent. Cover these cases when changing sorting.
- Preserve the persisted interpretation of sorting settings, including the existing `is_ascending` field, unless an explicit behavior change includes compatibility handling.
- Keep network, downloads, and integration work outside the UI frame. Preserve request-ID checks that prevent stale asynchronous results from replacing current state.

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
- Preserve existing license notices and provenance. For new dependencies, verify necessity, license, enabled features, and relevant transitive risks. Keep `Cargo.lock` and affected notices consistent; avoid unrelated lockfile churn.
- `THIRD_PARTY_NOTICES.md` and the icon license files are already embedded in Settings. Update those owners instead of adding a competing notice system.

## Development and validation

- Use the repository's nightly Rust toolchain. Cargo artifact dependencies require the existing `bindeps` configuration and the Windows hook target, including when building the Linux loader.
- On Windows, use an x64 Visual Studio Developer PowerShell with the MSVC C++ tools and Windows SDK. If detection picks an incomplete installation, initialize a working Build Tools installation explicitly.
- Linux CI installs `libgtk-3-dev`, `gcc-mingw-w64`, and `pkg-config`; consult the workflow before changing platform setup.
- For a development preview, use a dedicated directory outside the repository, for example `cargo run --locked -- --appdata ../mint-maintained-dev-data`. Do not add a real game path or credentials merely to populate a UI demo.
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

## Git authority and skill routing

- Use [mint-maintained-git-workflow](.agents/skills/mint-maintained-git-workflow/SKILL.md) for branching, staging, committing, pushing, PR preparation, merging, and releases.
- The integration branch is `develop`; `master` remains the release branch. Feature/fix PRs target `develop`; release PRs target `master`. These instructions do not themselves create branches or configure GitHub protection.
- Interpret `dev`, `develop`, and `development` as the repository's actual `develop` branch. Do not create separate branches for these aliases.
- Follow the skill's setup guidance if `develop` is missing locally or remotely. Keep `master` under its existing name; do not substitute it for the integration branch.
- Creating/switching branches, committing, pushing, opening PRs, merging, changing repository settings, and publishing releases require authorization for the relevant action. Existing authorization in the conversation remains valid; do not ask for it again.
- Keep routine work off shared integration/release branches. Do not rewrite history, change Git identity/signing, or alter remotes without an explicit request. Never push to a historical upstream repository.
- Use English Conventional Commit subjects and DCO sign-offs through the Git skill. Preserve user-approved PR wording and user-added screenshots when updating a PR.
