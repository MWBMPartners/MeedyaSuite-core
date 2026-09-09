// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Chroma: fold a spectrum's energy into 12 pitch classes (C, C#, D, ...
// B), throwing octave away entirely, so key detection can ask "how much
// C, how much G, how much E is in this recording" without caring which
// octave any of it came from — a C played on a bass and a C played on a
// flute both vote for the same pitch class.
//
// Runs its own long-window STFT (separate from onset.rs's short-window
// one — see stft.rs's module doc for why tempo and key can't share a
// single STFT) and, before folding anything, estimates how far the
// recording's tuning sits from standard A440. A track genuinely recorded
// at A432, or a rip pitch-shifted by a percent or two, would otherwise
// land every note's pitch class a semitone off from where key.rs's
// profiles expect it — the correction is small in absolute terms but the
// difference between "detected correctly" and "off by a semitone" for
// every single note.

use crate::stft::StftFrames;
use crate::MonoSignal;

/// STFT size for chroma/key: 8192 samples. At the working sample rate
/// (~11025-12000 Hz) that's a 743-680 ms window — long, on purpose: key
/// detection needs FREQUENCY resolution, not time resolution (nobody
/// needs to know which millisecond a note started to know what key it's
/// in). At C2 (65.4 Hz) the next semitone up (C#2, 69.3 Hz) is only 3.9
/// Hz away; a 1024-sample window (as onset.rs uses) would only resolve
/// about 1.45 bins across that gap — not reliably separable. An
/// 8192-sample window resolves about 2.9 bins across it, comfortably
/// enough to tell the two apart.
pub const KEY_FFT_SIZE: usize = 8192;

/// Hop size for chroma/key: 4096 samples (50% overlap). No need for
/// onset.rs's dense overlap here — a chord doesn't move fast enough to
/// need tracking every 12ms the way a percussive attack does. At the
/// working rate this gives roughly 2.7-2.9 frames/second, plenty to
/// average over a whole track.
pub const KEY_HOP: usize = 4096;

/// Low end of the pitch range folded into chroma: C2, 65.4 Hz.
pub const CHROMA_MIN_HZ: f64 = 65.4;

/// High end of the pitch range folded into chroma: C7, 2093 Hz. Together
/// with [`CHROMA_MIN_HZ`] this covers essentially the full musically
/// meaningful range of a recording — below C2 is sub-bass with little
/// harmonic content to spare for pitch-class voting, above C7 is mostly
/// upper harmonics of notes whose fundamentals are already counted
/// lower down.
pub const CHROMA_MAX_HZ: f64 = 2093.0;

/// Frames quieter than this (RMS, dBFS relative to full scale) are
/// skipped entirely. There is nothing musically meaningful to fold out
/// of a frame that's mostly noise floor, and including it would only
/// dilute the average with samples that shouldn't be voting at all.
const RMS_SILENCE_FLOOR_DBFS: f64 = -60.0;

/// Minimum amount of non-silent audio, in seconds, needed before this
/// module will report a chroma vector at all. Below this there simply
/// isn't enough independent evidence for an average to mean anything —
/// a handful of frames could easily average out to something that looks
/// like a key purely by chance.
const MIN_ACTIVE_SECONDS: f64 = 2.0;

/// Width of each tuning-offset histogram bucket, in cents (1/100th of a
/// semitone).
const TUNING_BUCKET_CENTS: f64 = 10.0;

/// How far from standard tuning (in cents, each direction) the tuning
/// histogram searches. ±50 cents is exactly half a semitone either way —
/// wide enough to catch A432 (roughly -32 cents) and ordinary pitch-drift
/// rips, without wrapping around into "this is actually a different
/// note" territory.
const TUNING_RANGE_CENTS: f64 = 50.0;

/// Number of histogram buckets: the range [-50, 50] in steps of 10,
/// inclusive of both ends, is 11 buckets. Written as a literal (checked
/// against the two constants above by `tuning_bucket_count_matches_range`
/// in this module's tests) rather than computed from them at the
/// definition site, since computing an array length from `f64` consts
/// at that point would trade a self-evident number for a harder-to-read
/// expression for no real benefit.
const TUNING_BUCKETS: usize = 11;

/// Result of folding a signal into pitch classes.
pub struct ChromaResult {
    /// Average pitch-class distribution across every non-silent frame,
    /// index 0 = C, ascending semitones to index 11 = B. Each
    /// contributing frame was normalised to sum to 1 before averaging
    /// (see [`compute_chroma`]), so this itself sums to ~1 — though
    /// nothing downstream relies on that; Pearson correlation is
    /// invariant to overall scale.
    pub chroma: [f64; 12],
    /// Estimated tuning offset from standard A440, in cents. Negative
    /// means flat (e.g. roughly -32 for a recording tuned to A432).
    pub tuning_cents: f64,
    /// How many STFT frames actually contributed (cleared the RMS
    /// floor). Exposed mainly for diagnostics/tests — `compute_chroma`
    /// has already applied the [`MIN_ACTIVE_SECONDS`] gate by the time
    /// this comes back, so a caller never needs to re-check it.
    pub active_frames: usize,
}

