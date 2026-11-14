#!/usr/bin/env bash
# Checks the prebuilt Windows codec libraries that build.rs downloads: the archive of the pinned
# release can be downloaded, has the pinned SHA-256 and holds the library versions that the
# source packages under sys/ bundle. Read-only; a crate must never be published with a pin that
# fails here, because a published version cannot be corrected.
#
# Usage: check-windows-prebuilt.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
tag=$(sed -n 's/^const PREBUILT_TAG: &str = "\(.*\)";$/\1/p' "$root/build.rs")
target=$(sed -n 's/^const PREBUILT_TARGET: &str = "\(.*\)";$/\1/p' "$root/build.rs")
sha=$(grep -A1 '^const PREBUILT_SHA256' "$root/build.rs" | grep -oE '[0-9a-f]{64}')
[[ -n $tag && -n $target && -n $sha ]] || { echo "::error::Could not read the pin from build.rs."; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
archive="hwcodec-$tag-$target.tar.gz"
url="https://github.com/Mumcord/hwcodec/releases/download/$tag/$archive"
echo "Checking $url"
curl -fsSL --retry 5 --retry-delay 5 -o "$work/$archive" "$url"
echo "$sha  $work/$archive" | sha256sum --check --quiet

mkdir "$work/libs"
tar -xzf "$work/$archive" -C "$work/libs"
failed=0
while read -r prefix crate; do
  bundled=$(sed -n 's/^pub const VERSION: &str = "\(.*\)";$/\1/p' "$root/sys/$crate/src/lib.rs")
  prebuilt=$(cat "$work/libs/$prefix/VERSION")
  if [[ $bundled != "$prebuilt" ]]; then
    echo "::error::$tag holds $crate $prebuilt, but sys/$crate bundles $bundled. Publish a new native-libs release and pin it in build.rs (RELEASING.md)."
    failed=1
  fi
done <<'EOF'
libvpx_static libvpx-src
svtav1_static svt-av1-src
dav1d_static dav1d-src
hwenc_sdk libvpl-src
EOF
[[ $failed == 0 ]] && echo "$tag matches the pin in build.rs and the versions under sys/."
exit "$failed"
