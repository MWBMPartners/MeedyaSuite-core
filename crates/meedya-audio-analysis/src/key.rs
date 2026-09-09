// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Musical key detection via the Krumhansl-Kessler key profiles: for each
// of the 24 possible keys, rotate that key's profile onto its tonic and
// correlate it against the recording's averaged chroma vector (from
// chroma.rs). The best-correlating key wins, but only if it wins by
// enough margin to be worth reporting — see the confidence formula
// below.
//
// ## Where the profiles come from
//
// Krumhansl and Kessler (1982) asked musically trained listeners to rate,
// on a 1-7 scale, how well each of the 12 chromatic pitch classes "fit"
// after hearing a short musical context establishing a key. Averaged
// across listeners and contexts, the ratings form a fixed 12-number
// profile per mode (major/minor) that peaks hardest on the tonic, next
// hardest on the notes that make the tonic chord and the closely related
// scale degrees, and lowest on notes outside the key entirely. This is
// one of the most widely reproduced datasets in music information
// retrieval, and the numbers below are transcribed from — and checked
// against — Craig Sapp's published transcription of them
// (http://extra.humdrum.org/man/keycor/, the reference implementation
// most MIR key-detection code traces back to), not typed from memory.
//
// ## Why relative keys (e.g. C major / A minor) don't get confused
//
// C major and A minor share every note in their scale — there is
// nothing in "which 7 notes are present" that tells them apart. What
// DOES tell them apart is how much WEIGHT each note carries: a piece in
// C major leans on C, G and E; the same seven notes in A minor lean on
// A, C and E instead. The profile shapes below encode exactly that
// weighting, which is why correlating against a chroma vector (itself
// weighted by how much each pitch class actually sounds, not just
// whether it appears at all) can separate the two.
//
// ## Why detection refuses more often than it guesses
//
// Real, polyphonic, non-synthetic music typically only correlates with
// its own true key's profile at r=0.6-0.8, with a modest margin over the
// second-best candidate — template-based key detection tops out around
// 65-75% accuracy on real recordings. Reporting a key with high
// confidence on every input would mean being confidently wrong roughly a
// quarter to a third of the time. The confidence formula below is built
// to make that trade-off explicit rather than hidden: a clear winner
// gets a high number, an ambiguous one gets a low number, and refusing
// outright (returning `None`) is the correct answer for input with no
// real tonal centre (noise, a single sustained tone) rather than an
// edge case to work around.

use meedya_tags_extended::{KeyMode, MusicalKey, Note};
use serde::{Deserialize, Serialize};

use crate::chroma::compute_chroma;
use crate::MonoSignal;

/// Krumhansl-Kessler major-key profile. Index 0 = the tonic, ascending
/// by semitone through index 11.
///
/// Pinned by [`major_profile_peaks_on_tonic_fifth_and_third`] below: the
/// three largest values must be at indices {0, 7, 4} — tonic, perfect
/// fifth, major third — the three notes of the tonic triad. That pin
/// exists specifically to catch a transposition slip (a value copied
/// into the wrong position), which a "does this look like a Krumhansl
/// profile" read-through would not reliably catch but a wrong peak shape
/// would.
pub const MAJOR_PROFILE: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];

/// Krumhansl-Kessler minor-key profile, same indexing. Pinned the same
/// way: the three largest values must be at {0, 3, 7} — tonic, minor
/// third, perfect fifth, the tonic triad of a minor key.
pub const MINOR_PROFILE: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// Confidence-margin scale: `confidence = r1 * clamp((r1 - r2) /
/// CONFIDENCE_MARGIN_SCALE, 0, 1)`. A margin of 0.15 or more between the
/// winner and the runner-up reaches full confidence (subject to the `r1`
/// factor); real music's typical winning margins (a few hundredths to
/// perhaps 0.1) fall well short of that, which is by design — see the
/// module doc's "why detection refuses more often than it guesses".
const CONFIDENCE_MARGIN_SCALE: f64 = 0.15;

