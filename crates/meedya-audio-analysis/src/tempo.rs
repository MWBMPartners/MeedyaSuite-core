// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Tempo (BPM) detection via autocorrelation of the spectral-flux onset
// envelope, with a soft prior towards common tempos and a confidence
// score built from three independent, multiplied-together signals.
//
// ## Why autocorrelation, and why a prior
//
// A signal that repeats every T seconds also, mathematically, repeats
// every 2T, 3T, 4T seconds — a beat every half second is ALSO,
// trivially, a beat every second (just skip every other one). Nothing
// in the raw data can tell 120 BPM apart from 60 BPM or 240 BPM on its
// own; they are the same signal read at different resolutions. A human
// listener resolves this by which tempo "feels danceable" — most pop
// and dance music sits close to 120 BPM, and human perception of tempo
// tops out somewhere well past 200 BPM before it starts hearing a fast
// tempo as a slow one with subdivided beats. `prior_weight` encodes that
// same soft bias as a log-normal curve centred on 120 BPM.
//
// **The prior breaks ties. It must never manufacture confidence.** A
// perfectly periodic click track has EQUALLY strong raw autocorrelation
// at its true period and at every multiple of it — there is nothing in
// the data to prefer one over another, and the prior choosing 140 over
// 70 does not mean the data supports 140 twice as strongly as it
// supports 70. That is why the confidence score (below) is built
// entirely from the RAW, un-weighted autocorrelation values, never from
// the prior-weighted scores used to pick the winner. Mixing the two
// would let an arbitrary, made-up bias about what tempo is "common"
// masquerade as evidence.

use crate::onset::{onset_fps, spectral_flux_onset_envelope};
use crate::MonoSignal;
use serde::{Deserialize, Serialize};

/// Centre of the log-normal tempo prior, in BPM. 120 is the closest
/// thing dance/pop music has to a "default" tempo — not a claim that
/// most music is exactly 120 BPM, but a reasonable centre for a prior
/// that only ever needs to break ties between octave-related readings.
const PRIOR_CENTER_BPM: f64 = 120.0;

/// Width of the log-normal tempo prior, in octaves (log2 units).
///
/// Calibrated to 1.5, not the brief's starting point of 1.0 — 1.0 was
/// narrow enough to actively pick the WRONG winner on the light-offbeat
/// test (70 BPM clicks with a subtle offbeat click at half the period):
/// the true period's raw r_hat was clearly the stronger reading
/// (measured ~0.93 vs ~0.74 for the half-period artefact), but at
/// sigma=1.0 the prior's pull towards 120 BPM was strong enough to flip
/// the prior-WEIGHTED score in favour of the half-period reading anyway
/// (140 BPM sits much closer to 120 in log2 space than 70 does). That is
/// exactly the failure the module doc warns about — the prior
/// overruling real evidence instead of only breaking a genuine tie. At
/// 1.5, a full octave away from the centre still carries `exp(-0.5 *
/// (1/1.5)^2) ~= 0.80` of the peak weight: still enough to break an
/// EXACT tie (where nothing at all distinguishes two readings, e.g. a
/// perfectly periodic click train's true period vs. its sub-harmonic),
/// while no longer strong enough to override a real ~25% raw-evidence
/// gap in the other direction.
const PRIOR_SIGMA_OCTAVES: f64 = 1.5;

/// How close (as a fraction) a candidate lag has to be to an exact
/// integer multiple of the winner's lag to be treated as a "slower
/// harmonic" of it, for the ambiguity calculation below. 5% comfortably
/// covers the +/-1 frame of quantisation noise an integer-lag search
/// has near small multiples, without being so wide it starts swallowing
/// lags that are genuinely a different tempo.
const HARMONIC_TOLERANCE: f64 = 0.05;

/// A slower harmonic (2x, 3x or 4x the winner's lag) is normally NOT
/// counted as a rival — periodic signals always have real
/// autocorrelation energy there, and that is not evidence of doubt about
/// the winner. It IS counted when its RAW (un-weighted) autocorrelation
/// exceeds the winner's by more than this factor: that specific
/// situation means the data itself preferred the slower reading and only
/// the prior pulled the winner away from it, which is genuine ambiguity
/// the confidence score must reflect.
const HARMONIC_OVERRULE_FACTOR: f64 = 1.05;

/// Denominator in the ambiguity factor `(1 - rival/winner) / AMBIGUITY_SPREAD`.
/// A rival scoring within 50% of the winner is treated as maximal
/// ambiguity (factor clamps to 0); a rival scoring at half the winner's
/// mark (a comfortable, unambiguous margin) already reaches full
/// confidence (factor clamps to 1) rather than needing to fall all the
/// way to zero.
const AMBIGUITY_SPREAD: f64 = 0.5;

/// Window length, in seconds, for the segment-agreement check.
const SEGMENT_WINDOW_SECONDS: f64 = 8.0;

