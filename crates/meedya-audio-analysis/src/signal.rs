// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Shared front end: turn arbitrary interleaved PCM into a mono signal at
// a low, fixed working sample rate that every later stage builds on.
//
// Two reasons this exists as its own module instead of being inlined
// wherever audio first arrives:
//
// 1. Tempo and key detection both only care about audio well below the
//    Nyquist frequency of any real recording (tempo: roughly 20 Hz to
//    5 kHz; key: roughly 65 Hz to 2 kHz). Doing the STFTs in tempo.rs
//    and key.rs at, say, 96 kHz would spend the vast majority of the
//    FFT's work on frequency content neither of them looks at.
// 2. It has to be safe to run on a Raspberry Pi against a 192 kHz
//    master. Ten minutes of 192 kHz mono audio, buffered whole, is
//    about 460 MB before it is even decimated — a bad idea on a
//    machine with 1-4 GB of RAM. The decimator in this module is
//    written to be fed a chunk (a decoder "packet") at a time and to
//    carry only a fixed, tiny amount of state (30 samples per halving
//    stage) between chunks, so the native-rate signal never has to
//    exist all at once, anywhere.

use std::sync::LazyLock;

/// The lowest sample rate this crate will decimate a signal down to.
///
/// 11025 Hz still carries about 5.5 kHz of usable bandwidth — comfortably
/// more than tempo needs (it only looks at roughly 20 Hz-5 kHz) or key
/// needs (it only looks at roughly 65 Hz-2 kHz, C2 to C7) — while cutting
/// the sample count, and therefore every later stage's CPU cost, by up to
/// 16x relative to a 192 kHz source. It is also a floor rather than a
/// fixed target: a source that already arrives at or below this rate
/// (e.g. 16 kHz voice-grade audio) is passed through completely unchanged
/// rather than being resampled up or down to hit it exactly, since the
/// only thing that matters downstream is that every constant is derived
/// from whatever the real working rate ends up being.
pub const WORKING_RATE_FLOOR_HZ: f64 = 11025.0;

/// Number of taps in each half-band decimation filter (one filter run
/// before every "keep every second sample" step).
///
/// 31 is a classic small-filter size for this job: enough taps to get a
/// usefully narrow transition band and >40 dB of stopband attenuation
/// (see the `decimator_attenuates_aliasing` test) without costing more
/// than a few dozen multiply-adds per output sample — this runs on every
/// sample of every stage, so it has to stay cheap on modest hardware.
pub const DECIMATION_FIR_TAPS: usize = 31;

/// Low-pass cutoff for a decimation stage, expressed as a FRACTION of
/// that stage's own incoming sample rate rather than an absolute Hz
/// value.
///
/// After a halving stage, the new Nyquist frequency is exactly 1/4 of the
/// incoming rate. A cutoff of exactly 0.25 would try to brick-wall right
/// at that edge, which a 31-tap filter cannot do without ringing badly;
/// 0.22 leaves a bit of headroom below the edge for the filter's
/// transition band. Because this is a fraction of the INCOMING rate for
/// whichever stage is running, the same 31 coefficients are correct at
/// every stage of every halving chain, regardless of the signal's actual
/// native sample rate — nothing here is tuned for 44100 or 48000
/// specifically.
pub const DECIMATION_CUTOFF_FRACTION: f64 = 0.22;

/// A single audio channel of samples at a known sample rate.
///
/// Everywhere downstream of the shared front end (onset detection, tempo,
/// chroma, key) works on this type rather than raw interleaved PCM, so
/// none of that code has to think about channel count or native sample
/// rate again — the front end has already collapsed both.
#[derive(Debug, Clone)]
pub struct MonoSignal {
    /// Samples in `[-1.0, 1.0]`-normalised float form (values can exceed
    /// that range slightly after the decimation filter, the same way any
    /// low-pass filter can overshoot on a sharp transient — nothing here
    /// clips or clamps, since clamping would distort the very frequency
    /// content tempo/key analysis reads).
    pub samples: Vec<f32>,
    /// The rate `samples` was recorded at. This is the DECIMATED
    /// ("working") rate, not the original file's native rate.
    pub sample_rate: u32,
}

