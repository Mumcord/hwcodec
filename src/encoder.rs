use std::mem::MaybeUninit;
use std::os::raw::{c_int, c_ulong};
use std::ptr;

use crate::codec::{Codec, VideoEncoder};
use crate::ffi::{self, check};
use crate::{EncodedFrame, EncoderConfig, Error, I420Frame, Result, TEMPORAL_LAYERS, vp9_threads};

/// Most threads libvpx runs an encoder with.
const MAX_THREADS: u32 = 64;

/// How libvpx spreads the frames over a number of temporal layers.
struct Layering {
    mode: c_int,
    /// Layer of each frame of one period.
    pattern: &'static [u32],
    /// Per layer, every how many frames one of that layer or below comes.
    decimator: &'static [u32],
    /// Cumulative share of the bitrate each layer may use, in percent (libvpx's suggested
    /// split).
    bitrate_percent: &'static [u32],
}

fn layering(layers: u8) -> Layering {
    match layers {
        1 => Layering {
            mode: ffi::VP9E_TEMPORAL_LAYERING_MODE_NOLAYERING as c_int,
            pattern: &[0],
            decimator: &[1],
            bitrate_percent: &[100],
        },
        2 => Layering {
            mode: ffi::VP9E_TEMPORAL_LAYERING_MODE_0101 as c_int,
            pattern: &[0, 1],
            decimator: &[2, 1],
            bitrate_percent: &[60, 100],
        },
        // Layer 0 every 4th frame, layer 1 in between, layer 2 on every odd frame.
        _ => Layering {
            mode: ffi::VP9E_TEMPORAL_LAYERING_MODE_0212 as c_int,
            pattern: &[0, 2, 1, 2],
            decimator: &[4, 2, 1],
            bitrate_percent: &[40, 60, 100],
        },
    }
}

/// Presentation times are milliseconds.
const TIMEBASE_DEN: c_int = 1000;

/// Fastest realtime preset that still keeps text readable; tuned with the `encode_speed` example.
const CPU_USED: c_int = 7;

/// Realtime VP9 encoder with L1T3 temporal layers (or the number the configuration asks for).
pub struct Vp9Encoder {
    // Boxed: libvpx keeps pointers into the context and configuration between calls.
    ctx: Box<ffi::vpx_codec_ctx_t>,
    cfg: Box<ffi::vpx_codec_enc_cfg_t>,
    /// What the encoder was opened with; size, frame rate and bitrate are kept up to date.
    config: EncoderConfig,
    layers: u8,
    /// The size libvpx was opened at; a running encoder must never grow past it.
    opened: (u32, u32),
}

// SAFETY: the libvpx context is owned exclusively by this value and only used through `&mut self`;
// libvpx has no thread affinity, so moving it to another thread is fine.
unsafe impl Send for Vp9Encoder {}

impl Vp9Encoder {
    /// An encoder tuned for screen content (text, sharp edges).
    pub fn new(config: EncoderConfig) -> Result<Self> {
        Self::open(config, true)
    }

    /// An encoder tuned for natural video such as a camera picture: the
    /// screen-content tuning would waste bits on camera noise.
    pub fn natural(config: EncoderConfig) -> Result<Self> {
        Self::open(config, false)
    }

