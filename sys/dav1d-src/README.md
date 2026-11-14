# hwcodec-dav1d-src

The source code of dav1d 1.5.4, bundled for the build of the
[hwcodec](https://github.com/Mumcord/hwcodec) crate. It is not meant to be used on its own:
`hwcodec`'s build script compiles the library from `upstream/` during the build, without network
access.

- Source: https://code.videolan.org/videolan/dav1d/-/archive/1.5.4/dav1d-1.5.4.tar.gz
- Licence: `BSD-2-Clause`; the upstream licence text is in `upstream/COPYING`.
- The sources are unmodified. To keep the package small, these parts of the archive are left out:
  the git metadata files.

The wrapper `src/lib.rs` in this crate is covered by the same licence as the bundled upstream code.