impl MonoSignal {
    /// Duration of `samples` in seconds, computed from the actual sample
    /// count rather than trusted from anywhere else.
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / self.sample_rate as f64
    }
}

/// Mix interleaved multi-channel PCM down to mono by averaging the
/// channels of each frame.
///
/// A plain average (not e.g. taking just the left channel) is used
/// because tempo and key detection both want the FULL musical content —
/// a mix engineered with, say, lead vocal panned centre and guitars
/// panned hard left/right would lose real information if only one
/// channel were kept.
///
/// `channels == 1` is the common case and returns a copy of `interleaved`
/// unchanged (there is nothing to mix). A `channels` of zero is treated
/// as 1 defensively — it should never happen, but dividing by zero on
/// malformed input would be worse than silently treating it as mono.
pub fn mix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    if channels == 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        // `chunks` can hand back a short final chunk if `interleaved.len()`
        // is not an exact multiple of `channels` (malformed/truncated
        // input) — dividing by the chunk's own length rather than the
        // nominal channel count keeps that partial frame's average
        // correct instead of quietly under-weighting it.
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Compute the decimation factor — always a power of two — that brings
/// `native_rate` down to the lowest rate that is still `>= WORKING_RATE_FLOOR_HZ`.
///
/// Worked examples from the brief this crate implements:
/// 44100 -> /4 = 11025, 48000 -> /4 = 12000, 96000 -> /8 = 12000,
/// 192000 -> /16 = 12000, 22050 -> /2 = 11025, 16000 -> /1 (no-op).
pub fn decimation_factor(native_rate: u32) -> u32 {
    let mut d: u32 = 1;
    while (native_rate as f64) / (d as f64 * 2.0) >= WORKING_RATE_FLOOR_HZ {
        d *= 2;
    }
    d
}

/// The 31-tap decimation kernel, shared by every halving stage of every
/// [`Decimator`] instance.
///
/// It is the same array everywhere because [`DECIMATION_CUTOFF_FRACTION`]
/// is defined relative to each stage's own incoming rate rather than as
/// an absolute frequency — so "the filter for a 44100 Hz stage" and "the
/// filter for a 12000 Hz stage" are mathematically the same filter, just
/// applied to different sample rates. Computed once, lazily, the first
/// time any decimator is constructed, rather than per-instance: it is
/// cheap either way (31 numbers), but there is no reason to redo the
/// trigonometry every time a new stage is built.
static DECIMATION_KERNEL: LazyLock<[f32; DECIMATION_FIR_TAPS]> =
    LazyLock::new(build_decimation_kernel);

/// Design a windowed-sinc low-pass FIR filter: an ideal (infinite,
/// brick-wall) low-pass impulse response, truncated to `DECIMATION_FIR_TAPS`
/// taps and tapered with a Hann window so the truncation doesn't ring.
///
/// Standard formula: `h[n] = 2*fc*sinc(2*fc*(n-M))` for the ideal filter
/// (`M` is the tap-count midpoint, `fc` the cutoff as a fraction of the
/// sample rate), multiplied by a Hann window `w[n]`, then rescaled so the
/// taps sum to exactly 1.0 (unity gain at DC / zero Hz) — without that
/// rescale, floating-point rounding in the sinc/window multiplication
/// would leave the filter's passband gain very slightly off from 1.0,
/// which would show up directly as amplitude loss in the
/// `decimator_preserves_passband_amplitude` test.
fn build_decimation_kernel() -> [f32; DECIMATION_FIR_TAPS] {
    use std::f64::consts::PI;

    let n = DECIMATION_FIR_TAPS;
    // The kernel is symmetric about this midpoint — tap `m` is the
    // "centre" tap. `Decimator::process` below relies on that symmetry
    // (see the comment there) to avoid having to reverse either the
    // kernel or the sample window before the dot product.
    let m = (n - 1) as f64 / 2.0;
    let fc = DECIMATION_CUTOFF_FRACTION;

    let mut taps = [0.0f64; DECIMATION_FIR_TAPS];
    for (i, tap) in taps.iter_mut().enumerate() {
        let x = i as f64 - m;
        let sinc = if x == 0.0 {
            // sinc(0) = 1 by the removable-singularity definition; the
            // ideal low-pass impulse response at its own centre is 2*fc.
            2.0 * fc
        } else {
            (2.0 * PI * fc * x).sin() / (PI * x)
        };
        // Hann window: 0 at both edges, 1 at the centre.
        let hann = 0.5 - 0.5 * (2.0 * PI * i as f64 / (n - 1) as f64).cos();
        *tap = sinc * hann;
    }

    let sum: f64 = taps.iter().sum();
    let mut out = [0.0f32; DECIMATION_FIR_TAPS];
    for (o, t) in out.iter_mut().zip(taps.iter()) {
        *o = (t / sum) as f32;
    }
    out
}

