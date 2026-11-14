//! The source code of the Intel VPL dispatcher (libvpl) 2.17.0, bundled so that the `hwcodec` crate can build the library
//! without network access. This crate compiles nothing itself; `hwcodec`'s build script reads
//! the sources from [`source_dir`].

use std::path::PathBuf;

/// The bundled upstream release.
pub const VERSION: &str = "2.17.0";

/// The directory that holds the upstream source tree.
pub fn source_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("upstream")
}
