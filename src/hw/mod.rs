//! Direct hardware encoders, each behind [`VideoEncoder`]: NVENC (NVIDIA), VA-API (Linux, Intel
//! and AMD), AMF (AMD) and Quick Sync through oneVPL (Intel).
//!
//! Each one is compiled only with its cargo feature, and only on Windows and Linux (VA-API on
//! Linux only). The GPU drivers are loaded at runtime ([`dl`]), so a missing driver, an
//! unsupported GPU or a build without the feature all end in the same
//! [`Error::HardwareUnavailable`], and the caller falls back to the software encoders.

#[cfg(all(feature = "amf", any(target_os = "windows", target_os = "linux")))]
mod amf;
#[cfg(any(
    all(
        any(feature = "nvenc", feature = "amf"),
        any(target_os = "windows", target_os = "linux")
    ),
    all(feature = "vaapi", target_os = "linux")
))]
mod dl;
#[cfg(all(feature = "nvenc", any(target_os = "windows", target_os = "linux")))]
mod nvenc;
// Used by the vendor encoders that don't report temporal layer ids themselves.
#[cfg(all(
    any(feature = "amf", feature = "qsv"),
    any(target_os = "windows", target_os = "linux")
))]
mod obu;
#[cfg(all(feature = "qsv", any(target_os = "windows", target_os = "linux")))]
mod qsv;
#[cfg(all(feature = "vaapi", target_os = "linux"))]
mod vaapi;

use std::fmt;

use crate::detect::UnavailableReason;
use crate::{Codec, EncoderConfig, Error, Result, VideoEncoder};

/// What a backend learns about the driver and device while it opens an encoder; detection reads
/// it, so that no error text has to be interpreted.
#[derive(Debug, Default)]
pub(crate) struct Probe {
    /// Why opening failed, set by the place that knows the cause. `None` after a failure means
    /// "for another reason".
    pub(crate) reason: Option<UnavailableReason>,
    /// The graphics device, as the driver names it.
    pub(crate) device: Option<String>,
    /// The driver version, as the driver reports it.
    pub(crate) driver_version: Option<String>,
}

/// What is known about `encoder` and `codec` without touching a driver: `Some` when the pair can
/// never work on this operating system or with this vendor interface.
pub(crate) fn known_unavailable(
    encoder: HardwareEncoder,
    codec: Codec,
) -> Option<(UnavailableReason, &'static str)> {
    let os = match encoder {
        _ if cfg!(not(any(target_os = "windows", target_os = "linux"))) => {
            Some("hardware encoders are only used on Windows and Linux")
        }
        HardwareEncoder::Nvenc => None,
        HardwareEncoder::Vaapi if cfg!(not(target_os = "linux")) => {
            Some("VA-API only exists on Linux")
        }
        HardwareEncoder::Vaapi => None,
        HardwareEncoder::Amf if cfg!(not(windows)) => {
            Some("AMF is only used on Windows; Linux uses VA-API")
        }
        HardwareEncoder::Amf => None,
        HardwareEncoder::Qsv if cfg!(not(windows)) => {
            Some("Quick Sync is only used on Windows; Linux uses VA-API")
        }
        HardwareEncoder::Qsv => None,
    };
    if let Some(detail) = os {
        return Some((UnavailableReason::NotAvailableOnThisOs, detail));
    }
    match (encoder, codec) {
        (HardwareEncoder::Nvenc, Codec::Vp9) => Some((
            UnavailableReason::CodecNotSupported,
            "NVENC has no VP9 encoder",
        )),
        (HardwareEncoder::Amf, Codec::Vp9) => Some((
            UnavailableReason::CodecNotSupported,
            "AMF has no VP9 encoder",
        )),
        _ => None,
    }
}

/// Generated vendor bindings, one module per SDK.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[allow(
    missing_docs,
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code,
    unused_imports,
    unnecessary_transmutes,
    improper_ctypes,
    unsafe_op_in_unsafe_fn,
    clippy::all,
    clippy::undocumented_unsafe_blocks
)]
mod ffi {
    #[cfg(feature = "nvenc")]
    pub mod nvenc {
        include!(concat!(env!("OUT_DIR"), "/nvenc.rs"));
    }
    #[cfg(feature = "amf")]
    pub mod amf {
        include!(concat!(env!("OUT_DIR"), "/amf.rs"));
    }
    #[cfg(feature = "qsv")]
    pub mod vpl {
        include!(concat!(env!("OUT_DIR"), "/vpl.rs"));
    }
    #[cfg(all(feature = "vaapi", target_os = "linux"))]
    pub mod va {
        include!(concat!(env!("OUT_DIR"), "/va.rs"));
    }
    #[cfg(all(feature = "vaapi", target_os = "linux"))]
    pub mod va_drm {
        include!(concat!(env!("OUT_DIR"), "/va_drm.rs"));
    }
}