    fn open(config: EncoderConfig, screen_content: bool) -> Result<Self> {
        validate(&config)?;
        let layers = config.temporal_layers.unwrap_or(TEMPORAL_LAYERS);
        let layering = layering(layers);
        // SAFETY: plain libvpx calls on locally owned, correctly sized structs; every status is
        // checked before the context is used further.
        unsafe {
            let iface = ffi::vpx_codec_vp9_cx();
            let mut cfg = Box::new(MaybeUninit::<ffi::vpx_codec_enc_cfg_t>::zeroed().assume_init());
            let status = ffi::vpx_codec_enc_config_default(iface, &mut *cfg, 0);
            check(ptr::null_mut(), status, "default encoder config")?;

            cfg.g_w = config.width;
            cfg.g_h = config.height;
            cfg.g_timebase = ffi::vpx_rational {
                num: 1,
                den: TIMEBASE_DEN,
            };
            cfg.g_threads = config.threads.unwrap_or_else(vp9_threads);
            cfg.g_lag_in_frames = 0;
            // Without it, dropped upper-layer frames would still have updated the entropy context
            // the next base-layer frame decodes against.
            cfg.g_error_resilient = ffi::VPX_ERROR_RESILIENT_DEFAULT;
            cfg.g_pass = ffi::VPX_RC_ONE_PASS;
            cfg.rc_end_usage = ffi::VPX_CBR;
            cfg.rc_min_quantizer = 2;
            // The full range: no frame is ever dropped, so on busy content (scrolling text) a
            // lower cap left rate control no room and the stream ran at about twice the target.
            cfg.rc_max_quantizer = 63;
            cfg.rc_undershoot_pct = 50;
            cfg.rc_overshoot_pct = 50;
            cfg.rc_buf_initial_sz = 500;
            cfg.rc_buf_optimal_sz = 600;
            // Little headroom above the optimal level: after a step down (720p15 → 480p15) the
            // resize keyframe comes out tiny and a 1000 ms buffer let the next frames spend the
            // saved bits at once, about 30% over the new target for a second, more than a
            // throttled viewer's upload has room for.
            cfg.rc_buf_sz = 700;
            // Keep every frame: the sender decides what to drop via the temporal layers.
            cfg.rc_dropframe_thresh = 0;
            // Keyframes only on request (new viewer, packet loss) or a size change.
            cfg.kf_mode = ffi::VPX_KF_DISABLED;

            cfg.ss_number_layers = 1;
            cfg.ts_number_layers = u32::from(layers);
            cfg.ts_periodicity = layering.pattern.len() as u32;
            cfg.ts_layer_id[..layering.pattern.len()].copy_from_slice(layering.pattern);
            cfg.ts_rate_decimator[..layering.decimator.len()].copy_from_slice(layering.decimator);
            cfg.temporal_layering_mode = layering.mode;
            set_layer_bitrates(&mut cfg, config.bitrate_kbps, layers);

            let mut ctx = Box::new(MaybeUninit::<ffi::vpx_codec_ctx_t>::zeroed().assume_init());
            let status = ffi::vpx_codec_enc_init_ver(
                &mut *ctx,
                iface,
                &*cfg,
                0,
                ffi::VPX_ENCODER_ABI_VERSION as c_int,
            );
            check(&mut *ctx, status, "encoder init")?;
            let mut encoder = Self {
                ctx,
                cfg,
                config,
                layers,
                opened: (config.width, config.height),
            };
            encoder.configure_controls(screen_content)?;
            Ok(encoder)
        }
    }

    /// The configuration the encoder currently runs with.
    pub fn config(&self) -> EncoderConfig {
        self.config
    }

