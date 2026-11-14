//! Fallback from one encoder to the next, in an order the caller can replace.

use std::fmt;

use crate::{
    Codec, EncoderConfig, Error, HardwareEncoder, Result, VideoDecoder, VideoEncoder,
    open_hardware_encoder,
};

/// One entry of a fallback order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EncoderKind {
    /// A hardware encoder.
    Hardware(HardwareEncoder),
    /// The software encoder of the codec (SVT-AV1 for AV1, libvpx for VP9).
    Software,
}

impl fmt::Display for EncoderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hardware(encoder) => write!(f, "{encoder} (hardware)"),
            Self::Software => f.write_str("software"),
        }
    }
}

/// The order [`open_best_encoder`] tries by default for `codec` on this operating system: the
/// hardware encoders that exist here, best first, then software.
#[must_use]
pub fn default_order(codec: Codec) -> Vec<EncoderKind> {
    use HardwareEncoder::{Amf, Nvenc, Qsv, Vaapi};
    let hardware: &[HardwareEncoder] = match codec {
        Codec::Av1 if cfg!(target_os = "linux") => &[Nvenc, Vaapi],
        Codec::Vp9 if cfg!(target_os = "linux") => &[Vaapi],
        Codec::Av1 if cfg!(windows) => &[Nvenc, Amf, Qsv],
        Codec::Vp9 if cfg!(windows) => &[Qsv],
        _ => &[],
    };
    hardware
        .iter()
        .map(|&encoder| EncoderKind::Hardware(encoder))
        .chain([EncoderKind::Software])
        .collect()
}

/// Opens the first encoder of `order` that works for `codec` and tells which one it is; an
/// encoder that fails to open is skipped. Without `order` the [`default_order`] is used. Fails
/// with the last encoder's error when none opens.
pub fn open_best_encoder(
    codec: Codec,
    config: &EncoderConfig,
    order: Option<&[EncoderKind]>,
) -> Result<(EncoderKind, Box<dyn VideoEncoder>)> {
    let default;
    let order = match order {
        Some(order) => order,
        None => {
            default = default_order(codec);
            &default
        }
    };
    let mut last = None;
    for &kind in order {
        let opened = match kind {
            EncoderKind::Hardware(encoder) => open_hardware_encoder(encoder, codec, *config),
            EncoderKind::Software => open_software_encoder(codec, *config),
        };
        match opened {
            Ok(encoder) => return Ok((kind, encoder)),
            Err(err) => {
                log::warn!("{kind:?} {codec:?} encoder did not open; trying the next: {err}");
                last = Some(err);
            }
        }
    }
    Err(last.unwrap_or_else(|| Error::InvalidConfig("no encoder to try".into())))
}

fn open_software_encoder(codec: Codec, config: EncoderConfig) -> Result<Box<dyn VideoEncoder>> {
    match codec {
        #[cfg(feature = "svt-av1")]
        Codec::Av1 => Ok(Box::new(crate::Av1Encoder::new(
            config,
            crate::Av1Options::default(),
        )?)),
        #[cfg(feature = "libvpx")]
        Codec::Vp9 => Ok(Box::new(crate::Vp9Encoder::new(config)?)),
        // Only reachable for a codec whose feature is off.
        #[allow(unreachable_patterns)]
        Codec::Av1 | Codec::Vp9 => {
            let _ = config;
            Err(not_built(codec, "encoder"))
        }
    }
}

/// Opens the software decoder for `codec` (dav1d for AV1, libvpx for VP9).
pub fn open_software_decoder(codec: Codec) -> Result<Box<dyn VideoDecoder>> {
    match codec {
        #[cfg(feature = "dav1d")]
        Codec::Av1 => Ok(Box::new(crate::Av1Decoder::new()?)),
        #[cfg(feature = "libvpx")]
        Codec::Vp9 => Ok(Box::new(crate::Vp9Decoder::new()?)),
        // Only reachable for a codec whose feature is off.
        #[allow(unreachable_patterns)]
        Codec::Av1 | Codec::Vp9 => Err(not_built(codec, "decoder")),
    }
}

