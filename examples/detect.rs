//! Prints which hardware encoders work on this machine for AV1 and VP9, and why the others do
//! not: `cargo run --example detect`. The output is meant to be pasted into a bug report; it
//! names the graphics device and driver version and nothing else about the machine or its user.
//!
//! A machine without a GPU is not an error: every encoder is listed with its reason.

use hwcodec::{Codec, EncoderStatus, detect_encoders};

fn main() {
    println!(
        "hwcodec {} on {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for (name, codec) in [("AV1", Codec::Av1), ("VP9", Codec::Vp9)] {
        println!();
        println!("{name} hardware encoders");
        for report in detect_encoders(codec) {
            match &report.status {
                EncoderStatus::Unavailable { reason, detail, .. } => {
                    println!("  {}: does not work – {reason}", report.encoder);
                    println!("    driver text:    {detail}");
                }
                status => println!("  {}: {status}", report.encoder),
            }
            let or_not_reported = |value: &Option<String>| match value {
                Some(value) => value.clone(),
                None => "not reported".to_owned(),
            };
            println!("    device:         {}", or_not_reported(&report.device));
            println!(
                "    driver version: {}",
                or_not_reported(&report.driver_version)
            );
        }
    }
}