/// Hop between segment windows, in seconds (windows overlap 50%).
const SEGMENT_HOP_SECONDS: f64 = 4.0;

/// How close (as a fraction of the global BPM) a segment's own local
/// tempo estimate has to land to count as "agreeing" with the global
/// answer.
const SEGMENT_AGREEMENT_TOLERANCE: f64 = 0.03;

/// Below this much total onset ENERGY (sum of the envelope squared),
/// autocorrelation is meaningless to compute at all — there is nothing
/// there to correlate. True digital silence has EXACTLY zero energy
/// here (every step of the onset envelope computation is deterministic
/// and silence produces all-zero output at every stage), so this floor
/// only ever needs to catch that exact case and float-noise near it; it
/// is what makes `detect_tempo` return `None` for silence.
const AUTOCORR_ENERGY_FLOOR: f64 = 1e-9;

/// Result of tempo detection.
///
/// `Option<TempoEstimate>` rather than a bare `f64` plus a separate
/// "did we find one" flag: a value without a confidence figure attached
/// would invite a caller to skip checking it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TempoEstimate {
    /// The detected tempo in beats per minute, parabola-refined to
    /// sub-lag precision (see [`refine_tau`]) rather than left at the
    /// integer-frame resolution of the raw autocorrelation search.
    pub bpm: f64,
    /// Overall confidence in `[0.0, 1.0]`: `prominence * ambiguity *
    /// segment_agreement`. Computed entirely from evidence, never from
    /// the tempo prior — see the module doc.
    pub confidence: f64,
    /// How far the winning lag's raw autocorrelation stands above the
    /// average across the searched tempo range, in `[0.0, 1.0]`. Low on
    /// noise (nothing stands out), high on a strongly periodic signal.
    pub prominence: f64,
    /// Fraction of independently-analysed 8-second (4-second-stepped)
    /// windows whose own local tempo estimate lands within 3% of the
    /// overall answer, in `[0.0, 1.0]`. Low when the tempo genuinely
    /// changes partway through the analysed audio.
    pub segment_agreement: f64,
    /// The strongest rival candidate tempo, if any competitor cleared
    /// the bar described in the module doc (a faster subdivision, or a
    /// slower harmonic whose raw correlation beat the winner's).
    pub rival_bpm: Option<f64>,
    /// The rival's prior-weighted score, for callers that want to see
    /// how close the contest was in the same units the winner was
    /// chosen by.
    pub rival_score: Option<f64>,
}

/// Detect the dominant tempo of `signal` within `[min_bpm, max_bpm]`.
///
/// Returns `None` when there is no usable periodicity to report at all
/// — currently only true silence (see [`AUTOCORR_ENERGY_FLOOR`]) and an
/// onset envelope too short to search the requested range trigger this.
/// A signal with real but ambiguous or noisy content (white noise, a
/// tempo that changes partway through) still returns `Some`, with that
/// ambiguity reflected in a low `confidence` rather than hidden behind
/// `None` — a caller filtering on confidence gets the same result either
/// way, but gets to see WHY the answer isn't trustworthy.
///
/// # Blocking
///
/// Synchronous and CPU-bound — see the crate-level doc's "Sync, not
/// async" section. Cost is dominated by the autocorrelation search
/// (roughly `(max_bpm - min_bpm)`-proportional lags, each an O(envelope
/// length) sum), which for the default 60-200 BPM range over a 10-minute
/// clip at the working sample rate runs in low tens of milliseconds on
/// modern hardware — call from a blocking-safe context if invoking from
/// inside an async runtime.
pub fn detect_tempo(signal: &MonoSignal, min_bpm: f64, max_bpm: f64) -> Option<TempoEstimate> {
    if min_bpm <= 0.0 || max_bpm <= 0.0 || min_bpm >= max_bpm {
        return None;
    }
    let envelope = spectral_flux_onset_envelope(signal);
    let fps = onset_fps(signal.sample_rate);
    detect_tempo_from_envelope(&envelope, fps, min_bpm, max_bpm)
}

