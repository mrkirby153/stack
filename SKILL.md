---
name: git-stacked-prs
description: Manage stacks of dependent git branches / stacked PRs with the local `stack` CLI (this repository). Use when the user wants to init or insert stack layers, restack after the target moved, advance after the bottom PR merged, resolve restack/advance conflicts, or undo a restack/advance. Covers `stack init/insert/remove/restack/advance/status/up/down` and their guards.
---

# Stacked PRs with `stack`

`stack` manages **stacks of branches**: an ordered list of branches, each built on
the one below it, down to a target branch (usually `main`). Each layer stores a
`base_oid`; its own commits are `base_oid..branch_tip`, and `restack`/`advance`
re-apply those ranges onto the new base — so a layer's work is never mixed with
the work of the layers below it.

Everything is local: state lives in `.git/stack/`, the tool never talks to a
remote and never pushes. Run it from inside the target repository's working
tree (it operates on the repo containing the cwd).

## Command reference

| Command                                                                       | What it does                                                                                      |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `stack init [--target BR] [--name NAME] [--base REF]`                         | New stack with the **current branch** as its bottom layer (target defaults to `main`).            |
| `stack insert NAME [BRANCH] [--at N \| --before L \| --after L] [--from REF]` | Add an **existing branch** as a layer (base inferred from the layer below if omitted).            |
| `stack status`                                                                | Show the stack, mark the current layer, flag layers that need restacking or have broken metadata. |
| `stack restack [--continue \| --undo]`                                        | Re-apply every layer onto the layer below, bottom-up.                                             |
| `stack advance [--continue \| --undo] [--force]`                              | Drop the (externally merged) bottom layer, re-target the rest onto the target.                    |
| `stack up` / `down` / `top` / `bottom`                                        | Check out a neighbouring / end layer.                                                             |
| `stack remove NAME BRANCH` / `stack delete NAME` / `stack list`               | Metadata management (branches are not touched by `remove`/`delete`).                              |

## The stacked-PR loop

```sh
# 1. Build the stack (branches are created with plain git)
git checkout -b part1 main && <commit> && stack init --target main
git checkout -b part2 part1 && <commit> && stack insert part1 part2
git checkout -b part3 part2 && <commit> && stack insert part1 part3
git push -f                          # tool never pushes; push affected branches yourself

# 2. When main moves
stack restack && git push -f

# 3. When the BOTTOM layer's PR merges into main (squash-merge is fine)
git fetch origin main:main           # local main MUST be at the merged tip first
stack advance                        # drops part1, re-targets part2/part3 onto main
git push -f
```

Key semantics:

- **`advance` only drops the bottom layer.** It assumes that layer's work is
  already in the target (via a PR). Its branch is left untouched — just removed
  from the stack metadata.
- **`restack` keeps every layer.** It is how you move the whole stack after the
  target (or a lower layer) changed.
- **Local target must be fresh before `advance`.** If local `main` hasn't
  received the merge, `advance` refuses; fix with `git fetch origin main:main`
  (or pull on main), not `--force`.
- **A layer behind the target is not an error.** `stack status` treats the
  bottom being behind the target as up to date — that's the normal
  post-merge state.

## Reading `stack status`

```
Current stack: part1 (target: main)

  part1
* part2  1 commit(s) behind — needs restack
  part3  needs restack (layers below changed)

Stack needs to be restacked
```

- `*` marks the layer of the current branch.
- `needs restack` → run `stack restack` before pushing/advancing.
- `! inconsistent` / `! empty` / `! branch not found` → metadata or branch
  damage; fix the branch or re-record the layer (`stack remove` + `stack insert --from <base>`) before any rebase.

## Conflicts during `restack` / `advance`

A failing layer stops the operation **before touching further layers**, leaves
the cherry-pick in place (you are on a **detached HEAD**), and preserves state:

```sh
# resolve the conflict in the working tree
git add <files>
stack restack --continue            # finishes the pick, then continues with remaining layers
```

- To abandon and restore the exact pre-operation state (refs, metadata,
  checked-out branch): `stack restack --undo` / `stack advance --undo`.
- Only one operation can be in flight at a time; the error will tell you which
  command's `--continue`/`--undo` to use.
- If the **same layer fails twice** with the same "cherry-pick is now empty"
  message, stop — do not loop `--skip`/`--continue`; use the recovery below.

### When a layer's work is already in the target (squash-merge)

If the target received the layer's work by squash-merge, the commits have new
OIDs, so re-applying them produces "empty" cherry-picks that `--continue`
cannot skip (it re-issues the pick). Don't fight the sequencer — recover
through the tool's metadata instead:

```sh
stack restack --undo                          # back to a clean state
stack remove <stack> <merged-layer>           # drop the merged layer from the stack
stack restack                                 # re-target the remaining layers onto the target
```

This reaches the same final state a successful `advance` would have produced
(the dropped branch and any _unmerged_ commits on it are left untouched).

## Refusals and how to respond

| Error                                              | Meaning                                                                                 | Fix                                                                                                                                                         |
| -------------------------------------------------- | --------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Target … is behind the bottom layer …`            | Bottom's work not in the target yet (unmerged PR or stale local target).                | `git fetch origin main:main`, then retry. `advance --force` overrides — **only with explicit user approval**.                                               |
| `Stack is not up to date — refusing to advance`    | A surviving layer is out of date / broken / missing. The offending layer(s) are listed. | `stack restack`, then retry `advance`. If the merged bottom blocks the restack (squash-merge case), use the `--undo` + `remove` + `restack` recovery above. |
| `empty: no commits over base`                      | Layer has no commits of its own.                                                        | Make the first commit, or fix the layer's recorded base.                                                                                                    |
| `inconsistent: base … is not an ancestor of tip …` | Branch history was rewritten against its recorded base.                                 | Fix the branch or re-insert the layer with `--from <base>`.                                                                                                 |
| `A restack/advance is already in progress`         | A previous operation stopped at a conflict.                                             | `--continue` (after resolving) or `--undo`.                                                                                                                 |

## Agent rules of thumb

- Always `stack status` first; act on what it reports.
- The tool never pushes and never touches remotes — push affected branches with
  `git push -f` only when the user wants that.
- Never pass `--force` unless the user explicitly asked.
- `advance` requires ≥1 remaining layer of meaning — advancing a 1-layer stack
  leaves an empty stack (the branch remains, only metadata drops it).
- Branches in flight on other machines/CI are not a concern: state is
  per-repository (`.git/stack/`), so work on a copy of the repo is fully
  isolated.
