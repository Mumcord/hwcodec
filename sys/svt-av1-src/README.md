# hwcodec-svt-av1-src

The source code of SVT-AV1 4.2.0, bundled for the build of the
[hwcodec](https://github.com/Mumcord/hwcodec) crate. It is not meant to be used on its own:
`hwcodec`'s build script compiles the library from `upstream/` during the build, without network
access.

- Source: https://gitlab.com/AOMediaCodec/SVT-AV1/-/archive/v4.2.0/SVT-AV1-v4.2.0.tar.gz
- Licence: `BSD-3-Clause-Clear AND BSD-2-Clause`; the upstream licence text is in `upstream/LICENSE.md`, `upstream/LICENSE-BSD2.md` and `upstream/PATENTS.md`.
- The sources are unmodified. To keep the package small, these parts of the archive are left out:
  `Docs/`, `test/`, `ffmpeg_plugin/`, `third_party/googletest/`, `snap/`, `Dockerfile` and the git metadata files.

The wrapper `src/lib.rs` in this crate is covered by the same licence as the bundled upstream code.