/// Core algorithm, separated from [`detect_tempo`] so the segment-
/// agreement check below can run the identical winner-picking logic
/// (minus the outer confidence bookkeeping) over sub-windows of the same
/// envelope without going back through STFT/onset computation.
fn detect_tempo_from_envelope(
    envelope: &[f32],
    fps: f64,
    min_bpm: f64,
    max_bpm: f64,
) -> Option<TempoEstimate> {
    let tau_min = bpm_to_tau(max_bpm, fps).floor().max(1.0) as usize; // fastest tempo -> smallest lag
    let tau_max = bpm_to_tau(min_bpm, fps).ceil() as usize; // slowest tempo -> largest lag
    if tau_max < tau_min {
        return None;
    }

    // Compute the autocorrelation table over a 1-lag-padded range so
    // the parabolic refinement always has real neighbours to interpolate
    // against, even when the winner sits at the very edge of the
    // requested range.
    let search_lo = tau_min.saturating_sub(1).max(1);
    let search_hi = (tau_max + 1).min(envelope.len().saturating_sub(1));
    if search_hi < search_lo {
        return None;
    }
    let table = normalised_autocorrelation(envelope, search_lo, search_hi)?;

    let winner_lo = tau_min.max(search_lo);
    let winner_hi = tau_max.min(search_hi);
    if winner_hi < winner_lo {
        return None;
    }
    let winner = pick_peak(&table, fps, winner_lo, winner_hi)?;

    let refined_tau = refine_tau(&table, winner.tau);
    // Keep the answer inside the range the caller asked for.
    //
    // Two things push outward before this point: the search converts the
    // requested tempo range into whole frames and rounds outward so a
    // tempo sitting between two frames is not missed, and the refinement
    // then fits a curve through the winning frame and its neighbours,
    // which can land beyond it. Either can carry the result past the
    // boundary.
    //
    // Without this, asking for 100 to 140 could hand back something
    // outside that, which makes the setting a suggestion rather than a
    // limit — and a caller who narrowed the range did so for a reason.
    let bpm = tau_to_bpm(refined_tau, fps).clamp(min_bpm, max_bpm);

    // Prominence: how far the winner's RAW autocorrelation stands above
    // the average across the searched range.
    let mean_r_hat = mean_over(&table, winner_lo, winner_hi);
    let prominence = if (1.0 - mean_r_hat).abs() < 1e-9 {
        // Degenerate case: the search range's mean r_hat is ~1.0, which
        // would divide by ~zero below. This only happens when every
        // candidate lag is nearly perfectly correlated (e.g. an
        // extremely short, still-settling envelope) — treat it as
        // maximally prominent rather than producing a wild ratio.
        1.0
    } else {
        ((winner.r_hat - mean_r_hat) / (1.0 - mean_r_hat)).clamp(0.0, 1.0)
    };

    let rival = find_rival(&table, &winner, winner_lo, winner_hi, fps);
    // Judged on the RAW correlation, never the prior-weighted score.
    //
    // This is the rule this module states at the top, and it was being
    // broken here. Comparing weighted scores lets the pull toward common
    // tempos make a close call look decisive: two candidates the audio
    // barely distinguishes come out far apart once one of them has been
    // favoured for sitting near 120 beats per minute.
    //
    // The weighting exists to settle a tie the audio genuinely cannot —
    // an exactly periodic signal really is both 70 and 140 — and settling
    // it is all it may do. Letting it also decide how SURE we are means
    // reporting confidence in a coin flip, which is the one outcome this
    // whole design is meant to avoid.
    //
    // The weighted scores still choose the winner, and are still reported
    // as diagnostics.
    let ambiguity = ambiguity_factor(winner.r_hat, rival.as_ref());

    let segment_agreement_value = segment_agreement(envelope, fps, min_bpm, max_bpm, bpm);

    let confidence = prominence * ambiguity * segment_agreement_value;

    Some(TempoEstimate {
        bpm,
        confidence,
        prominence,
        segment_agreement: segment_agreement_value,
        rival_bpm: rival.as_ref().map(|r| tau_to_bpm(r.tau as f64, fps)),
        rival_score: rival.as_ref().map(|r| r.score),
    })
}

/// Convert a tempo in BPM to a lag in onset-envelope frames, at `fps`
/// frames/second: `tau = 60 * fps / bpm`.
fn bpm_to_tau(bpm: f64, fps: f64) -> f64 {
    60.0 * fps / bpm
}

/// The inverse of [`bpm_to_tau`]: `bpm = 60 * fps / tau`.
fn tau_to_bpm(tau: f64, fps: f64) -> f64 {
    60.0 * fps / tau
}

/// A candidate lag along with its raw and prior-weighted scores.
#[derive(Clone, Copy)]
struct PeakPick {
    tau: usize,
    /// Raw, un-weighted normalised autocorrelation at `tau` — the only
    /// number confidence is ever computed from.
    r_hat: f64,
    /// `r_hat * prior_weight(bpm(tau))` — what the winner is chosen by.
    score: f64,
}

/// Autocorrelation values for a contiguous range of lags, indexed from
/// an arbitrary starting lag rather than from zero (the search range
/// this crate cares about never includes tau=0, so storing a dense
/// zero-based array would waste the low end of it).
struct AutocorrTable {
    lo: usize,
    values: Vec<f64>,
}

impl AutocorrTable {
    /// The normalised autocorrelation at `tau`, or `0.0` if `tau` falls
    /// outside the range this table was computed over. Zero — "no
    /// correlation" — is a safe default for a lag nobody asked to
    /// evaluate, and keeps every caller from having to handle an
    /// `Option` for what is, in practice, always an in-range lookup
    /// (the 1-lag padding in `detect_tempo_from_envelope` guarantees
    /// that for every lookup this module actually performs).
    fn get(&self, tau: usize) -> f64 {
        if tau < self.lo {
            return 0.0;
        }
        self.values.get(tau - self.lo).copied().unwrap_or(0.0)
    }
}

