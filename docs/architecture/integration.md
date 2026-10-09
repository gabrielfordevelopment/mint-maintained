# Game integration and hook contract

## Owners and data flow

1. [Providers](../../src/providers/mod_store.rs) resolve mod specifications, dependencies, metadata and cached/downloaded content. Keep pinned/latest versions, offline use, and ordered results compatible.
2. [src/integrate.rs](../../src/integrate.rs) reads the game/mod pak files, applies asset patches, writes `mods_P.pak`, and installs the Windows proxy DLL. Uninstall removes integration outputs from the selected installation.
3. [mint_lib/src/mod_info.rs](../../mint_lib/src/mod_info.rs) defines the shared `Meta` data serialized into the pak's `meta` entry with postcard.
4. [hook/src/lib.rs](../../hook/src/lib.rs) reads that entry inside the game and uses [hook_resolvers](../../hook_resolvers/src/lib.rs) to find game-version-dependent native functions and memory structures.

Treat metadata writer/reader shape, pak paths, proxy DLL names, game signatures, and native layouts as a coordinated compatibility boundary. A Rust type compiling in both crates does not prove that an already installed pak and DLL agree. Review Steam and Microsoft Store installation variants together.

## Filesystem and runtime boundaries

Integration writes into the selected game directory, independently of `--appdata`. It builds the pak and hook in temporary files beside their destinations, flushes their contents, and prepares recovery copies before replacing outputs. [transaction.rs](../../src/integrate/transaction.rs) owns replacement and rollback. On a replacement failure, previously replaced outputs are restored; if restoration itself fails, preserve the recovery copy and report its path. This is a recoverable multi-file operation, not a guarantee of atomicity across power loss or forced process termination.

Integration and uninstall acquire an OS file lock through `.mint-install.lock` in the game's pak directory. The lock file may remain on disk; ownership is released when its handle closes, including process termination. Never remove the file as a substitute for waiting for the owner. This serializes cooperating MINT operations; it does not coordinate third-party programs or older MINT versions.

GUI cancellation signals the operation without abandoning an already-running blocking writer. Downloads can stop early; integration checks cancellation between assets and before committing outputs. Once replacement begins, complete it or roll back before reporting completion. Keep the GUI busy until the worker returns. Uninstall still has its existing sequential removal semantics; do not describe it as a transactional restore.

Use controlled fixtures for ordinary development. Real-game integration, uninstall, launch, and injected-hook checks require authorization covering that target and operation.

For integration changes, establish expected outputs and behavior on read/write failure before implementation. Preserve inputs, avoid unrelated cleanup, and report any rollback limitation. For unsafe/native changes, inspect the relevant call sites, layout assumptions, lifetime, and supported game version; do not change signatures based solely on a host build.

## Validation and releases

Use [tests/lint/mod.rs](../../tests/lint/mod.rs) and existing pak fixtures for lint regressions. Test serialization compatibility when the shared metadata changes. Host tests cover `mint` and `mint_lib`; report actual in-game testing separately with platform/game version and operations performed.

[tests/reliability.rs](../../tests/reliability.rs) covers malformed input preserving installed outputs and ordered download identity. Transaction unit tests cover replacement failure/rollback/retry, equal-sized changed outputs, cancellation and lock lifetime. Shared profile load ordering is owned by `ModData::enabled_mods_ordered`: descending priority with stable ties and enabled-group filtering, used by CLI and GUI integration.

[mint_lib/src/update.rs](../../mint_lib/src/update.rs), the downloader in [src/gui/message.rs](../../src/gui/message.rs), [release.yml](../../.github/workflows/release.yml), and [scripts/release.py](../../scripts/release.py) jointly define repository URLs, archive names, and extraction paths. Versioned `mint-vX.Y.Z-<target>.zip` downloads have byte-identical `mint-<target>.zip` aliases for existing updaters. Preserve root `mint.exe`/`mint`, executable permissions, and packaged notices. The Cargo workspace version also populates integration metadata; it remains a string in the existing serialized contract. Read the [Git workflow](../../.agents/skills/mint-maintained-git-workflow/SKILL.md) before any release operation; preparing a build is not authorization to publish it.
