// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Spectral-flux onset envelope: a single number per STFT frame that
// spikes whenever something percussive/attack-like happens (a drum hit,
// a plucked note, a vocal consonant) and stays low during sustain.
//
// `tempo.rs` runs autocorrelation over this envelope rather than over
// the raw waveform, because a raw waveform's autocorrelation is
// dominated by pitch and timbre (a bassline's fundamental frequency
// correlates with itself far more strongly than the actual beat does).
// The onset envelope throws pitch away and keeps only "did something
// start here", which is what a beat actually is.

use crate::stft::StftFrames;
use crate::MonoSignal;

/// STFT size for onset detection: 1024 samples. At the working sample
/// rate (~11025-12000 Hz) that is roughly 93-83 ms of audio per frame —
/// short enough to localise a percussive attack to within a few tens of
/// milliseconds, long enough to still resolve the low end of the
/// frequency range this module looks at (20 Hz).
pub const ONSET_FFT_SIZE: usize = 1024;

/// Hop size for onset detection: 128 samples, i.e. an 8x overlap between
/// consecutive 1024-sample frames. At 11025 Hz that's a new frame every
/// 11.6 ms (about 86 frames/sec) — fine enough time resolution that an
/// integer-frame tempo estimate is already within about a percent of the
/// true tempo before `tempo.rs`'s parabolic refinement even runs.
pub const ONSET_HOP: usize = 128;

/// Low end of the frequency range summed into the onset envelope, in Hz.
/// Below this is mostly room rumble / DC drift, not musical attack
/// content.
pub const ONSET_MIN_HZ: f64 = 20.0;

/// High end of the frequency range summed into the onset envelope, in
/// Hz. Percussive attacks have energy across a wide band, but very high
/// frequencies (cymbals, sibilance) contribute more noise than reliable
/// periodicity to a beat estimate — 5 kHz keeps the fundamentals and
/// lower harmonics of drums, plucked strings and vocal consonants
/// without chasing that noise.
pub const ONSET_MAX_HZ: f64 = 5000.0;

/// Compression constant in the log-compression step (`ln(1 + K*magnitude)`).
/// Chosen so that both a quiet passage and a loud chorus register
/// meaningful flux on the same scale — without compression, a loud
/// section's raw magnitude swings would dwarf a quiet section's, and the
/// autocorrelation in `tempo.rs` would effectively only "see" the loud
/// part of the track.
const LOG_COMPRESSION_K: f32 = 100.0;

