//! Speed self-test that tells whether this machine is fast enough for software AV1 encoding.
//!
//! SVT-AV1 encodes about one second of screen-like frames at the given size and rate; the result
//! is the share of the per-frame time budget (`1 s / fps`) an average frame took.

#![cfg(feature = "svt-av1")]

use std::time::{Duration, Instant};

use crate::detect::test_frame;
use crate::{Av1Encoder, Av1Options, EncoderConfig, Error, I420Frame, Result};

/// Software AV1 passes when an average frame takes less than this share (in percent) of its
/// time budget.
pub const DEFAULT_PASS_PERCENT: u32 = 50;

/// One test never takes longer than this; a slower machine fails it anyway.
const TIME_LIMIT: Duration = Duration::from_secs(2);

/// What [`measure_av1`] measured.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct SelftestResult {
    /// The share of the per-frame time budget an average frame took, in percent (100 = exactly
    /// real time).
    pub budget_percent: f32,
    /// The measured average encode time of one frame.
    pub average_frame_time: Duration,
    /// Whether that is below [`DEFAULT_PASS_PERCENT`].
    pub passed: bool,
}

impl SelftestResult {
    /// Whether the measurement is below `pass_percent`, for a threshold other than the default.
    #[must_use]
    pub fn passes_at(&self, pass_percent: u32) -> bool {
        f64::from(self.budget_percent) < f64::from(pass_percent)
    }
}

/// Measures whether software AV1 encodes `width` x `height` pictures at `fps` frames per second
/// in real time on this CPU. Fails when the encoder does not start or a frame fails.
pub fn measure_av1(
    width: u32,
    height: u32,
    fps: u32,
    options: Av1Options,
) -> Result<SelftestResult> {
    if fps == 0 {
        return Err(Error::InvalidConfig("0 frames per second".into()));
    }
    let bitrate_kbps = if height >= 1080 { 4000 } else { 2000 };
    let config = EncoderConfig::new(width, height, fps, bitrate_kbps);
    let mut encoder = Av1Encoder::new(config, options)?;
    let mut frame = test_frame(width, height, 0).into_data();
    let y_len = width as usize * height as usize;
    let started = Instant::now();
    let mut timed = Duration::ZERO;
    let mut frames = 0u32;
    // The first frame is a keyframe and also warms the encoder up; it is not timed.
    for n in 0..=fps {
        // Scroll the picture a little, like text in a moving window.
        frame[..y_len].rotate_left(width as usize * 4);
        let picture = I420Frame::new(width, height, frame)?;
        let began = Instant::now();
        let result = encoder.encode(&picture, i64::from(n * 1000 / fps), n == 0);
        let took = began.elapsed();
        frame = picture.into_data();
        result?;
        if n > 0 {
            timed += took;
            frames += 1;
        }
        if started.elapsed() > TIME_LIMIT {
            break;
        }
    }
    let average_frame_time = timed / frames.max(1);
    let budget_percent = (average_frame_time.as_secs_f64() * f64::from(fps) * 100.0) as f32;
    let result = SelftestResult {
        budget_percent,
        average_frame_time,
        passed: false,
    };
    Ok(SelftestResult {
        passed: result.passes_at(DEFAULT_PASS_PERCENT),
        ..result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_below_the_threshold_only() {
        let result = SelftestResult {
            budget_percent: 49.9,
            average_frame_time: Duration::from_millis(16),
            passed: true,
        };
        assert!(result.passes_at(DEFAULT_PASS_PERCENT));
        assert!(!result.passes_at(49));
        assert!(result.passes_at(80));
    }

    #[test]
    fn measures_a_small_size_within_the_time_limit() {
        let started = Instant::now();
        let result = measure_av1(854, 480, 15, Av1Options::default()).expect("SVT-AV1 runs");
        assert!(result.budget_percent > 0.0);
        assert!(result.average_frame_time > Duration::ZERO);
        let budget = result.average_frame_time.as_secs_f64() * 15.0 * 100.0;
        assert!((f64::from(result.budget_percent) - budget).abs() < 0.01);
        assert_eq!(result.passed, result.passes_at(DEFAULT_PASS_PERCENT));
        assert!(started.elapsed() < TIME_LIMIT + Duration::from_secs(2));
    }

    #[test]
    fn a_rate_of_zero_is_refused() {
        assert!(matches!(
            measure_av1(854, 480, 0, Av1Options::default()),
            Err(Error::InvalidConfig(_))
        ));
    }
}
