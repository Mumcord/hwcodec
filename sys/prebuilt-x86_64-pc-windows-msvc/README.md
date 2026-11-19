# hwcodec-prebuilt-x86_64-pc-windows-msvc

Prebuilt codec libraries for the build of the [hwcodec](https://github.com/Mumcord/hwcodec) crate on
Windows (`x86_64-pc-windows-msvc`). It is not meant to be used on its own: `hwcodec` depends on it
only on that platform and links the libraries from `prebuilt/`, because the codec sources are not
built on Windows. On Linux and macOS this package is neither downloaded nor used.

## Contents

Static libraries built for the dynamic C runtime (`/MD`), with their headers:

| Folder in `prebuilt/` | Library | Licence |
| --- | --- | --- |
| `libvpx_static` | libvpx | `BSD-3-Clause`, with the patent grant in `PATENTS` |
| `svtav1_static` | SVT-AV1 (encoder) | `BSD-3-Clause-Clear AND BSD-2-Clause`, with the Alliance for Open Media patent licence in `PATENTS.md` |
| `dav1d_static` | dav1d | `BSD-2-Clause` |
| `hwenc_sdk` | Intel VPL dispatcher (libvpl) | `MIT` |

The exact library versions are in `prebuilt/README.txt` and in the `VERSION` file of each folder.
The licence texts are in `prebuilt/LICENSES/`.

## Where the binaries come from and how to verify them

They are built from the unmodified sources in the packages `hwcodec-libvpx-src`,
`hwcodec-svt-av1-src`, `hwcodec-dav1d-src` and `hwcodec-libvpl-src` by the workflow "Windows codec
libraries" of the hwcodec repository (`.github/workflows/native-libs.yml`, script
`.github/scripts/build-windows-libs.sh`) and published as one archive in a GitHub release named
`native-libs-<number>`. The number is the last part of this package's version: `0.1.0+native-libs.2`
holds the archive of
[`native-libs-2`](https://github.com/Mumcord/hwcodec/releases/tag/native-libs-2). That release
states the commit and the workflow run that built the archive, and its SHA-256.

`prebuilt/` is that archive, unpacked. To verify it, download the archive from the release, check
its SHA-256 against the one in the release description (the same value is `PREBUILT_SHA256` in
hwcodec's `build.rs`), unpack it and compare the files with `prebuilt/`: they are identical. The
release pipeline of hwcodec makes this comparison before it publishes this package.

The binaries are not kept in the git repository. A checkout therefore has no `prebuilt/` directory;
`hwcodec`'s build then downloads the same archive and checks the same SHA-256.

The wrapper `src/lib.rs` in this crate is licensed under `MIT OR Apache-2.0`, like hwcodec.
