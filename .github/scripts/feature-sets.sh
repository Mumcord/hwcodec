#!/usr/bin/env bash
# Proves that hwcodec builds with fewer cargo features than the default, which is how the phone
# builds of an application use it: without any codec, or with libvpx and dav1d only.
#
#   sets     Clippy (warnings are errors), the tests and the documentation for each reduced
#            feature set: no feature at all, every software codec alone, the phone set, the
#            software codecs without hardware encoders, and the full set without one hardware
#            vendor at a time. The full set is what the other CI steps check.
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

case "$mode" in
  sets)
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