    /// Encodes one frame. `pts` is in milliseconds and must increase. With `force_keyframe` the
    /// frame becomes a keyframe (and thus layer 0).
    pub fn encode(
        &mut self,
        frame: &I420Frame,
        pts: i64,
        force_keyframe: bool,
    ) -> Result<Vec<EncodedFrame>> {
        if frame.width() != self.cfg.g_w || frame.height() != self.cfg.g_h {
            return Err(Error::InvalidFrame(format!(
                "frame is {}x{}, encoder is set to {}x{}",
                frame.width(),
                frame.height(),
                self.cfg.g_w,
                self.cfg.g_h
            )));
        }
        let flags = if force_keyframe {
            ffi::VPX_EFLAG_FORCE_KF as ffi::vpx_enc_frame_flags_t
        } else {
            0
        };
        let duration = c_ulong::from(TIMEBASE_DEN as u32 / self.config.fps);
        // SAFETY: the image only borrows `frame`'s buffer for the duration of the encode call;
        // libvpx reads but never writes input planes. Packet buffers are copied out before the
        // next libvpx call invalidates them.
        unsafe {
            let img = wrap_image(frame);
            let status = ffi::vpx_codec_encode(
                &mut *self.ctx,
                &img,
                pts,
                duration,
                flags,
                c_ulong::from(ffi::VPX_DL_REALTIME),
            );
            check(&mut *self.ctx, status, "encode")?;

            let mut layer = MaybeUninit::<ffi::vpx_svc_layer_id_t>::zeroed().assume_init();
            let status = ffi::vpx_codec_control_(
                &mut *self.ctx,
                ffi::VP9E_GET_SVC_LAYER_ID as c_int,
                &mut layer as *mut ffi::vpx_svc_layer_id_t,
            );
            check(&mut *self.ctx, status, "read temporal layer id")?;
            let temporal_layer = u8::try_from(layer.temporal_layer_id)
                .ok()
                .filter(|id| *id < self.layers)
                .ok_or_else(|| Error::Codec {
                    library: "libvpx",
                    operation: "read temporal layer id",
                    message: format!("unexpected layer id {}", layer.temporal_layer_id),
                })?;

            let mut frames = Vec::new();
            let mut iter: ffi::vpx_codec_iter_t = ptr::null();
            loop {
                let pkt = ffi::vpx_codec_get_cx_data(&mut *self.ctx, &mut iter);
                if pkt.is_null() {
                    break;
                }
                if (*pkt).kind != ffi::VPX_CODEC_CX_FRAME_PKT {
                    continue;
                }
                let out = &(*pkt).data.frame;
                frames.push(EncodedFrame {
                    data: std::slice::from_raw_parts(out.buf as *const u8, out.sz).to_vec(),
                    keyframe: out.flags & ffi::VPX_FRAME_IS_KEY != 0,
                    temporal_layer,
                    pts: out.pts,
                });
            }
            Ok(frames)
        }
    }