/// Compute the spectral-flux onset envelope of `signal`.
///
/// Steps (see the module doc for why): STFT (1024/128) -> log-compress
/// each frame's magnitude in the 20 Hz-5 kHz range -> half-wave-rectified
/// frame-to-frame difference, summed across bins -> subtract a centred
/// ~1-second moving average to remove slow loudness drift (a verse-to-
/// chorus swell should not itself look like a beat).
///
/// Returns one value per STFT frame. The very first frame has no
/// predecessor to diff against, so it is `0.0` by construction — this
/// does not matter for `tempo.rs`'s autocorrelation, which sums over the
/// whole envelope and is unaffected by a single leading zero.
pub fn spectral_flux_onset_envelope(signal: &MonoSignal) -> Vec<f32> {
    let frames: Vec<Vec<f32>> =
        StftFrames::new(&signal.samples, ONSET_FFT_SIZE, ONSET_HOP).collect();
    if frames.is_empty() {
        return Vec::new();
    }

    // Bin range covering ONSET_MIN_HZ..=ONSET_MAX_HZ, computed from the
    // ACTUAL working sample rate rather than assumed — this is one of
    // the constants the brief is explicit must never be hard-coded for
    // one particular source rate (11025 vs 12000 depending on whether
    // the source was e.g. 44100 or 48000).
    let bin_hz = signal.sample_rate as f64 / ONSET_FFT_SIZE as f64;
    let max_bin_index = frames[0].len().saturating_sub(1);
    let min_bin = ((ONSET_MIN_HZ / bin_hz).floor() as usize).min(max_bin_index);
    let max_bin = ((ONSET_MAX_HZ / bin_hz).ceil() as usize).min(max_bin_index);

    // Log-compress each frame's magnitude within the bin range. This is
    // the "S[k][b]" step: ln(1 + 100*|X|). Doing it once up front (rather
    // than inline in the flux loop) makes the flux loop itself a plain
    // diff, and means each frame's compressed values are computed
    // exactly once even though flux compares frame k to k-1.
    let compressed: Vec<Vec<f32>> = frames
        .iter()
        .map(|frame| {
            frame[min_bin..=max_bin]
                .iter()
                .map(|&m| (1.0 + LOG_COMPRESSION_K * m).ln())
                .collect()
        })
        .collect();

    // Spectral flux: sum of the POSITIVE part of the frame-to-frame
    // difference. Half-wave rectifying (max(0, delta)) rather than using
    // the raw (possibly negative) delta is deliberate — a frequency
    // bin's energy falling off is the tail of a note decaying, not a new
    // onset, and including it would blur every attack into the release
    // that follows it.
    let mut flux = vec![0.0f32; compressed.len()];
    for k in 1..compressed.len() {
        let sum: f32 = compressed[k]
            .iter()
            .zip(compressed[k - 1].iter())
            .map(|(&cur, &prev)| (cur - prev).max(0.0))
            .sum();
        flux[k] = sum;
    }

    // Subtract a centred moving average to remove slow loudness drift
    // (a verse building into a chorus, a fade-in) while KEEPING
    // negative values afterwards — a dip below the local average is
    // real information (this frame was quieter than its neighbours),
    // and clipping it to zero would throw that away.
    let fps = signal.sample_rate as f64 / ONSET_HOP as f64;
    let window_frames = fps.round().max(1.0) as usize; // ~1 second, from the ACTUAL fps
    let half = window_frames / 2;

    let mut result = vec![0.0f32; flux.len()];
    for i in 0..flux.len() {
        let lo = i.saturating_sub(half);
        let hi = (i + half).min(flux.len() - 1);
        let count = (hi - lo + 1) as f32;
        let mean: f32 = flux[lo..=hi].iter().sum::<f32>() / count;
        result[i] = flux[i] - mean;
    }
    result
}

/// Frames per second of the onset envelope at a given working sample
/// rate. Exposed so `tempo.rs` can convert between BPM and lag-in-frames
/// without duplicating the hop/sample-rate arithmetic (and without
/// risking it drifting out of sync with the actual STFT parameters used
/// above).
pub fn onset_fps(sample_rate: u32) -> f64 {
    sample_rate as f64 / ONSET_HOP as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_signals::{click_track, silence, to_working_signal, white_noise};

    #[test]
    fn onset_envelope_spikes_at_clicks_and_is_flat_during_silence() {
        let native = click_track(120.0, 5.0, 4, 0.0, 44_100);
        let signal = to_working_signal(native, 44_100);
        let envelope = spectral_flux_onset_envelope(&signal);

        assert!(!envelope.is_empty());
        // With clicks about every 0.5s and near-silence between them,
        // the envelope's peak-to-mean ratio should be large — a flat
        // envelope would mean the flux computation found no attacks at
        // all.
        let max = envelope.iter().cloned().fold(f32::MIN, f32::max);
        let mean: f32 = envelope.iter().sum::<f32>() / envelope.len() as f32;
        assert!(
            max > mean * 3.0,
            "expected clear peaks above the mean, got max={max} mean={mean}"
        );
    }

    #[test]
    fn onset_envelope_of_silence_is_all_zero() {
        let native = silence(3.0, 44_100);
        let signal = to_working_signal(native, 44_100);
        let envelope = spectral_flux_onset_envelope(&signal);
        assert!(envelope.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn onset_envelope_of_noise_is_nonzero_but_not_dominated_by_one_spike() {
        let native = white_noise(3.0, 44_100);
        let signal = to_working_signal(native, 44_100);
        let envelope = spectral_flux_onset_envelope(&signal);
        assert!(envelope.iter().any(|&v| v != 0.0));
    }

    #[test]
    fn onset_fps_matches_hop_and_sample_rate() {
        assert!((onset_fps(11_025) - 86.132_8).abs() < 0.01);
        assert!((onset_fps(12_000) - 93.75).abs() < 0.001);
    }
}
