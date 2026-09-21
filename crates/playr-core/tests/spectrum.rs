//! The spectrogram read with a track's peaks, against signals built by hand.

use playr_core::spectrum::{BANDS, FFT, HOP};
use playr_core::wave::Peaks;

const RATE: u32 = 44_100;

/// `seconds` of a sine at `hz` and `amplitude`, mono.
fn sine(hz: f32, amplitude: f32, seconds: f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    (0..n)
        .map(|i| amplitude * (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin())
        .collect()
}

/// The band holding `hz`.
fn band_of(peaks: &Peaks, hz: f32) -> usize {
    (0..BANDS)
        .find(|&b| {
            let (lo, hi) = peaks.spectrum.band_hz(b);
            lo <= hz && hz < hi
        })
        .unwrap()
}

/// A tone on bin 47, about 1012 Hz. Between bins, a Hann window reads up to
/// 1.42 dB low.
const ON_BIN: f32 = 47.0 * RATE as f32 / FFT as f32;

#[test]
fn a_full_scale_sine_reads_near_0_db_in_its_band_and_far_below_elsewhere() {
    let peaks = Peaks::from_interleaved(&sine(ON_BIN, 1.0, 1.0), 1, RATE);
    let s = &peaks.spectrum;
    let column = s.column(RATE as u64 / 4, RATE as u64 / 2).unwrap();
    let band = band_of(&peaks, ON_BIN);
    let loudest = (0..BANDS)
        .max_by(|&a, &b| column[a].total_cmp(&column[b]))
        .unwrap();
    assert_eq!(loudest, band);
    assert!(column[band].abs() < 0.5, "{} dB", column[band]);
    assert!(column[band_of(&peaks, 8000.0)] < -80.0);
    assert!(column[band_of(&peaks, 100.0)] < -80.0);
    assert!((s.loudest() - column[band]).abs() < 0.6);
}

#[test]
fn half_the_amplitude_reads_6_db_lower() {
    let full = Peaks::from_interleaved(&sine(1000.0, 1.0, 0.5), 1, RATE);
    let half = Peaks::from_interleaved(&sine(1000.0, 0.5, 0.5), 1, RATE);
    let band = band_of(&full, 1000.0);
    let level = |p: &Peaks| p.spectrum.column(4096, 8192).unwrap()[band];
    let drop = level(&full) - level(&half);
    assert!((drop - 6.02).abs() < 0.6, "{drop} dB");
}

#[test]
fn stereo_is_read_as_the_channels_mean() {
    let mono = sine(440.0, 0.5, 0.5);
    let stereo: Vec<f32> = mono.iter().flat_map(|&s| [s, s]).collect();
    assert_eq!(
        Peaks::from_interleaved(&mono, 1, RATE).spectrum,
        Peaks::from_interleaved(&stereo, 2, RATE).spectrum
    );
}

#[test]
fn every_hop_has_an_entry_centred_on_it() {
    // Silence, then a tone from the start of hop 40.
    let start = 40 * HOP as usize;
    let mut signal = vec![0.0; start];
    signal.extend(sine(1000.0, 1.0, 0.5));
    let peaks = Peaks::from_interleaved(&signal, 1, RATE);
    let s = &peaks.spectrum;
    assert_eq!(s.entries(), signal.len().div_ceil(HOP as usize));
    let band = band_of(&peaks, 1000.0);
    let hop = |i: u64| s.column(i * HOP, (i + 1) * HOP).unwrap()[band];
    // A window reaches 768 frames before its hop: under two hops.
    assert!(hop(37) < -100.0, "{}", hop(37));
    assert!(hop(40) > -10.0, "{}", hop(40));
    assert!(s.column(0, start as u64 - 2 * HOP).unwrap()[band] < -100.0);
}

#[test]
fn a_wide_column_is_the_loudest_of_the_hops_inside_it() {
    let mut signal = sine(300.0, 0.2, 1.0);
    signal.extend(sine(5000.0, 0.9, 1.0));
    signal.extend(sine(60.0, 0.5, 1.0));
    let s = Peaks::from_interleaved(&signal, 1, RATE).spectrum;
    let (a, b) = (10 * HOP, 200 * HOP);
    let wide = s.column(a, b).unwrap();
    let mut each = [f32::NEG_INFINITY; BANDS];
    for i in a / HOP..b / HOP {
        let hop = s.column(i * HOP, (i + 1) * HOP).unwrap();
        for (e, h) in each.iter_mut().zip(hop) {
            *e = e.max(h);
        }
    }
    assert_eq!(wide, each);
}

#[test]
fn nothing_is_held_past_the_end_or_for_an_empty_track() {
    let s = Peaks::from_interleaved(&sine(440.0, 0.5, 0.1), 1, RATE).spectrum;
    let past = s.entries() as u64 * HOP;
    assert!(s.column(past, past + HOP).is_none());
    assert!(s.column(100, 100).is_none());
    let empty = Peaks::from_interleaved(&[], 1, RATE).spectrum;
    assert_eq!(empty.entries(), 0);
    assert!(empty.column(0, HOP).is_none());
}

#[test]
fn bands_are_even_in_log_frequency_at_any_rate() {
    for rate in [8_000, 44_100, 96_000, 192_000] {
        let s = Peaks::from_interleaved(&[0.0; 16], 1, rate).spectrum;
        assert_eq!(s.band_hz(0).0, 20.0);
        assert!((s.band_hz(BANDS - 1).1 / (rate as f32 / 2.0) - 1.0).abs() < 1e-4);
        let ratio = |b: usize| s.band_hz(b).1 / s.band_hz(b).0;
        for b in 1..BANDS {
            assert!((ratio(b) / ratio(0) - 1.0).abs() < 1e-3, "{rate}: band {b}");
        }
    }
}

#[test]
fn bands_narrower_than_a_bin_rise_smoothly_to_a_tone_without_repeating() {
    // A tone on bin 8, about 172 Hz, where bands are a fraction of a bin.
    let hz = 8.0 * RATE as f32 / FFT as f32;
    let peaks = Peaks::from_interleaved(&sine(hz, 1.0, 0.5), 1, RATE);
    let bin = RATE as f32 / FFT as f32;
    let narrow: Vec<usize> = (0..BANDS)
        .filter(|&b| {
            let (lo, hi) = peaks.spectrum.band_hz(b);
            (lo / bin).ceil() >= (hi / bin).ceil()
        })
        .collect();
    let crossover = peaks.spectrum.band_hz(*narrow.last().unwrap()).1;
    eprintln!("bands narrower than a bin up to {crossover:.0} Hz");
    // Bands from bin 7 to bin 8: all narrow, rising to the tone.
    let around: Vec<usize> = (0..BANDS)
        .filter(|&b| {
            let (lo, hi) = peaks.spectrum.band_hz(b);
            let centre = (lo * hi).sqrt() / bin;
            (7.0..8.0).contains(&centre)
        })
        .collect();
    assert!(around.len() >= 2 && around.iter().all(|b| narrow.contains(b)));
    let column = peaks.spectrum.column(4096, 8192).unwrap();
    let levels: Vec<f32> = around.iter().map(|&b| column[b]).collect();
    assert!(levels.windows(2).all(|w| w[1] > w[0]), "{levels:?}");
}

#[test]
fn heights_rise_from_0_at_20_hz_to_1_at_half_the_rate() {
    let s = Peaks::from_interleaved(&[0.0; 16], 1, RATE).spectrum;
    assert_eq!(s.height_of(20.0), Some(0.0));
    assert!((s.height_of(RATE as f32 / 2.0).unwrap() - 1.0).abs() < 1e-6);
    assert_eq!(s.height_of(10.0), None);
    let heights: Vec<f32> = [50.0, 100.0, 1000.0, 10_000.0]
        .iter()
        .map(|&hz| s.height_of(hz).unwrap())
        .collect();
    assert!(heights.windows(2).all(|w| w[1] > w[0]), "{heights:?}");
    // A band's lower edge sits at its share of the height.
    let (lo, _) = s.band_hz(64);
    assert!((s.height_of(lo).unwrap() - 0.5).abs() < 1e-4);
}
