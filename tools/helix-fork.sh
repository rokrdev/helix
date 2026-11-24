#!/usr/bin/env bash
# helix-fork.sh — maintain rokrdev/helix with patches cherry-picked from open
# upstream PRs that are slow to merge.
#
# Layout:
#   master          mirror of upstream/master (never carries our patches)
#   custom          our patched branch: merges every patch/pr-<N> topic branch
#   patch/pr-<N>    one topic branch per upstream PR, mirrors the PR head (never rebased)
#
# Workflow:
#   tools/helix-fork.sh fetch            fetch upstream + all open PR heads
#   tools/helix-fork.sh add-pr 1234      merge PR #1234 into custom, re-run to pick up PR updates
#   tools/helix-fork.sh sync             ff master to upstream, merge master into custom
#   tools/helix-fork.sh build            cargo build the custom branch
#   tools/helix-fork.sh push             push master + custom, force-with-lease patch branches
#   tools/helix-fork.sh status           patch branches (local + origin) and what custom adds
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

UPSTREAM="upstream"
ORIGIN="origin"
CUSTOM="custom"
PR_PREFIX="patch/pr-"

die() { echo "error: $*" >&2; exit 1; }

fetch() {
  echo "== fetching upstream =="
  git fetch "$UPSTREAM" || die "fetch upstream failed"
  echo "== fetching upstream PR heads =="
  git fetch "$UPSTREAM" '+refs/pull/*/head:refs/remotes/upstream/pr-*' \
    || die "fetch PR heads failed"
}

# Branch switches carry local edits along, so a dirty tree breaks merges in odd ways.
require_clean_tree() {
  git diff --quiet && git diff --cached --quiet || die "working tree not clean, commit or stash first"
}

# Mid-merge advice only makes sense if a merge actually started.
merge_failed() {
  local src="$1"
  shift
  git rev-parse -q --verify MERGE_HEAD >/dev/null 2>&1 \
    || die "merge of $src into $CUSTOM failed"
  {
    echo "CONFLICT merging $src into $CUSTOM. You are still on $CUSTOM, mid-merge."
    echo "Resolve the conflicted files, then run:"
    echo "  git add <files>"
    echo "  git commit"
    local line
    for line in "$@"; do echo "$line"; done
    echo "To bail out instead: git merge --abort"
  } >&2
  exit 1
}

ensure_custom() {
  git rev-parse --verify "$CUSTOM" >/dev/null 2>&1 || {
    echo "creating $CUSTOM from master"
    git branch "$CUSTOM" "master"
  }
}

