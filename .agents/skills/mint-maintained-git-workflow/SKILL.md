---
name: mint-maintained-git-workflow
description: Handle MINT Maintained branches, staging, commits, pushes, pull requests, merges, and releases in gabrielfordevelopment/mint-maintained, using develop for integration and master for releases. Use when the user requests Git delivery work or preparation of its PR text; preserve approved wording and keep historical upstream repositories untouched.
---

# MINT Maintained Git workflow

Read the repository's [AGENTS.md](../../../AGENTS.md) for architecture, compatibility rules, and required validation. Apply only the actions authorized by the user; preparing a local change or PR description does not authorize publication.

## Establish state and scope

1. Inspect the worktree, branch, history, remotes, push URLs, tracking configuration, and any relevant open PR before changing Git state. Preserve unrelated work and user-created commits.
2. The canonical destination is `gabrielfordevelopment/mint-maintained`. Verify it explicitly for GitHub operations; fork-network defaults can point at the historical parent. Never send commits or PRs to Trumank or Wasserkleber without a new explicit user request.
3. Check the existing conversation for authorization. A request to open/update a PR permits the branch push needed to deliver that change, but does not authorize merging, tagging, or publishing a release. A local-only request prohibits those remote changes.
4. Do not modify Git identity, signing, remotes, default-branch settings, or protection rules unless that change is requested. Missing credentials or permissions are blockers to report, not reasons to change the destination.

## Branch model

- `develop` is the repository's default branch and the default target for all delivery and PRs. A general request to commit, push, or merge uses `develop` as the eventual PR destination, never the release branch. Perform only the delivery actions the user authorized; a commit-only request does not authorize a push or PR.
- Interpret `dev`, `develop`, and `development` as the actual `develop` branch. These are aliases in conversation, not separate Git branches.
- `master` contains release-ready history and keeps its existing name. When the user refers to the release branch as `main`, use the existing `master`; do not create or rename a branch. Target `master` only with explicit user authorization for that destination in the current task, through a release PR from `develop` or an explicitly requested hotfix PR. Release tags belong to tested commits on `master`.
- Start ordinary work from the latest fetched `origin/develop`. Use a short English branch name with a task-based prefix: `feature/`, `fix/`, `chore/`, `docs/`, `refactor/`, `perf/`, `test/`, `ci/`, `build/`, `style/`, or `revert/`. Never use `codex/` or other agent/tool prefixes, even when an agent's default suggests them.
- Create a new branch without tracking the base branch, for example `git switch --no-track -c fix/missing-metadata origin/develop`. On its first authorized push, set its upstream to the same-named branch on `origin`.
- Continue an existing task branch when appropriate. Never reset, rebase, squash, amend, or force-push user history without explicit authorization. Use merge-based updates when an authorized task needs the current integration branch.
- Never commit directly to, merge locally into, or push directly to `develop` or `master`. All changes enter through GitHub PRs, including documentation, agent guidance, and version bumps. An explicitly requested release hotfix may branch from `master`; deliver it through a PR and bring it back to `develop` through another authorized PR.

## Initial develop setup

This is a conditional setup workflow, not permission to change branches while editing documentation. Inspect actual local and remote refs first; skip completed steps rather than recreating them.

When the user authorizes the branch setup:

1. Verify the own-repository remote, a suitable worktree state, and the exact history to preserve. Compare existing local and remote `master` and `develop` refs; do not overwrite an existing branch.
2. Keep `master` unchanged and create `develop` from its accepted baseline unless existing refs require a different, reviewed integration. Retain unrelated task branches. For an explicitly local-only setup, use the verified local baseline and defer remote synchronization/publication.
3. Create the setup task branch from local `develop` without tracking the base. Commit guidance and configuration there so they can later be reviewed in a PR to `develop`.
4. Keep release automation restricted to `master`, update development/release instructions in `README.md`, and inspect workflow branch filters and repository links. The manual release workflow must use the committed Cargo version and one tested commit for checks, builds, and tagging.
5. When remote setup is authorized, fetch and compare the current remote state before publishing. Push only to the own repository, establish matching tracking refs, and set `develop` as the GitHub default branch. Refresh `origin/HEAD` after the remote default changes.
6. Keep `master` as the release branch. Do not rename/delete it or alter protection settings as part of this setup. Report the resulting local/remote branches and settings actually verified; documentation policy is not evidence that GitHub protection has been configured.

## Stage and commit

1. Check `git branch --show-current` before every commit. Never commit on `develop` or `master`; move task changes to a task branch first. Review the complete diff, including new files, for scope, accidental data/credentials, generated artifacts, dependency churn, and stale guidance. Stage only intended paths or hunks.
2. Complete the checks required by `AGENTS.md` for this change type. Inspect the staged diff and run `git diff --cached --check`. Resolve introduced failures without unrelated cleanup; report any environment limitation before claiming readiness.
3. Use the existing configured human author identity. Add a DCO trailer with `git commit -s`; do not invent an identity or add an AI co-author trailer.
4. Write one short English Conventional Commit subject of at most 70 characters, counting the type, optional scope, punctuation, and spaces. Describe the outcome, for example `fix: handle adjacent mods without metadata`. Do not add a commit description, explanatory body, bullet list, or PR text. The required DCO sign-off trailer is the only exception to the subject-only format. Check the complete message before committing; apply the same format to merge commits and do not accept an automatically generated merge description.
5. Verify the resulting commit, sign-off, branch, and remaining worktree changes. If the request is local-only, stop delivery here; do not push.

## Push and pull requests