/// Compute `r_hat[tau] = r[tau] / r[0]` for every `tau` in `[lo, hi]`,
/// where `r[tau] = sum_k envelope[k] * envelope[k+tau]` and `r[0] = sum_k
/// envelope[k]^2`.
///
/// Returns `None` when `r[0]` is at or below [`AUTOCORR_ENERGY_FLOOR`] —
/// there is no periodicity to measure in a signal with no onset energy
/// at all, and dividing by ~zero would produce nonsense instead of a
/// meaningful ratio.
fn normalised_autocorrelation(envelope: &[f32], lo: usize, hi: usize) -> Option<AutocorrTable> {
    let r0: f64 = envelope.iter().map(|&o| (o as f64).powi(2)).sum();
    if r0 <= AUTOCORR_ENERGY_FLOOR {
        return None;
    }
    let mut values = Vec::with_capacity(hi - lo + 1);
    for tau in lo..=hi {
        let r: f64 = if tau >= envelope.len() {
            0.0
        } else {
            (0..envelope.len() - tau)
                .map(|k| envelope[k] as f64 * envelope[k + tau] as f64)
                .sum()
        };
        values.push(r / r0);
    }
    Some(AutocorrTable { lo, values })
}

/// The tempo prior: a log-normal curve centred on [`PRIOR_CENTER_BPM`]
/// with width [`PRIOR_SIGMA_OCTAVES`] octaves. See the module doc for
/// why this exists and the hard rule about what it may and may not
/// influence.
fn prior_weight(bpm: f64) -> f64 {
    let z = (bpm / PRIOR_CENTER_BPM).log2() / PRIOR_SIGMA_OCTAVES;
    (-0.5 * z * z).exp()
}

/// The prior-weighted score at `tau`: `r_hat(tau) * prior_weight(bpm(tau))`.
/// This is what a winner is chosen by — see the module doc for why
/// confidence never uses it directly.
fn score_at(table: &AutocorrTable, fps: f64, tau: usize) -> f64 {
    table.get(tau) * prior_weight(tau_to_bpm(tau as f64, fps))
}

/// Find the highest-scoring lag (`r_hat * prior_weight`) in `[lo, hi]`.
fn pick_peak(table: &AutocorrTable, fps: f64, lo: usize, hi: usize) -> Option<PeakPick> {
    let mut best: Option<PeakPick> = None;
    for tau in lo..=hi {
        let r_hat = table.get(tau);
        let score = r_hat * prior_weight(tau_to_bpm(tau as f64, fps));
        if best.is_none_or(|b| score > b.score) {
            best = Some(PeakPick { tau, r_hat, score });
        }
    }
    best
}

/// Whether `tau`'s score is a local maximum — at least as high as both
/// of its immediate neighbours' scores.
///
/// This is what tells a genuinely separate candidate tempo (a real,
/// distinct bump in the score curve — the classic case being a real
/// peak at the octave-related lag) apart from a lag that simply sits on
/// the shoulder of the SAME peak the winner sits on. A correlation peak
/// from a real periodic signal is never a single-lag spike; it has
/// width, and the immediate neighbours of the winning lag score only
/// slightly lower than the winner for the mundane reason that they are
/// measuring almost the same period, not because they are independent
/// evidence for a different tempo. Without this check, `find_rival`
/// below would treat the winner's own shoulder as its own rival on
/// almost every clean, unambiguous click track — which is exactly the
/// failure this function exists to prevent (found while calibrating the
/// tempo tests: a lag one frame away from a clean 90 BPM winner, itself
/// no more than the winner's own shoulder, was crashing confidence from
/// ~0.88 down to ~0.16).
fn is_local_score_maximum(table: &AutocorrTable, fps: f64, tau: usize) -> bool {
    let here = score_at(table, fps, tau);
    let left = if tau == 0 {
        f64::MIN
    } else {
        score_at(table, fps, tau - 1)
    };
    let right = score_at(table, fps, tau + 1);
    here >= left && here >= right
}

/// Refine an integer-lag winner to fractional-lag precision by fitting a
/// parabola through `r_hat` at `tau-1`, `tau`, `tau+1` and taking its
/// vertex.
///
/// Without this, the tempo estimate can only ever land on the discrete
/// BPM values an integer lag produces — near 120 BPM at the working
/// sample rate, adjacent integer lags are roughly 1.4 BPM apart, which
/// would show up directly as quantisation error against the ±1.0 BPM
/// tolerance the click-track tests require.
fn refine_tau(table: &AutocorrTable, tau: usize) -> f64 {
    let y0 = table.get(tau.saturating_sub(1));
    let y1 = table.get(tau);
    let y2 = table.get(tau + 1);
    let denom = y0 - 2.0 * y1 + y2;
    if denom.abs() < 1e-12 {
        return tau as f64;
    }
    let delta = (0.5 * (y0 - y2) / denom).clamp(-1.0, 1.0);
    tau as f64 + delta
}

