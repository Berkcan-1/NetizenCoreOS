extern crate std;

use crate::adpcm::AdpcmReader;
use crate::mixer::{limit, Mixer};
use crate::synth::sine_q15;
use crate::theme::Sound;
use std::vec::Vec;

const ALL: [Sound; 14] = [
    Sound::Startup,
    Sound::Shutdown,
    Sound::Click,
    Sound::Open,
    Sound::Close,
    Sound::Notify,
    Sound::Success,
    Sound::Warning,
    Sound::Error,
    Sound::Lock,
    Sound::Unlock,
    Sound::VolumeTick,
    Sound::Connect,
    Sound::Disconnect,
];

fn render(mixer: &mut Mixer, max_frames: usize) -> Vec<i16> {
    let mut all = Vec::new();
    let mut slot = [0i16; 1024];
    while mixer.active() && all.len() / 2 < max_frames {
        mixer.mix(&mut slot);
        all.extend_from_slice(&slot);
    }
    all
}

fn full_volume() -> Mixer {
    let mut mixer = Mixer::new();
    mixer.set_gain(100, false);
    for _ in 0..0 {}
    let mut warm = [0i16; 2048];
    mixer.mix(&mut warm);
    mixer
}

fn peak(samples: &[i16]) -> i32 {
    samples.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0)
}

