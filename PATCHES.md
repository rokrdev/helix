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

    ln -sf ~/src/helix-fork/target/release/hx ~/.local/bin/hx   # if ~/.local/bin is on PATH
    # then in a new shell: hx --version  (should show your custom build)

To revert to stock Homebrew helix: remove the symlink (`which hx` falls back to
`/opt/homebrew/bin/hx`).

## Adding a new workflow idea

Branch per PR keeps each change isolated and reviewable. If you'd rather track
patches as a flat `patches/*.patch` quilt (git-format-patch style) for portability
into CI or a build pipeline, say the word — it's a thin wrapper over what's here.