    /// Changes the output size (the 720p ↔ 480p quality steps, a shared window that is resized).
    /// Frames passed to [`Self::encode`] afterwards must have the new size. Shrinking, and
    /// growing back up to the opened size, reuses the scaled previous frames.
    pub fn set_resolution(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(Error::InvalidConfig(format!("size {width}x{height}")));
        }
        let config = EncoderConfig {
            width,
            height,
            ..self.config
        };
        validate(&config)?;
        if width > self.opened.0 || height > self.opened.1 {
            // libvpx corrupts its heap when a running VP9 encoder grows past the size it was
            // opened at, so a new one is opened. Its first frame is a keyframe, which the size
            // change needs anyway; bitrate and frame rate carry over.
            *self = Self::new(config)?;
            return Ok(());
        }
        self.cfg.g_w = width;
        self.cfg.g_h = height;
        self.apply_config("set resolution")?;
        self.config = config;
        Ok(())
    }

    /// Changes the total bitrate, split across the temporal layers as at creation.
    pub fn set_bitrate(&mut self, bitrate_kbps: u32) -> Result<()> {
        if bitrate_kbps == 0 {
            return Err(Error::InvalidConfig("bitrate 0".into()));
        }
        set_layer_bitrates(&mut self.cfg, bitrate_kbps, self.layers);
        self.apply_config("set bitrate")?;
        self.config.bitrate_kbps = bitrate_kbps;
        Ok(())
    }

    /// Changes the frame rate; the frame duration libvpx's rate control plans with follows it.
    pub fn set_fps(&mut self, fps: u32) -> Result<()> {
        let config = EncoderConfig { fps, ..self.config };
        validate(&config)?;
        self.config = config;
        Ok(())
    }

    fn apply_config(&mut self, operation: &'static str) -> Result<()> {
        // SAFETY: the context is initialised and the configuration is a valid, owned struct.
        let status = unsafe { ffi::vpx_codec_enc_config_set(&mut *self.ctx, &*self.cfg) };
        check(&mut *self.ctx, status, operation)
    }

    fn configure_controls(&mut self, screen_content: bool) -> Result<()> {
        let multithreaded = c_int::from(self.cfg.g_threads > 1);
        let controls: [(ffi::vp8e_enc_control_id, c_int, &'static str); 9] = [
            (ffi::VP8E_SET_CPUUSED, CPU_USED, "set speed"),
            (
                ffi::VP9E_SET_TUNE_CONTENT,
                if screen_content {
                    ffi::VP9E_CONTENT_SCREEN as c_int
                } else {
                    ffi::VP9E_CONTENT_DEFAULT as c_int
                },
                "tune for the content",
            ),
            // Cyclic refresh: spreads quality updates over frames instead of keyframe spikes.
            (ffi::VP9E_SET_AQ_MODE, 3, "set aq mode"),
            (ffi::VP8E_SET_STATIC_THRESHOLD, 1, "set static threshold"),
            (ffi::VP9E_SET_TILE_COLUMNS, 2, "set tile columns"),
            (
                ffi::VP9E_SET_ROW_MT,
                multithreaded,
                "set row multithreading",
            ),
            (
                ffi::VP9E_SET_FRAME_PARALLEL_DECODING,
                0,
                "set frame parallel",
            ),
            (ffi::VP9E_SET_NOISE_SENSITIVITY, 0, "disable denoiser"),
            (ffi::VP9E_SET_SVC, 1, "enable temporal layers"),
        ];
        // A single layer is a plain stream: in libvpx's layered mode without layering, rate
        // control ignores the bitrate. So the layer controls (the last one here and the
        // parameters below) are left out.
        let layered = self.layers > 1;
        let count = controls.len() - usize::from(!layered);
        for (id, value, operation) in &controls[..count] {
            // SAFETY: each of these controls takes a single int argument.
            let status = unsafe { ffi::vpx_codec_control_(&mut *self.ctx, *id as c_int, *value) };
            check(&mut *self.ctx, status, operation)?;
        }
        if !layered {
            return Ok(());
        }

        // SAFETY: zeroed is a valid value for this plain C struct; libvpx copies it.
        let status = unsafe {
            let mut svc = MaybeUninit::<ffi::vpx_svc_extra_cfg_t>::zeroed().assume_init();
            for layer in 0..usize::from(self.layers) {
                svc.max_quantizers[layer] = self.cfg.rc_max_quantizer as c_int;
                svc.min_quantizers[layer] = self.cfg.rc_min_quantizer as c_int;
                svc.scaling_factor_num[layer] = 1;
                svc.scaling_factor_den[layer] = 1;
            }
            svc.temporal_layering_mode = self.cfg.temporal_layering_mode;
            ffi::vpx_codec_control_(
                &mut *self.ctx,
                ffi::VP9E_SET_SVC_PARAMETERS as c_int,
                &mut svc as *mut ffi::vpx_svc_extra_cfg_t,
            )
        };
        check(&mut *self.ctx, status, "set layer parameters")
    }
}

impl VideoEncoder for Vp9Encoder {
    fn codec(&self) -> Codec {
        Codec::Vp9
    }

    fn config(&self) -> EncoderConfig {
        Vp9Encoder::config(self)
    }

    fn temporal_layers(&self) -> u8 {
        self.layers
    }

    fn encode(
        &mut self,
        frame: &I420Frame,
        pts: i64,
        force_keyframe: bool,
    ) -> Result<Vec<EncodedFrame>> {
        Vp9Encoder::encode(self, frame, pts, force_keyframe)
    }

    fn set_resolution(&mut self, width: u32, height: u32) -> Result<()> {
        Vp9Encoder::set_resolution(self, width, height)
    }

    fn set_bitrate(&mut self, bitrate_kbps: u32) -> Result<()> {
        Vp9Encoder::set_bitrate(self, bitrate_kbps)
    }

