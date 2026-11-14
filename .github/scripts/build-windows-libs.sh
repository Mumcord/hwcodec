#!/usr/bin/env bash
# Builds the four codec libraries hwcodec links (libvpx, SVT-AV1, dav1d and the oneVPL
# dispatcher) for x86_64-pc-windows-msvc from the bundled sources under sys/, as static
# libraries on the dynamic C runtime (/MD), like Rust's MSVC target.
#
# Usage: build-windows-libs.sh <output directory>
#
# Result, one prefix per library, named after the variable that selects it:
#   <out>/libvpx_static/   include/vpx/*.h      lib/vpx.lib        VERSION
#   <out>/svtav1_static/   include/svt-av1/*.h  lib/SvtAv1Enc.lib  VERSION
#   <out>/dav1d_static/    include/dav1d/*.h    lib/dav1d.lib      VERSION
#   <out>/hwenc_sdk/                            lib/vpl.lib        VERSION
#   <out>/LICENSES/<library>/                   the licence texts of each library
# The same tree is what hwcodec's build downloads on Windows, and each prefix can be handed to
# the build directly (LIBVPX_STATIC_DIR, SVTAV1_STATIC_DIR, DAV1D_STATIC_DIR, HWENC_SDK_DIR).
#
# Runs in Git Bash on a machine with Visual Studio (C++ workload), cmake, meson, ninja, nasm and
# MSYS2 in C:\msys64 (libvpx's configure and make need a POSIX shell; make, diffutils and perl
# are installed into it if missing). The build
# options are the ones build.rs uses for the bundled builds on Linux and macOS.
set -euo pipefail

