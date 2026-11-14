#!/usr/bin/env bash
# Fetches the prebuilt Windows codec libraries that build.rs pins (PREBUILT_TAG and
# PREBUILT_SHA256) and unpacks them into the companion package, which is published with them
# but does not keep them in git:
#
#   sys/prebuilt-x86_64-pc-windows-msvc/prebuilt/
#
# Nothing is unpacked unless the archive has the pinned SHA-256, holds the library versions that
# the source packages under sys/ bundle, and is the archive the companion package's version
# names (0.1.0+native-libs.<n> for the release native-libs-<n>).
#
# Usage: fetch-windows-prebuilt.sh [directory]
#   directory: where to unpack instead (it must not exist yet); used to compare a packed crate
#   with the archive.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
package="$root/sys/prebuilt-x86_64-pc-windows-msvc"
destination=${1:-$package/prebuilt}

tag=$(sed -n 's/^const PREBUILT_TAG: &str = "\(.*\)";$/\1/p' "$root/build.rs")
target=$(sed -n 's/^const PREBUILT_TARGET: &str = "\(.*\)";$/\1/p' "$root/build.rs")
sha=$(grep -A1 '^const PREBUILT_SHA256' "$root/build.rs" | grep -oE '[0-9a-f]{64}' || true)
[[ -n $tag && -n $target && -n $sha ]] || { echo "::error::Could not read the pin from build.rs."; exit 1; }

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$package/Cargo.toml")
if [[ "${version#*+}" != "native-libs.${tag#native-libs-}" ]]; then
  echo "::error::build.rs pins $tag, but the package hwcodec-prebuilt-$target has version $version. The part after the plus sign must be native-libs.${tag#native-libs-} (RELEASING.md)."
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
archive="hwcodec-$tag-$target.tar.gz"
url="https://github.com/Mumcord/hwcodec/releases/download/$tag/$archive"
echo "Fetching $url"
curl -fsSL --retry 5 --retry-delay 5 -o "$work/$archive" "$url"
(cd "$work" && echo "$sha  $archive" | sha256sum --check --quiet)

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
[[ $failed == 0 ]] || exit 1

# Replace, never merge: the directory must hold exactly the archive.
rm -rf "$destination"
mkdir -p "$(dirname "$destination")"
mv "$work/libs" "$destination"
echo "$tag (SHA-256 $sha) unpacked into $destination"