/// Find the strongest rival to `winner` among the other CANDIDATE PEAKS
/// in `[lo, hi]` — genuinely separate local maxima of the score curve,
/// never a lag that merely sits on the shoulder of the winner's own
/// peak (see [`is_local_score_maximum`], and the module doc's "the
/// prior breaks ties" discussion for the harmonic-forgiveness rule
/// applied below: a slower harmonic (2x/3x/4x the winner's lag, within
/// [`HARMONIC_TOLERANCE`]) is skipped UNLESS its raw `r_hat` beats the
/// winner's by more than [`HARMONIC_OVERRULE_FACTOR`] — meaning the
/// prior, not the data, picked the winner over it. A faster subdivision
/// (any lag smaller than the winner's) is never exempted; the periodic-
/// signal argument that justifies forgiving slower harmonics has no
/// mirror image at faster lags).
fn find_rival(
    table: &AutocorrTable,
    winner: &PeakPick,
    lo: usize,
    hi: usize,
    fps: f64,
) -> Option<PeakPick> {
    let mut best: Option<PeakPick> = None;
    for tau in lo..=hi {
        if tau == winner.tau || !is_local_score_maximum(table, fps, tau) {
            continue;
        }
        let r_hat = table.get(tau);

        if tau > winner.tau {
            let ratio = tau as f64 / winner.tau as f64;
            let nearest_multiple = ratio.round();
            let is_forgivable_harmonic = (2.0..=4.0).contains(&nearest_multiple)
                && ((ratio - nearest_multiple).abs() / nearest_multiple) <= HARMONIC_TOLERANCE
                && r_hat <= winner.r_hat * HARMONIC_OVERRULE_FACTOR;
            if is_forgivable_harmonic {
                continue;
            }
        }

        let score = score_at(table, fps, tau);
        if best.is_none_or(|b| score > b.score) {
            best = Some(PeakPick { tau, r_hat, score });
        }
    }
    best
}

/// The ambiguity confidence factor: `clamp((1 - rival.score /
/// winner.score) / AMBIGUITY_SPREAD, 0, 1)`, or `1.0` (no ambiguity) when
/// there is no rival at all.
fn ambiguity_factor(winner_r_hat: f64, rival: Option<&PeakPick>) -> f64 {
    let Some(rival) = rival else {
        return 1.0;
    };
    if winner_r_hat.abs() < f64::EPSILON {
        // The "winner" barely scored anything at all — there is nothing
        // for a rival's score to be meaningfully compared against.
        // Treating this as maximal ambiguity (rather than dividing by
        // ~zero) is the safe default: it can only ever pull an already
        // near-baseless confidence figure further down, never inflate
        // it.
        return 0.0;
    }
    ((1.0 - rival.r_hat / winner_r_hat) / AMBIGUITY_SPREAD).clamp(0.0, 1.0)
}

/// Mean of `table`'s raw `r_hat` values over `[lo, hi]`.
fn mean_over(table: &AutocorrTable, lo: usize, hi: usize) -> f64 {
    let sum: f64 = (lo..=hi).map(|tau| table.get(tau)).sum();
    sum / (hi - lo + 1) as f64
}

/// Segment agreement: split `envelope` into overlapping windows, run an
/// independent (unrefined) peak-pick on each, and report the fraction
/// whose own local BPM lands within [`SEGMENT_AGREEMENT_TOLERANCE`] of
/// `global_bpm`.
///
/// Segments too quiet to produce any usable local estimate (their own
/// autocorrelation energy floor trips) are excluded from both the
/// numerator and denominator — there is nothing to agree OR disagree
/// with a silent stretch about.
///
/// Fewer than two usable windows (a short clip, or a clip mostly made of
/// segments too quiet to analyse) returns `1.0`: there isn't enough
/// independent evidence to detect disagreement, and treating that as
/// "fully agrees" is the choice that doesn't penalise short-but-valid
/// input for a check that fundamentally needs multiple windows to say
/// anything.
fn segment_agreement(
    envelope: &[f32],
    fps: f64,
    min_bpm: f64,
    max_bpm: f64,
    global_bpm: f64,
) -> f64 {
    let window_frames = (SEGMENT_WINDOW_SECONDS * fps).round().max(1.0) as usize;
    let hop_frames = (SEGMENT_HOP_SECONDS * fps).round().max(1.0) as usize;

    if envelope.len() < window_frames {
        return 1.0;
    }

    let mut windows = 0usize;
    let mut agreeing = 0usize;
    let mut start = 0usize;
    while start + window_frames <= envelope.len() {
        let segment = &envelope[start..start + window_frames];
        if let Some(local_bpm) = pick_segment_bpm(segment, fps, min_bpm, max_bpm) {
            windows += 1;
            if ((local_bpm - global_bpm) / global_bpm).abs() <= SEGMENT_AGREEMENT_TOLERANCE {
                agreeing += 1;
            }
        }
        start += hop_frames;
    }

    if windows < 2 {
        1.0
    } else {
        agreeing as f64 / windows as f64
    }
}

