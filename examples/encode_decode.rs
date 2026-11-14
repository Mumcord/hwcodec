//! Software round trip for AV1 and VP9: `cargo run --release --example encode_decode`.
//!
//! Generates screen-like BGRA pictures, converts them to I420, encodes them with the software
//! encoder, decodes the stream again and checks that every picture comes back. It then shows
//! [`open_best_encoder`] with the default order and with a custom order. Needs no GPU.

use hwcodec::{
    Codec, EncoderConfig, EncoderKind, HardwareEncoder, I420Frame, convert, default_order,
    open_best_encoder, open_software_decoder,
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const FPS: u32 = 30;
const BITRATE_KBPS: u32 = 800;
const FRAMES: u32 = 60;
/// Below this the decoded picture no longer looks like the source.
const MIN_PSNR_DB: f64 = 30.0;

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> AnyResult<()> {
    let config = EncoderConfig::new(WIDTH, HEIGHT, FPS, BITRATE_KBPS);
    for (name, codec) in [("AV1", Codec::Av1), ("VP9", Codec::Vp9)] {
        round_trip(name, codec, &config)?;
    }

    println!();
    for (name, codec) in [("AV1", Codec::Av1), ("VP9", Codec::Vp9)] {
        // The default order: the hardware encoders of this operating system, then software.
        let order: Vec<String> = default_order(codec)
            .iter()
            .map(ToString::to_string)
            .collect();
        let (kind, _encoder) = open_best_encoder(codec, &config, None)?;
        println!("{name} default order [{}] opened: {kind}", order.join(", "));

        // A custom order: software first, so hardware is never tried.
        let software_first = [
            EncoderKind::Software,
            EncoderKind::Hardware(HardwareEncoder::Vaapi),
            EncoderKind::Hardware(HardwareEncoder::Nvenc),
        ];
        let (kind, _encoder) = open_best_encoder(codec, &config, Some(&software_first))?;
        println!("{name} software-first order opened: {kind}");
    }
    Ok(())
}

/// Encodes [`FRAMES`] pictures in software, decodes them and compares them with the input.
fn round_trip(name: &str, codec: Codec, config: &EncoderConfig) -> AnyResult<()> {
    let (_, mut encoder) = open_best_encoder(codec, config, Some(&[EncoderKind::Software]))?;
    let mut decoder = open_software_decoder(codec)?;
    let mut bytes = 0;
    let mut decoded_frames = 0;
    let mut worst_psnr = f64::INFINITY;
    for index in 0..FRAMES {
        let bgra = screen_like_bgra(WIDTH, HEIGHT, index);
        let frame = convert::bgra_to_i420(&bgra, WIDTH, HEIGHT, WIDTH, HEIGHT)
            .ok_or("the picture does not fit the encoder size")?;
        let pts = i64::from(index * 1000 / FPS);
        for packet in encoder.encode(&frame, pts, false)? {
            bytes += packet.data.len();
            let Some(decoded) = decoder.decode(&packet.data)? else {
                continue;
            };
            if (decoded.width(), decoded.height()) != (frame.width(), frame.height()) {
                return Err(format!(
                    "{name}: decoded {}x{} instead of {}x{}",
                    decoded.width(),
                    decoded.height(),
                    frame.width(),
                    frame.height()
                )
                .into());
            }
            decoded_frames += 1;
            // The first pictures are still sharpening while the rate control settles.
            if index >= FRAMES / 2 {
                worst_psnr = worst_psnr.min(luma_psnr(&frame, &decoded));
            }
        }
    }
    if decoded_frames != FRAMES {
        return Err(format!("{name}: {decoded_frames} of {FRAMES} pictures came back").into());
    }
    if worst_psnr < MIN_PSNR_DB {
        return Err(format!("{name}: picture quality too low ({worst_psnr:.1} dB)").into());
    }
    println!(
        "{name}: {FRAMES} pictures {WIDTH}x{HEIGHT} encoded to {} kB and decoded again, \
         luma PSNR at least {worst_psnr:.1} dB – ok",
        bytes / 1000
    );
    Ok(())
}

/// A packed BGRA picture: a colour gradient with a dark block moving across it.
fn screen_like_bgra(width: u32, height: u32, index: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let block_x = (index as usize * 6) % w;
    let mut bgra = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let in_block = x >= block_x && x < block_x + 80 && y >= h / 3 && y < h / 3 + 60;
            if in_block {
                bgra.extend_from_slice(&[30, 30, 30, 255]);
            } else {
                let blue = (x * 255 / w) as u8;
                let green = (y * 255 / h) as u8;
                bgra.extend_from_slice(&[blue, green, 160, 255]);
            }
        }
    }
    bgra
}

/// Peak signal-to-noise ratio of the brightness plane, in dB; higher is closer to the source.
fn luma_psnr(source: &I420Frame, decoded: &I420Frame) -> f64 {
    let squared_error: f64 = source
        .y()
        .iter()
        .zip(decoded.y())
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
        .sum();
    let mean = squared_error / source.y().len() as f64;
    if mean == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (255.0f64.powi(2) / mean).log10()
}
