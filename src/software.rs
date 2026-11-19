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

/// Whether this CPU can run the software AV1 encoder. Always `true`.
///
/// It used to report `false` on x86 CPUs without AVX2: SVT-AV1 4.2's SSSE3 code, which such CPUs
/// pick, read a reference block with an aligned load that is not always aligned
/// (`svt_aom_sub_pixel_variance16xh_ssse3`), and the encoder killed the whole process with a
/// general protection fault. The bundled SVT-AV1 4.3.0 fixes that load, so no CPU is refused any
/// more. Callers that check this before opening an encoder can drop the check; the function goes
/// away with the next breaking release.
pub fn av1_cpu_supported() -> bool {
    true
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
        assert!(av1_cpu_supported());
    }
}