/// The hardware encoders this crate knows, one per vendor interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HardwareEncoder {
    /// NVIDIA NVENC.
    Nvenc,
    /// VA-API (Linux; Intel and AMD).
    Vaapi,
    /// AMD AMF.
    Amf,
    /// Intel Quick Sync through oneVPL.
    Qsv,
}

impl HardwareEncoder {
    /// Every hardware encoder.
    pub const ALL: [Self; 4] = [Self::Nvenc, Self::Vaapi, Self::Amf, Self::Qsv];

    pub(crate) fn unavailable(self, reason: impl Into<String>) -> Error {
        Error::HardwareUnavailable {
            encoder: self,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for HardwareEncoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Nvenc => "NVENC",
            Self::Vaapi => "VA-API",
            Self::Amf => "AMF",
            Self::Qsv => "Quick Sync",
        })
    }
}

/// Opens `encoder` for `codec`, or fails with [`Error::HardwareUnavailable`] when this build,
/// system or GPU can't provide it.
pub fn open_hardware_encoder(
    encoder: HardwareEncoder,
    codec: Codec,
    config: EncoderConfig,
) -> Result<Box<dyn VideoEncoder>> {
    open(encoder, codec, config, &mut Probe::default())
}

/// [`open_hardware_encoder`], plus what the backend learned about the driver and device on the
/// way.
pub(crate) fn probe(
    encoder: HardwareEncoder,
    codec: Codec,
    config: EncoderConfig,
) -> (Result<Box<dyn VideoEncoder>>, Probe) {
    let mut probe = Probe::default();
    let result = open(encoder, codec, config, &mut probe);
    (result, probe)
}

fn open(
    encoder: HardwareEncoder,
    codec: Codec,
    config: EncoderConfig,
    probe: &mut Probe,
) -> Result<Box<dyn VideoEncoder>> {
    match encoder {
        #[cfg(all(feature = "nvenc", any(target_os = "windows", target_os = "linux")))]
        HardwareEncoder::Nvenc => nvenc::open(codec, config, probe),
        #[cfg(all(feature = "vaapi", target_os = "linux"))]
        HardwareEncoder::Vaapi => vaapi::open(codec, config, probe),
        #[cfg(not(target_os = "linux"))]
        HardwareEncoder::Vaapi => Err(encoder.unavailable("VA-API only exists on Linux")),
        #[cfg(all(feature = "amf", any(target_os = "windows", target_os = "linux")))]
        HardwareEncoder::Amf => amf::open(codec, config, probe),
        #[cfg(all(feature = "qsv", any(target_os = "windows", target_os = "linux")))]
        HardwareEncoder::Qsv => qsv::open(codec, config, probe),
        // Only reachable for an encoder whose feature is off or that this system doesn't have.
        #[allow(unreachable_patterns)]
        _ => {
            let _ = (codec, config, probe);
            Err(encoder.unavailable("not built into this binary"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Without the GPU (as in CI), every encoder reports itself unavailable instead of failing to
    // load or crashing. Implemented encoders may open on a machine that has the hardware.
    #[test]
    fn every_hardware_encoder_reports_unavailable() {
        let config = EncoderConfig::new(1280, 720, 30, 2000);
        for encoder in HardwareEncoder::ALL {
            for codec in [Codec::Av1, Codec::Vp9] {
                match open_hardware_encoder(encoder, codec, config) {
                    Err(Error::HardwareUnavailable { encoder: e, .. }) => assert_eq!(e, encoder),
                    Err(err) => panic!("{encoder} {codec:?}: unexpected error {err}"),
                    Ok(_) if (encoder, codec) == (HardwareEncoder::Nvenc, Codec::Av1) => {}
                    Ok(_) if encoder == HardwareEncoder::Vaapi => {}
                    Ok(_) if (encoder, codec) == (HardwareEncoder::Amf, Codec::Av1) => {}
                    Ok(_) if encoder == HardwareEncoder::Qsv => {}
                    Ok(_) => panic!("{encoder} {codec:?}: not implemented yet"),
                }
            }
        }
    }

    #[test]
    fn vendors_without_vp9_are_known_before_opening() {
        for encoder in [HardwareEncoder::Nvenc, HardwareEncoder::Amf] {
            assert!(known_unavailable(encoder, Codec::Vp9).is_some());
        }
        if cfg!(target_os = "linux") {
            assert_eq!(known_unavailable(HardwareEncoder::Nvenc, Codec::Av1), None);
            assert_eq!(known_unavailable(HardwareEncoder::Vaapi, Codec::Vp9), None);
            assert_eq!(
                known_unavailable(HardwareEncoder::Amf, Codec::Av1).map(|(reason, _)| reason),
                Some(UnavailableReason::NotAvailableOnThisOs)
            );
        }
    }
}
