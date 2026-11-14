//! `open_best_encoder` with the default order and with a custom order. Written to hold on a
//! machine without a GPU (software opens) and on one with a working GPU (its encoder opens).

#[allow(dead_code)] // the other integration tests use the rest of the shared helpers
mod common;

use common::pattern;
use hwcodec::{
    Codec, EncoderConfig, EncoderKind, HardwareEncoder, VideoEncoder, default_order,
    detect_encoder, detect_encoders, open_best_encoder, open_software_decoder,
};

const CODECS: [Codec; 2] = [Codec::Av1, Codec::Vp9];

// The size and rate detection tests with, so "works" there means "opens" here.
fn config() -> EncoderConfig {
    EncoderConfig::new(1280, 720, 30, 2000)
}

/// The opened encoder must really produce a stream the software decoder plays.
fn assert_encodes(codec: Codec, encoder: &mut dyn VideoEncoder) {
    let EncoderConfig { width, height, .. } = encoder.config();
    let mut decoder = open_software_decoder(codec).expect("decoder");
    let mut pictures = 0;
    for index in 0..5 {
        let frame = pattern(width, height, index);
        let packets = encoder
            .encode(&frame, i64::from(index * 33), index == 0)
            .expect("encode");
        for packet in packets {
            if decoder.decode(&packet.data).expect("decode").is_some() {
                pictures += 1;
            }
        }
    }
    assert_eq!(pictures, 5, "{codec:?}");
}

#[test]
fn a_custom_order_with_software_first_opens_software() {
    let order = [
        EncoderKind::Software,
        EncoderKind::Hardware(HardwareEncoder::Vaapi),
        EncoderKind::Hardware(HardwareEncoder::Nvenc),
        EncoderKind::Hardware(HardwareEncoder::Amf),
        EncoderKind::Hardware(HardwareEncoder::Qsv),
    ];
    for codec in CODECS {
        let (kind, mut encoder) =
            open_best_encoder(codec, &config(), Some(&order)).expect("encoder");
        assert_eq!(kind, EncoderKind::Software, "{codec:?}");
        assert_eq!(encoder.codec(), codec);
        assert_encodes(codec, encoder.as_mut());
    }
}

#[test]
fn the_default_order_opens_the_first_encoder_that_works() {
    for codec in CODECS {
        let expected = default_order(codec)
            .into_iter()
            .find(|kind| match kind {
                EncoderKind::Hardware(encoder) => detect_encoder(codec, *encoder).works(),
                _ => true,
            })
            .expect("the default order ends with software");
        // Without a working hardware encoder (a machine without a GPU, CI) that is software.
        if !detect_encoders(codec).iter().any(|report| report.works()) {
            assert_eq!(expected, EncoderKind::Software, "{codec:?}");
        }

        let (kind, mut encoder) = open_best_encoder(codec, &config(), None).expect("encoder");
        assert_eq!(kind, expected, "{codec:?}");
        assert_eq!(encoder.codec(), codec);
        assert_encodes(codec, encoder.as_mut());
    }
}

#[test]
fn an_encoder_that_does_not_open_is_skipped() {
    // NVENC has no VP9 encoder on any machine, so software is reached through the fallback.
    let order = [
        EncoderKind::Hardware(HardwareEncoder::Nvenc),
        EncoderKind::Software,
    ];
    let (kind, _encoder) = open_best_encoder(Codec::Vp9, &config(), Some(&order)).expect("encoder");
    assert_eq!(kind, EncoderKind::Software);
}