/// Fold `signal` into an averaged, tuning-corrected 12-bin pitch-class
/// distribution.
///
/// Returns `None` when there isn't enough non-silent audio to build a
/// meaningful average from (see [`MIN_ACTIVE_SECONDS`]) — silence and
/// very short clips both hit this.
pub fn compute_chroma(signal: &MonoSignal) -> Option<ChromaResult> {
    let sample_rate = signal.sample_rate as f64;
    let bin_hz = sample_rate / KEY_FFT_SIZE as f64;

    // `.max(1.0)` keeps this away from bin 0 (DC, 0 Hz) even in a
    // pathological case with an extremely low working rate — `hz_to_midi`
    // is undefined at 0 Hz (log2(0) = -infinity), and DC never carries
    // pitch information anyway.
    let min_bin = (CHROMA_MIN_HZ / bin_hz).floor().max(1.0) as usize;
    let max_bin = ((CHROMA_MAX_HZ / bin_hz).ceil() as usize).min(KEY_FFT_SIZE / 2);
    if max_bin < min_bin {
        return None;
    }

    // Each bin's frequency (and therefore its raw, uncorrected MIDI note
    // number) depends only on its index and the sample rate — fixed for
    // every frame, so computed once rather than every time a frame
    // touches it.
    let bin_midi: Vec<f64> = (min_bin..=max_bin)
        .map(|b| hz_to_midi(b as f64 * bin_hz))
        .collect();

    let fps = sample_rate / KEY_HOP as f64;
    let min_active_frames = (MIN_ACTIVE_SECONDS * fps).round().max(1.0) as usize;

    let num_frames = StftFrames::new(&signal.samples, KEY_FFT_SIZE, KEY_HOP).num_frames();
    let mut is_active = vec![false; num_frames];
    let mut active_count = 0usize;
    let mut tuning_histogram = [0.0f64; TUNING_BUCKETS];

    // Pass 1: which frames are loud enough to use at all, and the
    // tuning-offset vote. The vote has to run on the UNCORRECTED mapping
    // — the whole point of it is to find the correction, so by
    // definition it can't already know what that correction is.
    for (frame_index, magnitudes) in
        StftFrames::new(&signal.samples, KEY_FFT_SIZE, KEY_HOP).enumerate()
    {
        let start = frame_index * KEY_HOP;
        if !frame_is_audible(&signal.samples[start..start + KEY_FFT_SIZE]) {
            continue;
        }
        is_active[frame_index] = true;
        active_count += 1;

        for (i, &midi) in bin_midi.iter().enumerate() {
            let magnitude = magnitudes[min_bin + i];
            if magnitude <= 0.0 {
                continue;
            }
            let cents_from_nearest_semitone = (midi - midi.round()) * 100.0;
            if let Some(bucket) = tuning_bucket(cents_from_nearest_semitone) {
                tuning_histogram[bucket] += magnitude as f64;
            }
        }
    }

    if active_count < min_active_frames {
        return None;
    }

    // The tuning offset is the histogram's mode — the single most
    // common "how far off the nearest standard semitone is this bin"
    // reading, weighted by how much energy backed each reading.
    let tuning_cents = tuning_histogram
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(bucket, _)| bucket_to_cents(bucket))
        .unwrap_or(0.0);
    let tuning_semitones = tuning_cents / 100.0;

    // Pass 2: fold every active frame into a pitch-class vector using
    // the now-known tuning correction, normalising EACH FRAME to sum to
    // 1 before it's added into the running total — this is what makes a
    // quiet intro's vote count as much as a loud chorus's, rather than
    // the average being dominated by whichever section happened to be
    // loudest.
    let mut chroma = [0.0f64; 12];
    for (frame_index, magnitudes) in
        StftFrames::new(&signal.samples, KEY_FFT_SIZE, KEY_HOP).enumerate()
    {
        if !is_active[frame_index] {
            continue;
        }
        let mut frame_chroma = [0.0f64; 12];
        let mut frame_bin_count = [0usize; 12];
        for (i, &midi) in bin_midi.iter().enumerate() {
            let magnitude = magnitudes[min_bin + i] as f64;
            if magnitude <= 0.0 {
                continue;
            }
            let corrected_midi = midi - tuning_semitones;
            let pitch_class = (corrected_midi.round() as i64).rem_euclid(12) as usize;
            frame_chroma[pitch_class] += magnitude;
            frame_bin_count[pitch_class] += 1;
        }

        // Density-correct: on a LINEAR-frequency FFT, a pitch class near
        // the top of the analysed range spans far more bins per
        // semitone than one near the bottom (a semitone is a constant
        // RATIO of frequency, so its width in absolute Hz -- and
        // therefore in FFT bins -- grows with frequency). Left as a raw
        // per-class SUM, that means a pitch class with a wider bucket
        // accumulates more magnitude even from input with no real tonal
        // structure at all, purely because it summed more terms. This
        // is not a hypothetical: white noise's chroma came out as a
        // near-perfect ramp from the lowest pitch class in range up to
        // the highest, tracking the bin-count ratio almost exactly,
        // which was strong enough to make several seeds of pure noise
        // "detect" as A minor. Averaging within each pitch class fixes
        // it — a REAL spectral peak's magnitude is still captured
        // correctly (it's concentrated in a handful of bins near its
        // own centre regardless of how wide the surrounding bucket is,
        // so averaging the mostly-empty rest of a wide bucket barely
        // moves it), while flat noise's per-class average no longer
        // depends on bucket width at all.
        let mut frame_sum = 0.0f64;
        for pc in 0..12 {
            if frame_bin_count[pc] > 0 {
                frame_chroma[pc] /= frame_bin_count[pc] as f64;
                frame_sum += frame_chroma[pc];
            }
        }
        if frame_sum > 0.0 {
            for pc in 0..12 {
                chroma[pc] += frame_chroma[pc] / frame_sum;
            }
        }
    }
    for v in chroma.iter_mut() {
        *v /= active_count as f64;
    }

    Some(ChromaResult {
        chroma,
        tuning_cents,
        active_frames: active_count,
    })
}

