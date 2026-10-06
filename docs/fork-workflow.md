# Our fork workflow

Original project (upstream): https://github.com/deepentropy/ibx
Our GitHub fork (origin): https://github.com/mjarvel/ibx
Existing local clone: G:\repos\ibx

A fork is your own repository on GitHub, connected to the original project's
history. A clone is its local copy on disk. A branch names a line of commits.
Committing records work locally; pushing publishes those commits to a remote.
A pull request proposes changes for the original maintainers to review and merge.
Pushing to our fork alone does not change the original repository.

## Branch roles

| Branch | Purpose |
| --- | --- |
| upstream/main | Our local reference to the original project's main, refreshed by fetch. |
| origin/main and local main | Clean mirror of the original main; keep our custom patches off it. |
| origin/dev and local dev | Our working version, containing our patches and deliberately integrated upstream updates. |
| codex/patch-ibx-bounded-lifecycle | Preserved tested patch at f391500b55af8893a803c1c7c279edf1171579c9, also pushed to our fork. |

`origin` and `upstream` are remote nicknames, not branches. They point to our fork
and the original project respectively. main remains the fork's default branch;
dev is where we normally work. The wrapper still requires deliberate exact-commit
adoption and offline checks; fetching/syncing main does not adopt it automatically.

## Current checkpoint, 2026-10-06

Our fork main matches inspected upstream e491575a59d9ef5d069a1e0c4afa132bce32a281.
The tested patch was based on 53cfa34b9813b480f311a36c458f0a35aa4d31e2.
Upstream has 233 newer commits. Both dev and the contribution branch were initially
pushed at the tested patch; subsequent workflow documentation belongs only to dev.
A Git merge-tree preview found conflicts in seven files:

- src/api/client/mod.rs
- src/auth/session.rs
- src/engine/hot_loop/mod.rs
- src/gateway.rs
- src/protocol/connection.rs
- src/protocol/fixcomp.rs
- src/protocol/ns.rs

The preview did not change files or branches. No unresolved working-tree merge was
started. The patch has not been tested against current upstream. Its original
Windows offline validation remains attached to the exact tested commit.

## Updating later

When the checkout is clean, the usual sequence is:

```powershell
git fetch upstream
git switch main
git merge --ff-only upstream/main
git push origin main
git switch dev
git merge main
# Resolve any conflicts, review changed behavior, run inspected offline tests.
# Commit the resolution if Git has not already created the merge commit.
git push origin dev
```

Fetch downloads history without changing checked-out files. Updating main uses
fast-forward only: it refuses an unexpected divergence rather than discarding work.
Merging main into dev brings the maintainers' work into our patched version.
Conflicts mean both sides changed overlapping code; each resolution needs review
and tests. Do not mechanically select our whole file or the whole upstream file.
Do not run these merge commands blindly at the present checkpoint: the seven
conflicts need an implementation/reconciliation pass first. No force push/reset
is part of this normal workflow. Syncing main is deliberate, not automatic.

## Possible contributions

Use a focused contribution branch for a PR to deepentropy/ibx main; keep dev as
our integration branch. Further pushes to an open PR's source branch update the
PR, so unrelated day-to-day dev work should not share its source branch.

Recommendation: reconcile against current upstream first, check which gaps still
exist, and rerun offline tests. Then prepare two focused proposals:

1. Opt-in strict historical parser: preserve absent statistics, reject malformed
   fields/completion, and enforce byte/row bounds without changing legacy callers.
2. Controlled lifecycle: single paper-login scope, explicit stop/deadline/ownership,
   bounded authentication and checked shutdown. Explain the additional Rustls trust
   semantics and dependencies, and remaining production gaps.

The lifecycle patch is a larger API/design change and may need maintainer discussion.
Do not submit it as a small cosmetic bug fix or claim complete production acceptance.
The current combined patch is useful as our preserved working version; separate PR
branches can carry narrower reviewed changes after reconciliation.

A PR is an invitation, not a command to merge. Maintainers may request changes,
accept part of the proposal or decline it. Our fork can retain our needed changes
regardless. If a change is accepted upstream, sync main and reconcile dev so our
custom implementation no longer duplicates/conflicts with the accepted one.
No PR, issue, maintainer message or review request has been sent at this checkpoint.

Official GitHub guidance:
- https://docs.github.com/en/pull-requests/how-tos/work-with-forks/syncing-a-fork
- https://docs.github.com/en/pull-requests/how-tos/create-pull-requests/creating-a-pull-request-from-a-fork
