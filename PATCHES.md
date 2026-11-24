# Helix Fork — custom patches from unmerged upstream PRs

Maintains a personal fork of [helix-editor/helix](https://github.com/helix-editor/helix)
carrying patches pulled from open upstream PRs that are taking too long to merge.

Remote: `git@github.com:rokrdev/helix.git`

## Layout

    master            mirror of upstream/master — never carries our patches
    custom            our patched branch (merges every patch/pr-<N>)
    patch/pr-<N>      one topic branch per upstream PR, a copy of the PR head (never rebased)
    tools/helix-fork.sh   all maintenance operations

The fork's `master` stays byte-identical to upstream, so `git diff master custom`
always shows exactly what you're adding.

## Day-to-day

Add a PR you want:

    tools/helix-fork.sh add-pr 1234

This fetches only that PR's head (`refs/pull/1234/head`), then points
`patch/pr-1234` at it. It does not fetch other PRs and does not rebase anything.
Then it merges the branch into `custom` with `--no-ff`. The merge commit is
`merge upstream PR #1234 (patch/pr-1234)` the first time and
`update upstream PR #1234 (patch/pr-1234)` after that.

To pick up new PR commits, run `add-pr` again. Only the new commits come in, and
`custom` is never rewritten. If the PR head is already in `custom`, the script
prints "already up to date" and does nothing.

If the author force-pushed, the patch branch moves to the rewritten head and the
merge is a normal three-way merge. Changes `custom` already has resolve cleanly.
A real change can conflict. Then the script stops on `custom` in the middle of
the merge. Resolve the files, then `git add` them and run `git commit`. The patch
branch is already at the PR head by then.

`add-pr` and `sync` refuse to run when tracked files have uncommitted changes.
Commit or stash first. They also refuse while a merge is in progress, even if you
already resolved it. Finish it with `git commit` or drop it with `git merge --abort`.

`add-pr`, `sync` and `build` switch branches while they work. When they finish
they put you back on the branch you started on, or on the same commit if you
started detached. `tools/` only exists on `custom`, so run the script from there
and it stays in your working tree. If a merge conflicts, or a merge hook stops it
before the commit, you stay on `custom`, mid-merge, wherever you started. Other
errors also try to take you back. If that switch fails, the script prints
`note: could not switch back to <branch>, you are on <branch>` and keeps its own
exit status.

Pull fresh upstream:

    tools/helix-fork.sh sync

This fetches upstream (branches only, no PR heads), fast-forwards `master`, and
merges `master` into `custom` with a merge commit. Nothing is rebased, so history
is never rewritten and `sync` never needs a force push. The patch branches are not
touched, because they are already merged into `custom`.

If the merge conflicts, the script stops on `custom` in the middle of the merge.
Resolve the files, then `git add` them and run `git commit`. If the conflict is
because upstream now contains one of your patches, take upstream's side. See
"When upstream merges a patch you carry" below.

Build the patched editor:

    tools/helix-fork.sh build         # -> target/release/hx
    tools/helix-fork.sh build -- --jobs 1   # pass extra args to cargo

Publish to your fork:

    tools/helix-fork.sh push          # master, custom, then local patch/pr-* branches

`master` and `custom` are pushed without force. They only ever move forward.

Patch branches copy PR heads, and authors rewrite their PRs. So `push` sends
every local `patch/pr-*` branch with `--force-with-lease`. The lease only
overwrites what your last fetch or push saw on `origin`. If it is rejected with
"stale info", run `git fetch origin` and push again. With no local patch
branches, `push` prints a skip message and sends only `master` and `custom`.

See your patch branches and what custom adds vs upstream:

    tools/helix-fork.sh status

`status` lists each `patch/pr-*` branch twice if it exists in both places. A
`local` row is a local branch. An `origin` row is the remote-tracking copy from
your last fetch or push. Each row says whether the branch is merged into `custom`.
Then it shows the diffstat of `custom` against `master`.

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
| patch/pr-16163  | #16163 terminal block cursor                  | --no-ff merge (60640ef1f) |
| patch/pr-15216  | #15216 call hierarchy picker                  | --no-ff merge (4c48e1b9a) |

The terminal block cursor (#16163) is **opt-in** — `editor.cursor-shape` defaults
to the software `block` cursor. To use the native terminal block cursor (fixes
Kitty/Ghostty cursor-trail artifacts), set in `config.toml`:

    [editor.cursor-shape]
    normal = "terminal-block"
    insert = "terminal-block"
    select = "terminal-block"

Added value is `terminal-block` alongside the existing `block`, `bar`,
`underline`, `hidden`. Note the terminal block cursor is not theme-colored;
pick a theme whose primary cursor color matches your terminal's cursor color.

## When upstream merges a patch you carry

Run `sync` as usual. If upstream's version is identical to yours, the merge is
clean. You still delete the branch and the table row. If upstream merged the PR, or
shipped the same fix as one of your commits, but not identically, the merge can
conflict. Keep upstream's version of every conflicted hunk and commit the merge.

Then drop the patch. Delete the local branch first. `push` sends every local
`patch/pr-*` branch, so a leftover would be pushed to `origin` again.

    git branch -D patch/pr-<N>             # skip if there is no local branch
    git push origin --delete patch/pr-<N>

Remove its row from the "Active patches" table. Last, run `git diff master custom`.
It should show only the patches you still carry. Delete anything left over from the
dropped patch.

## Caveat: updating a squash-merged PR

#13133 was applied as a **squash**: the feature's net tree is a single commit on
`custom` (`a6c1c6634`), so the patch branch's original commits are *not* recorded
as ancestors of `custom`. If upstream updates #13133, `add-pr` can't detect
it's already applied and will try to re-merge from scratch → conflicts.
`status` shows `patch/pr-13133` as "not merged into custom" for the same reason.

To refresh a squash-merged PR later:
1. `git fetch upstream '+refs/pull/13133/head:refs/remotes/upstream/pr-13133'`
2. Get just the delta since the version you applied (old PR tip `a828b5609`):
   `git diff a828b5609 refs/remotes/upstream/pr-13133` — review it.
3. Apply that delta to `custom` (`git apply`, or edit manually), build, commit.

PRs that merge cleanly (like #3958) don't have this problem. Their commits stay
ancestors of `custom`, so `sync` just carries them forward in the merge.

## Runtime gotcha (missing syntax highlighting)

A raw `cargo build` does **not** copy the runtime assets (tree-sitter grammars,
queries, themes) next to the binary. The health check shows it clearly:

    hx --health   # -> "Runtime directory does not exist: target/release/runtime"

Symptoms: code renders as a single color (no highlighting). Fix options:
- `tools/helix-fork.sh build` now copies `runtime/` to `target/release/runtime`
  automatically after compiling — use it instead of bare `cargo build`.
- Or set `export HELIX_RUNTIME="$PWD/runtime"` before running the binary.

On a freshly-cloned machine: clone, `git checkout custom`, `tools/helix-fork.sh build`.

## Install on a new system (one command)

The repo ships a self-contained bootstrap, `tools/install.sh`, that clones the fork,
installs Rust if missing, builds, copies the runtime, and symlinks `hx`:

    # on a fresh machine:
    git clone git@github.com:rokrdev/helix.git
    cd helix
    tools/install.sh          # build + install to ~/.local/bin/hx

Idempotent — re-running it pulls upstream updates from origin and rebuilds.
Env overrides: `HELIX_FORK_REPO`, `HELIX_FORK_BRANCH`, `HELIX_INSTALL_DIR`,
`HELIX_WORK_DIR`. Ensure `~/.local/bin` precedes any distro `hx` on PATH.

## Inline git blame (#13133) is opt-in

The feature defaults to off (`show = "never"`, `auto-fetch = false`). To see it
inline, add to `config.toml` (keys are kebab-case):

    [editor.inline-blame]
    show = "cursor-line"      # or "all-lines", or "never" to disable
    auto-fetch = true

Manual trigger while editing: `:blame-line` or the `B` key.