/// MIDI note number (69.0 = A4 = 440 Hz) for a frequency in Hz, using
/// STANDARD A440 — the tuning correction is applied separately, after
/// this, once the offset is known. `midi = 69 + 12*log2(freq/440)`.
fn hz_to_midi(freq: f64) -> f64 {
    69.0 + 12.0 * (freq / 440.0).log2()
}

/// Whether a frame's underlying time-domain samples are loud enough to
/// bother analysing: RMS, converted to dBFS, at or above
/// [`RMS_SILENCE_FLOOR_DBFS`]. Exact digital silence (RMS of precisely
/// 0.0) is handled as a special case rather than falling through to
/// `log10(0.0)` (`-inf`, which technically already compares below the
/// floor correctly, but there is no reason to rely on IEEE-754 float
/// edge behaviour when a plain early return says the same thing more
/// clearly).
fn frame_is_audible(window: &[f32]) -> bool {
    let rms =
        (window.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / window.len() as f64).sqrt();
    if rms <= 0.0 {
        return false;
    }
    20.0 * rms.log10() >= RMS_SILENCE_FLOOR_DBFS
}

/// Map a "cents from the nearest standard semitone" value to a tuning
/// histogram bucket, or `None` if it falls outside the ±50 cent search
/// range.
fn tuning_bucket(cents: f64) -> Option<usize> {
    if cents.abs() > TUNING_RANGE_CENTS {
        return None;
    }
    Some(((cents + TUNING_RANGE_CENTS) / TUNING_BUCKET_CENTS).round() as usize)
}

/// The inverse of [`tuning_bucket`]'s indexing: bucket `i` represents
/// `-50 + i*10` cents.
fn bucket_to_cents(bucket: usize) -> f64 {
    bucket as f64 * TUNING_BUCKET_CENTS - TUNING_RANGE_CENTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_bucket_count_matches_the_declared_range() {
        // Guards the literal TUNING_BUCKETS against TUNING_RANGE_CENTS /
        // TUNING_BUCKET_CENTS drifting out of sync with it.
        let expected = ((2.0 * TUNING_RANGE_CENTS / TUNING_BUCKET_CENTS).round() as usize) + 1;
        assert_eq!(TUNING_BUCKETS, expected);
    }

    #[test]
    fn tuning_bucket_round_trip() {
        for &cents in &[-50.0, -32.0, -10.0, 0.0, 10.0, 32.0, 50.0] {
            let bucket = tuning_bucket(cents).expect("within range");
            assert!(bucket < TUNING_BUCKETS);
            // The round trip only needs to land within one bucket width
            // (10 cents) of the original value, not reproduce it
            // exactly -- that's the entire point of bucketing.
            assert!((bucket_to_cents(bucket) - cents).abs() <= TUNING_BUCKET_CENTS);
        }
        assert!(tuning_bucket(51.0).is_none());
        assert!(tuning_bucket(-51.0).is_none());
    }

    #[test]
    fn hz_to_midi_matches_known_notes() {
        assert!((hz_to_midi(440.0) - 69.0).abs() < 1e-9); // A4
        assert!((hz_to_midi(261.625_565_3) - 60.0).abs() < 1e-4); // C4 (middle C)
    }

    #[test]
    fn frame_is_audible_respects_the_silence_floor() {
        let loud = vec![0.5f32; 1000];
        let silent = vec![0.0f32; 1000];
        assert!(frame_is_audible(&loud));
        assert!(!frame_is_audible(&silent));
    }
}
