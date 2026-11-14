#!/usr/bin/env bash
# Proves that hwcodec builds with fewer cargo features than the default, which is how the phone
# builds of an application use it: without any codec, or with libvpx and dav1d only.
#
#   sets     Clippy (warnings are errors), the tests and the documentation for each reduced
#            feature set: no feature at all, every software codec alone, the phone set, the
#            software codecs without hardware encoders, and the full set without one hardware
#            vendor at a time. The full set is what the other CI steps check.
#            The codec libraries are built from the bundled sources once, by a build with the
#            default features (the same one the other CI steps run), and every set links those
#            through the variables for prebuilt libraries. Otherwise each set would compile
#            the bundled sources again, which takes minutes per set. With
#            HWCODEC_USE_SYSTEM_LIBS=1 or the variables already set, those libraries are used.
#   phones   `cargo check` and Clippy without any feature for Android and iOS. The build script
#            is given a libclang, a pkg-config and C compilers that do not exist, so the step
#            fails if a build without features ever reaches for native code again. Needs the
#            rustup targets (`rustup target add aarch64-linux-android aarch64-apple-ios`) and
#            nothing else: no NDK, no Xcode. The phone targets are compiled, never run.
#
# Usage: feature-sets.sh sets|phones
set -euo pipefail

mode=${1:?usage: feature-sets.sh sets|phones}
cd "$(dirname "$0")/../.."

software="svt-av1,libvpx,dav1d"
sets=(
  ""
  "libvpx"
  "dav1d"
  "svt-av1"
  "libvpx,dav1d"
  "$software"
  "$software,amf,qsv,vaapi"
  "$software,nvenc,qsv,vaapi"
  "$software,nvenc,amf,vaapi"
  "$software,nvenc,amf,qsv"
)

phone_targets=(aarch64-linux-android aarch64-apple-ios)

group() {
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::group::$*"; else echo "== $*"; fi
}

endgroup() {
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::endgroup::"; fi
}

# Points the variables for prebuilt libraries at what a build with the default features built
# from the bundled sources. cargo reports the library directories of that build itself, so no
# layout of the target directory is assumed.
reuse_bundled_libraries() {
  local directory prefix
  while read -r directory; do
    prefix=$(dirname "$directory")
    if [ -f "$directory/libvpx.a" ]; then export LIBVPX_STATIC_DIR=$prefix; fi
    if [ -f "$directory/libSvtAv1Enc.a" ]; then export SVTAV1_STATIC_DIR=$prefix; fi
    if [ -f "$directory/libdav1d.a" ]; then export DAV1D_STATIC_DIR=$prefix; fi
    if [ -f "$directory/libvpl.a" ]; then export HWENC_SDK_DIR=$prefix; fi
  done < <(cargo build --message-format=json |
    jq -r 'select(.reason == "build-script-executed") | .linked_paths[]' |
    sed 's/^native=//')
  for variable in LIBVPX_STATIC_DIR SVTAV1_STATIC_DIR DAV1D_STATIC_DIR HWENC_SDK_DIR; do
    if [ -z "${!variable:-}" ]; then
      echo "::error::the build with the default features left no library for $variable."
      exit 1
    fi
    echo "$variable=${!variable}"
  done
}

case "$mode" in
  sets)
    if [ -z "${HWCODEC_USE_SYSTEM_LIBS:-}${LIBVPX_STATIC_DIR:-}${SVTAV1_STATIC_DIR:-}${DAV1D_STATIC_DIR:-}${HWENC_SDK_DIR:-}" ]; then
      group "build the bundled sources once"
      reuse_bundled_libraries
      endgroup
    fi
    for set in "${sets[@]}"; do
      group "features: ${set:-none}"
      cargo clippy --all-targets --no-default-features --features "$set" -- -D warnings
      cargo test --no-default-features --features "$set"
      RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --no-default-features --features "$set"
      endgroup
    done
    ;;
  phones)
    # Nothing native may be needed: every tool the build script could reach for is missing.
    export LIBCLANG_PATH=/nonexistent PKG_CONFIG=/nonexistent
    export CC=/nonexistent CXX=/nonexistent TARGET_CC=/nonexistent TARGET_CXX=/nonexistent
    for target in "${phone_targets[@]}"; do
      group "no features: $target"
      cargo check --no-default-features --target "$target"
      cargo clippy --no-default-features --target "$target" -- -D warnings
      endgroup
    done
    ;;
  *)
    echo "usage: feature-sets.sh sets|phones" >&2
    exit 2
    ;;
esac