/// A simplified, unrefined peak-pick over one segment — used only by
/// [`segment_agreement`], which just needs "roughly what tempo does this
/// slice look like", not a fully refined, confidence-scored estimate.
fn pick_segment_bpm(segment: &[f32], fps: f64, min_bpm: f64, max_bpm: f64) -> Option<f64> {
    let tau_min = bpm_to_tau(max_bpm, fps).floor().max(1.0) as usize;
    let tau_max = bpm_to_tau(min_bpm, fps).ceil() as usize;
    if tau_max >= segment.len() {
        return None;
    }
    let table = normalised_autocorrelation(segment, tau_min, tau_max)?;
    let winner = pick_peak(&table, fps, tau_min, tau_max)?;
    Some(tau_to_bpm(winner.tau as f64, fps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_signals::{click_track, silence, to_working_signal, white_noise};
    use crate::{DEFAULT_MAX_BPM, DEFAULT_MIN_BPM};

    fn build_click_signal(
        bpm: f64,
        seconds: f64,
        accent_every: usize,
        offbeat_level: f32,
        native_rate: u32,
    ) -> MonoSignal {
        let native = click_track(bpm, seconds, accent_every, offbeat_level, native_rate);
        to_working_signal(native, native_rate)
    }

    fn assert_click_tempo(bpm: f64, min_confidence: f64) {
        let signal = build_click_signal(bpm, 20.0, 4, 0.0, 44_100);
        let est = detect_tempo(&signal, DEFAULT_MIN_BPM, DEFAULT_MAX_BPM)
            .expect("expected a tempo estimate");
        assert!(
            (est.bpm - bpm).abs() <= 1.0,
            "expected bpm within 1.0 of {bpm}, got {}",
            est.bpm
        );
        assert!(
            est.confidence >= min_confidence,
            "expected confidence >= {min_confidence} for a clean {bpm} bpm click track, got {}",
            est.confidence
        );
    }

    // ----------------------------------------------------------
    // Basic BPM sweep
    // ----------------------------------------------------------

    #[test]
    fn detects_60_bpm() {
        assert_click_tempo(60.0, 0.8);
    }
    #[test]
    fn detects_90_bpm() {
        assert_click_tempo(90.0, 0.8);
    }
    #[test]
    fn detects_120_bpm() {
        assert_click_tempo(120.0, 0.8);
    }
    #[test]
    fn detects_128_bpm() {
        assert_click_tempo(128.0, 0.8);
    }
    #[test]
    fn detects_140_bpm() {
        assert_click_tempo(140.0, 0.8);
    }
    #[test]
    fn detects_174_bpm() {
        assert_click_tempo(174.0, 0.8);
    }

    // ----------------------------------------------------------
    // Cross-sample-rate equivalence
    // ----------------------------------------------------------

    #[test]
    fn detection_is_consistent_across_source_sample_rates() {
        // Exercises the /8 (96000 -> 12000) and /4 (48000 -> 12000)
        // decimation paths, not just the /4 (44100 -> 11025) path every
        // other test in this file goes through.
        let bpm = 120.0;
        let sig_44100 = build_click_signal(bpm, 20.0, 4, 0.0, 44_100);
        let sig_48000 = build_click_signal(bpm, 20.0, 4, 0.0, 48_000);
        let sig_96000 = build_click_signal(bpm, 20.0, 4, 0.0, 96_000);
        assert_eq!(sig_44100.sample_rate, 11_025);
        assert_eq!(sig_48000.sample_rate, 12_000);
        assert_eq!(sig_96000.sample_rate, 12_000);

        for signal in [&sig_44100, &sig_48000, &sig_96000] {
            let est = detect_tempo(signal, DEFAULT_MIN_BPM, DEFAULT_MAX_BPM)
                .expect("expected a tempo estimate at every source rate");
            assert!(
                (est.bpm - bpm).abs() <= 1.0,
                "got {} at rate {}",
                est.bpm,
                signal.sample_rate
            );
            assert!(
                est.confidence >= 0.8,
                "got confidence {} at rate {}",
                est.confidence,
                signal.sample_rate
            );
        }
    }

    // ----------------------------------------------------------
    // The 70-vs-140 octave trap
    // ----------------------------------------------------------

    #[test]
    fn a_140_bpm_click_track_is_read_as_140_not_its_70_bpm_sub_harmonic() {
        // A perfectly periodic click train correlates equally at its
        // true period and at every multiple of it -- nothing in the
        // data alone distinguishes 140 from 70. This is exactly the
        // case the tempo prior exists to break: 140 sits much closer to
        // the 120 BPM prior centre (in log2 space) than 70 does, so the
        // prior-weighted winner should land on 140.
        let signal = build_click_signal(140.0, 20.0, 4, 0.0, 44_100);
        let est = detect_tempo(&signal, DEFAULT_MIN_BPM, DEFAULT_MAX_BPM)
            .expect("expected a tempo estimate");
        assert!(
            (est.bpm - 140.0).abs() <= 1.0,
            "expected the prior to break the 140-vs-70 tie towards 140, got {}",
            est.bpm
        );
        assert!(est.confidence >= 0.8, "got confidence {}", est.confidence);
    }

    // ----------------------------------------------------------
    // Offbeat energy
    // ----------------------------------------------------------

    #[test]
    fn light_offbeats_do_not_disturb_a_70_bpm_reading() {
        let signal = build_click_signal(70.0, 20.0, 4, 0.15, 44_100);
        let est = detect_tempo(&signal, DEFAULT_MIN_BPM, DEFAULT_MAX_BPM)
            .expect("expected a tempo estimate");
        assert!((est.bpm - 70.0).abs() <= 1.0, "got {}", est.bpm);
        assert!(est.confidence >= 0.7, "got confidence {}", est.confidence);
    }

    #[test]
    fn heavy_offbeats_create_genuine_low_confidence_octave_ambiguity() {
        // At 50% offbeat level there is real, comparably-strong onset
        // energy at both the true period and half of it -- this is
        // exactly the situation the ambiguity factor exists to detect.
        // Which octave numerically wins is not the point of this test;
        // the confidence dropping IS.
        let signal = build_click_signal(70.0, 20.0, 4, 0.5, 44_100);
        let est = detect_tempo(&signal, DEFAULT_MIN_BPM, DEFAULT_MAX_BPM)
            .expect("expected a tempo estimate");
        assert!(
            (est.bpm - 70.0).abs() <= 1.0 || (est.bpm - 140.0).abs() <= 1.0,
            "expected either the 70 or 140 bpm reading, got {}",
            est.bpm
        );
        assert!(
            est.confidence <= 0.5,
            "expected low confidence from genuine octave ambiguity, got {} (bpm {})",
            est.confidence,
            est.bpm
        );
    }

    // ----------------------------------------------------------
    // Tempo change mid-track
    // ----------------------------------------------------------

    #[test]
    fn a_tempo_change_partway_through_lowers_confidence_via_segment_disagreement() {
        let native_rate = 44_100u32;
        let mut native = click_track(120.0, 12.0, 4, 0.0, native_rate);
        native.extend(click_track(150.0, 12.0, 4, 0.0, native_rate));
        let signal = to_working_signal(native, native_rate);

        let est = detect_tempo(&signal, DEFAULT_MIN_BPM, DEFAULT_MAX_BPM)
            .expect("expected a tempo estimate");
        assert!(
            est.confidence <= 0.5,
            "expected segment disagreement to lower confidence for a mid-track tempo change, got {} (bpm {}, segment_agreement {})",
            est.confidence, est.bpm, est.segment_agreement
        );
    }

    // ----------------------------------------------------------
    // Noise and silence
    // ----------------------------------------------------------

    #[test]
    fn white_noise_has_low_confidence_but_still_returns_a_result() {
        let native = white_noise(20.0, 44_100);
        let signal = to_working_signal(native, 44_100);
        let est = detect_tempo(&signal, DEFAULT_MIN_BPM, DEFAULT_MAX_BPM)
            .expect("white noise has real (if unstructured) onset energy, so this should be Some");
        assert!(
            est.confidence <= 0.2,
            "expected low confidence for white noise, got {}",
            est.confidence
        );
    }

    #[test]
    fn silence_yields_no_tempo_estimate() {
        let native = silence(20.0, 44_100);
        let signal = to_working_signal(native, 44_100);
        assert!(
            detect_tempo(&signal, DEFAULT_MIN_BPM, DEFAULT_MAX_BPM).is_none(),
            "silence has zero onset energy -- there is nothing to report a tempo about"
        );
    }

    // ----------------------------------------------------------
    // A search range that excludes the true tempo
    // ----------------------------------------------------------

    #[test]
    fn a_search_range_excluding_the_true_tempo_does_not_return_a_falsely_confident_answer() {
        // The brief this crate implements describes this case as
        // resolving to "None, or the 180 harmonic with low confidence".
        // A literal 180 BPM result is not reachable here: detect_tempo's
        // own contract (and its winner-selection code) restricts the
        // returned bpm to lie within [min_bpm, max_bpm] by construction
        // -- 180 is outside [100, 140] and can never be the winner of a
        // search confined to that range. What IS testable, and what
        // actually matters, is the underlying claim: searching a range
        // that excludes a click track's true 90 BPM period must not
        // produce a falsely confident wrong answer. Either no candidate
        // clears the bar (None), or one does but with visibly low
        // confidence.
        let signal = build_click_signal(90.0, 20.0, 4, 0.0, 44_100);
        if let Some(est) = detect_tempo(&signal, 100.0, 140.0) {
            assert!(
                est.confidence <= 0.5,
                "expected low confidence for a tempo range that excludes the click track's true period, got bpm={} confidence={}",
                est.bpm, est.confidence
            );
        }
    }

    // ----------------------------------------------------------
    // Argument validation
    // ----------------------------------------------------------

    #[test]
    fn invalid_bpm_range_returns_none_rather_than_panicking() {
        let signal = build_click_signal(120.0, 6.0, 4, 0.0, 44_100);
        assert!(detect_tempo(&signal, 0.0, 200.0).is_none());
        assert!(detect_tempo(&signal, 200.0, 60.0).is_none());
        assert!(detect_tempo(&signal, 120.0, 120.0).is_none());
    }

    // ----------------------------------------------------------
    // Internal helpers
    // ----------------------------------------------------------

    #[test]
    fn bpm_tau_round_trip() {
        let fps = 86.132_8;
        for bpm in [60.0, 90.0, 120.0, 174.0, 200.0] {
            let tau = bpm_to_tau(bpm, fps);
            let back = tau_to_bpm(tau, fps);
            assert!((back - bpm).abs() < 1e-6);
        }
    }

    #[test]
    fn prior_weight_peaks_at_centre_and_is_symmetric_in_log_space() {
        let at_centre = prior_weight(PRIOR_CENTER_BPM);
        assert!(
            (at_centre - 1.0).abs() < 1e-9,
            "expected weight 1.0 exactly at the centre"
        );
        // 60 and 240 are both exactly one octave from 120 -- the prior
        // should weight them identically.
        let below = prior_weight(60.0);
        let above = prior_weight(240.0);
        assert!(
            (below - above).abs() < 1e-9,
            "expected symmetric weighting in log2 space"
        );
        assert!(below < at_centre);
    }
    // ── Two faults an independent review found, both now pinned ─────────
    //
    // Neither had a test, which is why both survived. These are written to
    // fail if either comes back.

    #[test]
    fn the_tempo_preference_cannot_make_a_close_call_look_certain() {
        // The rule this module states at the top: the pull toward common
        // tempos settles a tie the audio cannot, and does nothing else. It
        // must never decide how SURE we are.
        //
        // Here the audio barely separates two candidates — 0.80 against
        // 0.78, a 2% gap — but the winner sits near 120 beats per minute
        // and so carries a much better weighted score. Judged on the
        // weighted scores this looks decisive; judged on the audio it is
        // very nearly a coin flip, and the answer must reflect that.
        let winner_r_hat = 0.80;
        let rival = PeakPick {
            tau: 40,
            r_hat: 0.78,
            // Weighted far below the winner, purely by the preference.
            score: 0.40,
        };

        let honest = ambiguity_factor(winner_r_hat, Some(&rival));
        assert!(
            honest < 0.2,
            "a 2% gap in the audio must read as ambiguous, got {honest}"
        );

        // What the fault produced: comparing the weighted scores instead.
        let flattering = ((1.0 - rival.score / 0.95) / AMBIGUITY_SPREAD).clamp(0.0, 1.0);
        assert!(
            flattering > honest,
            "this pins the difference — weighting made it look better than the audio warrants"
        );
    }

    #[test]
    fn no_rival_at_all_still_means_no_doubt() {
        // The other side of the same function, so the fix above cannot
        // have quietly made everything ambiguous.
        assert_eq!(ambiguity_factor(0.9, None), 1.0);
    }

    #[test]
    fn the_answer_stays_inside_the_range_that_was_asked_for() {
        // Two things push outward before the answer is formed: the search
        // rounds the requested range outward in whole frames so a tempo
        // between two of them is not missed, and the refinement then fits
        // a curve that can land beyond the winning frame. Either can carry
        // the result past the boundary, which would make the setting a
        // suggestion rather than a limit.
        // These exact cases were found by trying many combinations with the
        // limit removed, and every one of them escaped: the true tempo sits
        // just OUTSIDE the requested range, the search still reaches it
        // because the range was rounded outward into whole frames, and the
        // refinement then lands the answer beyond the boundary. A 120 track
        // asked for 121 to 200 came back as 120.06.
        //
        // Chosen deliberately rather than guessed. An earlier version of this
        // test used ranges that happened not to trigger it, and so passed
        // whether the limit was applied or not — proving nothing.
        let sample_rate = 44_100;
        for (bpm, lo, hi) in [
            (120.0, 121.0, 200.0),
            (120.0, 60.0, 119.0),
            (90.0, 91.0, 96.0),
            (128.0, 122.0, 127.0),
        ] {
            let signal = build_click_signal(bpm, 20.0, 4, 0.0, sample_rate);
            if let Some(estimate) = detect_tempo(&signal, lo, hi) {
                assert!(
                    estimate.bpm >= lo && estimate.bpm <= hi,
                    "asked for {lo}-{hi} on a {bpm} track and got {}",
                    estimate.bpm
                );
            }
        }
    }
}