fn write_wav(path: &str, samples: &[i16]) {
    let mut bytes = Vec::new();
    let data_len = (samples.len() * 2) as u32;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&(48_000u32 * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn sine_matches_reference() {
    let mut worst = 0.0f64;
    for phase in 0..=65_535u32 {
        let expected = (phase as f64 / 65_536.0 * core::f64::consts::TAU).sin() * 32_767.0;
        let got = sine_q15(phase as u16) as f64;
        worst = worst.max((expected - got).abs());
    }
    assert!(worst < 0.0012 * 32_767.0, "en buyuk sinus hatasi {}", worst);
}

#[test]
fn limiter_is_bounded_and_monotonic() {
    let mut last = i16::MIN as i32;
    for value in -400_000..=400_000i32 {
        let out = limit(value) as i32;
        assert!(out >= last, "monoton degil: {} -> {}", value, out);
        assert!((-32_767..=32_767).contains(&out));
        last = out;
    }
    assert_eq!(limit(1000), 1000);
    assert_eq!(limit(-1000), -1000);
}

#[test]
fn startup_blob_decodes_close_to_source_wav() {
    let blob = nc_assets::startup_sound().expect("baslangic sesi gomulu degil");
    let mut reader = AdpcmReader::new(blob).expect("baslik gecersiz");
    assert_eq!(reader.rate(), 44_100);

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/sound/startup.wav");
    let wav = std::fs::read(path).unwrap();
    let data_pos = wav.windows(4).position(|w| w == b"data").unwrap() + 8;
    let source: Vec<i16> = wav[data_pos..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    let max_source = source.iter().map(|v| (*v as i32).abs()).max().unwrap() as f64;
    let gain = (22_000.0 / max_source).min(3.0);

    let mut signal = 0f64;
    let mut noise = 0f64;
    let mut frames = 0usize;
    while let Some((l, r)) = reader.next_frame() {
        let sl = source[frames * 2] as f64 * gain;
        let sr = source[frames * 2 + 1] as f64 * gain;
        signal += sl * sl + sr * sr;
        noise += (l as f64 - sl).powi(2) + (r as f64 - sr).powi(2);
        frames += 1;
    }
    assert_eq!(frames, source.len() / 2);
    let snr = 10.0 * (signal / noise.max(1.0)).log10();
    assert!(snr > 24.0, "ADPCM SNR cok dusuk: {:.1} dB", snr);
}

#[test]
fn startup_plays_three_seconds_at_output_rate() {
    let blob = nc_assets::startup_sound().unwrap();
    let mut mixer = full_volume();
    assert!(mixer.play_adpcm(Sound::Startup, blob));
    let out = render(&mut mixer, 48_000 * 10);
    let seconds = out.len() as f64 / 2.0 / 48_000.0;
    assert!((2.95..3.12).contains(&seconds), "sure {:.3} sn", seconds);
    assert!(peak(&out) > 8_000 && peak(&out) <= 32_767);
    let tail = &out[out.len() - 400..];
    assert!(peak(tail) < 600, "kuyruk sessizlige inmiyor: {}", peak(tail));
}

#[test]
fn every_sound_is_audible_bounded_and_terminates() {
    for sound in ALL {
        if sound == Sound::Startup {
            continue;
        }
        let mut mixer = full_volume();
        assert!(mixer.play_synth(sound), "{:?} baslamadi", sound);
        let out = render(&mut mixer, 48_000 * 5);
        assert!(!mixer.active(), "{:?} bitmedi", sound);
        let p = peak(&out);
        assert!(p > 1_500, "{:?} cok sessiz: {}", sound, p);
        assert!(p <= 32_767, "{:?} tasma", sound);
        let seconds = out.len() as f64 / 2.0 / 48_000.0;
        assert!(seconds < 2.0, "{:?} cok uzun: {}", sound, seconds);
        let tail = &out[out.len().saturating_sub(64)..];
        assert!(peak(tail) < 400, "{:?} sonu tiklamali: {}", sound, peak(tail));
    }
}

#[test]
fn mute_and_zero_volume_are_silent() {
    let mut mixer = Mixer::new();
    mixer.set_gain(100, true);
    let mut warm = [0i16; 4096];
    mixer.mix(&mut warm);
    assert!(mixer.play_synth(Sound::Error));
    let out = render(&mut mixer, 48_000 * 2);
    assert_eq!(peak(&out), 0);

    let mut mixer = Mixer::new();
    mixer.set_gain(0, false);
    mixer.mix(&mut warm);
    assert!(mixer.play_synth(Sound::Notify));
    assert_eq!(peak(&render(&mut mixer, 48_000 * 2)), 0);
}

#[test]
fn volume_curve_is_monotonic() {
    let mut last = -1;
    for volume in 0..=100u8 {
        let mut mixer = Mixer::new();
        mixer.set_gain(volume, false);
        let mut warm = [0i16; 8192];
        mixer.mix(&mut warm);
        mixer.play_synth(Sound::Notify);
        let p = peak(&render(&mut mixer, 48_000 * 2));
        assert!(p >= last, "ses {} -> tepe {} < {}", volume, p, last);
        last = p;
    }
}

#[test]
fn duplicates_are_suppressed_and_voice_pool_survives_spam() {
    let mut mixer = full_volume();
    assert!(mixer.play_synth(Sound::Click));
    assert!(!mixer.play_synth(Sound::Click));
    for _ in 0..500 {
        for sound in ALL {
            if sound != Sound::Startup {
                mixer.play_synth(sound);
            }
        }
        let mut slot = [0i16; 256];
        mixer.mix(&mut slot);
        assert!(mixer.voice_count() <= crate::mixer::MAX_VOICES);
    }
    let out = render(&mut mixer, 48_000 * 10);
    assert!(peak(&out) <= 32_767);
    assert!(!mixer.active());
}

#[test]
fn high_priority_sound_steals_a_slot_from_low_priority() {
    let mut mixer = full_volume();
    let mut guard = 0;
    while mixer.voice_count() < crate::mixer::MAX_VOICES && guard < 400 {
        mixer.play_synth(Sound::Click);
        let mut slot = [0i16; 4000];
        mixer.mix(&mut slot);
        guard += 1;
    }
    let before = mixer.voice_count();
    assert!(mixer.play_synth(Sound::Error));
    assert!(mixer.voice_count() <= before.max(1) + 1);
}

#[test]
fn dump_theme_when_requested() {
    let Ok(dir) = std::env::var("NC_AUDIO_DUMP") else { return; };
    std::fs::create_dir_all(&dir).unwrap();
    for sound in ALL {
        let mut mixer = full_volume();
        if sound == Sound::Startup {
            mixer.play_adpcm(sound, nc_assets::startup_sound().unwrap());
        } else {
            mixer.play_synth(sound);
        }
        let out = render(&mut mixer, 48_000 * 6);
        write_wav(&std::format!("{}/{:?}.wav", dir, sound), &out);
    }
}

fn sine_pcm(rate: u32, channels: usize, freq: f64, frames: usize, amp: f64) -> Vec<u8> {
    let mut out = Vec::new();
    for n in 0..frames {
        let v = (amp * (core::f64::consts::TAU * freq * n as f64 / rate as f64).sin()) as i16;
        for _ in 0..channels {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

fn drain_stream(mixer: &mut Mixer, max_frames: usize) -> Vec<i16> {
    render(mixer, max_frames)
}

fn dominant_hz(mono: &[f64], rate: f64) -> f64 {
    let n = mono.len();
    let mut best = (0.0f64, 0.0f64);
    let mut hz = 100.0;
    while hz < 8000.0 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in mono.iter().enumerate() {
            let a = core::f64::consts::TAU * hz * i as f64 / rate;
            re += v * a.cos();
            im += v * a.sin();
        }
        let power = re * re + im * im;
        if power > best.1 {
            best = (hz, power);
        }
        hz += 10.0;
    }
    let _ = n;
    best.0
}

#[test]
fn stream_plays_pcm_at_right_pitch_for_each_rate() {
    for (rate, freq) in [(8_000u32, 440.0f64), (22_050, 880.0), (44_100, 1000.0), (48_000, 1500.0)] {
        let mut mixer = full_volume();
        let id = mixer.streams.open(7, rate, 1).unwrap();
        let pcm = sine_pcm(rate, 1, freq, (rate / 4) as usize, 12_000.0);
        assert_eq!(mixer.streams.write(7, id, &pcm).unwrap(), (rate / 4) as usize);
        assert!(mixer.active());
        let out = drain_stream(&mut mixer, 48_000);
        assert!(!mixer.active());
        let mono: Vec<f64> = out.chunks(2).map(|c| c[0] as f64).take(8192).collect();
        let hz = dominant_hz(&mono, 48_000.0);
        assert!((hz - freq).abs() <= 20.0, "{} Hz akis {} Hz duydu", freq, hz);
        assert!(peak(&out) > 5_000 && peak(&out) < 20_000, "seviye {}", peak(&out));
        let seconds = out.len() as f64 / 2.0 / 48_000.0;
        assert!((seconds - 0.25).abs() < 0.03, "sure {}", seconds);
    }
}

#[test]
fn stereo_stream_keeps_channels_apart() {
    let mut mixer = full_volume();
    let id = mixer.streams.open(1, 48_000, 2).unwrap();
    let mut pcm = Vec::new();
    for n in 0..4800 {
        let v = (10_000.0 * (core::f64::consts::TAU * 1000.0 * n as f64 / 48_000.0).sin()) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
        pcm.extend_from_slice(&0i16.to_le_bytes());
    }
    mixer.streams.write(1, id, &pcm).unwrap();
    let out = drain_stream(&mut mixer, 20_000);
    let left = out.chunks(2).map(|c| (c[0] as i32).abs()).max().unwrap();
    let right = out.chunks(2).map(|c| (c[1] as i32).abs()).max().unwrap();
    assert!(left > 3_000 && right == 0, "{} {}", left, right);
}

#[test]
fn stream_write_is_nonblocking_and_reports_partial() {
    let mut mixer = full_volume();
    let id = mixer.streams.open(1, 8_000, 1).unwrap();
    let capacity = 4_000usize;
    let pcm = sine_pcm(8_000, 1, 300.0, 10_000, 5_000.0);
    assert_eq!(mixer.streams.write(1, id, &pcm).unwrap(), capacity);
    assert_eq!(mixer.streams.status(1, id).unwrap(), (capacity, 0));
    assert_eq!(mixer.streams.write(1, id, &pcm).unwrap(), 0);
    let mut slot = [0i16; 1024];
    mixer.mix(&mut slot);
    let (queued, free) = mixer.streams.status(1, id).unwrap();
    assert!(queued < capacity && free > 0);
    assert!(mixer.streams.write(1, id, &pcm[..1001]).unwrap() <= 500);
}

#[test]
fn streams_are_isolated_per_owner_and_limited() {
    let mut mixer = full_volume();
    let a = mixer.streams.open(1, 44_100, 2).unwrap();
    assert!(mixer.streams.write(2, a, &[0; 8]).is_err());
    assert!(mixer.streams.status(2, a).is_err());
    assert!(mixer.streams.close(2, a, true).is_err());
    let _a2 = mixer.streams.open(1, 44_100, 2).unwrap();
    assert!(mixer.streams.open(1, 44_100, 2).is_err());
    let _b = mixer.streams.open(2, 44_100, 2).unwrap();
    let _b2 = mixer.streams.open(2, 44_100, 2).unwrap();
    assert!(mixer.streams.open(3, 44_100, 2).is_err());
    assert!(mixer.streams.open(9, 100, 2).is_err());
    assert!(mixer.streams.open(9, 44_100, 3).is_err());
    mixer.streams.release_owner(1);
    assert!(mixer.streams.open(3, 44_100, 2).is_ok());
    assert!(mixer.streams.write(1, a, &[0; 8]).is_err());
}

#[test]
fn closing_drains_then_removes_and_discard_stops_at_once() {
    let mut mixer = full_volume();
    let id = mixer.streams.open(1, 48_000, 1).unwrap();
    mixer.streams.write(1, id, &sine_pcm(48_000, 1, 500.0, 2_000, 9_000.0)).unwrap();
    mixer.streams.close(1, id, false).unwrap();
    assert_eq!(mixer.streams.write(1, id, &[0; 16]).unwrap(), 0);
    let out = drain_stream(&mut mixer, 10_000);
    assert!(out.len() / 2 >= 1_900 && out.len() / 2 < 3_100);
    assert_eq!(mixer.streams.count(), 0);

    let id = mixer.streams.open(1, 48_000, 1).unwrap();
    mixer.streams.write(1, id, &sine_pcm(48_000, 1, 500.0, 2_000, 9_000.0)).unwrap();
    mixer.streams.close(1, id, true).unwrap();
    assert!(!mixer.active());
}

#[test]
fn starved_stream_goes_idle_and_resumes_without_garbage() {
    let mut mixer = full_volume();
    let id = mixer.streams.open(1, 48_000, 1).unwrap();
    mixer.streams.write(1, id, &sine_pcm(48_000, 1, 700.0, 1_500, 8_000.0)).unwrap();
    let first = drain_stream(&mut mixer, 10_000);
    assert!(!mixer.active());
    assert!(first.len() / 2 < 2_600);
    mixer.streams.write(1, id, &sine_pcm(48_000, 1, 700.0, 1_500, 8_000.0)).unwrap();
    assert!(mixer.active());
    let second = drain_stream(&mut mixer, 10_000);
    assert!(peak(&second) > 3_000 && peak(&second) <= 32_767);
}

#[test]
fn stream_volume_scales_level() {
    let level = |percent: u32| {
        let mut mixer = full_volume();
        let id = mixer.streams.open(1, 48_000, 1).unwrap();
        mixer.streams.set_volume(1, id, percent).unwrap();
        mixer.streams.write(1, id, &sine_pcm(48_000, 1, 600.0, 4_000, 16_000.0)).unwrap();
        peak(&drain_stream(&mut mixer, 10_000))
    };
    let (low, mid, high) = (level(25), level(50), level(100));
    assert!(low < mid && mid < high, "{} {} {}", low, mid, high);
    assert_eq!(level(0), 0);
}

#[test]
fn garbage_and_odd_length_pcm_never_panics() {
    let mut mixer = full_volume();
    let id = mixer.streams.open(1, 44_100, 2).unwrap();
    let junk: Vec<u8> = (0..4097u32).map(|n| (n.wrapping_mul(2654435761) >> 13) as u8).collect();
    for _ in 0..50 {
        let _ = mixer.streams.write(1, id, &junk);
        let mut slot = [0i16; 1024];
        mixer.mix(&mut slot);
        assert!(slot.iter().all(|s| *s >= -32767));
    }
    assert!(mixer.streams.write(1, id, &[1, 2, 3]).unwrap() == 0);
}
