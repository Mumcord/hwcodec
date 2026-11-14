//! Links the native codec libraries and generates their bindings, one step per cargo feature.
//!
//! Software codecs (`libvpx`, `svt-av1`, `dav1d`) and the oneVPL dispatcher of Quick Sync (`qsv`,
//! Windows and Linux only). Each library comes from the first of these that applies:
//!
//! 1. A prebuilt static library, when its variable points at a prefix:
//!
//!    | Library | Variable            | Static prefix contents                                      |
//!    |---------|---------------------|-------------------------------------------------------------|
//!    | libvpx  | `LIBVPX_STATIC_DIR` | `include/vpx/`, `lib/vpx.lib` (MSVC) or `lib/libvpx.a`      |
//!    | SVT-AV1 | `SVTAV1_STATIC_DIR` | `include/svt-av1/`, `lib/SvtAv1Enc.lib` or `lib/libSvtAv1Enc.a` |
//!    | dav1d   | `DAV1D_STATIC_DIR`  | `include/dav1d/`, `lib/dav1d.lib` or `lib/libdav1d.a`       |
//!    | libvpl  | `HWENC_SDK_DIR`     | `lib/vpl.lib` or `lib/libvpl.a`                             |
//!
//! 2. The system library found with pkg-config, when `HWCODEC_USE_SYSTEM_LIBS=1` is set.
//! 3. Otherwise a static library built here from the sources bundled in the `hwcodec-*-src`
//!    crates (`sys/`), with the pinned versions and without network access. This needs build
//!    tools on the machine; a missing one stops the build with a message that names it.
//!
//!    Windows is the exception to step 3: the sources are not built there. For
//!    `x86_64-pc-windows-msvc` all four libraries exist prebuilt from the same bundled sources:
//!
//!    - Taken from crates.io, the companion package `hwcodec-prebuilt-x86_64-pc-windows-msvc`
//!      contains them and they are linked from there, without network access.
//!    - In a git checkout that package is empty (the binaries are not kept in git). Then the
//!      build downloads the same files as one archive ([`prebuilt_url`]), checks it against
//!      the SHA-256 pinned in [`PREBUILT_SHA256`] and unpacks it into `OUT_DIR`.
//!      `HWCODEC_NO_DOWNLOAD=1` forbids that download.
//!
//!    Every other Windows target needs the variables of step 1.
//!
//! The bindings are generated from the headers of exactly the library that is linked, because
//! the libvpx ABI versions are checked at runtime and SVT-AV1's configuration struct changes
//! between releases.
//!
//! Hardware encoders (`nvenc`, `amf`, `qsv`, `vaapi`; Windows and Linux only, VA-API on Linux
//! only): the bindings come from the vendor headers bundled in `vendor/include`. The GPU drivers
//! themselves are loaded at runtime, so nothing is linked for them. The exception is Quick Sync,
//! which talks to the driver through the oneVPL dispatcher library: that one is linked as
//! described above, plus the C++ runtime it needs.

// Which of the helpers below are used depends on the enabled cargo features and the target.
#![allow(dead_code)]

use std::env;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(any(feature = "libvpx", feature = "svt-av1", feature = "dav1d"))]
struct Library {
    name: &'static str,
    env: &'static str,
    /// The library name for `rustc-link-lib` and its archive file name on MSVC and elsewhere.
    link: &'static str,
    msvc_archive: &'static str,
    archive: &'static str,
    /// A header that must exist below `include/` in a static prefix.
    header: &'static str,
    pkg_config: &'static str,
    min_version: &'static str,
    packages: &'static str,
}

#[cfg(feature = "libvpx")]
const VPX: Library = Library {
    name: "libvpx",
    env: "LIBVPX_STATIC_DIR",
    link: "vpx",
    msvc_archive: "vpx.lib",
    archive: "libvpx.a",
    header: "vpx/vpx_encoder.h",
    pkg_config: "vpx",
    min_version: "1.8",
    packages: "Debian/Ubuntu: libvpx-dev, Fedora: libvpx-devel, Arch: libvpx",
};

// SVT-AV1 4 is the first release with the per-frame rate, frame-rate and layer-count events the
// encoder relies on for live changes.
#[cfg(feature = "svt-av1")]
const SVT_AV1: Library = Library {
    name: "SVT-AV1",
    env: "SVTAV1_STATIC_DIR",
    link: "SvtAv1Enc",
    msvc_archive: "SvtAv1Enc.lib",
    archive: "libSvtAv1Enc.a",
    header: "svt-av1/EbSvtAv1Enc.h",
    pkg_config: "SvtAv1Enc",
    min_version: "4.0",
    packages: "Fedora: svt-av1-devel, Arch: svt-av1",
};