add_pr() {
  local n="$1"
  [[ "$n" =~ ^[0-9]+$ ]] || die "PR number must be numeric, got: $n"
  require_clean_tree
  fetch
  local src="refs/remotes/$UPSTREAM/pr-$n"
  git rev-parse --verify "$src" >/dev/null 2>&1 \
    || die "no PR $n in fetched refs — is it open? (run fetch, check gh pr view $n)"
  local topic="$PR_PREFIX$n"
  # The branch mirrors the PR head, even after an author force-push.
  git branch -f "$topic" "$src" || die "cannot update topic branch $topic"
  ensure_custom
  if git merge-base --is-ancestor "$topic" "$CUSTOM"; then
    echo "PR $n already up to date in $CUSTOM"
    return
  fi
  # Custom is never rewritten, so an earlier merge stays in its first-parent log.
  local verb="merge"
  [[ -z "$(git log --first-parent --fixed-strings --grep="upstream PR #$n ($topic)" -1 --format=%h "$CUSTOM")" ]] \
    || verb="update"
  git checkout -q "$CUSTOM" || die "checkout $CUSTOM failed"
  echo "== merging $topic into $CUSTOM =="
  git merge --no-ff "$topic" -m "$verb upstream PR #$n ($topic)" || merge_failed "$topic"
  git checkout -q master
}

sync() {
  require_clean_tree
  fetch
  echo "== fast-forwarding master to $UPSTREAM/master =="
  git checkout -q master || die "checkout master failed"
  git merge --ff-only "$UPSTREAM/master" \
    || die "master has diverged from $UPSTREAM/master, fix it by hand (master must only mirror upstream)"
  # After the ff, so a new custom starts at the new master.
  ensure_custom
  git checkout -q "$CUSTOM" || die "checkout custom failed"
  local sha
  sha="$(git rev-parse --short master)"
  if git merge-base --is-ancestor master "$CUSTOM"; then
    echo "custom already contains master @ $sha, nothing to merge"
  else
    echo "== merging master @ $sha into custom =="
    git merge --no-ff master -m "merge upstream/master ($sha) into custom" \
      || merge_failed master \
        "Rule: if a conflict is because upstream now contains one of your patches," \
        "take upstream's side, then drop that patch's patch/pr-<N> branch and table row."
  fi
  git checkout -q master
  echo "custom synced. run 'tools/helix-fork.sh build' to compile."
}

build() {
  ensure_custom
  git checkout -q "$CUSTOM"
  echo "== building custom branch @ $(git rev-parse --short HEAD) =="
  cargo build --release "$@"
  # helix looks for its runtime (grammars, queries, themes) next to the binary.
  # A raw cargo build does NOT ship it there, which breaks syntax highlighting.
  # Copy the source runtime/ so the binary finds grammars anywhere. We skip
  # `grammars/sources/`: those are build-only grammar checkouts whose nested,
  # read-only `.git` object stores bloat the copy and make a re-run fail with
  # "Permission denied". `rm -rf` first also stops rsync from nesting `runtime`
  # inside an already-present destination.
  rm -rf "$REPO_ROOT/target/release/runtime"
  rsync -a --exclude '/grammars/sources/' \
    "$REPO_ROOT/runtime/" "$REPO_ROOT/target/release/runtime/" \
    && echo "== runtime -> target/release/runtime =="
  git checkout -q master
  echo "binary: $REPO_ROOT/target/release/hx"
}

push() {
  local remote="${1:-$ORIGIN}"
  echo "== pushing master =="
  git push "$remote" master
  echo "== pushing custom =="
  git push "$remote" custom
  local patches
  patches="$(git for-each-ref --format='%(refname:short)' "refs/heads/$PR_PREFIX*")"
  if [[ -z "$patches" ]]; then
    echo "== no local $PR_PREFIX* branches, skipping patch branches =="
    return
  fi
  # Patch branches mirror PR heads, which authors rewrite. The lease limits the damage.
  echo "== pushing patch branches (--force-with-lease) =="
  # shellcheck disable=SC2086 # branch names never contain spaces
  git push --force-with-lease "$remote" $patches \
    || die "patch branch push failed. If it says stale info, run: git fetch $remote"
}

status() {
  ensure_custom
  echo "== master @ $(git rev-parse --short master) =="
  echo "== custom @ $(git rev-parse --short custom) =="
  echo
  echo "patch branches:"
  local ref name sha where merged found=0
  while read -r ref name sha; do
    found=1
    where="local"
    [[ "$ref" == refs/remotes/* ]] && where="$ORIGIN"
    if git merge-base --is-ancestor "$ref" "$CUSTOM"; then
      merged="merged into $CUSTOM"
    else
      merged="not merged into $CUSTOM"
    fi
    printf '  %-28s %s  %-7s %s\n' "$name" "$sha" "$where" "$merged"
  done < <(git for-each-ref --format='%(refname) %(refname:short) %(objectname:short)' \
    "refs/heads/$PR_PREFIX*" "refs/remotes/$ORIGIN/$PR_PREFIX*")
  [[ "$found" -eq 1 ]] || echo "  (none yet — run: tools/helix-fork.sh add-pr <N>)"
  echo
  echo "custom vs master diffstat:"
  git diff --stat master custom
}

case "${1:-}" in
  fetch)   fetch ;;
  add-pr)  [[ $# -ge 2 ]] || die "usage: helix-fork.sh add-pr <PR#>"; add_pr "$2" ;;
  sync)    sync ;;
  build)   shift; build "$@" ;;
  push)    shift; push "$@" ;;
  status)  status ;;
  *) echo "usage: helix-fork.sh {fetch|add-pr <N>|sync|build [ARGS]|push [remote]|status}"; exit 0 ;;
esac