    fn set_fps(&mut self, fps: u32) -> Result<()> {
        Vp9Encoder::set_fps(self, fps)
    }
}

impl Drop for Vp9Encoder {
    fn drop(&mut self) {
        // SAFETY: the context was initialised in `new` and is destroyed exactly once.
        unsafe {
            ffi::vpx_codec_destroy(&mut *self.ctx);
        }
    }
}

fn validate(config: &EncoderConfig) -> Result<()> {
    if config.width == 0 || config.height == 0 {
        return Err(Error::InvalidConfig(format!(
            "size {}x{}",
            config.width, config.height
        )));
    }
    config.validate_common(TEMPORAL_LAYERS)?;
    match config.threads {
        Some(threads) if threads == 0 || threads > MAX_THREADS => Err(Error::InvalidConfig(
            format!("{threads} threads (libvpx runs with 1 to {MAX_THREADS})"),
        )),
        _ => Ok(()),
    }
}

fn set_layer_bitrates(cfg: &mut ffi::vpx_codec_enc_cfg_t, bitrate_kbps: u32, layers: u8) {
    cfg.rc_target_bitrate = bitrate_kbps;
    for (layer, percent) in layering(layers).bitrate_percent.iter().enumerate() {
        let kbps = bitrate_kbps.saturating_mul(*percent) / 100;
        cfg.ts_target_bitrate[layer] = kbps;
        // VP9 reads the per-layer targets from here; `ts_target_bitrate` is the VP8 field.
        cfg.layer_target_bitrate[layer] = kbps;
    }
}

