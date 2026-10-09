# Game integration and hook contract

## Owners and data flow

1. [Providers](../../src/providers/mod_store.rs) resolve mod specifications, dependencies, metadata and cached/downloaded content. Keep pinned/latest versions, offline use, and ordered results compatible.
2. [src/integrate.rs](../../src/integrate.rs) reads the game/mod pak files, applies asset patches, writes `mods_P.pak`, and installs the Windows proxy DLL. Uninstall removes integration outputs from the selected installation.
3. [mint_lib/src/mod_info.rs](../../mint_lib/src/mod_info.rs) defines the shared `Meta` data serialized into the pak's `meta` entry with postcard.
4. [hook/src/lib.rs](../../hook/src/lib.rs) reads that entry inside the game and uses [hook_resolvers](../../hook_resolvers/src/lib.rs) to find game-version-dependent native functions and memory structures.

Treat metadata writer/reader shape, pak paths, proxy DLL names, game signatures, and native layouts as a coordinated compatibility boundary. A Rust type compiling in both crates does not prove that an already installed pak and DLL agree. Review Steam and Microsoft Store installation variants together.

## Filesystem and runtime boundaries

Integration writes into the real selected game directory, independently of `--appdata`. Existing output can be truncated/replaced during integration; do not assume the operation is transactional or that cancellation restores the previous installation. Use controlled fixtures for ordinary development. Real-game integration, uninstall, launch, and injected-hook checks require authorization covering that target and operation.

For integration changes, establish expected outputs and behavior on read/write failure before implementation. Preserve inputs, avoid unrelated cleanup, and report any rollback limitation. For unsafe/native changes, inspect the relevant call sites, layout assumptions, lifetime, and supported game version; do not change signatures based solely on a host build.

## Validation and releases

Use [tests/lint/mod.rs](../../tests/lint/mod.rs) and existing pak fixtures for lint regressions. Test serialization compatibility when the shared metadata changes. Host tests cover `mint` and `mint_lib`; report actual in-game testing separately with platform/game version and operations performed.

[mint_lib/src/update.rs](../../mint_lib/src/update.rs), the downloader in [src/gui/message.rs](../../src/gui/message.rs), and [release.yml](../../.github/workflows/release.yml) jointly define repository URLs, archive names, and extraction paths. Preserve root `mint.exe`/`mint` and the Windows/Linux archive names. Read the [Git workflow](../../.agents/skills/mint-maintained-git-workflow/SKILL.md) before any release operation; preparing a build is not authorization to publish it.