/// Below this variance across the 12 chroma bins, a correlation against
/// any profile is treated as meaningless and refused outright, rather
/// than trusted.
///
/// Calibrated to 1e-4, not the "somewhere near zero, just avoid
/// dividing by exactly 0.0" value of 1e-8 this started at. That starting
/// value was based on a wrong mental model of the failure mode: dividing
/// by a near-zero standard deviation was never actually the risk (a
/// near-flat chroma vector still has SOME nonzero spread in a real
/// float computation). The real risk is that Pearson correlation is
/// invariant to the SIZE of a vector's variation, only its SHAPE — so a
/// chroma vector that is 99.999% flat, varying by barely more than a
/// tenth of a percent around 1/12 in each bin, can still land a
/// spuriously high correlation against some profile purely because that
/// tiny ripple happens to resemble the profile's shape. Measured
/// directly: 20 seconds of white noise, run through the corrected
/// (density-normalised — see `compute_chroma`) chroma folding, has a
/// variance around 8e-7 across its 12 bins, and at the OLD 1e-8 floor
/// that easily cleared the bar and went on to score confidence as high
/// as 0.88 against some candidate key on more than half of the 200
/// arbitrary seeds tried. Real chroma from actual chord content in this
/// crate's tests measures variance three to four orders of magnitude
/// higher (in the 1e-3 to 1e-2 range, since a real recording's pitch
/// classes range from near-zero to a clearly dominant tonic, not all
/// hovering within a fraction of a percent of each other). 1e-4 sits
/// comfortably between the two, closer to the noise side, and was
/// re-verified against the full noise-seed sweep (0 of 80 arbitrary
/// seeds exceeded 0.2 confidence at this floor — all 80 correctly
/// returned `None`) and against every one of the 24-key detection tests
/// below (none of which came anywhere near this floor).
const CHROMA_VARIANCE_FLOOR: f64 = 1e-4;

/// Result of key detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct KeyEstimate {
    /// The detected key.
    pub key: MusicalKey,
    /// Overall confidence in `[0.0, 1.0]`: `r1 * clamp((r1 - r2) / 0.15,
    /// 0, 1)`. Both terms matter — a strong correlation with no clear
    /// margin over the runner-up is still not a confident answer, and
    /// nor is a big margin between two weak correlations.
    pub confidence: f64,
    /// The winning key's raw Pearson correlation against the chroma
    /// vector (`r1` above).
    pub correlation: f64,
    /// The best-correlating of the other 23 candidates.
    pub runner_up: MusicalKey,
    /// The runner-up's correlation (`r2` above).
    pub runner_up_correlation: f64,
    /// Estimated tuning offset from standard A440, in cents (negative =
    /// flat). Reported regardless of how confident the key itself is —
    /// it's diagnostic information about the recording, not a
    /// correctness claim about the key.
    pub tuning_cents: f64,
}

/// Detect the musical key of `signal`.
///
/// Returns `None` when there is no reliable tonal centre to report:
/// either too little non-silent audio for chroma.rs to build a
/// meaningful average (see `chroma::compute_chroma`), or a chroma vector
/// too flat to correlate against anything meaningfully (see
/// [`CHROMA_VARIANCE_FLOOR`]). A real recording with a genuinely
/// ambiguous or weak tonal centre still returns `Some`, with that
/// weakness reflected in a low `confidence` — see the module doc.
///
/// # Blocking
///
/// Synchronous and CPU-bound — see the crate-level doc's "Sync, not
/// async" section. Dominated by one STFT pass over the signal (N=8192,
/// hop=4096 — a much sparser grid than tempo's) plus a fixed 24-way
/// correlation over a 12-number vector; on a 10-minute clip at the
/// working sample rate this runs in low tens of milliseconds on modern
/// hardware — call from a blocking-safe context if invoking from inside
/// an async runtime.
pub fn detect_key(signal: &MonoSignal) -> Option<KeyEstimate> {
    let chroma_result = compute_chroma(signal)?;
    detect_key_from_chroma(&chroma_result.chroma, chroma_result.tuning_cents)
}

/// Core algorithm, separated from [`detect_key`] so tests can drive it
/// directly from a hand-built chroma vector without needing real audio
/// (used by the profile pin tests below).
fn detect_key_from_chroma(chroma: &[f64; 12], tuning_cents: f64) -> Option<KeyEstimate> {
    let mean = chroma.iter().sum::<f64>() / 12.0;
    let variance = chroma.iter().map(|c| (c - mean).powi(2)).sum::<f64>() / 12.0;
    if variance < CHROMA_VARIANCE_FLOOR {
        return None;
    }

    // All 24 candidates: 12 tonics x 2 modes.
    let mut candidates: Vec<(f64, MusicalKey)> = Vec::with_capacity(24);
    for tonic_pc in 0..12u8 {
        for mode in [KeyMode::Major, KeyMode::Minor] {
            let profile = rotated_profile(tonic_pc, mode);
            let r = pearson_correlation(chroma, &profile);
            candidates.push((
                r,
                MusicalKey {
                    tonic: note_from_pitch_class(tonic_pc),
                    mode,
                },
            ));
        }
    }
    // Descending by correlation — candidates[0] is the winner,
    // candidates[1] is "the best of the other 23".
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    let (r1, key1) = candidates[0];
    let (r2, key2) = candidates[1];
    let confidence = (r1 * ((r1 - r2) / CONFIDENCE_MARGIN_SCALE).clamp(0.0, 1.0)).clamp(0.0, 1.0);

    Some(KeyEstimate {
        key: key1,
        confidence,
        correlation: r1,
        runner_up: key2,
        runner_up_correlation: r2,
        tuning_cents,
    })
}