#[cfg(feature = "dav1d")]
const DAV1D: Library = Library {
    name: "dav1d",
    env: "DAV1D_STATIC_DIR",
    link: "dav1d",
    msvc_archive: "dav1d.lib",
    archive: "libdav1d.a",
    header: "dav1d/dav1d.h",
    pkg_config: "dav1d",
    min_version: "1.0",
    packages: "Debian/Ubuntu: libdav1d-dev, Fedora: libdav1d-devel, Arch: dav1d",
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed={SYSTEM_LIBS}");
    #[cfg(feature = "libvpx")]
    libvpx();
    #[cfg(feature = "svt-av1")]
    svt_av1();
    #[cfg(feature = "dav1d")]
    dav1d();
    hardware_encoders();
}

/// Whether the cargo feature `name` is switched on for this build.
fn feature(name: &str) -> bool {
    let variable = format!("CARGO_FEATURE_{}", name.to_uppercase().replace('-', "_"));
    env::var_os(variable).is_some()
}

fn msvc() -> bool {
    env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc")
}

#[cfg(feature = "libvpx")]
fn libvpx() {
    let vpx = find(&VPX, hwcodec_libvpx_src::VERSION, || {
        build_libvpx(&hwcodec_libvpx_src::source_dir())
    });
    generate(
        &vpx,
        bindgen::Builder::default()
            .header("wrapper.h")
            .allowlist_function("vpx_.*")
            .allowlist_type("vpx_.*|vp8e_.*|vp9e_.*|vp8_.*")
            .allowlist_var("VPX_.*|VP8_.*|VP8E_.*|VP9E_.*"),
        "vpx.rs",
    );
    println!("cargo:rerun-if-changed=wrapper.h");
}

#[cfg(feature = "svt-av1")]
fn svt_av1() {
    let svt = find(&SVT_AV1, hwcodec_svt_av1_src::VERSION, || {
        build_svt_av1(&hwcodec_svt_av1_src::source_dir())
    });
    generate(
        &svt,
        bindgen::Builder::default()
            // FreeBSD's port points pkg-config at include/svt-av1 and keeps the headers flat
            // there, while static builds and the Linux packages keep them below
            // include/svt-av1/. Accept both layouts.
            .header_contents(
                "svt.h",
                "#if __has_include(<svt-av1/EbSvtAv1Enc.h>)\n\
                 #include <svt-av1/EbSvtAv1Enc.h>\n\
                 #else\n\
                 #include <EbSvtAv1Enc.h>\n\
                 #endif\n",
            )
            .allowlist_function("svt_av1_.*")
            .allowlist_type("Eb.*|SvtAv1.*|PrivDataType|PredStructure")
            .allowlist_var("EB_.*"),
        "svt.rs",
    );
    // av1_svt.rs formats SVT-AV1's log messages with vsnprintf, which modern MSVC only defines
    // inline in its headers; this library exports it as a function.
    if msvc() {
        println!("cargo:rustc-link-lib=dylib=legacy_stdio_definitions");
    }
}

#[cfg(feature = "dav1d")]
fn dav1d() {
    let dav1d = find(&DAV1D, hwcodec_dav1d_src::VERSION, || {
        build_dav1d(&hwcodec_dav1d_src::source_dir())
    });
    generate(
        &dav1d,
        bindgen::Builder::default()
            // errno.h for EAGAIN, which dav1d returns negated and whose value differs per OS.
            .header_contents("dav1d.h", "#include <errno.h>\n#include <dav1d/dav1d.h>\n")
            .allowlist_function("dav1d_.*")
            .allowlist_type("Dav1d.*")
            .allowlist_var("DAV1D_.*|EAGAIN"),
        "dav1d.rs",
    );
}

/// Generates the bindings of the enabled hardware encoders from the bundled vendor headers and
/// links the oneVPL dispatcher for Quick Sync; see the module documentation.
fn hardware_encoders() {
    let os = env::var("CARGO_CFG_TARGET_OS").expect("cargo sets CARGO_CFG_TARGET_OS");
    if os != "windows" && os != "linux" {
        return;
    }
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let include = [manifest_dir.join("vendor/include")];

    if feature("nvenc") {
        generate(
            &include,
            bindgen::Builder::default()
                .header_contents("nvenc.h", "#include <ffnvcodec/nvEncodeAPI.h>\n")
                // The driver library is loaded at runtime: its two entry points become a loader
                // struct.
                .dynamic_library_name("NvEncodeApi")
                .allowlist_function("NvEncodeAPICreateInstance|NvEncodeAPIGetMaxSupportedVersion")
                .allowlist_type("_?NV_ENC.*|_?NVENC.*|_?NV_ENCODE.*|PNV.*|GUID|_GUID")
                .allowlist_var("NV_ENC.*|NVENC.*|NV_MAX.*"),
            "nvenc.rs",
        );
    }
    if feature("amf") {
        generate(
            &include,
            bindgen::Builder::default()
                .header_contents(
                    "amf.h",
                    "#include <AMF/core/Factory.h>\n#include <AMF/components/VideoEncoderAV1.h>\n",
                )
                .allowlist_type("AMF.*|amf_.*")
                .allowlist_var("AMF_.*"),
            "amf.rs",
        );
    }
    #[cfg(feature = "qsv")]
    {
        link_vpl_dispatcher();
        generate(
            &include,
            bindgen::Builder::default()
                .header_contents("vpl.h", "#include <vpl/mfx.h>\n")
                .allowlist_function("MFX.*")
                .allowlist_type("mfx.*|MFX.*")
                .allowlist_var("MFX_.*"),
            "vpl.rs",
        );
    }
    if feature("vaapi") && os == "linux" {
        // libva and libva-drm are loaded at runtime, each into its own loader struct.
        generate(
            &include,
            bindgen::Builder::default()
                .header_contents(
                    "va.h",
                    "#include <va/va.h>\n#include <va/va_enc_av1.h>\n#include <va/va_enc_vp9.h>\n",
                )
                .dynamic_library_name("LibVa")
                .allowlist_function("va.*")
                .allowlist_type("VA.*|_VA.*")
                .allowlist_var("VA_.*"),
            "va.rs",
        );
        generate(
            &include,
            bindgen::Builder::default()
                .header_contents("va_drm.h", "#include <va/va_drm.h>\n")
                .dynamic_library_name("LibVaDrm")
                .allowlist_function("vaGetDisplayDRM")
                .allowlist_recursively(false)
                .raw_line("use super::va::VADisplay;"),
            "va_drm.rs",
        );
    }
}

