#!/usr/bin/env bash
# Checks a packed hwcodec-prebuilt-x86_64-pc-windows-msvc crate before it may be published: its
# prebuilt/ directory must be, byte for byte, the contents of the archive that build.rs pins by
# SHA-256, with all four libraries. A published version can never be corrected, and this is the
# one package that is packed from files which are not in git.
#
# Usage: verify-prebuilt-package.sh <file.crate>
set -euo pipefail

crate=${1:?usage: verify-prebuilt-package.sh <file.crate>}
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

"$here/fetch-windows-prebuilt.sh" "$work/archive"
mkdir "$work/crate"
tar -xzf "$crate" -C "$work/crate" --strip-components=1

for lib in libvpx_static/lib/vpx.lib svtav1_static/lib/SvtAv1Enc.lib dav1d_static/lib/dav1d.lib \
  hwenc_sdk/lib/vpl.lib; do
  if [[ ! -s "$work/crate/prebuilt/$lib" ]]; then
    echo "::error::$(basename "$crate") does not contain prebuilt/$lib. Was fetch-windows-prebuilt.sh run before packaging?"
    exit 1
  fi
done
if ! diff -r "$work/archive" "$work/crate/prebuilt"; then
  echo "::error::The prebuilt/ directory in $(basename "$crate") differs from the pinned archive."
  exit 1
fi
size=$(stat -c %s "$crate")
echo "$(basename "$crate"): prebuilt/ is identical to the pinned archive ($(find "$work/crate/prebuilt" -type f | wc -l) files, package size $size bytes)."
# crates.io refuses packages above 10 MB.
if ((size > 10 * 1024 * 1024)); then
  echo "::error::$(basename "$crate") is $size bytes; crates.io accepts at most 10 MB."
  exit 1
fi
