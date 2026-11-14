# hwcodec-libvpx-src

The source code of libvpx 1.15.2, bundled for the build of the
[hwcodec](https://github.com/Mumcord/hwcodec) crate. It is not meant to be used on its own:
`hwcodec`'s build script compiles the library from `upstream/` during the build, without network
access.

- Source: https://github.com/webmproject/libvpx/archive/refs/tags/v1.15.2.tar.gz
- Licence: `BSD-3-Clause`; the upstream licence text is in `upstream/LICENSE` and `upstream/PATENTS`.
- The sources are unmodified. To keep the package small, these parts of the archive are left out:
  `test/`, `build_debug/`, `third_party/googletest/src/` and the git metadata files.

The wrapper `src/lib.rs` in this crate is covered by the same licence as the bundled upstream code.
