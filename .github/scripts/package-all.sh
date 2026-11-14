#!/usr/bin/env bash
# Packs all six packages exactly as a release would upload them, without publishing anything.
#
# The package with the prebuilt Windows libraries is packed from files that are not in git: they
# are fetched from the pinned, checksummed archive first. cargo only packs such files with
# --allow-dirty, so this script itself makes sure that nothing else in the checkout differs from
# git, and that the packed libraries are exactly the archive's.
#
# Usage: package-all.sh
set -euo pipefail

cd "$(dirname "$0")/../.."
.github/scripts/fetch-windows-prebuilt.sh
if [[ -n $(git status --porcelain) ]]; then
  echo "::error::The checkout differs from git; packages are only built from committed files:"
  git status --short
  exit 1
fi
# The whole workspace at once: hwcodec can only be packed alone once the other packages are on
# crates.io.
cargo package --workspace --allow-dirty
packages=$(cargo metadata --no-deps --format-version 1 | jq -r .target_directory)/package
.github/scripts/verify-prebuilt-package.sh "$packages"/hwcodec-prebuilt-x86_64-pc-windows-msvc-*.crate
