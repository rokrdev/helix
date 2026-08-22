# Helix Fork — custom patches from unmerged upstream PRs

Maintains a personal fork of [helix-editor/helix](https://github.com/helix-editor/helix)
carrying patches pulled from open upstream PRs that are taking too long to merge.

Remote: `git@github.com:rokrdev/helix.git`

## Layout

    master            mirror of upstream/master — never carries our patches
    custom            our patched branch (merges every patch/pr-<N>)
    patch/pr-<N>      one topic branch per upstream PR, rebased onto master
    tools/helix-fork.sh   all maintenance operations

The fork's `master` stays byte-identical to upstream, so `git diff master custom`
always shows exactly what you're adding.

## Day-to-day

Add a PR you want:

    tools/helix-fork.sh add-pr 1234

This fetches upstream + open PR heads, recreates `patch/pr-1234` from the PR's
current tip, rebases it onto `master`, and merges it (--no-ff) into `custom`.
The PR author updating their branch is reflected next time you re-add or run sync.

Pull fresh upstream and rebase everything:

    tools/helix-fork.sh sync

Build the patched editor:

    tools/helix-fork.sh build         # -> target/release/hx
    tools/helix-fork.sh build -- --jobs 1   # pass extra args to cargo

Publish to your fork:

    tools/helix-fork.sh push          # pushes master, custom, all patch/pr-*

See what custom adds vs upstream:

    tools/helix-fork.sh status

## Installing instead of Homebrew hx

The built binary is `target/release/hx`. Symlink it onto your PATH ahead of the
Homebrew one:

    ln -sf ~/projects/helix/target/release/hx ~/.local/bin/hx   # if ~/.local/bin is on PATH
    # then in a new shell: hx --version  (should show your custom build)

To revert to stock Homebrew helix: remove the symlink (`which hx` falls back to
`/opt/homebrew/bin/hx`).

## Adding a new workflow idea

Branch per PR keeps each change isolated and reviewable. If you'd rather track
patches as a flat `patches/*.patch` quilt (git-format-patch style) for portability
into CI or a build pipeline, say the word — it's a thin wrapper over what's here.

## Active patches

| Branch          | Upstream PR                                   | How applied on `custom` |
|-----------------|-----------------------------------------------|--------------------------|
| patch/pr-13133  | #13133 Inline git blame                       | squash merge (a6c1c6634) |
| patch/pr-3958   | #3958 custom names for custom commands        | --no-ff merge (d10f8209c) |

## Caveat: updating a squash-merged PR

#13133 was applied as a **squash**: the feature's net tree is a single commit on
`custom` (`a6c1c6634`), so the patch branch's original commits are *not* recorded
as ancestors of `custom`. If upstream updates #13133, `sync`/`add-pr` can't detect
it's already applied and will try to re-merge from scratch → conflicts.

To refresh a squash-merged PR later:
1. `git fetch upstream '+refs/pull/13133/head:refs/remotes/upstream/pr-13133'`
2. Get just the delta since the version you applied (old PR tip `a828b5609`):
   `git diff a828b5609 refs/remotes/upstream/pr-13133` — review it.
3. Apply that delta to `custom` (`git apply`, or edit manually), build, commit.

PRs that merge cleanly (like #3958) don't have this problem — they keep proper
ancestry, so `sync` re-rebases and re-merges them automatically.

## Runtime gotcha (missing syntax highlighting)

A raw `cargo build` does **not** copy the runtime assets (tree-sitter grammars,
queries, themes) next to the binary. The health check shows it clearly:

    hx --health   # -> "Runtime directory does not exist: target/release/runtime"

Symptoms: code renders as a single color (no highlighting). Fix options:
- `tools/helix-fork.sh build` now copies `runtime/` to `target/release/runtime`
  automatically after compiling — use it instead of bare `cargo build`.
- Or set `export HELIX_RUNTIME="$PWD/runtime"` before running the binary.

On a freshly-cloned machine: clone, `git checkout custom`, `tools/helix-fork.sh build`.

## Inline git blame (#13133) is opt-in

The feature defaults to off (`show = "never"`, `auto-fetch = false`). To see it
inline, add to `config.toml` (keys are kebab-case):

    [editor.inline-blame]
    show = "cursor-line"      # or "all-lines", or "never" to disable
    auto-fetch = true

Manual trigger while editing: `:blame-line` or the `B` key.
