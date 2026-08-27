#!/usr/bin/env bash
# install.sh — build & install the custom helix fork on a fresh system.
#
# Usage on a new machine (after cloning this repo):
#   tools/install.sh
#
# Overrides (env vars):
#   HELIX_FORK_REPO=...   clone source (default git@github.com:rokrdev/helix.git)
#   HELIX_FORK_BRANCH=... branch to build (default custom)
#   HELIX_INSTALL_DIR=... where to symlink hx (default ~/.local/bin)
#   HELIX_WORK_DIR=...    where to clone/build (default ~/projects/helix)
#
# Idempotent: re-run to pull updates from origin and rebuild.
set -euo pipefail

REPO_URL="${HELIX_FORK_REPO:-git@github.com:rokrdev/helix.git}"
BRANCH="${HELIX_FORK_BRANCH:-custom}"
INSTALL_DIR="${HELIX_INSTALL_DIR:-$HOME/.local/bin}"
WORK_DIR="${HELIX_WORK_DIR:-$HOME/projects/helix}"

say() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }

# 1. Ensure Rust toolchain
if ! command -v cargo >/dev/null 2>&1; then
  say "Installing Rust (rustup) — needs network"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1090
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
fi
command -v cargo >/dev/null 2>&1 || { echo "error: cargo not available after rustup install" >&2; exit 1; }

# 2. Clone / update the fork
if [ ! -d "$WORK_DIR/.git" ]; then
  say "Cloning $REPO_URL -> $WORK_DIR (branch $BRANCH)"
  mkdir -p "$WORK_DIR"
  git clone --branch "$BRANCH" --single-branch "$REPO_URL" "$WORK_DIR"
else
  say "Updating existing clone $WORK_DIR"
  git -C "$WORK_DIR" fetch origin "$BRANCH"
fi
cd "$WORK_DIR"
git checkout -q "$BRANCH"
git pull -q origin "$BRANCH" || true

# 3. Build release (first build pulls ~1500 crates; takes a few minutes)
say "Building (cargo build --release)"
cargo build --release

# 4. helix looks for its runtime (grammars/themes/queries) next to the binary;
#    a plain cargo build does NOT ship it. Copy it so highlighting works.
#    Skip grammar `sources/` (build-only checkouts whose read-only `.git` object
#    stores bloat the copy and fail on re-run), and clear the destination first
#    so we don't nest `runtime` inside an already-present copy.
say "Copying runtime/ -> target/release/runtime"
rm -rf target/release/runtime
rsync -a --exclude '/grammars/sources/' runtime/ target/release/runtime/

# 5. Symlink onto PATH
mkdir -p "$INSTALL_DIR"
ln -sf "$WORK_DIR/target/release/hx" "$INSTALL_DIR/hx"

say "Installed: $INSTALL_DIR/hx -> $WORK_DIR/target/release/hx"
echo
echo "Ensure $INSTALL_DIR precedes any distro/brew helix on your PATH."
"$INSTALL_DIR/hx" --version
