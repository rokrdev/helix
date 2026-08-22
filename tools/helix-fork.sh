#!/usr/bin/env bash
# helix-fork.sh — maintain rokrdev/helix with patches cherry-picked from open
# upstream PRs that are slow to merge.
#
# Layout:
#   master          mirror of upstream/master (never carries our patches)
#   custom          our patched branch: merges every patch/pr-<N> topic branch
#   patch/pr-<N>    one topic branch per upstream PR, rebased onto master
#
# Workflow:
#   tools/helix-fork.sh fetch            fetch upstream + all open PR heads
#   tools/helix-fork.sh add-pr 1234      bring PR #1234 into custom as a clean merge
#   tools/helix-fork.sh sync             rebase custom on latest upstream master
#   tools/helix-fork.sh build            cargo build the custom branch
#   tools/helix-fork.sh push             push custom + master to your fork
#   tools/helix-fork.sh status           what your custom branch adds vs master
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

UPSTREAM="upstream"
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

ensure_custom() {
  git rev-parse --verify "$CUSTOM" >/dev/null 2>&1 || {
    echo "creating $CUSTOM from master"
    git branch "$CUSTOM" "master"
  }
}

add_pr() {
  local n="$1"
  [[ "$n" =~ ^[0-9]+$ ]] || die "PR number must be numeric, got: $n"
  fetch
  local src="refs/remotes/$UPSTREAM/pr-$n"
  git rev-parse --verify "$src" >/dev/null 2>&1 \
    || die "no PR $n in fetched refs — is it open? (run fetch, check gh pr view $n)"
  local topic="$PR_PREFIX$n"
  # (re)create topic branch from the PR head
  git branch -f "$topic" "$src" || die "cannot create topic branch $topic"
  # ensure it sits cleanly on top of current master
  if ! git merge-base --is-ancestor master "$topic"; then
    echo "rebasing $topic onto master"
    git checkout -q "$topic" || die "checkout $topic failed"
    if ! git rebase master; then
      echo "CONFLICT while rebasing $topic onto master. Resolve, then run:" >&2
      echo "  git rebase --continue" >&2
      echo "  tools/helix-fork.sh finish-pr $n" >&2
      exit 1
    fi
  else
    git checkout -q "$topic"
    git rebase master  # no-op if already based on master
  fi
  finish_pr "$n"
}

finish_pr() {
  local n="$1"
  local topic="$PR_PREFIX$n"
  ensure_custom
  git checkout -q "$CUSTOM" || die "checkout custom failed"
  if git merge-base --is-ancestor "$topic" "$CUSTOM"; then
    echo "PR $n already merged into custom (nothing to do)"
  else
    echo "merging $topic into custom"
    git merge --no-ff "$topic" -m "merge upstream PR #$n ($topic)" \
      || die "merge of $topic into custom had conflicts — resolve and commit manually"
  fi
  git checkout -q master
}

sync() {
  fetch
  ensure_custom
  echo "== rebasing custom onto master =="
  git checkout -q "$CUSTOM"
  git rebase master
  # rebase each topic branch onto the new master and re-merge
  for b in $(git for-each-ref --format='%(refname:short)' "refs/heads/$PR_PREFIX*"); do
    local n="${b#$PR_PREFIX}"
    echo "== re-syncing $b =="
    git checkout -q "$b"
    git rebase master || {
      echo "CONFLICT rebasing $b — resolve, then run 'git rebase --continue'" >&2
      exit 1
    }
    git checkout -q "$CUSTOM"
    git merge --no-ff "$b" -m "re-merge upstream PR #$n" || {
      echo "CONFLICT merging $b into custom — resolve manually" >&2
      exit 1
    }
  done
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
  # Copy the source runtime/ so the binary finds grammars anywhere.
  cp -R "$REPO_ROOT/runtime" "$REPO_ROOT/target/release/runtime" \
    && echo "== runtime -> target/release/runtime =="
  git checkout -q master
  echo "binary: $REPO_ROOT/target/release/hx"
}

push() {
  local remote="${1:-origin}"
  echo "== pushing master =="
  git push "$remote" master
  echo "== pushing custom =="
  git push "$remote" custom
  echo "== pushing patch branches =="
  git push "$remote" $(git for-each-ref --format='%(refname:short)' "refs/heads/$PR_PREFIX*")
}

status() {
  ensure_custom
  echo "== master @ $(git rev-parse --short master) =="
  echo "== custom @ $(git rev-parse --short custom) =="
  echo
  echo "patch branches:"
  git for-each-ref --format='%(refname:short)  %(objectname:short)' "refs/heads/$PR_PREFIX*" || true
  [[ -z "$(git for-each-ref --format='%(refname:short)' "refs/heads/$PR_PREFIX*")" ]] \
    && echo "  (none yet — run: tools/helix-fork.sh add-pr <N>)"
  echo
  echo "custom vs master diffstat:"
  git diff --stat master custom
}

case "${1:-}" in
  fetch)   fetch ;;
  add-pr)  [[ $# -ge 2 ]] || die "usage: helix-fork.sh add-pr <PR#>"; add_pr "$2" ;;
  finish-pr) [[ $# -ge 2 ]] || die "usage: helix-fork.sh finish-pr <PR#>"; finish_pr "$2" ;;
  sync)    sync ;;
  build)   shift; build "$@" ;;
  push)    shift; push "$@" ;;
  status)  status ;;
  *) echo "usage: helix-fork.sh {fetch|add-pr <N>|sync|build [ARGS]|push [remote]|status}"; exit 0 ;;
esac
