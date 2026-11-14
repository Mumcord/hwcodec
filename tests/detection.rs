//! The detection report: one entry per hardware encoder, and a reason with a text for every
//! encoder that does not work. Written to hold on a machine without a GPU (nothing works) and
//! on one with a working GPU (some encoders work).

use hwcodec::{
    Codec, EncoderReport, EncoderStatus, HardwareEncoder, UnavailableReason, detect_encoders,
};

const CODECS: [Codec; 2] = [Codec::Av1, Codec::Vp9];

fn report(codec: Codec, encoder: HardwareEncoder) -> EncoderReport {
    detect_encoders(codec)
        .into_iter()
        .find(|report| report.encoder == encoder)
        .expect("every hardware encoder is reported")
}

fn reason(report: &EncoderReport) -> Option<UnavailableReason> {
    match &report.status {
        EncoderStatus::Unavailable { reason, .. } => Some(*reason),
        _ => None,
    }
}

#[test]
fn every_encoder_is_reported_and_names_a_reason_when_it_does_not_work() {
    for codec in CODECS {
        let reports = detect_encoders(codec);
        let reported: Vec<_> = reports.iter().map(|report| report.encoder).collect();
        assert_eq!(reported, HardwareEncoder::ALL, "{codec:?}");
        for report in &reports {
            let context = format!("{codec:?} {}", report.encoder);
            match &report.status {
                EncoderStatus::Unavailable { reason, detail, .. } => {
                    assert!(!report.works(), "{context}");
                    assert!(!reason.to_string().is_empty(), "{context}");
                    assert!(!detail.is_empty(), "{context}: no driver text");
                    assert!(
                        report.status.to_string().contains(detail.as_str()),
                        "{context}"
                    );
                }
                _ => assert!(report.works(), "{context}"),
            }
        }
    }
}

#[test]
fn encoders_that_cannot_exist_here_say_so() {
    // NVENC has no VP9 encoder on any machine.
    let nvenc_vp9 = report(Codec::Vp9, HardwareEncoder::Nvenc);
    assert!(!nvenc_vp9.works());
    if cfg!(any(target_os = "windows", target_os = "linux")) {
        assert_eq!(
            reason(&nvenc_vp9),
            Some(UnavailableReason::CodecNotSupported)
        );
    }

    for codec in CODECS {
        let not_here: &[HardwareEncoder] = if cfg!(target_os = "linux") {
            &[HardwareEncoder::Amf, HardwareEncoder::Qsv]
        } else if cfg!(windows) {
            &[HardwareEncoder::Vaapi]
        } else {
            &HardwareEncoder::ALL
        };
        for &encoder in not_here {
            assert_eq!(
                reason(&report(codec, encoder)),
                Some(UnavailableReason::NotAvailableOnThisOs),
                "{codec:?} {encoder}"
            );
        }
    }
}
