# hwcodec-libvpl-src

The source code of the Intel VPL dispatcher (libvpl) 2.17.0, bundled for the build of the
[hwcodec](https://github.com/Mumcord/hwcodec) crate. It is not meant to be used on its own:
`hwcodec`'s build script compiles the library from `upstream/` during the build, without network
access.

- Source: https://github.com/intel/libvpl/archive/refs/tags/v2.17.0.tar.gz
- Licence: `MIT`; the upstream licence text is in `upstream/LICENSE` and `upstream/third-party-programs.txt`.
- The sources are unmodified. To keep the package small, these parts of the archive are left out:
  `doc/`, `libvpl/test/`, the contents of `examples/` (its `CMakeLists.txt` stays) and the git metadata files.

The wrapper `src/lib.rs` in this crate is covered by the same licence as the bundled upstream code.
