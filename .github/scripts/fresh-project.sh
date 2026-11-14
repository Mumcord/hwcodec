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
#   system      the system's libraries, with HWCODEC_USE_SYSTEM_LIBS=1; not what CI proves,
#               but it lets the script run on a machine without the tools for the bundled build
#
# Usage: fresh-project.sh bundled|packaged|downloaded|system
set -euo pipefail

mode=${1:?usage: fresh-project.sh bundled|packaged|downloaded|system}
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
  [[ $mode == system && $variable == HWCODEC_USE_SYSTEM_LIBS ]] && continue
  [[ -z ${!variable:-} ]] || fail "$variable is set; the check must run the default build."
done
packaged_marker="$root/sys/prebuilt-x86_64-pc-windows-msvc/prebuilt/README.txt"
case "$mode" in
  bundled) ;;
  system)
    [[ ${HWCODEC_USE_SYSTEM_LIBS:-} == 1 ]] || fail "HWCODEC_USE_SYSTEM_LIBS=1 must be set."
    ;;
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
# cargo records that output and, beside it in the file root-output, the script's OUT_DIR.
# Where it keeps both differs between cargo versions (build/hwcodec-<hash>/output up to 1.99,
# build/hwcodec/<hash>/run/stdout since 1.100), so the output is searched for by a line that
# only hwcodec's build script prints, and the OUT_DIR is read instead of assumed.
show_layout() {
  echo "Files of hwcodec's build script below target/release/build:"
  find target/release/build -path '*hwcodec*' -type f -not -path '*/out/*' | sort
}
output=$(find target/release/build -path '*hwcodec*' -type f \( -name output -o -name stdout \) \
  -not -path '*/out/*' -exec grep -l "^cargo:rerun-if-env-changed=HWCODEC_USE_SYSTEM_LIBS" {} +) || true
if [[ -z $output ]]; then
  show_layout
  fail "hwcodec's build script output was not found."
fi
[[ $(wc -l <<< "$output") -eq 1 ]] || fail "more than one build of hwcodec: $output"
# cargo notes the script's OUT_DIR in the file root-output beside its output.
if [[ ! -f $(dirname "$output")/root-output ]]; then
  show_layout
  fail "no root-output next to $output."
fi
out_native=$(tr -d '\r\n' < "$(dirname "$output")/root-output" | tr '\\' '/')
out_dir=$(unix_path "$out_native")
[[ -d $out_dir ]] || fail "the OUT_DIR $out_native does not exist."
[[ $out_native == *"$(basename "$parent")/fresh/target/"* ]] ||
  fail "the OUT_DIR $out_native is not in the fresh project's target directory."
echo "build script output: $output"
echo "OUT_DIR: $out_native"
searches=$(sed -n 's/^cargo:rustc-link-search=native=//p' "$output" | tr -d '\r' | tr '\\' '/' | sort -u)
if [[ $mode != system ]]; then
  for library in vpx SvtAv1Enc dav1d; do
    grep -q "^cargo:rustc-link-lib=static=$library" "$output" || fail "$library was not linked statically."
  done
  [[ -n $searches ]] || fail "the build script named no library directory."
fi

case "$mode" in
  system)
    expected=""
    source="the system's (HWCODEC_USE_SYSTEM_LIBS=1)"
    ;;
  bundled)
    expected="$out_native/"
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
    expected="$out_native/native-libs/"
    pin=$(grep -A1 '^const PREBUILT_SHA256' "$root/build.rs" | grep -oE '[0-9a-f]{64}')
    tag=$(sed -n 's/^const PREBUILT_TAG: &str = "\(.*\)";$/\1/p' "$root/build.rs")
    # The build script writes this file after it downloaded, checked and unpacked the archive.
    [[ $(cat "$out_dir/native-libs/.hwcodec-sha256" 2> /dev/null) == "$pin" ]] ||
      fail "$out_dir/native-libs holds no download with the pinned SHA-256."
    source="downloaded by the build script from the release $tag (SHA-256 $pin)"
    ;;
esac
while read -r search; do
  [[ -n $search ]] || continue
  [[ $search == *"$expected"* ]] || fail "a library was linked from $search, which is not $source."
  echo "linked from $search"
done <<< "$searches"

echo "fresh project ($mode) on ${RUNNER_OS:-$(uname -s)}: AV1 and VP9 round-tripped; codec libraries $source."
if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
  echo "Fresh project ($mode): AV1 and VP9 round-tripped; codec libraries $source." >> "$GITHUB_STEP_SUMMARY"
fi

cd "$root"
rm -rf "$parent"