/// Links the oneVPL dispatcher: the static build in `HWENC_SDK_DIR`, the system library, or a
/// build of the bundled sources; see the module documentation.
#[cfg(feature = "qsv")]
fn link_vpl_dispatcher() {
    const NAME: &str = "libvpl (the oneVPL dispatcher)";
    println!("cargo:rerun-if-env-changed=HWENC_SDK_DIR");
    let dir = if let Some(dir) = env::var_os("HWENC_SDK_DIR") {
        PathBuf::from(dir)
    } else if use_system_libs() {
        if let Err(err) = pkg_config::Config::new()
            .atleast_version("2.9")
            .probe("vpl")
        {
            panic!(
                "{NAME} for the `qsv` feature was not found via pkg-config ({err}). Install the \
                 development package (Debian/Ubuntu: libvpl-dev, Fedora: libvpl-devel, Arch: \
                 libvpl), unset {SYSTEM_LIBS} to build the bundled sources, set HWENC_SDK_DIR to \
                 a prefix with a static build of it, or switch the `qsv` feature off."
            );
        }
        return;
    } else {
        bundled(NAME, "HWENC_SDK_DIR", hwcodec_libvpl_src::VERSION, || {
            build_libvpl(&hwcodec_libvpl_src::source_dir())
        })
    };
    let lib_dir = dir.join("lib");
    let vpl = lib_dir.join(if msvc() { "vpl.lib" } else { "libvpl.a" });
    assert!(
        vpl.is_file(),
        "{} is missing (expected a static build of {NAME})",
        vpl.display()
    );
    println!("cargo:rerun-if-changed={}", vpl.display());
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=vpl");
    // The dispatcher is C++ and loads Intel's runtime itself. MSVC: the archive was built against
    // the dynamic C runtime (/MD), like Rust's default, so it needs the matching C++ library.
    // On Windows it also reads the registry (advapi32) and formats adapter GUIDs (ole32).
    if msvc() {
        println!("cargo:rustc-link-lib=dylib=msvcprt");
        println!("cargo:rustc-link-lib=dylib=advapi32");
        println!("cargo:rustc-link-lib=dylib=ole32");
    } else {
        println!("cargo:rustc-link-lib=dylib=stdc++");
        println!("cargo:rustc-link-lib=dylib=dl");
    }
}

/// Links `lib` (static prefix, pkg-config or a build of the bundled sources, see the module
/// documentation) and returns its include paths. `build` compiles the bundled sources of
/// `version` into the prefix it is given.
#[cfg(any(feature = "libvpx", feature = "svt-av1", feature = "dav1d"))]
fn find(lib: &Library, version: &str, build: impl FnOnce() -> Recipe) -> Vec<PathBuf> {
    println!("cargo:rerun-if-env-changed={}", lib.env);
    if let Some(dir) = env::var_os(lib.env) {
        return link_static(lib, Path::new(&dir));
    }
    if !use_system_libs() {
        return link_static(lib, &bundled(lib.name, lib.env, version, build));
    }
    pkg_config::Config::new()
        .atleast_version(lib.min_version)
        .probe(lib.pkg_config)
        .unwrap_or_else(|err| {
            panic!(
                "{} {}+ not found via pkg-config ({err}). Install the development package ({}), \
                 unset {SYSTEM_LIBS} to build the bundled sources, or set {} to a prefix with a \
                 static build.",
                lib.name, lib.min_version, lib.packages, lib.env
            )
        })
        .include_paths
}