fn not_built(codec: Codec, part: &str) -> Error {
    let what = match (codec, part) {
        (Codec::Av1, "encoder") => "the software AV1 encoder (feature svt-av1)",
        (Codec::Av1, _) => "the software AV1 decoder (feature dav1d)",
        (Codec::Vp9, "encoder") => "the software VP9 encoder (feature libvpx)",
        (Codec::Vp9, _) => "the software VP9 decoder (feature libvpx)",
    };
    Error::NotBuilt { what }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> EncoderConfig {
        EncoderConfig::new(320, 240, 15, 300)
    }

    #[test]
    fn kinds_read_as_plain_text() {
        assert_eq!(EncoderKind::Software.to_string(), "software");
        assert_eq!(
            EncoderKind::Hardware(HardwareEncoder::Vaapi).to_string(),
            "VA-API (hardware)"
        );
    }

    #[test]
    fn the_default_order_ends_with_software_and_names_it_once() {
        for codec in [Codec::Av1, Codec::Vp9] {
            let order = default_order(codec);
            assert_eq!(order.last(), Some(&EncoderKind::Software));
            let software = order.iter().filter(|k| **k == EncoderKind::Software);
            assert_eq!(software.count(), 1);
        }
    }

    #[test]
    fn an_empty_order_is_an_error() {
        let result = open_best_encoder(Codec::Vp9, &config(), Some(&[]));
        assert!(matches!(result, Err(Error::InvalidConfig(_))));
    }

    #[cfg(feature = "libvpx")]
    #[test]
    fn a_failing_hardware_encoder_falls_through_to_software() {
        // NVENC has no VP9 encoder on any machine.
        let order = [
            EncoderKind::Hardware(HardwareEncoder::Nvenc),
            EncoderKind::Software,
        ];
        let (kind, encoder) =
            open_best_encoder(Codec::Vp9, &config(), Some(&order)).expect("software");
        assert_eq!(kind, EncoderKind::Software);
        assert_eq!(encoder.codec(), Codec::Vp9);
    }

    #[cfg(all(feature = "libvpx", feature = "svt-av1"))]
    #[test]
    fn a_custom_order_with_software_first_returns_software() {
        let order = [
            EncoderKind::Software,
            EncoderKind::Hardware(HardwareEncoder::Vaapi),
        ];
        for codec in [Codec::Av1, Codec::Vp9] {
            let (kind, encoder) =
                open_best_encoder(codec, &config(), Some(&order)).expect("software");
            assert_eq!(kind, EncoderKind::Software);
            assert_eq!(encoder.codec(), codec);
        }
    }

    #[cfg(all(feature = "libvpx", feature = "svt-av1"))]
    #[test]
    fn the_default_order_always_opens_an_encoder() {
        for codec in [Codec::Av1, Codec::Vp9] {
            let (_, encoder) = open_best_encoder(codec, &config(), None).expect("an encoder");
            assert_eq!(encoder.codec(), codec);
        }
    }

    #[cfg(all(feature = "libvpx", feature = "dav1d"))]
    #[test]
    fn software_decoders_open_for_both_codecs() {
        for codec in [Codec::Av1, Codec::Vp9] {
            let decoder = open_software_decoder(codec).expect("decoder");
            assert_eq!(decoder.codec(), codec);
        }
    }

    #[cfg(not(feature = "libvpx"))]
    #[test]
    fn a_codec_that_is_switched_off_is_reported_as_not_built() {
        let order = [EncoderKind::Software];
        let result = open_best_encoder(Codec::Vp9, &config(), Some(&order));
        assert!(matches!(result, Err(Error::NotBuilt { .. })));
        assert!(matches!(
            open_software_decoder(Codec::Vp9),
            Err(Error::NotBuilt { .. })
        ));
    }
}
