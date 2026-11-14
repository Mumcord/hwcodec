//! The codec-independent interface: one encoder and one decoder trait, implemented by every
//! software and hardware codec of this crate, and the data they exchange.

use crate::{Error, I420Frame, Result};

/// Highest frame rate an encoder accepts.
pub(crate) const MAX_FPS: u32 = 120;

/// Largest picture an encoder accepts unless [`EncoderConfig::with_max_size`] says otherwise.
const DEFAULT_MAX_WIDTH: u32 = 7680;
/// See [`DEFAULT_MAX_WIDTH`].
const DEFAULT_MAX_HEIGHT: u32 = 4320;

/// The video formats this crate encodes and decodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Codec {
    /// VP9.
    Vp9,
    /// AV1.
    Av1,
}

/// What an encoder is opened with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EncoderConfig {
    /// Picture width in pixels.
    pub width: u32,
    /// Picture height in pixels.
    pub height: u32,
    /// Frames per second the rate control plans with.
    pub fps: u32,
    /// Total bitrate of all temporal layers in kbit/s.
    pub bitrate_kbps: u32,
    /// Number of temporal layers, or `None` for the encoder's own choice; see
    /// [`EncoderConfig::with_temporal_layers`].
    pub temporal_layers: Option<u8>,
    /// Number of CPU threads a software encoder uses, or `None` for its automatic choice; see
    /// [`EncoderConfig::with_threads`].
    pub threads: Option<u32>,
    /// Widest picture the encoder accepts; see [`EncoderConfig::with_max_size`].
    pub max_width: u32,
    /// Tallest picture the encoder accepts; see [`EncoderConfig::with_max_size`].
    pub max_height: u32,
}

impl EncoderConfig {
    /// A configuration for pictures of `width` x `height` pixels at `fps` frames per second and
    /// `bitrate_kbps` kbit/s.
    #[must_use]
    pub fn new(width: u32, height: u32, fps: u32, bitrate_kbps: u32) -> Self {
        Self {
            width,
            height,
            fps,
            bitrate_kbps,
            temporal_layers: None,
            threads: None,
            max_width: DEFAULT_MAX_WIDTH,
            max_height: DEFAULT_MAX_HEIGHT,
        }
    }

    /// Asks for exactly `layers` temporal layers (1 = no layering) instead of the encoder's own
    /// choice.
    ///
    /// Left alone, the software encoders use 3 layers and the hardware encoders 3 below 60 fps
    /// and 4 from 60 fps, or fewer when the GPU offers fewer. The software encoders accept 1–3
    /// and the hardware encoders 1–4; opening an encoder that cannot produce the requested
    /// number fails with [`Error::InvalidConfig`]. A requested number stays in place when the
    /// frame rate changes.
    #[must_use]
    pub fn with_temporal_layers(mut self, layers: u8) -> Self {
        self.temporal_layers = Some(layers);
        self
    }

    /// Makes a software encoder use exactly `threads` CPU threads instead of choosing by itself.
    ///
    /// Software encoders only: libvpx (VP9) accepts 1–64; SVT-AV1 manages its own threads and
    /// rejects a thread count with [`Error::InvalidConfig`]. Hardware encoders have no CPU
    /// encoding threads to limit and ignore the value.
    #[must_use]
    pub fn with_threads(mut self, threads: u32) -> Self {
        self.threads = Some(threads);
        self
    }

    /// Sets the largest picture the encoder accepts, when it is opened and when its picture size
    /// changes later (7680x4320 by default). A larger picture is rejected with
    /// [`Error::InvalidConfig`]. Limits of the encoder itself or of the GPU apply on top.
    #[must_use]
    pub fn with_max_size(mut self, max_width: u32, max_height: u32) -> Self {
        self.max_width = max_width;
        self.max_height = max_height;
        self
    }

    /// Checks the settings every encoder treats alike: the frame rate, the bitrate, the picture
    /// against the maximum picture size and the requested temporal layers against `max_layers`,
    /// the most this kind of encoder produces.
    // A build without any encoder (features off, or hardware only on another system) has no
    // caller.
    #[cfg_attr(
        not(any(
            feature = "svt-av1",
            feature = "libvpx",
            all(
                any(feature = "nvenc", feature = "amf", feature = "qsv"),
                any(target_os = "windows", target_os = "linux")
            ),
            all(feature = "vaapi", target_os = "linux")
        )),
        allow(dead_code)
    )]
    pub(crate) fn validate_common(&self, max_layers: u8) -> Result<()> {
        if self.width > self.max_width || self.height > self.max_height {
            return Err(Error::InvalidConfig(format!(
                "size {}x{} is larger than the maximum picture size {}x{}",
                self.width, self.height, self.max_width, self.max_height
            )));
        }
        if self.fps == 0 || self.fps > MAX_FPS {
            return Err(Error::InvalidConfig(format!(
                "fps {} (1 to {MAX_FPS} are accepted)",
                self.fps
            )));
        }
        if self.bitrate_kbps == 0 {
            return Err(Error::InvalidConfig("bitrate 0".into()));
        }
        match self.temporal_layers {
            Some(layers) if layers == 0 || layers > max_layers => Err(Error::InvalidConfig(
                format!("{layers} temporal layers (this encoder produces 1 to {max_layers})"),
            )),
            _ => Ok(()),
        }
    }
}

