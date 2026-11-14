# Changelog

All notable changes to this project are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows [Semantic Versioning](https://semver.org/). Below 1.0, interface changes may happen in any release and are listed here.

## [Unreleased]

First release (0.1.0).

### Added

- AV1 and VP9 encoding behind one interface (`VideoEncoder`): hardware encoders NVENC (Windows, Linux; AV1), VA-API (Linux; AV1, VP9), AMD AMF (Windows; AV1) and Quick Sync (Windows; AV1, VP9), software encoders SVT-AV1 and libvpx.
- Software decoding of AV1 (dav1d) and VP9 (libvpx) behind `VideoDecoder`.
- Encoder settings: frame rates up to 120 fps, temporal layers, thread count and maximum frame size; unsupported settings are rejected when the encoder is opened.
- Detection with reasons: `detect_encoders` reports for every encoder whether it works and, if not, why, together with device and driver version.
- Self-test: `selftest::measure_av1` checks that an encoder delivers a usable stream; `DEFAULT_PASS_PERCENT` is the default threshold.
- Fallback: `default_order`, `open_best_encoder` (custom order possible) and `open_software_decoder`.
- Build: bundled sources by default (Linux, macOS); prebuilt libraries through `LIBVPX_STATIC_DIR`, `SVTAV1_STATIC_DIR`, `DAV1D_STATIC_DIR` and `HWENC_SDK_DIR`; system libraries through `HWCODEC_USE_SYSTEM_LIBS=1`. The source build is not available on Windows yet.
- Colour conversion from BGRA to I420 (`convert::bgra_to_i420`).
- Cargo features `nvenc`, `amf`, `qsv`, `vaapi`, `svt-av1`, `libvpx`, `dav1d`, all on by default.
- Bundled vendor headers and codec sources; no FFmpeg.
- Examples and tests, including mocked-driver tests.

### Changed

- Extracted from the Mumcord `crates/screenshare` crate and relicensed under `MIT OR Apache-2.0`.
- Renamed `Encoder` to `Vp9Encoder` and `Decoder` to `Vp9Decoder`.
