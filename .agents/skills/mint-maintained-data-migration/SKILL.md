---
name: mint-maintained-data-migration
description: Change MINT settings, profiles, provider caches, persisted files, schemas or compatibility readers while preserving released data and offline use.
---

# Data migration

Read the affected sections of [persistence.md](../../../docs/architecture/persistence.md). Inspect the current reader, writer, defaults, released wire names, migration path, consumers, and tests before changing the representation.

1. Classify the data as settings, profiles/groups, provider metadata, downloaded blobs, or a new portable format. Do not treat durable user choices as disposable cache.
2. Define missing, legacy, current, malformed, unknown-version, failed-write, and retry behavior before changing the writer. Preserve unsupported data for recovery rather than resetting it.
3. Convert at the owning persistence boundary. Retain the old source until a validated replacement exists; preserve the existing legacy profile backup policy. Keep a single current authoritative representation.
4. Check initialization and drop-time writes as well as explicit saves. Return meaningful errors for expected input/IO failures; avoid a second panic while unwinding. Never log provider parameters or real tokens.
5. Use the existing temporary-directory tests to verify both results and files left on disk. Include reload/current-file precedence and a deterministic failed write; permission-bit tests alone are unreliable across Windows, Linux, and privileged runners.
6. Update the persistence contract only when its behavior, ownership, compatibility policy, or validation path changes. Complete the root validation requirements and report any untested upgrade path.