/// One "halve the sample rate" step: a 31-tap low-pass filter followed by
/// dropping every second sample.
///
/// Streaming, not batch: `process` can be called repeatedly with
/// consecutive chunks of a longer signal and produces the same result as
/// calling it once on the whole signal, by carrying two small pieces of
/// state between calls — the trailing `DECIMATION_FIR_TAPS - 1` raw
/// samples (so the filter has the context it needs for the first few
/// samples of the next chunk) and a one-bit "keep or drop" parity (so a
/// chunk boundary landing on an odd sample count doesn't shift which
/// samples get kept). Both together are a fixed, tiny amount of memory —
/// nothing here grows with how much audio has been processed.
struct HalvingStage {
    /// Trailing raw samples from the previous call (or zeros, at the very
    /// start — this is a deliberate zero-padded edge rather than needing
    /// a special first-call code path).
    history: [f32; DECIMATION_FIR_TAPS - 1],
    /// Whether the NEXT filtered sample produced should be kept (true) or
    /// dropped (false), carried across calls to `process`.
    keep_next: bool,
}

impl HalvingStage {
    fn new() -> Self {
        Self {
            history: [0.0; DECIMATION_FIR_TAPS - 1],
            keep_next: true,
        }
    }

    /// Filter and 2:1-decimate one chunk of raw samples.
    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }

        let taps = DECIMATION_FIR_TAPS;
        let hist_len = self.history.len();

        // `extended` = the last `hist_len` samples we've already seen,
        // followed by the new samples. Filtering only ever needs to look
        // backwards (this is a causal FIR), so this window is all the
        // context any output sample in this call needs.
        let mut extended = Vec::with_capacity(hist_len + input.len());
        extended.extend_from_slice(&self.history);
        extended.extend_from_slice(input);

        // Causal FIR: filtered[i] corresponds to input[i], computed from
        // extended[i ..= i + taps - 1]. The kernel is symmetric (tap k
        // equals tap taps-1-k — see build_decimation_kernel), so pairing
        // kernel[k] with window[k] in iteration order gives the identical
        // sum to pairing kernel[k] with the "true" causal partner
        // window[taps-1-k]; that symmetry is what lets this be a plain
        // zip instead of a reversed one.
        let kernel = &*DECIMATION_KERNEL;
        let mut filtered = Vec::with_capacity(input.len());
        for i in 0..input.len() {
            let window = &extended[i..i + taps];
            let acc: f32 = kernel.iter().zip(window.iter()).map(|(h, x)| h * x).sum();
            filtered.push(acc);
        }

        // Carry the trailing `hist_len` raw samples forward for next time.
        let start = extended.len() - hist_len;
        self.history.copy_from_slice(&extended[start..]);

        // Keep every second filtered sample, continuing the parity from
        // wherever the last call left off.
        let mut out = Vec::with_capacity(filtered.len() / 2 + 1);
        for s in filtered {
            if self.keep_next {
                out.push(s);
            }
            self.keep_next = !self.keep_next;
        }
        out
    }
}

/// Streaming decimator: repeatedly halves a mono signal's sample rate
/// until it reaches the lowest rate that is still `>= WORKING_RATE_FLOOR_HZ`.
///
/// Built once per source (it needs to know the native sample rate up
/// front to work out how many halving stages to chain), then fed the
/// signal a chunk at a time via [`Decimator::process`]. See the module
/// doc for why this matters: it is what lets `decode.rs` avoid ever
/// buffering a whole native-rate file in memory.
pub struct Decimator {
    stages: Vec<HalvingStage>,
    working_rate: u32,
}

