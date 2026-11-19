# Changelog

All notable changes to this project are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/). Below 1.0, interface changes may happen in any release and are listed here.

## [Unreleased]

### Fixed

- Software AV1 now works on x86 CPUs without AVX2. The bundled SVT-AV1 is updated from 4.2.0 to 4.3.0, which fixes the unaligned load in the SSE/SSSE3 sub-pixel variance kernels that such CPUs pick: 4.2.0 killed the whole process with a general protection fault in the middle of an encode. `Av1Encoder::new` no longer refuses those CPUs, and `av1_cpu_supported` always reports `true`; callers that check it can drop the check (the function goes away with the next breaking release).
- `selftest::measure_av1` fails a machine that cannot finish a single frame inside its time limit instead of passing it. A warm-up frame that ran out the limit on its own left no timed frame, and the result was reported as 0 % of the frame budget, that is, as fast.
- A static prefix (`SVTAV1_STATIC_DIR`, `LIBVPX_STATIC_DIR`, `DAV1D_STATIC_DIR`) built from an older version of its library is refused at build time instead of being linked silently. The header and archive of an old prefix look exactly like current ones, so a leftover SVT-AV1 4.2.0 prefix kept the crash on x86 CPUs without AVX2 while the build looked healthy. A prefix with a newer version only warns, and one that names no version at all is linked as before.

## [0.1.0] - 2026-11-15

First public release.

### Added

- AV1 and VP9 encoding behind one interface (`VideoEncoder`): hardware encoders NVENC (Windows, Linux; AV1), VA-API (Linux; AV1, VP9), AMD AMF (Windows; AV1) and Quick Sync (Windows; AV1, VP9), software encoders SVT-AV1 and libvpx.
- Software decoding of AV1 (dav1d) and VP9 (libvpx) behind `VideoDecoder`.
- Encoder settings: frame rates up to 120 fps, temporal layers, thread count and maximum frame size; unsupported settings are rejected when the encoder is opened.
- Detection with reasons: `detect_encoders` reports for every encoder whether it works and, if not, why, together with device and driver version.
- Self-test: `selftest::measure_av1` checks that an encoder delivers a usable stream; `DEFAULT_PASS_PERCENT` is the default threshold.
- Fallback: `default_order`, `open_best_encoder` (custom order possible) and `open_software_decoder`.
- Build: bundled sources by default (Linux, macOS); prebuilt libraries through `LIBVPX_STATIC_DIR`, `SVTAV1_STATIC_DIR`, `DAV1D_STATIC_DIR` and `HWENC_SDK_DIR`; system libraries through `HWCODEC_USE_SYSTEM_LIBS=1`. On Windows (`x86_64-pc-windows-msvc`) the libraries are linked prebuilt: from crates.io they come inside the package `hwcodec-prebuilt-x86_64-pc-windows-msvc`, without any download; a git checkout downloads the same files from the release `native-libs-1` of this repository and checks them against a SHA-256 fixed in the crate (`HWCODEC_NO_DOWNLOAD=1` forbids that download).
- Colour conversion from BGRA to I420 (`convert::bgra_to_i420`).
- Colour conversion from I420 to RGBA: `I420Frame::to_rgba`, and `I420Frame::append_rgba`, which writes the picture behind what a buffer already holds (for example a frame header) and so saves a picture-sized allocation and copy. The conversion handles two pixels per chroma sample and takes about a third less time than the per-pixel loop it replaces; the pixels are identical. `examples/rgba_speed.rs` measures it.
- Cargo features `nvenc`, `amf`, `qsv`, `vaapi`, `svt-av1`, `libvpx`, `dav1d`, all on by default.
- Reduced builds: every subset of the cargo features builds, including none. A build without features has no build dependencies (no bindgen, no pkg-config) and needs no libclang, C compiler or native library on any target, which is what a phone build starts from. `TEMPORAL_LAYERS`, `AV1_TEMPORAL_LAYERS`, `Av1Options`, `vp9_threads` and `av1_cpu_supported` exist in every build, independent of the features. Android and iOS are compile-checked without features, not tested.
- Bundled vendor headers and codec sources; no FFmpeg.
- Examples and tests, including mocked-driver tests.

### Known limits

- Tuned for live video (low latency) only; a quality preset is planned for 0.2.
- Decoding is software only; hardware decoding is planned for 0.2.
- macOS has software encoding and decoding only. AMF and Quick Sync exist on Windows only; on Linux, AMD and Intel graphics cards are used through VA-API.
- Tested on real hardware so far: VA-API AV1 encoding on an AMD Radeon RX 9070 XT (Linux, Mesa 26.2.5). Every other hardware path is untested; the README has the table and says how to report results.
- The prebuilt Windows libraries exist for `x86_64-pc-windows-msvc` only. Other Windows targets and cross builds need the `*_STATIC_DIR` variables.

### Changed

- Extracted from the Mumcord `crates/screenshare` crate and relicensed under `MIT OR Apache-2.0`.
- Renamed `Encoder` to `Vp9Encoder` and `Decoder` to `Vp9Decoder`.