/// Links the static library in `dir` and returns its include path.
#[cfg(any(feature = "libvpx", feature = "svt-av1", feature = "dav1d"))]
fn link_static(lib: &Library, dir: &Path) -> Vec<PathBuf> {
    let include = dir.join("include");
    let lib_dir = dir.join("lib");
    // MSVC names static libraries `name.lib`, every other toolchain `libname.a`.
    let archive = lib_dir.join(if msvc() {
        lib.msvc_archive
    } else {
        lib.archive
    });
    for required in [&include.join(lib.header), &archive] {
        assert!(
            required.is_file(),
            "{}: {} is missing (expected a static build of {})",
            dir.display(),
            required.display(),
            lib.name
        );
    }
    // A rebuilt library at the same path (cache refresh) must relink.
    println!("cargo:rerun-if-changed={}", archive.display());
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static={}", lib.link);
    vec![include]
}

fn generate(include_paths: &[PathBuf], builder: bindgen::Builder, file: &str) {
    let mut builder = builder
        .prepend_enum_name(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    for path in include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }
    // bindgen panics instead of returning an error when it cannot load libclang.
    let bindings =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || builder.generate()))
            .unwrap_or_else(|_| missing(&LIBCLANG, &format!("generate {file}"), None))
            .unwrap_or_else(|err| panic!("generating {file} failed: {err}"));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join(file);
    bindings
        .write_to_file(&out)
        .unwrap_or_else(|err| panic!("writing {}: {err}", out.display()));
}

/// Opt-in: link the system libraries found with pkg-config instead of building the bundled
/// sources.
const SYSTEM_LIBS: &str = "HWCODEC_USE_SYSTEM_LIBS";

/// The deployment target of the bundled builds on macOS.
const MACOS_DEPLOYMENT_TARGET: &str = "11.0";

fn use_system_libs() -> bool {
    env::var(SYSTEM_LIBS).is_ok_and(|value| !value.is_empty() && value != "0")
}

fn target_os() -> String {
    env::var("CARGO_CFG_TARGET_OS").expect("cargo sets CARGO_CFG_TARGET_OS")
}

/// A build tool the bundled builds need, and how to install it.
struct Tool {
    program: &'static str,
    install: &'static str,
}

const C_COMPILER: Tool = Tool {
    program: "cc",
    install: "Debian/Ubuntu: `sudo apt install build-essential`, Fedora: `sudo dnf install gcc \
              gcc-c++ make`, Arch: `sudo pacman -S base-devel`, macOS: `xcode-select --install`",
};
const CXX_COMPILER: Tool = Tool {
    program: "c++",
    install: C_COMPILER.install,
};
const MAKE: Tool = Tool {
    program: "make",
    install: C_COMPILER.install,
};
const PERL: Tool = Tool {
    program: "perl",
    install: "Debian/Ubuntu: `sudo apt install perl`, Fedora: `sudo dnf install perl`, Arch: \
              `sudo pacman -S perl`, macOS: `brew install perl`",
};
const NASM: Tool = Tool {
    program: "nasm",
    install: "Debian/Ubuntu: `sudo apt install nasm`, Fedora: `sudo dnf install nasm`, Arch: \
              `sudo pacman -S nasm`, macOS: `brew install nasm`",
};
const CMAKE: Tool = Tool {
    program: "cmake",
    install: "Debian/Ubuntu: `sudo apt install cmake`, Fedora: `sudo dnf install cmake`, Arch: \
              `sudo pacman -S cmake`, macOS: `brew install cmake`",
};
const MESON: Tool = Tool {
    program: "meson",
    install: "Debian/Ubuntu: `sudo apt install meson`, Fedora: `sudo dnf install meson`, Arch: \
              `sudo pacman -S meson`, macOS: `brew install meson`",
};
const NINJA: Tool = Tool {
    program: "ninja",
    install: "Debian/Ubuntu: `sudo apt install ninja-build`, Fedora: `sudo dnf install \
              ninja-build`, Arch: `sudo pacman -S ninja`, macOS: `brew install ninja`",
};
const LIBCLANG: Tool = Tool {
    program: "libclang",
    install: "Debian/Ubuntu: `sudo apt install libclang-dev`, Fedora: `sudo dnf install \
              clang-devel`, Arch: `sudo pacman -S clang`, macOS: `xcode-select --install`; if it \
              is installed in an unusual place, point LIBCLANG_PATH at its directory",
};

/// Stops the build because `tool` is not installed. `env` is the variable that selects a
/// prebuilt library instead, where there is one.
fn missing(tool: &Tool, purpose: &str, env: Option<&str>) -> ! {
    let alternative = env.map_or_else(String::new, |env| {
        format!(
            "\nAlternatively set {env} to a prefix with a prebuilt static library, or \
             {SYSTEM_LIBS}=1 to use the system libraries."
        )
    });
    panic!(
        "\n\nhwcodec: the build tool `{}` is missing; it is needed to {purpose}.\nInstall it: \
         {}.{alternative}\n\n",
        tool.program, tool.install
    );
}