impl Decimator {
    /// Create a decimator for a signal whose native rate is `native_rate`.
    pub fn new(native_rate: u32) -> Self {
        let factor = decimation_factor(native_rate);
        // `factor` is always a power of two by construction, so its
        // base-2 log (how many halving stages are needed) is exactly its
        // number of trailing zero bits.
        let num_stages = factor.trailing_zeros();
        let stages = (0..num_stages).map(|_| HalvingStage::new()).collect();
        Self {
            stages,
            working_rate: native_rate / factor,
        }
    }

    /// The sample rate this decimator's output is at. Fixed once the
    /// decimator is constructed (derived from the native rate given to
    /// [`Decimator::new`]), and does not change across calls to `process`.
    pub fn working_rate(&self) -> u32 {
        self.working_rate
    }

    /// Run one chunk of mono, native-rate samples through every halving
    /// stage in sequence, returning the decimated result.
    ///
    /// Safe to call repeatedly with consecutive chunks of a longer
    /// signal — see [`HalvingStage::process`] for what state makes that
    /// correct. When no halving is needed at all (the source was already
    /// at or below [`WORKING_RATE_FLOOR_HZ`]), `self.stages` is empty and
    /// this is a true no-op: the input is returned unchanged, not merely
    /// numerically close to it.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let mut buf = input.to_vec();
        for stage in &mut self.stages {
            buf = stage.process(&buf);
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----------------------------------------------------------
    // decimation_factor
    // ----------------------------------------------------------

    #[test]
    fn decimation_factor_matches_worked_examples() {
        assert_eq!(decimation_factor(44_100), 4);
        assert_eq!(decimation_factor(48_000), 4);
        assert_eq!(decimation_factor(96_000), 8);
        assert_eq!(decimation_factor(192_000), 16);
        assert_eq!(decimation_factor(22_050), 2);
        assert_eq!(decimation_factor(16_000), 1);
    }

    // ----------------------------------------------------------
    // mix_to_mono
    // ----------------------------------------------------------

    #[test]
    fn mix_to_mono_averages_stereo() {
        let interleaved = [1.0f32, -1.0, 0.5, 0.5];
        let mono = mix_to_mono(&interleaved, 2);
        assert_eq!(mono, vec![0.0, 0.5]);
    }

    #[test]
    fn mix_to_mono_is_a_no_op_for_mono_input() {
        let interleaved = [0.1f32, 0.2, -0.3];
        assert_eq!(mix_to_mono(&interleaved, 1), interleaved.to_vec());
    }

    // ----------------------------------------------------------
    // Decimator — /1 no-op
    // ----------------------------------------------------------

    #[test]
    fn decimator_is_a_true_no_op_when_no_halving_is_needed() {
        // 16000 Hz needs no halving at all (decimation_factor(16000) == 1).
        let input: Vec<f32> = (0..500).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut dec = Decimator::new(16_000);
        assert_eq!(dec.working_rate(), 16_000);
        let out = dec.process(&input);
        assert_eq!(out, input, "a /1 decimator must return its input unchanged");
    }

    // ----------------------------------------------------------
    // Decimator — passband amplitude preservation
    // ----------------------------------------------------------

    /// Amplitude (via RMS over the settled portion of the signal) of a
    /// pure sine tone, in decibels relative to full scale (a unit-amplitude
    /// sine has RMS = 1/sqrt(2)).
    fn rms_db(samples: &[f32]) -> f64 {
        let rms = (samples.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / samples.len() as f64)
            .sqrt();
        20.0 * rms.log10()
    }

    #[test]
    fn decimator_preserves_passband_amplitude() {
        // A 1 kHz tone at 44100 Hz is well within the passband of every
        // stage of the /4 decimation chain (44100 -> 22050 -> 11025), so
        // it should survive with very little amplitude loss.
        let sample_rate = 44_100u32;
        let seconds = 2.0;
        let freq = 1000.0;
        let n = (seconds * sample_rate as f64) as usize;
        let input: Vec<f32> = (0..n)
            .map(|i| {
                (2.0 * std::f64::consts::PI * freq * i as f64 / sample_rate as f64).sin() as f32
            })
            .collect();

        let mut dec = Decimator::new(sample_rate);
        let out = dec.process(&input);
        assert_eq!(dec.working_rate(), 11_025);

        // Skip the first ~20ms (filter settling / history-buffer warm-up)
        // and measure the rest.
        let skip = out.len() / 20;
        let measured = rms_db(&out[skip..]);
        // A unit-amplitude sine's RMS is 1/sqrt(2) => -3.01 dBFS.
        let expected = 20.0 * (1.0 / std::f64::consts::SQRT_2).log10();
        let loss_db = expected - measured;
        assert!(
            loss_db.abs() < 0.5,
            "expected < 0.5 dB loss for a 1 kHz tone, got {loss_db:.3} dB (measured {measured:.3} dBFS, expected {expected:.3} dBFS)"
        );
    }

    // ----------------------------------------------------------
    // Decimator — anti-aliasing
    // ----------------------------------------------------------

    #[test]
    fn decimator_attenuates_aliasing() {
        // An 8 kHz tone at 44100 Hz, decimated by 4 down to 11025 Hz,
        // would (without anti-alias filtering) fold to |11025 - 8000| =
        // 3025 Hz, at full strength. A correctly filtered decimation
        // should suppress the 8 kHz content well before it gets there —
        // by the time the second halving stage runs (11025's own filter
        // has a cutoff of 0.22*22050 ~= 4851 Hz), 8 kHz is deep in the
        // stopband. This checks the RESULT is quiet, not that a specific
        // frequency is present — with the tone suppressed there should
        // be very little energy left in the output at all.
        let sample_rate = 44_100u32;
        let seconds = 2.0;
        let freq = 8000.0;
        let n = (seconds * sample_rate as f64) as usize;
        let input: Vec<f32> = (0..n)
            .map(|i| {
                (2.0 * std::f64::consts::PI * freq * i as f64 / sample_rate as f64).sin() as f32
            })
            .collect();

        let mut dec = Decimator::new(sample_rate);
        let out = dec.process(&input);

        let skip = out.len() / 20;
        let measured = rms_db(&out[skip..]);
        // The pass-through reference (what an UNattenuated 8kHz tone
        // would measure at, i.e. the same -3.01 dBFS as the passband
        // test) minus 40 dB is the ceiling for how loud the aliased
        // residue is allowed to be.
        let passthrough = 20.0 * (1.0 / std::f64::consts::SQRT_2).log10();
        let attenuation_db = passthrough - measured;
        assert!(
            attenuation_db > 40.0,
            "expected > 40 dB of attenuation for an 8kHz tone aliasing to 3025Hz, got {attenuation_db:.1} dB (measured {measured:.3} dBFS)"
        );
    }

    // ----------------------------------------------------------
    // Decimator — streaming vs. batch equivalence
    // ----------------------------------------------------------

    #[test]
    fn decimator_streaming_matches_batch() {
        // Feeding a signal through in small, unevenly-sized chunks must
        // give bit-for-bit the same result as feeding it through in one
        // call — that equivalence is the entire point of carrying
        // history/parity state between calls instead of requiring the
        // whole signal up front.
        let sample_rate = 48_000u32;
        let n = 5000usize;
        let input: Vec<f32> = (0..n).map(|i| ((i as f32) * 0.037).sin()).collect();

        let mut batch_dec = Decimator::new(sample_rate);
        let batch_out = batch_dec.process(&input);

        let mut streamed_dec = Decimator::new(sample_rate);
        let mut streamed_out = Vec::new();
        // Deliberately uneven chunk sizes, including some smaller than
        // the filter's own tap count, to stress the history handling.
        let chunk_sizes = [1, 7, 3, 500, 13, 29, 4450];
        let mut pos = 0;
        for &size in &chunk_sizes {
            let end = (pos + size).min(input.len());
            if pos >= end {
                continue;
            }
            streamed_out.extend(streamed_dec.process(&input[pos..end]));
            pos = end;
        }

        assert_eq!(batch_out, streamed_out);
    }
}
