# hwcodec

Realtime AV1 and VP9 video encoding and decoding in Rust, behind one interface.

- **Hardware encoding:** NVIDIA NVENC, Intel Quick Sync (oneVPL), AMD AMF and VA-API (Linux). Which one exists depends on the operating system, see [Operating systems](#operating-systems).
- **Software encoding:** SVT-AV1 (AV1) and libvpx (VP9).
- **Decoding:** dav1d (AV1) and libvpx (VP9), software only in 0.1.
- No FFmpeg. Driver libraries are loaded at runtime, so a binary starts on machines without the GPU.
- Detection tells you which encoder works on this machine and, if not, why.

Licence: `MIT OR Apache-2.0`. Minimum Rust version: 1.90.

> **Status: 0.1, best effort.** Only part of the hardware was tested by the maintainers. See [Tested hardware](#tested-hardware).

## Quick start

```toml
[dependencies]
hwcodec = "0.1"
```

Detect what works on this machine:

```rust
use hwcodec::{Codec, detect_encoders};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for report in detect_encoders(Codec::Av1) {
        println!("{}: {}", report.encoder, report.status);
    }
    Ok(())
}
```

Open the best available encoder (hardware first, software as fallback):

```rust
use hwcodec::{Codec, EncoderConfig, open_best_encoder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = EncoderConfig::new(1920, 1080, 60, 6000);
    // `None` is the default order: hardware first, software last.
    let (kind, _encoder) = open_best_encoder(Codec::Av1, &config, None)?;
    println!("using {kind:?}");
    Ok(())
}
```

Pass `Some(&[EncoderKind::Software])` (or any other list) instead of `None` for your own order; `default_order(codec)` returns the default one.

Two examples in `examples/` show complete programs:

- `cargo run --example detect` prints which hardware encoders work for AV1 and VP9 and why the others do not.
- `cargo run --release --example encode_decode` encodes and decodes pictures in software and shows `open_best_encoder` with the default and a custom order. It needs no GPU.

`examples/encode_speed.rs` measures software encode speed (`cargo run --release --example encode_speed -- [vp9|av1|av1-text] [WIDTHxHEIGHT] [FPS]`). `examples/rgba_speed.rs` measures the I420 to RGBA conversion and needs no codec feature (`cargo run --release --no-default-features --example rgba_speed`). The API documentation (`cargo doc --open`) covers the rest.

## Operating systems

| Encoder | Windows | Linux | macOS | Codecs |
| --- | --- | --- | --- | --- |
| NVIDIA NVENC | yes | yes | – | AV1 |
| VA-API | – | yes | – | AV1, VP9 |
| AMD AMF | yes | – | – | AV1 |
| Intel Quick Sync | yes | – | – | AV1, VP9 |
| Software (SVT-AV1, libvpx) | yes | yes | yes | AV1, VP9 |

On Linux, AMD and Intel graphics cards are used through VA-API (Mesa or the Intel media driver). AMF and Quick Sync are Windows only. macOS has software encoding only.

## Cargo features

All features are on by default. Turn off what you do not need to shorten the first build.

| Feature | Enables |
| --- | --- |
| `nvenc` | NVIDIA NVENC (Windows, Linux) |
| `amf` | AMD AMF (Windows) |
| `qsv` | Intel Quick Sync (Windows) |
| `vaapi` | VA-API (Linux) |
| `svt-av1` | Software AV1 encoder |
| `libvpx` | VP9 encoder and decoder |
| `dav1d` | AV1 decoder |

Hardware features compile to nothing on platforms that do not have them.

### Feature subsets

Any subset of the features builds, including none. A codec whose feature is off is absent from the interface (there is no `Vp9Encoder` without `libvpx`), and `open_best_encoder` and `open_software_decoder` answer `Error::NotBuilt` for it.

| Build | Features | Needs |
| --- | --- | --- |
| No codec | `default-features = false` | Nothing besides Rust: no C compiler, no libclang, no pkg-config, no native library, on any target |
| Decode AV1 and VP9, encode VP9 (for example a phone) | `libvpx`, `dav1d` | libvpx and dav1d for the target, libclang |
| Software only | `svt-av1`, `libvpx`, `dav1d` | The three codec libraries, libclang |
| Everything (desktop) | default | See "Build requirements" |

```toml
hwcodec = { version = "0.1", default-features = false, features = ["libvpx", "dav1d"] }
```

These plain values exist in every build, also without any feature, so that code shared between a desktop and a phone build compiles unchanged: `TEMPORAL_LAYERS`, `AV1_TEMPORAL_LAYERS`, `Av1Options`, `vp9_threads` and `av1_cpu_supported`. So do the codec-independent parts: `VideoEncoder`, `VideoDecoder`, `EncoderConfig`, `EncodedFrame`, `I420Frame`, the colour conversion and the detection, which then reports every encoder as unavailable.

## Build requirements

By default the codec libraries are built from the sources bundled in the `hwcodec-*-src` packages, without network access. The **first build takes several minutes**; later builds reuse the result. You need:

- A C and C++ compiler, `make`, `perl`, `nasm` (x86 and x86-64 only), `cmake`, `meson`, `ninja` and libclang (for the bindings).
- If a tool is missing, the build stops with a message that names it.

Cross-compiling needs the variables for prebuilt libraries below; see "Cross-compiling".

### Building on Windows

On Windows the sources are not built. The build links the codec libraries **prebuilt**, so none of the tools above are needed except libclang. Which of the three cases applies:

| You build | Where the libraries come from | Network |
| --- | --- | --- |
| with one of the variables below set | your own library, for each variable that is set | none |
| hwcodec **from crates.io** (the normal case) | the package `hwcodec-prebuilt-x86_64-pc-windows-msvc`, which cargo fetches together with hwcodec | none beyond cargo itself |
| hwcodec from a **git checkout** or as a path dependency | a download during the build, checked against a fixed SHA-256 | one HTTPS download |

- **What the libraries are:** static libvpx, SVT-AV1, dav1d and the oneVPL dispatcher for `x86_64-pc-windows-msvc` (about 7 MB packed, 22 MB unpacked), built from the same bundled sources by this repository's workflow "Windows codec libraries", with the licence texts.
- **Origin:** a release of this repository named `native-libs-<number>`, currently [`native-libs-1`](https://github.com/Mumcord/hwcodec/releases/tag/native-libs-1). It states the library versions, the commit and workflow run that built the archive, and its SHA-256. The same SHA-256 is fixed in the crate's `build.rs` (`PREBUILT_SHA256`).
- **From crates.io:** the package `hwcodec-prebuilt-x86_64-pc-windows-msvc` contains that archive, unpacked. The release pipeline publishes it only after comparing its files byte for byte with the checksummed archive. cargo downloads the package only on 64-bit Windows with MSVC, never on Linux or macOS. To verify it yourself, download the archive from the release, check its SHA-256 and compare it with the package's `prebuilt/` folder.
- **From a git checkout:** the binaries are not kept in git, so that package is empty. The build then downloads `https://github.com/Mumcord/hwcodec/releases/download/native-libs-1/hwcodec-native-libs-1-x86_64-pc-windows-msvc.tar.gz` over HTTPS with `curl.exe` and unpacks it with `tar.exe`, which are part of Windows 10 and later, into the build directory. A download that does not match the SHA-256 is discarded and the build stops. Nothing is installed on the machine.
- **Opt-out:** set `HWCODEC_NO_DOWNLOAD=1` to forbid that download. A build from a git checkout then stops unless you supply the libraries with the variables below. It has no effect on a build from crates.io, which never downloads.
- **Supported target:** `x86_64-pc-windows-msvc` only, built on Windows. For other Windows targets (ARM64, the GNU toolchain) and when cross-compiling there are no prebuilt libraries: the build stops with a message that names the variables below.

To use your own libraries instead, run `.github/scripts/build-windows-libs.sh <directory>` in Git Bash (needs Visual Studio with the C++ workload, cmake, meson, ninja, nasm and MSYS2) and point the four variables at the folders it creates. A library whose variable is set is never taken from the package or the download.

### Prebuilt and system libraries

Use your own libraries instead of building the bundled sources. A prebuilt library wins over the system switch, which wins over the source build.

| Variable | Library | Expects |
| --- | --- | --- |
| `LIBVPX_STATIC_DIR` | libvpx | `include/vpx/`, `lib/libvpx.a` (MSVC: `lib/vpx.lib`) |
| `SVTAV1_STATIC_DIR` | SVT-AV1 | `include/svt-av1/`, `lib/libSvtAv1Enc.a` (MSVC: `lib/SvtAv1Enc.lib`) |
| `DAV1D_STATIC_DIR` | dav1d | `include/dav1d/`, `lib/libdav1d.a` (MSVC: `lib/dav1d.lib`) |
| `HWENC_SDK_DIR` | oneVPL dispatcher (Quick Sync) | `lib/libvpl.a` (MSVC: `lib/vpl.lib`) |

On Windows the libraries must be built for the dynamic C runtime (`/MD`), like Rust's MSVC target.

Set `HWCODEC_USE_SYSTEM_LIBS=1` to use the libraries installed on the system, found with `pkg-config`. Leave the variable unset to turn it off; do not set it to `0` or an empty value.

### Cross-compiling

The bundled sources are only built when the machine that builds is also the target (host == target). For any other target, for example Android or iOS from a desktop machine:

- **Without features** nothing native is involved: `cargo build --no-default-features --target aarch64-linux-android` needs only the rustup target, no NDK and no Xcode (linking your application still needs them, as for any Rust code).
- **With a software codec or `qsv`** build the library for the target yourself and point its variable from the table above at the result: `LIBVPX_STATIC_DIR`, `SVTAV1_STATIC_DIR`, `DAV1D_STATIC_DIR`, `HWENC_SDK_DIR`. Without the variable the build stops with a message that names it. `HWCODEC_USE_SYSTEM_LIBS=1` is for the build machine's own libraries and is the wrong choice for a cross build.
- **The bindings** are generated by bindgen from the headers in that prefix, for the target. It needs libclang on the build machine and the target's C headers (`errno.h`, `stdint.h` and the like), which libclang does not find by itself. Pass them through bindgen's variable, for example `BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$NDK/toolchains/llvm/prebuilt/linux-x86_64/sysroot"` for Android or `BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="-isysroot $(xcrun --sdk iphoneos --show-sdk-path)"` for iOS.
- The hardware features compile to nothing on targets other than Windows and Linux, so they can stay on or off there.

**Phone targets are compile-checked only, not tested.** CI runs `cargo check` and Clippy without features for `aarch64-linux-android` and `aarch64-apple-ios`. No codec has been built, linked or run on Android or iOS by this project: the steps above describe how the build script behaves, not a tested setup. The reduced feature sets themselves are built and tested on Linux in CI.

## Limits

- Tuned for live video (low latency). Other uses are not a goal in 0.1.
- AV1 and VP9 only, by design. Contributions for H.264 and H.265 are welcome.
- Decoding is software only in 0.1.
- macOS: software only. Hardware encoding and decoding through VideoToolbox is a planned extension.
- FreeBSD: best effort, not tested automatically.

## Planned for 0.2

- Hardware decoding.
- A quality preset next to the live preset.

## Support

Best effort. Issues and pull requests are read, but there is no guaranteed response time and no guarantee that every driver version works.

## Tested hardware

Everything not listed as tested is untested.

| Path | Codec | Hardware | OS | Driver | Date | Result |
| --- | --- | --- | --- | --- | --- | --- |
| VA-API | AV1 encode | AMD Radeon RX 9070 XT | Linux | Mesa 26.2.5 | 2026-11-14 | works (detection and test encode) |
| NVIDIA NVENC | AV1 encode | – | Windows | – | – | untested, reports welcome |
| NVIDIA NVENC | AV1 encode | – | Linux | – | – | untested, reports welcome |
| VA-API | VP9 encode | – | Linux | – | – | untested, reports welcome |
| VA-API (Intel) | AV1, VP9 encode | – | Linux | – | – | untested, reports welcome |
| AMD AMF | AV1 encode | – | Windows | – | – | untested, reports welcome |
| Intel Quick Sync | AV1, VP9 encode | – | Windows | – | – | untested, reports welcome |

### Reporting hardware results

Run the detection example (`cargo run --example detect`) and open an issue with its output, your operating system, GPU and driver version, and whether encoding worked.

The output lists each encoder with its status, device and driver version. If an encoder is reported as working but produces a wrong picture, or reported as unavailable although it should work, say so. Both are useful.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT licence](LICENSE-MIT), at your option.

Contributions are accepted under `MIT OR Apache-2.0`, without additional terms.

The bundled third-party sources and headers keep their own licences:

| Component | Licence |
| --- | --- |
| libvpx | BSD-3-Clause |
| SVT-AV1 | BSD-3-Clause-Clear and the AOM patent licence |
| dav1d | BSD-2-Clause |
| oneVPL (libvpl) | MIT |
| Vendor headers (NVENC, AMF, oneVPL, VA-API) | MIT |

The licence texts are in `vendor/licenses/` and in the source packages.