/// Describes `frame`'s packed planes as a libvpx image without copying.
///
/// # Safety
/// The returned image points into `frame` and must not outlive it.
unsafe fn wrap_image(frame: &I420Frame) -> ffi::vpx_image_t {
    let (w, h) = (frame.width(), frame.height());
    let chroma_w = w.div_ceil(2);
    // SAFETY: vpx_img_wrap only fills in the struct; the planes are overwritten below with the
    // packed layout because libvpx would otherwise round odd strides up.
    unsafe {
        let mut img = MaybeUninit::<ffi::vpx_image_t>::zeroed().assume_init();
        let base = frame.data().as_ptr() as *mut u8;
        ffi::vpx_img_wrap(&mut img, ffi::VPX_IMG_FMT_I420, w, h, 1, base);
        let luma = w as usize * h as usize;
        let chroma = chroma_w as usize * h.div_ceil(2) as usize;
        img.planes[0] = base;
        img.planes[1] = base.add(luma);
        img.planes[2] = base.add(luma + chroma);
        img.stride[0] = w as c_int;
        img.stride[1] = chroma_w as c_int;
        img.stride[2] = chroma_w as c_int;
        img
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> EncoderConfig {
        EncoderConfig::new(320, 240, 30, 500)
    }

    fn frame() -> I420Frame {
        I420Frame::new(320, 240, vec![128; I420Frame::buffer_len(320, 240)]).expect("frame")
    }

    /// The layer of each of the first `count` frames.
    fn layers_of(encoder: &mut Vp9Encoder, count: i64) -> Vec<u8> {
        (0..count)
            .map(|index| {
                let out = encoder.encode(&frame(), index * 33, false).expect("encode");
                out[0].temporal_layer
            })
            .collect()
    }

    #[test]
    fn defaults_are_three_layers_and_automatic_threads() {
        let mut encoder = Vp9Encoder::new(config()).expect("encoder");
        assert_eq!(encoder.temporal_layers(), TEMPORAL_LAYERS);
        assert_eq!(encoder.cfg.g_threads, vp9_threads());
        assert_eq!(encoder.config(), config());
        assert_eq!(layers_of(&mut encoder, 5), [0, 2, 1, 2, 0]);
    }

    #[test]
    fn produces_the_requested_layers() {
        let mut one = Vp9Encoder::new(config().with_temporal_layers(1)).expect("encoder");
        assert_eq!(one.temporal_layers(), 1);
        assert_eq!(layers_of(&mut one, 4), [0, 0, 0, 0]);

        let mut two = Vp9Encoder::new(config().with_temporal_layers(2)).expect("encoder");
        assert_eq!(two.temporal_layers(), 2);
        assert_eq!(layers_of(&mut two, 4), [0, 1, 0, 1]);
        two.set_bitrate(800).expect("bitrate");
        assert_eq!(two.cfg.layer_target_bitrate[..2], [480, 800]);
    }

    /// A picture of faint noise that differs on every frame, so the encoder can spend any
    /// bitrate the test asks for.
    fn noise(index: u32) -> I420Frame {
        let mut state = index.wrapping_mul(2_654_435_761).wrapping_add(1);
        let data = (0..I420Frame::buffer_len(320, 240))
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                112 + (state >> 27) as u8
            })
            .collect();
        I420Frame::new(320, 240, data).expect("frame")
    }

    /// The bitrate in kbit/s of the second of two seconds of noise at 30 fps.
    fn measured_kbps(layers: u8, bitrate_kbps: u32) -> u32 {
        let settings = EncoderConfig::new(320, 240, 30, bitrate_kbps).with_temporal_layers(layers);
        let mut encoder = Vp9Encoder::new(settings).expect("encoder");
        let mut bytes = 0;
        for index in 0..60u32 {
            let out = encoder
                .encode(&noise(index), i64::from(index * 1000 / 30), false)
                .expect("encode");
            if index >= 30 {
                bytes += out.iter().map(|frame| frame.data.len()).sum::<usize>();
            }
        }
        (bytes * 8 / 1000) as u32
    }

    #[test]
    fn the_output_size_follows_the_bitrate_with_one_and_two_layers() {
        for layers in [1, 2] {
            for target in [500, 1500, 4000] {
                let measured = measured_kbps(layers, target);
                assert!(
                    (target * 7 / 10..=target * 13 / 10).contains(&measured),
                    "{layers} layers at {target} kbit/s: {measured} kbit/s"
                );
            }
        }
    }

    #[test]
    fn uses_the_requested_threads() {
        let encoder = Vp9Encoder::new(config().with_threads(1)).expect("encoder");
        assert_eq!(encoder.cfg.g_threads, 1);
        let encoder = Vp9Encoder::new(config().with_threads(64)).expect("encoder");
        assert_eq!(encoder.cfg.g_threads, 64);
    }

    #[test]
    fn rejects_settings_it_cannot_honour() {
        for invalid in [
            EncoderConfig { fps: 0, ..config() },
            EncoderConfig {
                fps: 121,
                ..config()
            },
            config().with_temporal_layers(0),
            config().with_temporal_layers(4),
            config().with_threads(0),
            config().with_threads(65),
            config().with_max_size(319, 240),
        ] {
            assert!(matches!(
                Vp9Encoder::new(invalid),
                Err(Error::InvalidConfig(_))
            ));
        }
    }

    #[test]
    fn accepts_120_fps_and_keeps_the_settings_across_changes() {
        let settings = config().with_temporal_layers(2).with_max_size(640, 480);
        let mut encoder = Vp9Encoder::new(EncoderConfig {
            fps: 120,
            ..settings
        })
        .expect("encoder");
        encoder.set_fps(60).expect("fps");
        assert!(matches!(encoder.set_fps(121), Err(Error::InvalidConfig(_))));
        // Growing past the opened size opens a new encoder, which keeps the settings.
        encoder.set_resolution(640, 480).expect("grow");
        assert_eq!(encoder.temporal_layers(), 2);
        assert!(matches!(
            encoder.set_resolution(641, 480),
            Err(Error::InvalidConfig(_))
        ));
        assert_eq!(
            encoder.config(),
            EncoderConfig {
                width: 640,
                height: 480,
                fps: 60,
                ..settings
            }
        );
    }
}