/// How one bundled library is built: the tools to check first, then the commands to run in the
/// build directory.
struct Recipe {
    tools: Vec<&'static Tool>,
    commands: Vec<(&'static Tool, Command)>,
}

/// The place a bundled build of one library lives in, below `OUT_DIR`.
struct Layout {
    build: PathBuf,
    prefix: PathBuf,
}

fn layout(directory: &str) -> Layout {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join(directory);
    Layout {
        build: out.join("build"),
        prefix: out.join("install"),
    }
}

/// The C or C++ compiler to use: the conventional variable when it is set, otherwise the
/// system's default compiler.
fn compiler(variable: &str, default: &Tool) -> String {
    println!("cargo:rerun-if-env-changed={variable}");
    env::var(variable)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.program.to_owned())
}

/// A command for a bundled build, with the compilers set explicitly so that every build system
/// uses the ones that were checked.
fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .env("CC", compiler("CC", &C_COMPILER))
        .env("CXX", compiler("CXX", &CXX_COMPILER));
    command
}

fn jobs() -> String {
    env::var("NUM_JOBS").unwrap_or_else(|_| "1".to_owned())
}

/// The assembler is only needed for the x86 optimisations.
fn assembler() -> Option<&'static Tool> {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("cargo sets CARGO_CFG_TARGET_ARCH");
    (arch == "x86_64" || arch == "x86").then_some(&NASM)
}

/// The processor architecture as Apple's tools name it.
fn macos_arch() -> &'static str {
    match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        _ => "x86_64",
    }
}

/// Builds the bundled sources of `name` once per `OUT_DIR` and returns the prefix with the
/// static library. `env` is the variable that selects a prebuilt library instead.
fn bundled(name: &str, env: &str, version: &str, recipe: impl FnOnce() -> Recipe) -> PathBuf {
    let host = env::var("HOST").expect("cargo sets HOST");
    let target = env::var("TARGET").expect("cargo sets TARGET");
    if target_os() == "windows" {
        return prebuilt(name, env, version, &target);
    }
    assert!(
        host == target,
        "hwcodec: the bundled sources of {name} cannot be cross-compiled ({host} to {target}). \
         Set {env} to a prefix with a static library built for {target}."
    );

    let directory = name_directory(env);
    let Layout { build, prefix } = layout(&directory);
    let stamp = prefix.join(".hwcodec-built");
    if fs::read_to_string(&stamp).is_ok_and(|built| built == version) {
        return prefix;
    }

    let recipe = recipe();
    let purpose = format!("build the bundled {name} sources");
    for tool in &recipe.tools {
        let program = match tool.program {
            "cc" => compiler("CC", tool),
            "c++" => compiler("CXX", tool),
            program => program.to_owned(),
        };
        // The variables may carry arguments or a wrapper ("ccache gcc"); the first word must run.
        let program = program.split_whitespace().next().unwrap_or(tool.program);
        let found = Command::new(program)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if found.is_err_and(|err| err.kind() == ErrorKind::NotFound) {
            missing(tool, &purpose, Some(env));
        }
    }

    // Start from nothing: a half-finished earlier build must not leak into this one.
    for dir in [&build, &prefix] {
        if dir.exists() {
            fs::remove_dir_all(dir)
                .unwrap_or_else(|err| panic!("removing {}: {err}", dir.display()));
        }
        fs::create_dir_all(dir).unwrap_or_else(|err| panic!("creating {}: {err}", dir.display()));
    }
    // The tools' output goes to a file: cargo would read lines on stdout as instructions.
    let log_path = build.join("hwcodec-build.log");
    let log = File::create(&log_path)
        .unwrap_or_else(|err| panic!("creating {}: {err}", log_path.display()));
    let clone = || {
        log.try_clone()
            .unwrap_or_else(|err| panic!("opening {}: {err}", log_path.display()))
    };
    for (tool, mut command) in recipe.commands {
        let status = command
            .current_dir(&build)
            .stdin(Stdio::null())
            .stdout(clone())
            .stderr(clone())
            .status();
        match status {
            Ok(status) if status.success() => {}
            Err(err) if err.kind() == ErrorKind::NotFound => missing(tool, &purpose, Some(env)),
            Ok(status) => panic!(
                "\n\nhwcodec: building the bundled {name} sources failed ({status}) in \
                 `{command:?}`.\nThe end of {}:\n{}\n\n",
                log_path.display(),
                log_tail(&log_path)
            ),
            Err(err) => panic!("hwcodec: running `{command:?}` failed: {err}"),
        }
    }
    fs::write(&stamp, version).unwrap_or_else(|err| panic!("writing {}: {err}", stamp.display()));
    prefix
}

/// Forbids the download of the prebuilt libraries on Windows.
const NO_DOWNLOAD: &str = "HWCODEC_NO_DOWNLOAD";

/// The only target the prebuilt libraries exist for.
const PREBUILT_TARGET: &str = "x86_64-pc-windows-msvc";

/// The release of the hwcodec repository that holds the archive with the prebuilt libraries for
/// [`PREBUILT_TARGET`]: one prefix per library, named like [`name_directory`] names it, built by
/// `.github/scripts/build-windows-libs.sh` from the bundled sources. RELEASING.md describes how
/// a new one is published.
const PREBUILT_TAG: &str = "native-libs-1";

