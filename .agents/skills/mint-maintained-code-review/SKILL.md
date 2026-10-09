---
name: mint-maintained-code-review
description: Review MINT diffs or pull requests for correctness, saved-data loss, native integration, asynchronous state, UI regressions, dependencies and merge readiness.
---

# Code review

Inspect the complete relevant diff, its base, surrounding callers and tests. A review alone does not authorize modifying code. Prioritize concrete user impact over stylistic preferences.

- For storage changes, use [persistence.md](../../../docs/architecture/persistence.md) and the [migration skill](../mint-maintained-data-migration/SKILL.md). Check bytes preserved on error, initialization/drop writes, legacy/current precedence, and retry behavior.
- For GUI changes, read [desktop-ui.md](../../../docs/architecture/desktop-ui.md). Check stored versus displayed indices, group boundaries, missing metadata, compact sizing, themes, focus, and stale asynchronous completions. Reuse shared controls instead of parallel implementations.
- For pak, hook, or updater changes, read [integration.md](../../../docs/architecture/integration.md). Inspect writer/reader compatibility, actual write targets, error/cancellation behavior, native layouts and platform coverage. Host builds do not prove injected runtime correctness.
- Verify dependencies are necessary, their licenses/notices remain valid, lockfile changes are relevant, and untrusted archives/URLs and credentials stay within their intended boundaries.
- Require meaningful regression tests for logic and data changes, or concrete manual evidence where automation cannot observe the behavior. Match reported validation to the exact reviewed commit and identify actual coverage gaps.
- Confirm changed contracts and skill routes were updated. Prefer tests/CI for deterministic formatting or structural rules; do not turn optional refactors or speculative risks into merge blockers.

Report findings in severity order with impact, a precise file/line, affected contract, practical remedy, and whether they block merge. Finish with a clear readiness verdict and only material remaining validation gaps. If no findings remain, say so.
