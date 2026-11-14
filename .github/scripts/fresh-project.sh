#!/usr/bin/env bash
# Proves what the README promises to somebody who starts from nothing: a new, empty cargo
# project that adds hwcodec builds without any manual download or script, and the software
# round trip of examples/encode_decode.rs encodes and decodes AV1 and VP9 in it.
#
# The project is created with `cargo new` outside the checkout, gets hwcodec as a path
# dependency on the checkout and the example as its main.rs, and is built in its own target
# directory, so nothing an earlier step built is reused. Afterwards the script reads what
# hwcodec's build script told cargo to link and fails unless the codec libraries came from
# where the mode says:
#
#   bundled     built from the sources under sys/ (Linux and macOS)
#   packaged    the companion package with the prebuilt libraries, no download (Windows)
#   downloaded  the archive pinned in build.rs, downloaded by the build script (Windows)
#
# Usage: fresh-project.sh bundled|packaged|downloaded
set -euo pipefail

mode=${1:?usage: fresh-project.sh bundled|packaged|downloaded}
root=$(cd "$(dirname "$0")/../.." && pwd)

fail() {
  echo "::error::fresh project ($mode): $*"
  exit 1
}

# cargo.exe does not understand the /d/a/... paths of the Windows bash.
native_path() {
  if command -v cygpath > /dev/null; then cygpath -m "$1"; else echo "$1"; fi
}
unix_path() {
  if command -v cygpath > /dev/null; then cygpath -u "$1"; else echo "$1"; fi
}

# A variable that selects other libraries would make this a different build than the default.
for variable in HWCODEC_USE_SYSTEM_LIBS LIBVPX_STATIC_DIR SVTAV1_STATIC_DIR DAV1D_STATIC_DIR HWENC_SDK_DIR; do
  [[ -z ${!variable:-} ]] || fail "$variable is set; the check must run the default build."
done
packaged_marker="$root/sys/prebuilt-x86_64-pc-windows-msvc/prebuilt/README.txt"
case "$mode" in
  bundled) ;;
  packaged)
    [[ ${HWCODEC_NO_DOWNLOAD:-} == 1 ]] || fail "HWCODEC_NO_DOWNLOAD=1 must be set, so that a download would fail the build."
    [[ -f $packaged_marker ]] || fail "the companion package holds no libraries; run fetch-windows-prebuilt.sh first."
    ;;
  downloaded)
    [[ -z ${HWCODEC_NO_DOWNLOAD:-} ]] || fail "HWCODEC_NO_DOWNLOAD is set; the download could not happen."
    [[ ! -e $packaged_marker ]] || fail "the companion package holds libraries; nothing would be downloaded."
    ;;
  *) fail "unknown mode." ;;
esac

parent=$(mktemp -d "$(unix_path "${RUNNER_TEMP:-${TMPDIR:-/tmp}}")/hwcodec-fresh-XXXXXX")
case "$parent/" in
  "$root"/*) fail "$parent is inside the checkout." ;;
esac
project="$parent/fresh"

echo "::group::Create an empty project in $project and add hwcodec"
cargo new --bin --name fresh "$(native_path "$project")"
cd "$project"
cargo add --path "$(native_path "$root")"
cp "$root/examples/encode_decode.rs" src/main.rs
cat Cargo.toml
echo "::endgroup::"

echo "::group::Build and run it"
cargo run --release | tee run.log
echo "::endgroup::"

for codec in AV1 VP9; do
  grep -q "^$codec: .* encoded to .* and decoded again" run.log ||
    fail "the program did not report a $codec round trip."
done

# What hwcodec's build script told cargo: one search path and one library per codec library.
output=$(grep -l "rustc-link-lib=static=vpx" target/release/build/hwcodec-*/output) ||
  fail "hwcodec's build script output was not found."
[[ $(wc -l <<< "$output") -eq 1 ]] || fail "more than one build of hwcodec: $output"
out_dir=$(dirname "$output")/out
for library in vpx SvtAv1Enc dav1d; do
  grep -q "^cargo:rustc-link-lib=static=$library\$" "$output" || fail "$library was not linked statically."
done
searches=$(sed -n 's/^cargo:rustc-link-search=native=//p' "$output" | tr '\\' '/' | sort -u)
[[ -n $searches ]] || fail "the build script named no library directory."

case "$mode" in
  bundled)
    expected="$(basename "$parent")/fresh/target/release/build/hwcodec-"
    source="built from the bundled sources (sys/) in the project's own target directory"
    logs=$(find "$out_dir" -name hwcodec-build.log | wc -l)
    [[ $logs -ge 3 ]] || fail "only $logs build logs of bundled sources in $out_dir."
    ;;
  packaged)
    expected="/sys/prebuilt-x86_64-pc-windows-msvc/prebuilt/"
    source="taken from the companion package, download forbidden (HWCODEC_NO_DOWNLOAD=1)"
    [[ ! -e $out_dir/native-libs ]] || fail "the build unpacked a download into $out_dir/native-libs."
    ;;
  downloaded)
    expected="/out/native-libs/"
    pin=$(grep -A1 '^const PREBUILT_SHA256' "$root/build.rs" | grep -oE '[0-9a-f]{64}')
    tag=$(sed -n 's/^const PREBUILT_TAG: &str = "\(.*\)";$/\1/p' "$root/build.rs")
    # The build script writes this file after it downloaded, checked and unpacked the archive.
    [[ $(cat "$out_dir/native-libs/.hwcodec-sha256" 2> /dev/null) == "$pin" ]] ||
      fail "$out_dir/native-libs holds no download with the pinned SHA-256."
    source="downloaded by the build script from the release $tag (SHA-256 $pin)"
    ;;
esac
while read -r search; do
  [[ $search == *"$expected"* ]] || fail "a library was linked from $search, which is not $source."
  echo "linked from $search"
done <<< "$searches"

echo "fresh project ($mode) on ${RUNNER_OS:-$(uname -s)}: AV1 and VP9 round-tripped; codec libraries $source."
if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
  echo "Fresh project ($mode): AV1 and VP9 round-tripped; codec libraries $source." >> "$GITHUB_STEP_SUMMARY"
fi

cd "$root"
rm -rf "$parent"