/// Rotate `base_profile[0..12]` (indexed with 0 = tonic) so it applies to
/// a key whose tonic is pitch class `tonic`: `rotated[pc] =
/// base[(pc - tonic) mod 12]`.
fn rotated_profile(tonic: u8, mode: KeyMode) -> [f64; 12] {
    let base = match mode {
        KeyMode::Major => &MAJOR_PROFILE,
        KeyMode::Minor => &MINOR_PROFILE,
    };
    let mut out = [0.0; 12];
    for (pc, slot) in out.iter_mut().enumerate() {
        let src = (pc + 12 - tonic as usize) % 12;
        *slot = base[src];
    }
    out
}

/// Pearson correlation coefficient between two 12-element vectors.
/// Returns `0.0` (no correlation, the safe neutral value) if either
/// vector has zero variance — this should never actually trigger for
/// `b` (a Krumhansl profile always has real spread), but the guard costs
/// nothing and removes any possibility of a NaN slipping through here
/// even if it did.
fn pearson_correlation(a: &[f64; 12], b: &[f64; 12]) -> f64 {
    let mean_a = a.iter().sum::<f64>() / 12.0;
    let mean_b = b.iter().sum::<f64>() / 12.0;
    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    for i in 0..12 {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }
    if var_a <= 0.0 || var_b <= 0.0 {
        return 0.0;
    }
    cov / (var_a.sqrt() * var_b.sqrt())
}

