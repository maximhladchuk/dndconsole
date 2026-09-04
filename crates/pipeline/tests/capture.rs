//! Integration test against a real microphone.
//!
//! Skipped rather than failed when there is no input device or macOS has not granted
//! microphone access, so the suite still runs on a machine without a mic.

use std::time::Duration;

use dndsound_pipeline::{Capture, CaptureStatus, Error, TARGET_SAMPLE_RATE};

fn start() -> Option<Capture> {
    match Capture::start(None) {
        Ok(capture) => Some(capture),
        Err(Error::NoInputDevice) => {
            eprintln!("skipping: no microphone on this machine");
            None
        }
        Err(Error::PermissionDenied) => {
            eprintln!("skipping: microphone access has not been granted");
            None
        }
        Err(e) => panic!("capture failed for an unexpected reason: {e}"),
    }
}

#[test]
fn capture_produces_sixteen_kilohertz_audio_at_about_real_time() {
    let Some(capture) = start() else { return };

    assert_eq!(capture.status(), CaptureStatus::Running);
    assert!(!capture.device_name().is_empty());
    assert!(
        capture.input_sample_rate() >= 8_000,
        "implausible device rate {}",
        capture.input_sample_rate()
    );

    std::thread::sleep(Duration::from_millis(1_000));

    let pcm = capture.take_pcm();
    let expected = TARGET_SAMPLE_RATE as usize;

    // Allow generous slack for start-up and scheduling; the point is that the rate is
    // roughly real time, not that it is exact.
    assert!(
        pcm.len() > expected / 2,
        "expected about {expected} samples in one second, got {}",
        pcm.len()
    );
    assert!(
        pcm.len() < expected * 2,
        "far more audio than one second: {} samples",
        pcm.len()
    );

    assert!(
        pcm.iter().all(|s| s.is_finite()),
        "capture produced NaN or inf"
    );
    assert!(
        pcm.iter().all(|s| s.abs() <= 1.5),
        "capture produced out-of-range samples"
    );

    assert!(capture.level().is_finite());
    assert!(capture.level() >= 0.0);
}

#[test]
fn taking_the_audio_twice_does_not_return_it_twice() {
    let Some(capture) = start() else { return };

    std::thread::sleep(Duration::from_millis(300));
    let first = capture.take_pcm();
    let second = capture.take_pcm();

    assert!(!first.is_empty(), "expected some audio in 300 ms");
    assert!(
        second.len() < first.len(),
        "the buffer should have been drained, got {} then {}",
        first.len(),
        second.len()
    );
}

#[test]
fn stopping_the_capture_ends_it_cleanly() {
    let Some(mut capture) = start() else { return };

    std::thread::sleep(Duration::from_millis(200));
    let before = capture.produced_samples();
    assert!(before > 0, "no audio was produced before stopping");

    capture.stop();
    assert_eq!(capture.status(), CaptureStatus::Stopped);

    // Nothing should arrive after the stop.
    let after_stop = capture.produced_samples();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        capture.produced_samples(),
        after_stop,
        "audio kept arriving after stop()"
    );

    // Stopping twice must be harmless.
    capture.stop();
}

#[test]
fn requesting_a_microphone_that_does_not_exist_reports_it_clearly() {
    let err = match Capture::start(Some("Not A Real Microphone At All")) {
        Ok(_) => panic!("a microphone that does not exist should not open"),
        Err(e) => e,
    };
    assert!(
        matches!(err, Error::DeviceNotFound(_)),
        "expected DeviceNotFound, got {err:?}"
    );
    assert!(
        err.to_string().contains("unplugged"),
        "message should hint at the cause"
    );
}
