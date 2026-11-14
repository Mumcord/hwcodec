//! Detection of the hardware encoders that work on this machine, with a reason for each one that
//! does not.
//!
//! An encoder counts as working only if it opens and then encodes a few real frames, so a driver
//! that claims support but fails is never reported as usable. Nothing is remembered between
//! calls; a caller that wants to keep the result stores it itself.

use std::fmt;

use crate::hw::{known_unavailable, probe};
use crate::{Codec, EncoderConfig, HardwareEncoder, I420Frame, VideoEncoder};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_FPS: u32 = 30;
const TEST_BITRATE_KBPS: u32 = 2000;
const TEST_FRAMES: u32 = 5;

/// Why a hardware encoder cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UnavailableReason {
    /// The encoder does not exist on this operating system.
    NotAvailableOnThisOs,
    /// The GPU driver's library was not found.
    DriverNotFound,
    /// The GPU driver is older than the encoder needs.
    DriverTooOld,
    /// The device or its vendor interface cannot encode this codec.
    CodecNotSupported,
    /// The encoder did not open for another reason.
    OpeningFailed,
    /// The encoder opened but failed to encode the test picture.
    TestEncodeFailed,
}

/// Whether a hardware encoder works.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncoderStatus {
    /// The encoder opened and encoded the test picture.
    Works,
    /// The encoder cannot be used.
    #[non_exhaustive]
    Unavailable {
        /// The reason, from a fixed list.
        reason: UnavailableReason,
        /// The driver's or this crate's own error text.
        detail: String,
    },
}

/// The detection result for one hardware encoder and one codec.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EncoderReport {
    /// The encoder that was tested.
    pub encoder: HardwareEncoder,
    /// Whether it works, and if not, why.
    pub status: EncoderStatus,
    /// The graphics device, where the driver names it.
    pub device: Option<String>,
    /// The driver version, where the driver reports it.
    pub driver_version: Option<String>,
}

impl fmt::Display for UnavailableReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotAvailableOnThisOs => "not available on this operating system",
            Self::DriverNotFound => "driver not found",
            Self::DriverTooOld => "driver too old",
            Self::CodecNotSupported => "device does not support this codec",
            Self::OpeningFailed => "opening failed",
            Self::TestEncodeFailed => "test encode failed",
        })
    }
}

impl fmt::Display for EncoderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Works => f.write_str("works"),
            Self::Unavailable { reason, detail } => write!(f, "{reason}: {detail}"),
        }
    }
}

impl EncoderReport {
    /// Whether the encoder works.
    #[must_use]
    pub fn works(&self) -> bool {
        self.status == EncoderStatus::Works
    }
}

/// Tests every hardware encoder for `codec` by encoding a small test picture; one report each,
/// in the order of [`HardwareEncoder::ALL`].
#[must_use]
pub fn detect_encoders(codec: Codec) -> Vec<EncoderReport> {
    HardwareEncoder::ALL
        .into_iter()
        .map(|encoder| detect_encoder(codec, encoder))
        .collect()
}

/// Tests one hardware encoder for `codec` by encoding a small test picture.
#[must_use]
pub fn detect_encoder(codec: Codec, encoder: HardwareEncoder) -> EncoderReport {
    if let Some((reason, detail)) = known_unavailable(encoder, codec) {
        return EncoderReport {
            encoder,
            status: EncoderStatus::Unavailable {
                reason,
                detail: detail.into(),
            },
            device: None,
            driver_version: None,
        };
    }
    let config = EncoderConfig::new(TEST_WIDTH, TEST_HEIGHT, TEST_FPS, TEST_BITRATE_KBPS);
    let (opened, probe) = probe(encoder, codec, config);
    let status = match opened {
        Ok(mut opened) => match test_encode(opened.as_mut()) {
            Ok(()) => EncoderStatus::Works,
            Err(detail) => {
                log::warn!("{encoder} {codec:?} failed its test encode: {detail}");
                EncoderStatus::Unavailable {
                    reason: UnavailableReason::TestEncodeFailed,
                    detail,
                }
            }
        },
        Err(err) => {
            log::debug!("{encoder} {codec:?} did not open: {err}");
            EncoderStatus::Unavailable {
                reason: probe.reason.unwrap_or(UnavailableReason::OpeningFailed),
                detail: err.to_string(),
            }
        }
    };
    EncoderReport {
        encoder,
        status,
        device: probe.device,
        driver_version: probe.driver_version,
    }
}

