//! Plain values of the software codecs that a caller needs whether or not the codec itself is
//! part of the build: a build without `libvpx` or `svt-av1` (a phone, say) still plans layers,
//! threads and options for the streams it receives or for peers that do encode.

/// Number of VP9 temporal layers (L1T3) unless the configuration asks for fewer; also the most
/// the encoder produces. Layer ids run 0..TEMPORAL_LAYERS.
pub const TEMPORAL_LAYERS: u8 = 3;

/// Temporal layers (pattern 0-2-1-2). SVT-AV1's low-delay CBR mode allows at most three
/// (hierarchical levels 0–2), so unlike VP9 and the hardware encoders it has no fourth layer at
/// 60 fps; software AV1 is rarely fast enough for 60 fps anyway.
pub const AV1_TEMPORAL_LAYERS: u8 = 3;

/// Software AV1 options that only exist for this encoder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Av1Options {
    /// AV1's screen-content tools (palette, intra block copy), which keep text sharp.
    pub screen_content: bool,
}

impl Av1Options {
    /// Options with the screen-content tools switched on or off.
    #[must_use]
    pub fn new(screen_content: bool) -> Self {
        Self { screen_content }
    }
}

/// The libvpx thread count a software VP9 encoder is configured with unless the configuration
/// names one.
pub fn vp9_threads() -> u32 {
    // More than four threads gains little at 720p (tile columns cap the parallelism) and takes
    // cores from the game or app being shared.
    std::thread::available_parallelism()
        .map_or(1, |n| n.get().min(4))
        .try_into()
        .unwrap_or(1)
}

/// SVT-AV1 4.2's SSSE3 code, which it picks on x86 CPUs without AVX2, reads a reference block
/// with an aligned load that is not always aligned (`svt_aom_sub_pixel_variance16xh_ssse3`), so
/// the encoder kills the whole process with a general protection fault. Such CPUs are also far too
/// slow for real-time AV1, so callers should use VP9 there instead. Callers check this before
// The last sentence links to the encoder only in builds that have it.
#[cfg_attr(
    feature = "svt-av1",
    doc = "opening an encoder; [`Av1Encoder::new`](crate::Av1Encoder::new) refuses such CPUs as a safety net."
)]
#[cfg_attr(
    not(feature = "svt-av1"),
    doc = "opening an encoder; `Av1Encoder::new` (feature `svt-av1`) refuses such CPUs as a safety net."
)]
pub fn av1_cpu_supported() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plain_values_need_no_codec() {
        assert_eq!(TEMPORAL_LAYERS, 3);
        assert_eq!(AV1_TEMPORAL_LAYERS, 3);
        assert_eq!(Av1Options::default(), Av1Options::new(false));
        assert!(Av1Options::new(true).screen_content);
        assert!((1..=4).contains(&vp9_threads()));
        // Only x86 CPUs without AVX2 are refused.
        if cfg!(not(target_arch = "x86_64")) {
            assert!(av1_cpu_supported());
        }
    }
}
