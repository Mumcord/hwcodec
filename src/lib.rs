//! Realtime AV1 and VP9 video encoders and decoders behind one interface ([`VideoEncoder`],
//! [`VideoDecoder`]).
//!
//! - VP9 on libvpx (`Vp9Encoder`, `Vp9Decoder`; feature `libvpx`): realtime mode with three
//!   temporal layers (L1T3, pattern 0-2-1-2).
//! - AV1: SVT-AV1 encodes (`Av1Encoder`; feature `svt-av1`) in its low-delay realtime mode with
//!   the same three temporal layers; dav1d decodes (`Av1Decoder`; feature `dav1d`).
//! - Hardware encoders ([`open_hardware_encoder`]; features `nvenc`, `vaapi`, `amf`, `qsv`):
//!   NVENC, VA-API, AMF and Quick Sync on Windows and Linux, usable only where the GPU driver
//!   loads.
//!
//! - Detection ([`detect_encoders`]) tells which hardware encoders really work on this machine
//!   and why the others do not; [`open_best_encoder`] opens the first one that works and ends
//!   with software; `measure_av1` tells whether software AV1 runs in real time here.
//!
//! The crate stores nothing and prints nothing: results come back as values, diagnostics go to
//! the [`log`] crate.
//!
//! Every feature is on by default; a codec whose feature is off is not part of the build.
//!
//! Dropping every frame of the top layer halves the frame rate, dropping the top two quarters
//! it, and the remaining frames still decode. Each [`EncodedFrame`] carries its layer id so a
//! sender can thin out the stream per viewer without re-encoding. Resolution steps are a runtime
//! reconfiguration of the same encoder.
//!
//! Frames are I420 in one tightly packed buffer (Y, then U, then V; chroma planes are half the
//! size, rounded up).

#[cfg(feature = "dav1d")]
mod av1_dav1d;
#[cfg(feature = "svt-av1")]
mod av1_svt;
mod codec;
pub mod convert;
#[cfg(feature = "libvpx")]
mod decoder;
pub mod detect;
#[cfg(feature = "libvpx")]
mod encoder;
pub mod fallback;
#[cfg(feature = "libvpx")]
mod ffi;
mod frame;
mod hw;
pub mod selftest;

#[cfg(feature = "dav1d")]
pub use av1_dav1d::Av1Decoder;
#[cfg(feature = "svt-av1")]
pub use av1_svt::{AV1_TEMPORAL_LAYERS, Av1Encoder, Av1Options, av1_cpu_supported};
pub use codec::{Codec, EncodedFrame, EncoderConfig, VideoDecoder, VideoEncoder};
#[cfg(feature = "libvpx")]
pub use decoder::Vp9Decoder;
pub use detect::{
    EncoderReport, EncoderStatus, UnavailableReason, detect_encoder, detect_encoders,
};
#[cfg(feature = "libvpx")]
pub use encoder::{TEMPORAL_LAYERS, Vp9Encoder, threads as vp9_threads};
pub use fallback::{EncoderKind, default_order, open_best_encoder, open_software_decoder};
pub use frame::{I420Frame, MAX_DECODED_HEIGHT, MAX_DECODED_WIDTH};
pub use hw::{HardwareEncoder, open_hardware_encoder};
#[cfg(feature = "svt-av1")]
pub use selftest::{DEFAULT_PASS_PERCENT, SelftestResult, measure_av1};

/// What can go wrong in this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A call into a codec library or GPU driver failed.
    #[error("{library} {operation} failed: {message}")]
    Codec {
        /// The library or driver interface that failed.
        library: &'static str,
        /// What was being done.
        operation: &'static str,
        /// The library's own description of the failure.
        message: String,
    },
    /// A picture or compressed frame that cannot be used.
    #[error("invalid frame: {0}")]
    InvalidFrame(String),
    /// A configuration the encoder cannot run with.
    #[error("invalid encoder configuration: {0}")]
    InvalidConfig(String),
    /// The hardware encoder can't be used here (build, driver or GPU); use a software encoder.
    #[error("{encoder} hardware encoder unavailable: {reason}")]
    HardwareUnavailable {
        /// The encoder that was asked for.
        encoder: HardwareEncoder,
        /// Why it cannot be used.
        reason: String,
    },
    /// A codec whose cargo feature is switched off in this build.
    #[error("{what} is not built into this binary")]
    NotBuilt {
        /// The codec that is missing.
        what: &'static str,
    },
}

/// The result type of this crate.
pub type Result<T> = std::result::Result<T, Error>;
