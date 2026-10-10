# Persistence and migration

## Ownership

| Data | Owner | Contract |
| --- | --- | --- |
| Application directories | [src/lib.rs](../../src/lib.rs) | Existing legacy `drg-mod-integration` directories take precedence over `mint`; `--appdata` selects separate config/cache/data directories. |
| `config.json` | [src/state/mod.rs](../../src/state/mod.rs) | Provider parameters, game path, theme, and sorting settings; legacy and versioned readers. |
| `profiles.json`, `mod_data.json` | [src/state/mod.rs](../../src/state/mod.rs) | Legacy profiles migrate into ordered individual/group entries. Current mod data takes precedence over the retained legacy backup. |
| JSON writes | [src/state/config.rs](../../src/state/config.rs) | Serialize to a temporary file in the destination directory, then persist it over the destination. |
| Settings enums | [src/state/settings.rs](../../src/state/settings.rs) | Theme and sorting wire names remain unchanged; GUI conversion/rendering consumes these types. |
| Profile/group edits | [src/state/edits.rs](../../src/state/edits.rs) | Validate mutations on a candidate copy before replacing state; preserve shared group references and reject stale confirmed deletions. |
| `cache.json` and blobs | [src/providers/cache.rs](../../src/providers/cache.rs), [ModStore](../../src/providers/mod_store.rs) | Versioned provider metadata, provider type tags, numeric mod/file keys, and SHA-256-addressed downloaded blobs. |

The source types and version declarations are authoritative; do not duplicate schema versions or defaults in UI code. Provider metadata is a cache, but deleting it can remove offline functionality. OAuth parameters are private data and must not enter fixtures or diagnostics.

## Read and write behavior

- Missing files use the owner's explicit default. Missing optional fields use compatible field defaults.
- Known legacy data is converted at the persistence boundary. Do not scatter legacy readers through rendering or provider callers.
- Malformed data returns an error without replacing the affected file. Unknown config/cache versions must fail safely, preserving the bytes for a newer reader or recovery.
- Validate active-profile and group references after migration and before constructing the writable profile wrapper. Syntactically valid JSON with invalid references is an error, not permission to drop entries or create defaults. Read/validate settings and profiles before initialization writes either of them.
- `profiles.json` is retained after migration as a recovery copy. Once `mod_data.json` exists, it is authoritative; an invalid current file must not silently resurrect older profiles. Removing the backup is a separate, explicit cleanup decision.
- Initialization writes settings, migrated mod data, and provider cache. Merely launching the app is not a read-only inspection.
- `ConfigWrapper` also attempts a save on drop. Drop failures are logged instead of panicking; callers that need to report success must call and check `save()` explicitly. This best-effort fallback is not a transaction or a guarantee against power loss.
- Provider-store initialization returns cache-save failures. GUI save failures remain visible through error details and retain in-memory changes for retry; do not report them as saved. Startup errors open a recovery screen with the error chain, relevant folders and retry, without automatically repairing/resetting data.
- Preserve the direct-byte deserialization workaround for tagged caches with numeric keys. A `serde_json::Value` round trip is not an interchangeable replacement without representative tests.
- Preserve saved manual order and the meaning of released sorting fields. Sorting is presentation state, not a migration of stored mod order.
- Group management uses the existing shared `groups` map and profile references without a schema bump. Rename updates every reference; profile deletion/detachment keeps shared definitions; global group deletion removes all references. Ungroup leaves other profiles unchanged and copies members in place, applying a disabled group flag to the resulting standalone entries.
- Indexed drag moves use the same validated edit owner, interpreting insertion indices before source removal. Moving out of a disabled group preserves the effective disabled state; moving within a shared group changes that definition for all referring profiles. Reject nested groups, stale source snapshots and invalid destination indices atomically. Drag state and UI identities are transient and do not change the saved schema.

## Migration procedure and evidence

Mod-data deserialization dispatches by the presence of the `version` field. Invalid or unsupported versioned content is never retried as legacy data, which could otherwise discard the shared group map. Unversioned legacy profiles still follow the existing migration path.

`ModGroup.color` is an optional additive field in the existing mod-data format. Stable snake-case `GroupColor` names identify 15 palette choices; missing color defaults to gray and gray is omitted when saving. It belongs to the shared group definition, survives rename and reload, and never changes membership or enabled state. Unknown color values fail deserialization without rewriting the source file. No schema-version bump is needed for this compatible addition. Older binaries can read the extra field but may discard the color when saving; mod contents and profile references remain compatible.

Identify the affected reader, writer, external shape, defaults, consumers, and oldest supported representation. Complete and validate conversion before replacing the current file; retain the source until the replacement succeeds. Do not use an empty default as recovery from an unknown schema or failed write.

Use temporary directories and deterministic local fixtures. Cover missing, legacy, current, malformed, future-version, failed-write, and retry paths as applicable. Check both the returned result and the original bytes. Reopen the saved state to verify that retained legacy files do not override newer edits.

Regression owners are [persistence_tests.rs](../../src/state/persistence_tests.rs), the cache tests in [cache.rs](../../src/providers/cache.rs), and the existing state/provider tests. Never use a real user's config or credentials as fixtures.

## Partial provider failures

`ModStore::resolve_mods_partial` preserves primary input order, deduplicates exact requests, then resolves dependencies in stable batches. Import retains successful entries and leaves failed specifications available to retry. The strict `resolve_mods` API still returns an error if any required resolution fails; integration must not silently install an incomplete request.

Cache refresh continues independent entries when one fails, preserves unavailable metadata/blobs, reports failed entries, and retains the old watermark so failures can be retried. The store attempts to persist successful updates and reports write errors. A partial refresh is not a fully successful update. Never remove offline data just because a remote mod is temporarily inaccessible.
