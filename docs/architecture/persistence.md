# Persistence and migration

## Ownership

| Data | Owner | Contract |
| --- | --- | --- |
| Application directories | [src/lib.rs](../../src/lib.rs) | Existing legacy `drg-mod-integration` directories take precedence over `mint`; `--appdata` selects separate config/cache/data directories. |
| `config.json` | [src/state/mod.rs](../../src/state/mod.rs) | Provider parameters, game path, theme, and sorting settings; legacy and versioned readers. |
| `profiles.json`, `mod_data.json` | [src/state/mod.rs](../../src/state/mod.rs) | Legacy profiles migrate into ordered individual/group entries. Current mod data takes precedence over the retained legacy backup. |
| JSON writes | [src/state/config.rs](../../src/state/config.rs) | Serialize to a temporary file in the destination directory, then persist it over the destination. |
| `cache.json` and blobs | [src/providers/cache.rs](../../src/providers/cache.rs), [ModStore](../../src/providers/mod_store.rs) | Versioned provider metadata, provider type tags, numeric mod/file keys, and SHA-256-addressed downloaded blobs. |

The source types and version declarations are authoritative; do not duplicate schema versions or defaults in UI code. Provider metadata is a cache, but deleting it can remove offline functionality. OAuth parameters are private data and must not enter fixtures or diagnostics.

## Read and write behavior

- Missing files use the owner's explicit default. Missing optional fields use compatible field defaults.
- Known legacy data is converted at the persistence boundary. Do not scatter legacy readers through rendering or provider callers.
- Malformed data returns an error without replacing the affected file. Unknown config/cache versions must fail safely, preserving the bytes for a newer reader or recovery.
- `profiles.json` is retained after migration as a recovery copy. Once `mod_data.json` exists, it is authoritative; an invalid current file must not silently resurrect older profiles. Removing the backup is a separate, explicit cleanup decision.
- Initialization writes settings, migrated mod data, and provider cache. Merely launching the app is not a read-only inspection.
- `ConfigWrapper` also attempts a save on drop. Drop failures are logged instead of panicking; callers that need to report success must call and check `save()` explicitly. This best-effort fallback is not a transaction or a guarantee against power loss.
- Preserve the direct-byte deserialization workaround for tagged caches with numeric keys. A `serde_json::Value` round trip is not an interchangeable replacement without representative tests.
- Preserve saved manual order and the meaning of released sorting fields. Sorting is presentation state, not a migration of stored mod order.

## Migration procedure and evidence

Identify the affected reader, writer, external shape, defaults, consumers, and oldest supported representation. Complete and validate conversion before replacing the current file; retain the source until the replacement succeeds. Do not use an empty default as recovery from an unknown schema or failed write.

Use temporary directories and deterministic local fixtures. Cover missing, legacy, current, malformed, future-version, failed-write, and retry paths as applicable. Check both the returned result and the original bytes. Reopen the saved state to verify that retained legacy files do not override newer edits.

Regression owners are [persistence_tests.rs](../../src/state/persistence_tests.rs), the cache tests in [cache.rs](../../src/providers/cache.rs), and the existing state/provider tests. Never use a real user's config or credentials as fixtures.