/// The SHA-256 of the archive at [`prebuilt_url`]. Nothing else is ever unpacked or linked.
///
/// The tag and this checksum belong together and are only changed together, when a library
/// under `sys/` or its Windows build changes (RELEASING.md, "Windows: the prebuilt codec
/// libraries"). A `native-libs-*` release must never be changed or deleted: every published
/// version of this crate points at exactly the file pinned here. The files in the companion
/// package `hwcodec-prebuilt-x86_64-pc-windows-msvc` on crates.io are the contents of this same
/// archive (the release pipeline checks that byte for byte), so a build from crates.io links
/// them without any download. The download only happens on Windows, only when that package
/// holds no libraries (a git checkout or a path dependency), only for a library whose variable
/// (`LIBVPX_STATIC_DIR`, ...) is unset, and never with `HWCODEC_NO_DOWNLOAD=1`.
const PREBUILT_SHA256: &str = "adae2a3116c69d05631cc763533f9bbb50ad6ca8e9c75f23cd64e401a941f26d";

/// Where the archive of [`PREBUILT_TAG`] is downloaded from.
fn prebuilt_url() -> String {
    format!(
        "https://github.com/Mumcord/hwcodec/releases/download/{PREBUILT_TAG}/\
         hwcodec-{PREBUILT_TAG}-{PREBUILT_TARGET}.tar.gz"
    )
}

/// What to do instead of the download, for the messages below.
fn prebuilt_alternative() -> String {
    let url = prebuilt_url();
    format!(
        "Alternatively build the libraries yourself and set LIBVPX_STATIC_DIR, \
         SVTAV1_STATIC_DIR, DAV1D_STATIC_DIR and HWENC_SDK_DIR to their prefixes (see the \
         README, \"Windows\"), or download {url} by other means, unpack it and point \
         the four variables at its folders libvpx_static, svtav1_static, dav1d_static and \
         hwenc_sdk."
    )
}

/// Windows: returns the prefix of `name` among the prebuilt libraries. They come with the
/// companion package when that was taken from crates.io, and are downloaded otherwise.
fn prebuilt(name: &str, env: &str, version: &str, target: &str) -> PathBuf {
    let alternative = prebuilt_alternative();
    assert!(
        target == PREBUILT_TARGET,
        "\n\nhwcodec: there are no prebuilt libraries for {target}, only for {PREBUILT_TARGET}, \
         and the bundled sources are not built on Windows. Set {env} (and the variables of the \
         other libraries) to a prefix with a static build of {name} for {target}.\n\n"
    );
    let root = packaged_prebuilt().unwrap_or_else(|| downloaded_prebuilt(name, env, &alternative));
    let prefix = root.join(name_directory(env));
    let found = fs::read_to_string(prefix.join("VERSION")).unwrap_or_default();
    assert!(
        found.trim() == version,
        "\n\nhwcodec: the prebuilt libraries in {} hold {name} `{}`, but this version of the \
         crate bundles {version}. That is a mistake in this hwcodec release; please report it.\n\
         {alternative}\n\n",
        root.display(),
        found.trim()
    );
    prefix
}

/// The libraries inside the companion package `hwcodec-prebuilt-x86_64-pc-windows-msvc`. The
/// package on crates.io contains them, so a build from crates.io needs no download. In a git
/// checkout the package is empty, because the binaries are not kept in git.
#[cfg(all(
    target_os = "windows",
    target_arch = "x86_64",
    target_env = "msvc",
    any(
        feature = "libvpx",
        feature = "svt-av1",
        feature = "dav1d",
        feature = "qsv"
    )
))]
fn packaged_prebuilt() -> Option<PathBuf> {
    let dir = hwcodec_prebuilt_x86_64_pc_windows_msvc::dir();
    println!(
        "cargo:rerun-if-changed={}",
        hwcodec_prebuilt_x86_64_pc_windows_msvc::marker().display()
    );
    dir
}

/// The companion package is only a dependency of builds that run on 64-bit Windows with MSVC.
#[cfg(not(all(
    target_os = "windows",
    target_arch = "x86_64",
    target_env = "msvc",
    any(
        feature = "libvpx",
        feature = "svt-av1",
        feature = "dav1d",
        feature = "qsv"
    )
)))]
fn packaged_prebuilt() -> Option<PathBuf> {
    None
}