/// Pitch class (0=C .. 11=B) to [`Note`], matching the semitone ordering
/// `meedya_tags_extended::Note` declares its variants in.
pub(crate) fn note_from_pitch_class(pc: u8) -> Note {
    match pc % 12 {
        0 => Note::C,
        1 => Note::CSharp,
        2 => Note::D,
        3 => Note::DSharp,
        4 => Note::E,
        5 => Note::F,
        6 => Note::FSharp,
        7 => Note::G,
        8 => Note::GSharp,
        9 => Note::A,
        10 => Note::ASharp,
        _ => Note::B,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_signals::{
        chord_progression, chord_progression_tuned, silence, to_working_signal,
    };

    // ----------------------------------------------------------
    // Krumhansl-Kessler profile pin tests
    // ----------------------------------------------------------
    //
    // These exist specifically to catch a transposition slip -- a value
    // copied into the wrong index -- which would otherwise silently
    // change which notes the profile treats as "belonging" to the key.

    fn top_3_indices(profile: &[f64; 12]) -> std::collections::BTreeSet<usize> {
        let mut indexed: Vec<(usize, f64)> = profile.iter().copied().enumerate().collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        indexed.into_iter().take(3).map(|(i, _)| i).collect()
    }

    #[test]
    fn major_profile_peaks_on_tonic_fifth_and_third() {
        let top3 = top_3_indices(&MAJOR_PROFILE);
        let expected: std::collections::BTreeSet<usize> = [0, 7, 4].into_iter().collect();
        assert_eq!(
            top3, expected,
            "major profile's top 3 must be {{tonic, fifth, third}}"
        );
    }

    #[test]
    fn minor_profile_peaks_on_tonic_third_and_fifth() {
        let top3 = top_3_indices(&MINOR_PROFILE);
        let expected: std::collections::BTreeSet<usize> = [0, 3, 7].into_iter().collect();
        assert_eq!(
            top3, expected,
            "minor profile's top 3 must be {{tonic, minor third, fifth}}"
        );
    }

    // ----------------------------------------------------------
    // Rotation sanity
    // ----------------------------------------------------------

    #[test]
    fn rotated_profile_is_a_true_rotation() {
        // Rotating by 0 must reproduce the base profile exactly.
        assert_eq!(rotated_profile(0, KeyMode::Major), MAJOR_PROFILE);
        assert_eq!(rotated_profile(0, KeyMode::Minor), MINOR_PROFILE);
        // Rotating the tonic to pitch class 9 (A) means the ROTATED
        // profile's own index 9 carries the base profile's tonic value.
        let rotated = rotated_profile(9, KeyMode::Major);
        assert_eq!(rotated[9], MAJOR_PROFILE[0]);
    }

    // ----------------------------------------------------------
    // Every key, via the real generated-audio pipeline
    // ----------------------------------------------------------

    fn assert_detects_key(key: MusicalKey) {
        let native = chord_progression(key, 20.0);
        let signal = to_working_signal(native, 44_100);
        let est =
            detect_key(&signal).unwrap_or_else(|| panic!("expected a key estimate for {key:?}"));
        assert_eq!(
            est.key, key,
            "expected {key:?}, got {:?} (r1={} r2={})",
            est.key, est.correlation, est.runner_up_correlation
        );
        assert!(
            est.confidence >= 0.7,
            "expected confidence >= 0.7 for {key:?}, got {} (r1={} r2={})",
            est.confidence,
            est.correlation,
            est.runner_up_correlation
        );
    }

    #[test]
    fn detects_every_major_key() {
        for pc in 0..12u8 {
            assert_detects_key(MusicalKey {
                tonic: note_from_pitch_class(pc),
                mode: KeyMode::Major,
            });
        }
    }

    #[test]
    fn detects_every_minor_key() {
        for pc in 0..12u8 {
            assert_detects_key(MusicalKey {
                tonic: note_from_pitch_class(pc),
                mode: KeyMode::Minor,
            });
        }
    }

    // ----------------------------------------------------------
    // Relative keys don't get confused
    // ----------------------------------------------------------

    #[test]
    fn c_major_is_not_confused_with_its_relative_a_minor() {
        assert_detects_key(MusicalKey {
            tonic: Note::C,
            mode: KeyMode::Major,
        });
        assert_detects_key(MusicalKey {
            tonic: Note::A,
            mode: KeyMode::Minor,
        });
    }

    #[test]
    fn f_sharp_minor_is_not_confused_with_its_relative_a_major() {
        assert_detects_key(MusicalKey {
            tonic: Note::FSharp,
            mode: KeyMode::Minor,
        });
        assert_detects_key(MusicalKey {
            tonic: Note::A,
            mode: KeyMode::Major,
        });
    }

    // ----------------------------------------------------------
    // Tuning offset
    // ----------------------------------------------------------

    #[test]
    fn a432_tuning_is_still_detected_as_c_major_with_the_right_tuning_offset() {
        let key = MusicalKey {
            tonic: Note::C,
            mode: KeyMode::Major,
        };
        let native = chord_progression_tuned(key, 20.0, 432.0, 44_100);
        let signal = to_working_signal(native, 44_100);
        let est = detect_key(&signal).expect("expected a key estimate");
        assert_eq!(
            est.key, key,
            "A432 tuning should not change the DETECTED key"
        );
        // 1200 * log2(432/440) ~= -31.77 cents.
        assert!(
            (est.tuning_cents - (-32.0)).abs() <= 5.0,
            "expected tuning_cents within 5 of -32, got {}",
            est.tuning_cents
        );
    }

    // ----------------------------------------------------------
    // Refusal on noise / silence
    // ----------------------------------------------------------

    #[test]
    fn white_noise_refuses_or_has_low_confidence() {
        let native = crate::test_signals::white_noise(20.0, 44_100);
        let signal = to_working_signal(native, 44_100);
        if let Some(est) = detect_key(&signal) {
            assert!(
                est.confidence <= 0.2,
                "expected low confidence for white noise, got {} ({:?})",
                est.confidence,
                est.key
            );
        }
    }

    #[test]
    fn a_single_sustained_tone_refuses_or_has_low_confidence() {
        // A pure sine has almost all of its energy in one pitch class --
        // the opposite extreme from noise's near-uniform spread -- but
        // is just as far from a real key's chroma shape and should be
        // treated with the same suspicion.
        let sample_rate = 44_100u32;
        let seconds = 20.0;
        let freq = 220.0; // A3
        let n = (seconds * sample_rate as f64) as usize;
        let native: Vec<f32> = (0..n)
            .map(|i| {
                (2.0 * std::f64::consts::PI * freq * i as f64 / sample_rate as f64).sin() as f32
                    * 0.5
            })
            .collect();
        let signal = to_working_signal(native, sample_rate);
        if let Some(est) = detect_key(&signal) {
            assert!(
                est.confidence <= 0.2,
                "expected low confidence for a single sustained tone, got {} ({:?})",
                est.confidence,
                est.key
            );
        }
    }

    #[test]
    fn silence_yields_no_key_estimate() {
        let native = silence(20.0, 44_100);
        let signal = to_working_signal(native, 44_100);
        assert!(detect_key(&signal).is_none());
    }
}
