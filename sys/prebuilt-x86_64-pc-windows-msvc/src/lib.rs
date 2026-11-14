//! Prebuilt static libvpx, SVT-AV1, dav1d and Intel VPL dispatcher for `x86_64-pc-windows-msvc`,
//! shipped so that the `hwcodec` crate can link them on Windows without building the codec
//! sources and without network access. This crate compiles nothing itself; `hwcodec`'s build
//! script reads the libraries from [`dir`].

use std::path::PathBuf;

/// A file that is present exactly when this package holds the libraries.
pub fn marker() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prebuilt/README.txt")
}

/// The directory with the prebuilt libraries: one prefix per library (`libvpx_static`,
/// `svtav1_static`, `dav1d_static`, `hwenc_sdk`) and the licence texts.
///
/// `None` in a checkout of the git repository, which does not contain the binaries; the package
/// on crates.io always has them.
pub fn dir() -> Option<PathBuf> {
    marker()
        .is_file()
        .then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("prebuilt"))
}