/// Windows, without libraries in the companion package (a git checkout or a path dependency):
/// returns the directory the downloaded archive of prebuilt libraries is unpacked in, fetching
/// and unpacking the archive first if this `OUT_DIR` does not hold it yet.
fn downloaded_prebuilt(name: &str, env: &str, alternative: &str) -> PathBuf {
    println!("cargo:rerun-if-env-changed={NO_DOWNLOAD}");
    assert!(
        !env::var(NO_DOWNLOAD).is_ok_and(|value| !value.is_empty() && value != "0"),
        "\n\nhwcodec: {name} is needed, {env} is not set and {NO_DOWNLOAD} forbids downloading \
         the prebuilt libraries.\n{alternative}\n\n"
    );

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let root = out.join("native-libs");
    let stamp = root.join(".hwcodec-sha256");
    if !fs::read_to_string(&stamp).is_ok_and(|unpacked| unpacked == PREBUILT_SHA256) {
        // Start from nothing: a half-unpacked earlier archive must not leak into this one.
        if root.exists() {
            fs::remove_dir_all(&root)
                .unwrap_or_else(|err| panic!("removing {}: {err}", root.display()));
        }
        fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("creating {}: {err}", root.display()));
        let archive = out.join("native-libs.tar.gz");
        download_prebuilt(&archive, alternative);
        // tar.exe is part of Windows 10 and later, like curl.exe.
        run_system_tool("tar.exe", |tar| {
            tar.arg("-xf").arg(&archive).arg("-C").arg(&root);
        })
        .unwrap_or_else(|err| {
            panic!(
                "\n\nhwcodec: unpacking {} failed: {err}\n{alternative}\n\n",
                archive.display()
            )
        });
        // Best effort: the archive is only a copy of what was just unpacked.
        let _ = fs::remove_file(&archive);
        fs::write(&stamp, PREBUILT_SHA256)
            .unwrap_or_else(|err| panic!("writing {}: {err}", stamp.display()));
    }
    root
}

/// Downloads [`prebuilt_url`] to `archive` and stops the build unless it is exactly the file
/// with [`PREBUILT_SHA256`].
#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
fn download_prebuilt(archive: &Path, alternative: &str) {
    use sha2::{Digest, Sha256};

    let url = prebuilt_url();

    // curl.exe is part of Windows 10 and later. HTTPS only, also after redirects (GitHub
    // redirects release downloads to its file storage).
    run_system_tool("curl.exe", |curl| {
        curl.args(["--fail", "--location", "--silent", "--show-error"])
            .args(["--proto", "=https", "--proto-redir", "=https"])
            .args(["--retry", "3", "--connect-timeout", "30"])
            .arg("--output")
            .arg(archive)
            .arg(&url);
    })
    .unwrap_or_else(|err| {
        panic!(
            "\n\nhwcodec: downloading the prebuilt codec libraries from {url} failed \
             (is there a network connection?): {err}\n{alternative}\n\n"
        )
    });
    let bytes =
        fs::read(archive).unwrap_or_else(|err| panic!("reading {}: {err}", archive.display()));
    let found: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if found != PREBUILT_SHA256 {
        // Never leave a file that failed the check where a later step could pick it up.
        let _ = fs::remove_file(archive);
        panic!(
            "\n\nhwcodec: the download from {url} is not the expected file and was \
             discarded.\n  expected SHA-256: {PREBUILT_SHA256}\n  found SHA-256:    {found}\n\
             Try again; if it stays like this, please report it.\n{alternative}\n\n"
        );
    }
}

/// The download runs Windows' own tools, so it only works when the build runs on Windows.
#[cfg(not(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")))]
fn download_prebuilt(_archive: &Path, alternative: &str) {
    panic!(
        "\n\nhwcodec: the prebuilt codec libraries for {PREBUILT_TARGET} can only be downloaded \
         when building on 64-bit Windows with MSVC, not when cross-compiling.\n{alternative}\n\n"
    );
}

