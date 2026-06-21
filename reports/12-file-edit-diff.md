# Report 12: Git & Worktree Parity Audit

## Summary

Git and Worktree subsystems have been audited for parity between opencode (TypeScript) and rustcode (Rust). Three gaps were identified and fixed.

## Git Subsystem

**Status**: ✅ Complete parity

The rustcode `Git` struct implements all 15 interface methods from opencode:
- `run`, `branch`, `prefix`, `defaultBranch`, `hasHead`, `mergeBase`, `show`
- `status`, `diff`, `stats`, `patch`, `patchAll`, `patchUntracked`, `statUntracked`, `applyPatch`

Additional rustcode-only methods (not in opencode):
- `rev_parse_head`, `is_repo`, `find`, `remote`, `roots`, `origin`
- `reset_changes`, `soft_reset_changes`
- `worktree_create`, `worktree_remove`, `worktree_list`, `capture_patch`

## Worktree Subsystem

**Status**: ⚠️ 3 gaps fixed

### Gap 1: Missing submodule foreach commands in `reset()`
**Fixed in**: `crates/rustcode-core/src/worktree.rs`

opencode runs after reset:
1. `git submodule update --init --recursive --force`
2. `git submodule foreach --recursive git reset --hard`
3. `git submodule foreach --recursive git clean -fdx`
4. Status verification

rustcode only ran step 1. Now includes all 4 steps.

### Gap 2: Missing branch deletion in `remove()`
**Fixed in**: `crates/rustcode-core/src/worktree.rs`

opencode deletes the worktree's branch after removal:
```ts
const branch = entry.branch?.replace(/^refs\/heads\//, "")
if (branch) {
  const deleted = yield* git(["branch", "-D", branch], { cwd: ctx.worktree })
}
```

rustcode now includes branch deletion after successful removal.

### Gap 3: Missing status verification in `reset()`
**Fixed in**: `crates/rustcode-core/src/worktree.rs`

opencode verifies clean state after reset with `git status --porcelain=v1`. rustcode now includes this check.

## Files Modified

- `crates/rustcode-core/src/worktree.rs` (3 changes)

## Verification

Build passes with `cargo build` — no errors, only pre-existing warnings.