1. Recheck the push URL and branch immediately before an authorized push. Push the task branch explicitly to `origin`; never use an unqualified push when its destination is uncertain.
2. Use an explicit repository and base for GitHub CLI/API calls, such as `gh pr create --repo gabrielfordevelopment/mint-maintained --base develop --head <branch>`. Default to `develop` even when the user omits the destination. Target `master` only when the user explicitly requests that release-branch destination in the current task; a general delivery or merge request is insufficient.
3. Inspect the full PR diff against its actual base. Include only the intended commits; resolve accidental branch ancestry before publishing.
4. Use a short, clear English Conventional Commit-style PR title of at most 70 characters, describing the primary result. Prefer a plain, concrete outcome over a list of implementation details. PR descriptions remain separate from commit messages: write the PR body using the guidance below, with concrete changes and outcomes instead of a file inventory.
5. If the user approved a title/body, publish that exact wording unless they authorized an edit. Before updating an existing PR, read its current title/body and preserve user-added images, links, and notes. Do not replace them with an older local draft.
6. Pass multiline text as a structured argument or an exact UTF-8 temporary file with `--body-file`. Avoid shell interpolation and escaped-newline corruption.
7. Verify the remote head SHA, PR repository, base, head, title, and body after publication. If GitHub is briefly stale, recheck before attempting another write. Report the PR link and current CI status without treating queued jobs as passed.

### PR description

Use this structure for a new description unless the user supplies or approves different wording:

```markdown
## Summary

One or two sentences explaining the problem or need and the essential result.

## What changed

- Describe the material changes and their effects.

## Outcome

State what users or maintainers can now do, or what failure is prevented.
```

Add a `Validation` section when relevant, listing checks actually run and material limitations. Keep the body proportionate to the work: avoid repeating the summary in every section, cataloguing files, narrating the conversation, or claiming untested behavior. For a bug fix, a concrete before/after example is often more useful than implementation detail. For workflow changes, distinguish prepared configuration from settings already applied on GitHub. Preserve approved wording and user-added images as specified above.

## Merge and release

- Merge only when the user authorizes it. Verify the current PR head/base, intended diff, relevant review state, and required checks. A previous green run for another commit is insufficient.
- Both `develop` and `master` require a PR, an up-to-date base, resolved conversations, and the GitHub Actions checks `check (ubuntu-22.04)`, `check (windows-2022)`, and `Agent guidance`. Administrator bypass, force-push, and branch deletion are disabled by policy; do not weaken protection to deliver a change. No second-person approval is required for the sole-maintainer workflow. Verify live settings before claiming enforcement.
- Prefer merge commits to preserve individual commits, DCO trailers, and ancestry between `develop` and `master`. Do not silently switch to squash/rebase if repository settings disallow that method; explain the constraint and resolve it within the user's authorization.
- After merging, verify the remote result. Update local branches only when doing so preserves local work. Do not delete task branches as an automatic side effect unless cleanup is authorized.
- Release preparation, merging a release PR, pushing a tag, and publishing a release are distinct actions. A code merge does not authorize a version bump, tag, or publication.

### Version preparation and manual release

1. Inspect [Cargo.toml](../../../Cargo.toml), [Cargo.lock](../../../Cargo.lock), [CHANGELOG.md](../../../CHANGELOG.md), [release.yml](../../../.github/workflows/release.yml), and [scripts/release.py](../../../scripts/release.py). The single application version is `[workspace.package].version`; all four packages inherit it. The window title, CLI, logs, integration metadata, and update comparison use that Cargo version. Git build information is diagnostic only.
2. For an authorized version bump, choose a stable `X.Y.Z` greater than the latest public release (and greater than `0.2.10` for the first maintained release). Prefer a patch increment for compatible fixes and a minor increment for a feature release. Do not bump on every commit, generate timestamp versions, or reset the fork's version to a lower value.
3. Change the workspace version on a task branch from `develop`. Run `cargo check --offline -p mint_lib` without `--locked` to refresh the workspace entries in `Cargo.lock`; inspect the diff and exclude unrelated dependency changes. Final checks use `--locked` again. Add the version, date, and changes to `CHANGELOG.md`, retaining an Unreleased section and updating comparison links. Do not use `cargo release --execute` to commit or push onto a protected branch; the inherited `release.toml` is not the delivery procedure.
4. Deliver the version preparation through a PR to `develop`, then promote it through an authorized release PR from `develop` to `master`. Both branches remain PR-only. Release-only changes on `master` must return through a PR to `develop`.
5. Once the workflow is present on `master`, use **Actions > Release > Run workflow**, select **master**, and run it. No version input or manual tag push is needed. The run uses its fixed commit SHA, validates manifest/lockfile versions and the matching `CHANGELOG.md` section, reruns the shared Windows/Linux and script checks, then builds and packages both platforms before creating the matching tag and draft release. The draft body comes from that version's changelog section; keep it complete and user-facing. Branch pushes, merges, and tag pushes do not initiate a release.
6. Versioned assets are `mint-vX.Y.Z-<target>.zip`. Identical `mint-<target>.zip` aliases remain for the updater; keep root `mint.exe` on Windows and `mint` on Linux, executable permissions, licenses, and notices. Do not remove or rename updater aliases independently of released-client compatibility.
7. Existing releases, including drafts, are never overwritten. A tag pointing elsewhere is rejected. A matching tag without a release can be reused after a failed attempt. If an upload fails after a draft exists, inspect and repair that exact draft within the user's authorization rather than deleting or replacing it blindly.
8. Review/test the downloaded packages and changelog-derived release notes, then explicitly publish the draft when authorized. A successful workflow uploads a draft; it does not publish it or expose it as the latest update. Report that distinction, and report unavailable platform/game tests honestly.

## Report delivery

State what was changed locally and remotely, the branch/commit or PR link, checks actually run and their results, and anything still pending. Explicitly distinguish documentation-only preparation from an executed branch migration or configured repository protection.
