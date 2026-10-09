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

- `develop` is the integration branch and the default target for ordinary PRs.
- Interpret `dev`, `develop`, and `development` as the actual `develop` branch. These are aliases in conversation, not separate Git branches.
- `master` contains release-ready history and keeps its existing name. Promote changes through a release PR from `develop` to `master`; release tags belong to tested commits on `master`.
- Start ordinary work from the latest fetched `origin/develop`. Use descriptive English `feature/*`, `fix/*`, `docs/*`, `test/*`, `ci/*`, or `chore/*` names matching the task.
- Create a new branch without tracking the base branch, for example `git switch --no-track -c fix/missing-metadata origin/develop`. On its first authorized push, set its upstream to the same-named branch on `origin`.
- Continue an existing task branch when appropriate. Never reset, rebase, squash, amend, or force-push user history without explicit authorization. Use merge-based updates when an authorized task needs the current integration branch.
- Do not commit routine changes directly to `develop` or `master`. An explicitly requested release hotfix may branch from `master`; bring the fix back to `develop` through an authorized merge/PR afterward.

## Initial develop setup

This is a conditional setup workflow, not permission to change branches while editing documentation. Inspect actual local and remote refs first; skip completed steps rather than recreating them.

When the user authorizes the branch setup:

1. Verify the own-repository remote, a suitable worktree state, and the exact history to preserve. Compare existing local and remote `master` and `develop` refs; do not overwrite an existing branch.
2. Keep `master` unchanged and create `develop` from its accepted baseline unless existing refs require a different, reviewed integration. Retain unrelated task branches. For an explicitly local-only setup, use the verified local baseline and defer remote synchronization/publication.
3. Create the setup task branch from local `develop` without tracking the base. Commit guidance and configuration there so they can later be reviewed in a PR to `develop`.
4. Keep `release.toml` restricted to `master`, update development/release instructions in `README.md`, and inspect workflow branch filters and repository links. Tag-triggered release automation must verify that the release commit belongs to `master` and that the tag matches the Cargo version.
5. When remote setup is authorized, fetch and compare the current remote state before publishing. Push only to the own repository, establish matching tracking refs, and set `develop` as the GitHub default branch. Refresh `origin/HEAD` after the remote default changes.
6. Keep `master` as the release branch. Do not rename/delete it or alter protection settings as part of this setup. Report the resulting local/remote branches and settings actually verified; documentation policy is not evidence that GitHub protection has been configured.

## Stage and commit

1. Review the complete diff, including new files, for scope, accidental data/credentials, generated artifacts, dependency churn, and stale guidance. Stage only intended paths or hunks.
2. Complete the checks required by `AGENTS.md` for this change type. Inspect the staged diff and run `git diff --cached --check`. Resolve introduced failures without unrelated cleanup; report any environment limitation before claiming readiness.
3. Use the existing configured human author identity. Add a DCO trailer with `git commit -s`; do not invent an identity or add an AI co-author trailer.
4. Write an English, subject-only Conventional Commit message, with the DCO trailer as the exception. Keep the subject within 72 characters and describe the outcome, for example `fix: handle adjacent mods without metadata`.
5. Verify the resulting commit, sign-off, branch, and remaining worktree changes. If the request is local-only, stop delivery here; do not push.

## Push and pull requests

1. Recheck the push URL and branch immediately before an authorized push. Push the task branch explicitly to `origin`; never use an unqualified push when its destination is uncertain.
2. Use an explicit repository for GitHub CLI/API calls, such as `gh pr create --repo gabrielfordevelopment/mint-maintained --base develop --head <branch>`. Target `master` only for an authorized release or hotfix PR.
3. Inspect the full PR diff against its actual base. Include only the intended commits; resolve accidental branch ancestry before publishing.
4. Use an English Conventional Commit-style title within 72 characters, describing the primary result. Write the body using the guidance below, with concrete changes and outcomes instead of a file inventory.
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
- For an authorized release, inspect `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, `release.toml`, and `.github/workflows/release.yml` together. Use the existing version/tag format; do not apply CompareCode's timestamp versioning.
- Tag the verified, tested release commit on `master`. Confirm the tag/version match and the release workflow's master-ancestry guard before pushing a release tag.
- Keep archive/executable names compatible with the updater: `mint-x86_64-pc-windows-msvc.zip` contains root `mint.exe`; `mint-x86_64-unknown-linux-gnu.zip` contains root `mint`. Preserve packaged licenses and notices.
- Manual release-workflow runs currently produce artifacts; version tags produce draft releases. Re-read the workflow before use, and do not equate a draft with publication. If release-only changes landed on `master`, merge them back into `develop` through the authorized workflow.

## Report delivery

State what was changed locally and remotely, the branch/commit or PR link, checks actually run and their results, and anything still pending. Explicitly distinguish documentation-only preparation from an executed branch migration or configured repository protection.
