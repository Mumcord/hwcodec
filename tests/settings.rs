//! Encoder settings a caller may choose: a single layer at 120 frames per second, SVT-AV1 with
//! one and two layers, and a clear error for every value outside the allowed range. Software
//! only, so it runs without a GPU.

#[allow(dead_code)] // the other integration tests use the rest of the shared helpers
mod common;

use common::{pattern, psnr};
use hwcodec::{
    Av1Encoder, Av1Options, Codec, EncoderConfig, EncoderKind, Error, Result, VideoEncoder,
    Vp9Encoder, open_best_encoder, open_software_decoder,
};

const CODECS: [Codec; 2] = [Codec::Av1, Codec::Vp9];
const SOFTWARE: &[EncoderKind] = &[EncoderKind::Software];

// The bitrate grows with the frame rate so that every picture gets the same share of it.
fn config(fps: u32) -> EncoderConfig {
    EncoderConfig::new(640, 360, fps, 50 * fps.max(1))
}

fn open_software(codec: Codec, config: EncoderConfig) -> Result<Box<dyn VideoEncoder>> {
    open_best_encoder(codec, &config, Some(SOFTWARE)).map(|(_, encoder)| encoder)
}

/// Encodes `frames` pictures, decodes every one and returns the temporal layer of each.
fn round_trip(codec: Codec, encoder: &mut dyn VideoEncoder, frames: u32) -> Vec<u8> {
    let EncoderConfig {
        width, height, fps, ..
    } = encoder.config();
    let mut decoder = open_software_decoder(codec).expect("decoder");
    let mut layers = Vec::new();
    let mut last_psnr = 0.0;
    for index in 0..frames {
        let frame = pattern(width, height, index);
        let packets = encoder
            .encode(&frame, i64::from(index * 1000 / fps), false)
            .expect("encode");
        assert_eq!(packets.len(), 1, "{codec:?}: one packet per frame");
        let packet = &packets[0];
        assert_eq!(packet.keyframe, index == 0, "{codec:?} frame {index}");
        let decoded = decoder
            .decode(&packet.data)
            .expect("decode")
            .expect("picture");
        assert_eq!((decoded.width(), decoded.height()), (width, height));
        last_psnr = psnr(frame.y(), decoded.y());
        layers.push(packet.temporal_layer);
    }
    assert!(last_psnr > 30.0, "{codec:?}: luma PSNR {last_psnr:.1} dB");
    layers
}

#[test]
fn a_single_layer_at_120_fps_encodes_and_decodes() {
    for codec in CODECS {
        let mut encoder =
            open_software(codec, config(120).with_temporal_layers(1)).expect("encoder");
        assert_eq!(encoder.config().fps, 120);
        assert_eq!(encoder.temporal_layers(), 1, "{codec:?}");
        let layers = round_trip(codec, encoder.as_mut(), 48);
        assert!(
            layers.iter().all(|&layer| layer == 0),
            "{codec:?} {layers:?}"
        );
    }
}

#[test]
fn svt_av1_round_trips_with_one_and_two_layers() {
    for count in [1u8, 2] {
        let mut encoder = Av1Encoder::new(
            config(30).with_temporal_layers(count),
            Av1Options::default(),
        )
        .expect("encoder");
        assert_eq!(encoder.temporal_layers(), count);
        let layers = round_trip(Codec::Av1, &mut encoder, 32);
        assert_eq!(layers.iter().max(), Some(&(count - 1)), "{layers:?}");
    }
}

/// The error text of a rejected setting; fails when the setting was accepted or the error is
/// not the one for invalid settings.
fn rejection<T>(result: Result<T>, what: &str) -> String {
    match result {
        Err(Error::InvalidConfig(message)) => {
            assert!(!message.is_empty(), "{what}: empty error text");
            message
        }
        Err(other) => panic!("{what}: wrong kind of error: {other}"),
        Ok(_) => panic!("{what}: accepted"),
    }
}

#[test]
fn a_frame_rate_outside_1_to_120_is_rejected() {
    for codec in CODECS {
        for fps in [0, 121, 240] {
            let what = format!("{codec:?} at {fps} fps");
            let message = rejection(open_software(codec, config(fps)), &what);
            assert!(message.contains(&fps.to_string()), "{what}: {message}");
        }
        let mut encoder = open_software(codec, config(60)).expect("encoder");
        for fps in [0, 121] {
            rejection(encoder.set_fps(fps), &format!("{codec:?} set_fps({fps})"));
        }
        assert_eq!(encoder.config().fps, 60, "{codec:?} keeps its frame rate");
        encoder.set_fps(120).expect("120 fps is allowed");
    }
}

#[test]
fn a_layer_count_outside_1_to_3_is_rejected() {
    for codec in CODECS {
        for layers in [0, 4] {
            let what = format!("{codec:?} with {layers} layers");
            let message = rejection(
                open_software(codec, config(30).with_temporal_layers(layers)),
                &what,
            );
            assert!(message.contains("layers"), "{what}: {message}");
        }
    }
}

#[test]
fn a_thread_count_the_encoder_cannot_honour_is_rejected() {
    for threads in [0, 65] {
        let what = format!("VP9 with {threads} threads");
        let message = rejection(Vp9Encoder::new(config(30).with_threads(threads)), &what);
        assert!(message.contains("threads"), "{what}: {message}");
    }
    Vp9Encoder::new(config(30).with_threads(2)).expect("2 threads are allowed");
    // SVT-AV1 chooses its threads itself, so it accepts no explicit number.
    let message = rejection(
        Av1Encoder::new(config(30).with_threads(2), Av1Options::default()),
        "AV1 with 2 threads",
    );
    assert!(message.contains("threads"), "{message}");
}

#[test]
fn a_picture_above_the_maximum_size_is_rejected() {
    for codec in CODECS {
        let small_limit = config(30).with_max_size(320, 240);
        let what = format!("{codec:?} 640x360 with a 320x240 limit");
        let message = rejection(open_software(codec, small_limit), &what);
        assert!(message.contains("320x240"), "{what}: {message}");

        let mut encoder =
            open_software(codec, config(30).with_max_size(640, 360)).expect("encoder");
        rejection(
            encoder.set_resolution(1280, 720),
            &format!("{codec:?} set_resolution above the limit"),
        );
        let EncoderConfig { width, height, .. } = encoder.config();
        assert_eq!((width, height), (640, 360), "{codec:?} keeps its size");
    }
}