/// One compressed frame (VP9 or AV1), ready to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedFrame {
    /// The compressed picture.
    pub data: Vec<u8>,
    /// Whether the frame decodes without any earlier frame.
    pub keyframe: bool,
    /// Temporal layer 0..[`VideoEncoder::temporal_layers`]. A viewer that skips all frames above
    /// some layer still decodes the rest; keyframes are always layer 0.
    pub temporal_layer: u8,
    /// Milliseconds, as passed to [`VideoEncoder::encode`].
    pub pts: i64,
}

/// A realtime encoder with temporal layers. Every call is synchronous: a frame passed to
/// [`Self::encode`] comes back compressed from the same call.
pub trait VideoEncoder: Send {
    /// The format this encoder produces.
    fn codec(&self) -> Codec;

    /// The configuration the encoder currently runs with.
    fn config(&self) -> EncoderConfig;

    /// Number of temporal layers the current stream uses; layer ids run `0..temporal_layers()`.
    /// Dropping every frame above some layer leaves a stream that still decodes.
    fn temporal_layers(&self) -> u8;

    /// Encodes one frame of the configured size. `pts` is in milliseconds and must increase.
    /// With `force_keyframe` the frame becomes a keyframe (and thus layer 0).
    fn encode(
        &mut self,
        frame: &I420Frame,
        pts: i64,
        force_keyframe: bool,
    ) -> Result<Vec<EncodedFrame>>;

    /// Changes the output size; frames passed to [`Self::encode`] afterwards must have it.
    fn set_resolution(&mut self, width: u32, height: u32) -> Result<()>;

    /// Changes the total bitrate of all layers.
    fn set_bitrate(&mut self, bitrate_kbps: u32) -> Result<()>;

    /// Changes the frame rate the rate control plans with.
    fn set_fps(&mut self, fps: u32) -> Result<()>;
}

/// Decodes one compressed frame at a time into I420.
pub trait VideoDecoder: Send {
    /// The format this decoder reads.
    fn codec(&self) -> Codec;

    /// Decodes one compressed frame. Returns `None` when the decoder has no picture to show
    /// for it.
    fn decode(&mut self, data: &[u8]) -> Result<Option<I420Frame>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> EncoderConfig {
        EncoderConfig::new(1280, 720, 30, 1500)
    }

    #[test]
    fn defaults_leave_every_setting_to_the_encoder() {
        let config = config();
        assert_eq!(config.temporal_layers, None);
        assert_eq!(config.threads, None);
        assert_eq!((config.max_width, config.max_height), (7680, 4320));
        config.validate_common(3).expect("valid");
    }

    #[test]
    fn builders_set_the_settings() {
        let config = config()
            .with_temporal_layers(1)
            .with_threads(2)
            .with_max_size(1920, 1080);
        assert_eq!(config.temporal_layers, Some(1));
        assert_eq!(config.threads, Some(2));
        assert_eq!((config.max_width, config.max_height), (1920, 1080));
        config.validate_common(3).expect("valid");
    }

    #[test]
    fn accepts_frame_rates_from_1_to_120() {
        for fps in [1, 60, 61, 120] {
            EncoderConfig { fps, ..config() }
                .validate_common(3)
                .expect("valid");
        }
        for fps in [0, 121, 240] {
            let result = EncoderConfig { fps, ..config() }.validate_common(3);
            assert!(matches!(result, Err(Error::InvalidConfig(_))), "fps {fps}");
        }
    }

    #[test]
    fn rejects_a_picture_above_the_maximum_size() {
        for (width, height) in [(7681, 720), (1280, 4321)] {
            let result = EncoderConfig::new(width, height, 30, 1500).validate_common(3);
            assert!(matches!(result, Err(Error::InvalidConfig(_))));
        }
        EncoderConfig::new(7680, 4320, 30, 1500)
            .validate_common(3)
            .expect("the default maximum itself is valid");
        let small = config().with_max_size(1279, 720).validate_common(3);
        assert!(matches!(small, Err(Error::InvalidConfig(_))));
    }

    #[test]
    fn rejects_layer_counts_the_encoder_does_not_produce() {
        for layers in [1, 2, 3] {
            config()
                .with_temporal_layers(layers)
                .validate_common(3)
                .expect("valid");
        }
        for layers in [0, 4] {
            let result = config().with_temporal_layers(layers).validate_common(3);
            assert!(matches!(result, Err(Error::InvalidConfig(_))));
        }
    }
}