/// Runs a tool that ships with Windows, from the system directory so that no other program of
/// the same name on `PATH` is picked up, and returns its error output if it fails.
fn run_system_tool(program: &str, arguments: impl FnOnce(&mut Command)) -> Result<(), String> {
    let system = PathBuf::from(env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32")
        .join(program);
    let mut command = Command::new(&system);
    arguments(&mut command);
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("running {}: {err}", system.display()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} ended with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// The directory name of a library's bundled build, from the name of its variable.
fn name_directory(env: &str) -> String {
    env.trim_end_matches("_DIR").to_lowercase()
}

/// The last lines of a build log, for the failure message.
fn log_tail(path: &Path) -> String {
    let log = fs::read(path).unwrap_or_default();
    let log = String::from_utf8_lossy(&log);
    let lines: Vec<&str> = log.lines().collect();
    lines[lines.len().saturating_sub(40)..].join("\n")
}

// The options of the four builds below are the ones Mumcord's release builds use.

#[cfg(feature = "libvpx")]
fn build_libvpx(source: &Path) -> Recipe {
    let Layout { prefix, .. } = layout(&name_directory(VPX.env));
    let mut configure = command("sh");
    configure
        .arg(source.join("configure"))
        .arg(format!("--prefix={}", prefix.display()))
        .args(["--enable-static", "--disable-shared", "--enable-pic"])
        .args(["--disable-examples", "--disable-tools", "--disable-docs"])
        .arg("--disable-unit-tests");
    let mut make = command("make");
    make.arg(format!("-j{}", jobs()));
    let mut install = command("make");
    install.arg("install");
    if target_os() == "macos" {
        let arch = macos_arch();
        let flags = format!("-arch {arch} -mmacosx-version-min={MACOS_DEPLOYMENT_TARGET}");
        configure
            .arg(format!("--target={arch}-darwin20-gcc"))
            .arg(format!("--extra-cflags={flags}"))
            .arg(format!("--extra-cxxflags={flags}"));
        for command in [&mut configure, &mut make, &mut install] {
            command.env("MACOSX_DEPLOYMENT_TARGET", MACOS_DEPLOYMENT_TARGET);
        }
    }
    let mut tools = vec![&C_COMPILER, &CXX_COMPILER, &MAKE, &PERL];
    tools.extend(assembler());
    Recipe {
        tools,
        commands: vec![(&C_COMPILER, configure), (&MAKE, make), (&MAKE, install)],
    }
}

#[cfg(feature = "svt-av1")]
fn build_svt_av1(source: &Path) -> Recipe {
    let Layout { build, prefix } = layout(&name_directory(SVT_AV1.env));
    let mut configure = command("cmake");
    configure
        .arg("-S")
        .arg(source)
        .arg("-B")
        .arg(&build)
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display()))
        // SVT-AV1 writes its libraries to `Bin/` in its source directory by default; a build
        // script must leave the sources untouched.
        .arg(format!(
            "-DCMAKE_OUTPUT_DIRECTORY={}",
            build.join("bin").display()
        ))
        .args(["-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=OFF"])
        .args([
            "-DBUILD_APPS=OFF",
            "-DBUILD_TESTING=OFF",
            "-DSVT_AV1_LTO=OFF",
        ])
        .args([
            "-DCMAKE_POSITION_INDEPENDENT_CODE=ON",
            "-DCMAKE_INSTALL_LIBDIR=lib",
        ]);
    if target_os() == "macos" {
        configure
            .arg(format!("-DCMAKE_OSX_ARCHITECTURES={}", macos_arch()))
            .arg(format!(
                "-DCMAKE_OSX_DEPLOYMENT_TARGET={MACOS_DEPLOYMENT_TARGET}"
            ));
    }
    let mut tools = vec![&C_COMPILER, &CXX_COMPILER, &CMAKE, &MAKE];
    tools.extend(assembler());
    let mut commands = vec![(&CMAKE, configure)];
    commands.extend(cmake_build_and_install(&build));
    Recipe { tools, commands }
}

#[cfg(feature = "dav1d")]
fn build_dav1d(source: &Path) -> Recipe {
    let Layout { build, prefix } = layout(&name_directory(DAV1D.env));
    let mut setup = command("meson");
    setup
        .arg("setup")
        .arg(&build)
        .arg(source)
        .arg(format!("--prefix={}", prefix.display()))
        .args([
            "--libdir=lib",
            "--buildtype=release",
            "--default-library=static",
        ])
        .args([
            "-Dbitdepths=8",
            "-Denable_tools=false",
            "-Denable_examples=false",
        ])
        .args([
            "-Denable_tests=false",
            "-Denable_docs=false",
            "-Dlogging=false",
        ]);
    if target_os() == "macos" {
        // meson takes the deployment target as part of the compiler command.
        let cc = compiler("CC", &C_COMPILER);
        setup.env(
            "CC",
            format!("{cc} -mmacosx-version-min={MACOS_DEPLOYMENT_TARGET}"),
        );
    }
    let mut compile = command("meson");
    compile
        .args(["compile", "-C"])
        .arg(&build)
        .args(["-j", &jobs()]);
    let mut install = command("meson");
    install.args(["install", "-C"]).arg(&build);
    let mut tools = vec![&C_COMPILER, &MESON, &NINJA];
    tools.extend(assembler());
    Recipe {
        tools,
        commands: vec![(&MESON, setup), (&MESON, compile), (&MESON, install)],
    }
}

#[cfg(feature = "qsv")]
fn build_libvpl(source: &Path) -> Recipe {
    let Layout { build, prefix } = layout(&name_directory("HWENC_SDK_DIR"));
    let mut configure = command("cmake");
    configure
        .arg("-S")
        .arg(source)
        .arg("-B")
        .arg(&build)
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display()))
        .args(["-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=OFF"])
        .args(["-DBUILD_TESTS=OFF", "-DCMAKE_INSTALL_LIBDIR=lib"])
        // The examples are not part of the bundled sources.
        .args(["-DBUILD_EXAMPLES=OFF", "-DINSTALL_EXAMPLES=OFF"]);
    let mut commands = vec![(&CMAKE, configure)];
    commands.extend(cmake_build_and_install(&build));
    Recipe {
        tools: vec![&C_COMPILER, &CXX_COMPILER, &CMAKE, &MAKE],
        commands,
    }
}

#[cfg(any(feature = "svt-av1", feature = "qsv"))]
fn cmake_build_and_install(build: &Path) -> [(&'static Tool, Command); 2] {
    let mut compile = command("cmake");
    compile
        .arg("--build")
        .arg(build)
        .args(["--config", "Release", "--parallel", &jobs()]);
    let mut install = command("cmake");
    install
        .arg("--install")
        .arg(build)
        .args(["--config", "Release"]);
    [(&CMAKE, compile), (&CMAKE, install)]
}
