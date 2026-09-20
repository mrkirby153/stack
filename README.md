# stack

A small CLI for managing **stacks of git branches** — a stack is an ordered list of
branches, each built on the one below it, all the way down to a target branch
(usually `main`):

```
main (target)
 └── part1      ← bottom  (layers[0], closest to the target)
     └── part2
         └── part3        ← top (layers[last], furthest from the target)
```

`stack` tracks the stack's structure in metadata and re-applies each layer's
commits on top of the layer below it whenever the base moves — so you can keep
independent pieces of work in independent branches and rebase the whole chain
with one command.

Everything is local: state lives under `.git/stack/`, no remotes involved.

## Building

```sh
cargo build --release
# binary at target/release/stack
```

## Quick start

```sh
# On a branch you want to be the bottom of a new stack:
git checkout -b part1
stack init --target main        # stack "part1" = [part1]

# Stack more branches on top (branches must already exist):
git checkout -b part2 part1
# ... commit work on part2 ...
stack insert part1 part2        # base inferred, added at the top

git checkout -b part3 part2
# ... commit work on part3 ...
stack insert part1 part3
```

`stack status` shows the current stack and which layer you're on:

```
Current stack: part1

  part1
* part2
  part3
```

## Everyday workflow

The classic stacked-PR loop:

```sh
stack restack     # re-apply every layer onto the layer below (after main moved)
git push -f       # update your branches
stack up          # or: down / top / bottom — check out a neighbouring layer
# ... work, commit ...
git push
# When the BOTTOM layer's PR merges into main (squash-merge is fine):
git fetch origin main:main   # bring local main to the merged tip (or: git pull on main)
stack advance               # drop the bottom layer, re-target the rest onto main
git push -f
```

### `restack`

Re-applies each layer's own commits (`base..tip`) onto the new tip of the layer
below, bottom-up. Layers that already sit on their new base are skipped
(cheap ancestor check), and layers whose commits are already contained in the
new base (e.g. merged into the target in the meantime) are fast-forwarded
instead of re-applying. Branch refs are updated atomically and you stay on
your current branch.

### `advance`

Assumes the **bottom** layer (closest to the target) has been merged into the
target externally (e.g. via a PR). It drops that layer from the stack — its
branch is left untouched — and re-targets the remaining layers onto the
target.

Because the dropped layer's work must already be in the target, `advance`
refuses to run when the target is still *behind* the bottom layer's tip (i.e.
its work isn't in the target yet — either the PR hasn't landed, or your local
target is stale). The error tells you exactly what to do, typically:

```sh
git fetch origin main:main   # then retry `stack advance`
```

`stack advance --force` bypasses the check if you know what you're doing.
The check is deliberately TOCTOU-safe: it only looks at whether the target
*contains the bottom layer's work*, so a fast-moving target (hundreds of
commits a day) is fine as long as it has the merged commit.

### Conflicts

If a layer fails to re-apply cleanly, the operation stops **before touching
any further layers**, leaves the cherry-pick in place, and tells you what to
do:

```
Error: Layer 'part2' failed to rebase cleanly. Resolve the conflicts, then
resume with `stack restack --continue` — or restore the previous state with
`stack restack --undo`.
```

```sh
# 1. Resolve in the working tree (you'll be on a detached HEAD)
git add <files>
stack restack --continue        # finishes the pick, then does the remaining layers
```

### `--undo`

`restack --undo` and `advance --undo` restore exactly what a pre-operation
snapshot captured: all layer refs, the stack metadata, and the branch you were
on. If an operation is still in flight (mid-conflict), undo uses its fresher
embedded snapshot. Each command keeps its own snapshot, so
`restack --undo` reverts the last restack and `advance --undo` the last
advance.

## Command reference

| Command | Description |
|---|---|
| `stack init [--target BR] [--name NAME] [--base REF] [--force]` | Create a new stack with the **current branch** as its bottom layer. Target defaults to `main`, name defaults to the current branch. |
| `stack insert NAME [BRANCH] [--at N \| --before L \| --after L] [--from REF]` | Add an **existing branch** to stack `NAME` as a new layer. Position: `--at N` (0 = bottom), `--before`/`--after` a layer, or top (default). `--from` sets the layer's base; if omitted it's inferred as the merge-base with the layer below (or the target at the bottom). Stored as a SHA. |
| `stack remove NAME BRANCH` | Remove a layer from the stack (metadata only — the branch itself is not deleted). |
| `stack delete NAME` | Delete the stack's metadata entirely (branches are not touched). |
| `stack list` | List all stacks in the repository. |
| `stack status` | Show the current branch's stack with the current layer marked. |
| `stack up` / `stack down` | Check out the layer above / below the current one. No-op at the top / bottom. |
| `stack top` / `stack bottom` | Check out the top / bottom layer. |
| `stack restack [--continue \| --undo]` | Re-apply every layer onto the layer below. `--continue` resumes a conflicted restack after you resolve it; `--undo` restores the pre-restack state. |
| `stack advance [--continue \| --undo] [--force]` | Drop the (externally merged) bottom layer and re-target the rest onto the target. Refuses if the target is behind the bottom layer's work; `--force` overrides. Same `--continue`/`--undo` semantics. |

Every command also supports `--help` for details.

## How it works

- **Layers are defined by a replay range.** Each layer stores a `base_oid`;
  its commits are `base_oid..branch_tip`. `restack` cherry-picks that range
  onto the new base — so only the layer's own work is re-applied, never the
  work of the layers below it.
- **Detached-HEAD operations.** `restack`/`advance` detach HEAD while they
  work, update branch refs atomically in batches, then check your branch back
  out. A failed operation can never leave your checked-out branch mid-rebase.
- **Resumable by design.** After every layer the in-flight state is written to
  `.git/stack/restack_state.json`, so `--continue` and `--undo` always know
  exactly where the operation stopped.

### Guards

`restack`/`advance` refuse to start (before mutating anything) when:

- a layer's base equals its tip — it would have **no commits of its own**, and
  re-pointing it would silently drop its work (almost always bad metadata —
  check how it was added with `stack insert`), or
- a layer's base is not an ancestor of its tip — the replay range would
  silently include other layers' commits.

## Where state lives

```
.git/stack/
├── stacks/
│   └── <name>.json              # stack metadata: target + ordered layers
├── restack_state.json           # in-flight restack/advance (while one is paused)
├── undo_snapshot.json           # last completed restack (for `restack --undo`)
└── advance_undo_snapshot.json   # last completed advance (for `advance --undo`)
```

## Notes & limitations

- `stack insert` records an **existing** branch; it does not create branches.
  A freshly created branch with **no commits yet** can be inserted right away
  (its base is inferred from the layer below); just make your first commit
  before you `restack`, or the empty-layer guard will (correctly) refuse.
- `advance` checks that the target is not *behind* the bottom layer (target a
  strict ancestor of the bottom's tip) before dropping it — i.e. the bottom's
  work hasn't been incorporated into the target yet. This catches a stale local
  target and an unmerged bottom on a linear history, and does **not** false-
  positive on squash merges or on a fast-moving target that already contains
  the merge. It cannot detect a target that *diverged* from the bottom's base
  without containing its work; if you advance a bottom that wasn't actually
  merged, its branch still exists and `advance --undo` puts the layer back.
- `--undo` restores refs and metadata; it does not rewrite any commits you may
  have made *after* the operation.
- Stack metadata is per-repository (`.git/stack/`), not per-branch — any
  branch in the repo can operate on any stack by name.