/// Encodes a few frames; passes when every call works and a keyframe with data came out.
fn test_encode(encoder: &mut dyn VideoEncoder) -> Result<(), String> {
    let config = encoder.config();
    let mut keyframe = false;
    let mut bytes = 0;
    for n in 0..TEST_FRAMES {
        let frame = test_frame(config.width, config.height, n);
        let pts = i64::from(n * 1000 / TEST_FPS);
        let out = encoder
            .encode(&frame, pts, n == 0)
            .map_err(|err| err.to_string())?;
        keyframe |= out.iter().any(|f| f.keyframe);
        bytes += out.iter().map(|f| f.data.len()).sum::<usize>();
    }
    if !keyframe {
        return Err("the encoder produced no keyframe".into());
    }
    if bytes == 0 {
        return Err("the encoder produced no data".into());
    }
    Ok(())
}

/// A screen-like test picture: a soft background with rows of sharp "text" blocks; `n` shifts
/// the blocks so consecutive frames differ.
pub(crate) fn test_frame(width: u32, height: u32, n: u32) -> I420Frame {
    let (w, h) = (width as usize, height as usize);
    let mut data = vec![128u8; I420Frame::buffer_len(width, height)];
    for (y, row) in data[..w * h].chunks_exact_mut(w).enumerate() {
        for (x, px) in row.iter_mut().enumerate() {
            let text = !(y / 4).is_multiple_of(5)
                && !((x + n as usize * 3) / 6).is_multiple_of(3)
                && (y / 20).is_multiple_of(2);
            *px = if text {
                ((x * 7 + y * 13) % 64) as u8
            } else {
                (180 + (x + y) / 32 % 60) as u8
            };
        }
    }
    I420Frame::new(width, height, data).expect("buffer_len fits the size")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Holds with and without a GPU: an encoder either works or names a reason with a text.
    #[test]
    fn every_hardware_encoder_gets_a_report_with_a_reason() {
        for codec in [Codec::Av1, Codec::Vp9] {
            let reports = detect_encoders(codec);
            let tested: Vec<_> = reports.iter().map(|r| r.encoder).collect();
            assert_eq!(tested, HardwareEncoder::ALL);
            for report in reports {
                match &report.status {
                    EncoderStatus::Works => assert!(report.works()),
                    EncoderStatus::Unavailable { detail, .. } => {
                        assert!(!detail.is_empty(), "{:?}", report.encoder);
                    }
                }
            }
        }
    }

    #[test]
    fn reasons_and_statuses_read_as_plain_text() {
        let reason = UnavailableReason::DriverNotFound;
        assert_eq!(reason.to_string(), "driver not found");
        assert_eq!(
            UnavailableReason::NotAvailableOnThisOs.to_string(),
            "not available on this operating system"
        );
        assert_eq!(EncoderStatus::Works.to_string(), "works");
        let status = EncoderStatus::Unavailable {
            reason,
            detail: "libva.so.2 did not load".into(),
        };
        assert_eq!(
            status.to_string(),
            "driver not found: libva.so.2 did not load"
        );
    }

    #[test]
    fn one_encoder_is_reported_as_in_the_full_list() {
        // An encoder that never opens here, so both calls give the same answer.
        let single = detect_encoder(Codec::Vp9, HardwareEncoder::Nvenc);
        assert_eq!(single.encoder, HardwareEncoder::Nvenc);
        assert!(!single.works());
        assert!(detect_encoders(Codec::Vp9).contains(&single));
    }

    #[test]
    fn encoders_that_can_never_work_name_the_fixed_reason() {
        let reason = |codec, encoder| {
            let reports = detect_encoders(codec);
            let report = reports.iter().find(|r| r.encoder == encoder);
            match report.map(|r| &r.status) {
                Some(EncoderStatus::Unavailable { reason, .. }) => Some(*reason),
                _ => None,
            }
        };
        if cfg!(any(target_os = "windows", target_os = "linux")) {
            assert_eq!(
                reason(Codec::Vp9, HardwareEncoder::Nvenc),
                Some(UnavailableReason::CodecNotSupported)
            );
        }
        if cfg!(not(windows)) {
            for encoder in [HardwareEncoder::Amf, HardwareEncoder::Qsv] {
                assert_eq!(
                    reason(Codec::Av1, encoder),
                    Some(UnavailableReason::NotAvailableOnThisOs)
                );
            }
        }
    }

    #[cfg(feature = "libvpx")]
    #[test]
    fn a_working_encoder_passes_the_test_encode() {
        let config = EncoderConfig::new(TEST_WIDTH, TEST_HEIGHT, TEST_FPS, TEST_BITRATE_KBPS);
        let mut vp9 = crate::Vp9Encoder::new(config).expect("vp9");
        assert_eq!(test_encode(&mut vp9), Ok(()));
    }

    #[test]
    fn test_frames_have_the_right_size_and_move() {
        let first = test_frame(64, 48, 0);
        let second = test_frame(64, 48, 1);
        assert_eq!(first.data().len(), I420Frame::buffer_len(64, 48));
        assert_ne!(first.data(), second.data());
    }
}