out=${1:?usage: build-windows-libs.sh <output directory>}
root=$(cd "$(dirname "$0")/../.." && pwd)
mkdir -p "$out"
out=$(cd "$out" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# cmake, meson and MSVC are native Windows programs and do not understand paths like /tmp/....
win() { cygpath -m "$1"; }

# The version a source crate bundles, from its `VERSION` constant.
version() {
  sed -n 's/^pub const VERSION: &str = "\(.*\)";$/\1/p' "$root/sys/$1/src/lib.rs"
}

# finish <prefix name> <source crate> <licence files...>: records the version and the licences.
finish() {
  local prefix=$1 crate=$2
  shift 2
  version "$crate" > "$out/$prefix/VERSION"
  [[ -s "$out/$prefix/VERSION" ]] || { echo "no version found for $crate" >&2; exit 1; }
  mkdir -p "$out/LICENSES/$crate"
  local file
  for file in "$@"; do cp "$root/sys/$crate/upstream/$file" "$out/LICENSES/$crate/"; done
}

msbuild=$("C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe" \
  -latest -requires Microsoft.Component.MSBuild -find 'MSBuild\**\Bin\MSBuild.exe' | head -n 1)
[[ -n $msbuild ]] || { echo "MSBuild not found (is Visual Studio installed?)" >&2; exit 1; }

# --- libvpx -------------------------------------------------------------------------------------
# configure and make only write a Visual Studio project here; MSBuild compiles it.
mkdir -p "$work/vpx"
# The MSYS2 login shell starts with its own PATH; configure must still find this shell's NASM.
nasm_dir=$(dirname "$(command -v nasm)")
MSYSTEM=MSYS CHERE_INVOKING=1 MSYS2_PATH_TYPE=inherit C:/msys64/usr/bin/bash.exe -lc "
  set -euo pipefail
  # A fresh MSYS2 has none of the tools libvpx's configure and make run.
  pacman -S --noconfirm --needed make diffutils perl > /dev/null
  PATH=\"\$(cygpath -u '$(win "$nasm_dir")'):\$PATH\"
  cd '$(win "$work/vpx")'
  '$(win "$root/sys/libvpx-src/upstream/configure")' --target=x86_64-win64-vs17 \
    --enable-static --disable-shared --enable-pic \
    --disable-examples --disable-tools --disable-docs --disable-unit-tests
  make -j${NUMBER_OF_PROCESSORS:-4}
"
# Two settings of the generated project do not fit a library that is handed on: whole program
# optimisation leaves objects only this exact linker version can read (rustc cannot repack them
# into its own archives), and the debug information would live in a separate vpxmd.pdb that
# every later link asks for.
cat > "$work/vpx/hwcodec.props" <<'EOF'
<Project xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <ItemDefinitionGroup>
    <ClCompile>
      <WholeProgramOptimization>false</WholeProgramOptimization>
      <DebugInformationFormat>None</DebugInformationFormat>
    </ClCompile>
  </ItemDefinitionGroup>
</Project>
EOF
"$msbuild" "$(cygpath -w "$work/vpx/vpx.vcxproj")" -m -nologo -verbosity:minimal \
  -p:Configuration=Release -p:Platform=x64 -p:WholeProgramOptimization=false \
  -p:ForceImportAfterCppTargets="$(cygpath -w "$work/vpx/hwcodec.props")"
mkdir -p "$out/libvpx_static/include/vpx" "$out/libvpx_static/lib"
# The library on the dynamic C runtime is called vpxmd.lib; the build links `vpx`.
cp "$work/vpx/x64/Release/vpxmd.lib" "$out/libvpx_static/lib/vpx.lib"
# The headers `make install` installs elsewhere.
cp "$root"/sys/libvpx-src/upstream/vpx/*.h "$out/libvpx_static/include/vpx/"
finish libvpx_static libvpx-src LICENSE PATENTS AUTHORS

# --- SVT-AV1 ------------------------------------------------------------------------------------
cmake -S "$(win "$root/sys/svt-av1-src/upstream")" -B "$(win "$work/svt")" -A x64 \
  -DCMAKE_INSTALL_PREFIX="$(win "$work/svt-install")" \
  -DCMAKE_OUTPUT_DIRECTORY="$(win "$work/svt/bin")" \
  -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL -DCMAKE_POLICY_DEFAULT_CMP0091=NEW \
  -DBUILD_SHARED_LIBS=OFF -DBUILD_APPS=OFF -DBUILD_TESTING=OFF -DSVT_AV1_LTO=OFF \
  -DCMAKE_INSTALL_LIBDIR=lib
cmake --build "$(win "$work/svt")" --config Release --parallel
cmake --install "$(win "$work/svt")" --config Release
mkdir -p "$out/svtav1_static/include" "$out/svtav1_static/lib"
cp -R "$work/svt-install/include/svt-av1" "$out/svtav1_static/include/"
cp "$work/svt-install/lib/SvtAv1Enc.lib" "$out/svtav1_static/lib/"
finish svtav1_static svt-av1-src LICENSE.md LICENSE-BSD2.md PATENTS.md

# --- dav1d --------------------------------------------------------------------------------------
meson setup "$(win "$work/dav1d")" "$(win "$root/sys/dav1d-src/upstream")" --vsenv -Db_vscrt=md \
  --prefix="$(win "$work/dav1d-install")" --libdir=lib --buildtype=release \
  --default-library=static -Dbitdepths=8 -Denable_tools=false -Denable_examples=false \
  -Denable_tests=false -Denable_docs=false -Dlogging=false
meson compile -C "$(win "$work/dav1d")"
meson install -C "$(win "$work/dav1d")"
mkdir -p "$out/dav1d_static/include" "$out/dav1d_static/lib"
cp -R "$work/dav1d-install/include/dav1d" "$out/dav1d_static/include/"
# meson names static archives libdav1d.a even with MSVC; the build links dav1d.lib.
cp "$work/dav1d-install/lib/libdav1d.a" "$out/dav1d_static/lib/dav1d.lib"
finish dav1d_static dav1d-src COPYING

# --- oneVPL dispatcher --------------------------------------------------------------------------
cmake -S "$(win "$root/sys/libvpl-src/upstream")" -B "$(win "$work/vpl")" -A x64 \
  -DCMAKE_INSTALL_PREFIX="$(win "$work/vpl-install")" -DUSE_MSVC_STATIC_RUNTIME=OFF \
  -DBUILD_SHARED_LIBS=OFF -DBUILD_TESTS=OFF -DBUILD_EXAMPLES=OFF -DINSTALL_EXAMPLES=OFF \
  -DCMAKE_INSTALL_LIBDIR=lib
cmake --build "$(win "$work/vpl")" --config Release --parallel
cmake --install "$(win "$work/vpl")" --config Release
mkdir -p "$out/hwenc_sdk/lib"
cp "$work/vpl-install/lib/vpl.lib" "$out/hwenc_sdk/lib/"
finish hwenc_sdk libvpl-src LICENSE third-party-programs.txt

cat > "$out/README.txt" <<EOF
Prebuilt codec libraries for the hwcodec crate (https://github.com/Mumcord/hwcodec),
target x86_64-pc-windows-msvc, static, dynamic C runtime (/MD).

  libvpx $(version libvpx-src), SVT-AV1 $(version svt-av1-src), dav1d $(version dav1d-src), oneVPL dispatcher $(version libvpl-src)

Built from the unmodified sources bundled in the repository under sys/ by
.github/scripts/build-windows-libs.sh. The licence of each library is in LICENSES/.
EOF

echo "Windows codec libraries installed to $out"
