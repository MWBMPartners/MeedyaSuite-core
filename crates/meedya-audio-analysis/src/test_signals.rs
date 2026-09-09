// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Synthetic test fixtures — generated audio only, nothing copyrighted.
//
// Every tempo/key test in this crate needs audio with a KNOWN answer —
// "this is exactly 120 BPM", "this is exactly C major" — which no real
// recording can give you with certainty (even a metronome track has some
// human or hardware jitter). Generating the audio ourselves means the
// expected answer is exact by construction, and it means this crate adds
// zero new dev-dependencies and ships no copyrighted audio anywhere in
// its history.
//
// `#[cfg(test)]` on the module declaration in lib.rs means none of this
// — not even the xorshift RNG — is compiled into a release build of the
// crate. This file grows across the crate's build-out (click/noise/
// silence generators land with onset.rs, the chord-progression generator
// lands with key.rs, the WAV writer lands with decode.rs) rather than
// being written in one shot up front, so nothing here ever sits unused
// between commits.

use meedya_tags_extended::{KeyMode, MusicalKey, Note};

use crate::signal::Decimator;
use crate::MonoSignal;

/// A small, fast, fixed-seed PRNG for generating reproducible noise.
///
/// A real RNG crate would be overkill (and a new dev-dependency) for
/// "make some noise that isn't literally all zeros" — xorshift32 is
/// about the smallest generator that still passes basic randomness
/// tests, and a fixed seed means every test run generates byte-identical
/// audio, so a test failure is reproducible rather than a coin flip.
struct Xorshift32 {
    state: u32,
}

impl Xorshift32 {
    fn new(seed: u32) -> Self {
        // xorshift is undefined (gets stuck at 0 forever) if seeded with
        // 0, so nudge it to something else.
        Self {
            state: if seed == 0 { 0xDEAD_BEEF } else { seed },
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// A uniform sample in `[-1.0, 1.0]`.
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f64 / u32::MAX as f64) as f32 * 2.0 - 1.0
    }
}

/// Add a single 5ms decaying-noise "click" burst into `buf`, starting at
/// `start_time` seconds, scaled by `level`.
///
/// A burst of filtered noise rather than a pure tone because that's what
/// a real drum machine / metronome click actually is — mostly broadband
/// transient energy, not a single frequency — which matters for
/// onset.rs's spectral-flux detector: it should see a sharp jump across
/// many bins at once, the same shape a real percussive onset has.
fn add_click(buf: &mut [f32], start_time: f64, sample_rate: u32, level: f32, rng: &mut Xorshift32) {
    let duration_samples = (0.005 * sample_rate as f64).round() as usize;
    let start_sample = (start_time * sample_rate as f64).round() as usize;
    for i in 0..duration_samples {
        let idx = start_sample + i;
        if idx >= buf.len() {
            break;
        }
        // Exponential decay across the burst so it reads as a "click"
        // (sharp attack, fast decay) rather than a constant-level noise
        // gate switching on and off.
        let decay = (-6.0 * i as f64 / duration_samples as f64).exp() as f32;
        buf[idx] += level * decay * rng.next_f32();
    }
}

/// Generate a click track at `bpm` for `seconds`, at `sample_rate`.
///
/// `accent_every` makes every Nth beat (starting from the first) louder,
/// the way a metronome or drum machine emphasises the downbeat of a bar
/// — periodicity-wise this changes nothing (every beat still lands on
/// the same grid), but it keeps the fixture closer to how real rhythmic
/// audio is structured. `offbeat_level` (0.0 = none) additionally places
/// a second, quieter click exactly halfway between each pair of main
/// beats — this is what the "offbeat ambiguity" tempo tests use to
/// create a signal with real energy at DOUBLE the true tempo, the
/// classic tempo-octave-confusion case.
///
/// `offbeat_level` is applied to the offbeat click's amplitude via its
/// SQUARE, not directly. `onset.rs`'s spectral flux deliberately
/// log-compresses magnitude (`ln(1 + 100*|X|)`) precisely so a quiet
/// onset registers on the same scale as a loud one — which means a
/// linear amplitude fraction does NOT correspond to a proportional flux
/// contribution: at the magnitude range a 5ms noise burst actually
/// produces, log compression is deep enough into its saturating region
/// that even a "quiet" 15%-amplitude offbeat would otherwise show up as
/// a nearly-as-strong onset in flux terms, defeating the point of
/// calling it "light". Squaring keeps 0.0 and 1.0 meaning what they
/// already mean (no offbeat / equal to a main beat) while making the
/// parameter's qualitative meaning honest in between: 0.15 reads as
/// genuinely subtle, 0.5 reads as genuinely comparable — which is what
/// every test that calls this with those two values is actually
/// checking for.
pub(crate) fn click_track(
    bpm: f64,
    seconds: f64,
    accent_every: usize,
    offbeat_level: f32,
    sample_rate: u32,
) -> Vec<f32> {
    let n = (seconds * sample_rate as f64).round() as usize;
    let mut buf = vec![0.0f32; n];
    let mut rng = Xorshift32::new(0x5EED_0001);
    let beat_interval = 60.0 / bpm;

    let mut beat_index = 0usize;
    let mut t = 0.0;
    while t < seconds {
        let accented = accent_every > 0 && beat_index % accent_every == 0;
        let level = if accented { 1.0 } else { 0.75 };
        add_click(&mut buf, t, sample_rate, level, &mut rng);

        if offbeat_level > 0.0 {
            let offbeat_t = t + beat_interval / 2.0;
            if offbeat_t < seconds {
                add_click(
                    &mut buf,
                    offbeat_t,
                    sample_rate,
                    offbeat_level * offbeat_level,
                    &mut rng,
                );
            }
        }

        beat_index += 1;
        t += beat_interval;
    }
    buf
}

/// Generate `seconds` of white noise at `sample_rate`, scaled to avoid
/// clipping-scale amplitude (real white noise sources are rarely at
/// literal full scale, and there's no reason for the test fixture to be
/// either).
pub(crate) fn white_noise(seconds: f64, sample_rate: u32) -> Vec<f32> {
    let n = (seconds * sample_rate as f64).round() as usize;
    let mut rng = Xorshift32::new(0xC0FF_EE01);
    (0..n).map(|_| rng.next_f32() * 0.5).collect()
}

/// `seconds` of digital silence at `sample_rate`.
pub(crate) fn silence(seconds: f64, sample_rate: u32) -> Vec<f32> {
    vec![0.0f32; (seconds * sample_rate as f64).round() as usize]
}

/// Pitch class (0=C .. 11=B) of a [`Note`], matching the semitone
/// ordering `meedya_tags_extended::Note` declares its variants in. A
/// small local duplicate of the mapping `key.rs` also needs (there for
/// real analysis, here for generating test fixtures) rather than a
/// shared dependency — this module is test-only scaffolding and
/// shouldn't reach into another module's internals just to save twelve
/// match arms.
fn pitch_class_of(note: Note) -> i32 {
    match note {
        Note::C => 0,
        Note::CSharp => 1,
        Note::D => 2,
        Note::DSharp => 3,
        Note::E => 4,
        Note::F => 5,
        Note::FSharp => 6,
        Note::G => 7,
        Note::GSharp => 8,
        Note::A => 9,
        Note::ASharp => 10,
        Note::B => 11,
    }
}

/// Convert a MIDI note number (69.0 = A4 = `reference_a_hz`) to a
/// frequency in Hz, relative to an arbitrary tuning reference rather
/// than always 440 Hz — [`chord_progression_tuned`] uses this to
/// generate deliberately mistuned test audio (e.g. A=432).
fn midi_to_freq(midi: f64, reference_a_hz: f64) -> f64 {
    reference_a_hz * 2f64.powf((midi - 69.0) / 12.0)
}

/// Add a harmonically rich tone into `buf`: a fundamental at `freq`
/// plus its 2nd, 3rd and 4th harmonics at -6/-12/-18 dB, scaled by
/// `amplitude`.
///
/// Real instrument tones are never pure sines — they're a fundamental
/// plus a falling series of harmonics — and that harmonic content
/// matters for testing chroma folding specifically: a single pure sine
/// folds into exactly one pitch class with nothing else to corroborate
/// it, which is a much easier (and much less representative) case than
/// real polyphonic, harmonically rich music.
fn add_harmonic_tone(buf: &mut [f32], freq: f64, sample_rate: u32, amplitude: f32) {
    // (harmonic multiple, linear amplitude relative to the fundamental)
    let harmonics: [(f64, f32); 4] = [
        (1.0, 1.0),
        (2.0, 10f32.powf(-6.0 / 20.0)),
        (3.0, 10f32.powf(-12.0 / 20.0)),
        (4.0, 10f32.powf(-18.0 / 20.0)),
    ];
    for (i, sample) in buf.iter_mut().enumerate() {
        let t = i as f64 / sample_rate as f64;
        let mut s = 0.0f32;
        for (mult, amp) in harmonics {
            s += amp * (2.0 * std::f64::consts::PI * freq * mult * t).sin() as f32;
        }
        *sample += amplitude * s;
    }
}

/// Generate a four-bar chord progression in `key`, `seconds` long, tuned
/// to `reference_a_hz` (440.0 for standard tuning).
///
/// Bars: I-IV-V-I for a major key, i-iv-V-i for a minor key. The V
/// (dominant) chord is a MAJOR triad in both cases — standard
/// voice-leading practice (a minor key's dominant is usually borrowed
/// from the harmonic/melodic minor precisely so it resolves strongly to
/// the tonic), and also what makes the major/minor discrimination tests
/// in key.rs meaningful: the leading tone the major-V chord introduces
/// is NOT one of the seven natural-minor scale notes, so a minor-key
/// progression built this way isn't secretly indistinguishable from its
/// relative major.
///
/// Each chord voices three notes (root, third, fifth) as harmonically
/// rich tones via [`add_harmonic_tone`], plus the chord's own root
/// doubled an octave down (a bass-register reinforcement of the root,
/// common in real arrangements) — so every bar has four sounding notes,
/// two of them both being the root.
pub(crate) fn chord_progression_tuned(
    key: MusicalKey,
    seconds: f64,
    reference_a_hz: f64,
    sample_rate: u32,
) -> Vec<f32> {
    let n = (seconds * sample_rate as f64).round() as usize;
    let mut buf = vec![0.0f32; n];
    let tonic_pc = pitch_class_of(key.tonic);

    // Scale-degree offsets (semitones above the key tonic) for each
    // chord in the progression.
    let degrees: [[i32; 3]; 4] = match key.mode {
        KeyMode::Major => [[0, 4, 7], [5, 9, 12], [7, 11, 14], [0, 4, 7]], // I IV V I
        KeyMode::Minor => [[0, 3, 7], [5, 8, 12], [7, 11, 14], [0, 3, 7]], // i iv V i
    };

    let bar_samples = n / degrees.len();
    // Keep enough headroom that summing 4 harmonically-rich voices (3
    // chord tones + the doubled root, each with 4 harmonics) never gets
    // anywhere near clipping.
    let voice_amplitude = 0.12f32;

    for (bar_idx, chord) in degrees.iter().enumerate() {
        let start = bar_idx * bar_samples;
        let len = bar_samples.min(n.saturating_sub(start));
        if len == 0 {
            continue;
        }
        let bar_buf = &mut buf[start..start + len];

        for (voice_index, &degree) in chord.iter().enumerate() {
            let midi = 60.0 + tonic_pc as f64 + degree as f64; // middle-C-ish register
                                                               // `chord` is always laid out [root, third, fifth]. The THIRD
                                                               // is the one note that says whether a chord (and by
                                                               // extension the key) is major or minor -- root and fifth
                                                               // are identical between a key and its parallel opposite
                                                               // mode (A# minor and A# major share the same tonic AND the
                                                               // same fifth; only the third differs). A root-fifth-fifth
                                                               // voicing pattern across a I-IV-V-I skeleton means the root
                                                               // and fifth pitch classes each recur in MULTIPLE chords
                                                               // (the tonic is a chord tone of I and IV; the dominant's
                                                               // root doubles as a passing fifth elsewhere), while the
                                                               // third of the tonic chord specifically only ever appears
                                                               // in the two tonic-chord bars. Left at equal amplitude, that
                                                               // structural imbalance was diluting the very note that
                                                               // distinguishes major from minor enough to blur some
                                                               // parallel-mode pairs (e.g. A# minor vs A# major) under the
                                                               // brief's confidence margin -- so the third is voiced 50%
                                                               // louder than root/fifth, matching how a mixing engineer
                                                               // would actually bring out a chord's defining note rather
                                                               // than burying it.
            let amplitude = if voice_index == 1 {
                voice_amplitude * 1.5
            } else {
                voice_amplitude
            };
            add_harmonic_tone(
                bar_buf,
                midi_to_freq(midi, reference_a_hz),
                sample_rate,
                amplitude,
            );
        }
        // Root, doubled an octave down.
        let root_midi = 60.0 + tonic_pc as f64 + chord[0] as f64 - 12.0;
        add_harmonic_tone(
            bar_buf,
            midi_to_freq(root_midi, reference_a_hz),
            sample_rate,
            voice_amplitude,
        );
    }

    buf
}

/// [`chord_progression_tuned`] at standard A440 tuning, 44100 Hz.
pub(crate) fn chord_progression(key: MusicalKey, seconds: f64) -> Vec<f32> {
    chord_progression_tuned(key, seconds, 440.0, 44_100)
}

/// Run `native` (mono samples at `native_rate`) through the same
/// production decimation path `decode.rs`/`AudioAnalyser` use, returning
/// a [`MonoSignal`] at the resulting working rate.
///
/// Every tempo/key test builds its signal through this helper rather
/// than constructing a `MonoSignal` by hand at a convenient rate,
/// specifically so the constants calibrated in `onset.rs`/`tempo.rs`/
/// `key.rs` (which assume the WORKING rate, not an arbitrary one) are
/// exercised the same way production code exercises them.
pub(crate) fn to_working_signal(native: Vec<f32>, native_rate: u32) -> MonoSignal {
    let mut decimator = Decimator::new(native_rate);
    let samples = decimator.process(&native);
    MonoSignal {
        samples,
        sample_rate: decimator.working_rate(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xorshift_is_deterministic_and_bounded() {
        let mut a = Xorshift32::new(42);
        let mut b = Xorshift32::new(42);
        for _ in 0..100 {
            let (va, vb) = (a.next_f32(), b.next_f32());
            assert_eq!(va, vb, "same seed must give same sequence");
            assert!((-1.0..=1.0).contains(&va));
        }
    }

    #[test]
    fn click_track_has_bursts_separated_by_silence() {
        // 120 BPM = a click every 0.5s, each burst only 5ms long, so the
        // gap between bursts is overwhelmingly silent.
        let sample_rate = 44_100u32;
        let track = click_track(120.0, 2.0, 4, 0.0, sample_rate);

        // The very first click starts at t=0, so the opening samples
        // should have real energy in them.
        assert!(
            track[..100].iter().any(|&s| s != 0.0),
            "expected the first click's burst at the very start of the track"
        );

        // At t=0.1s the first burst (5ms) is long over and the next
        // click (at t=0.5s) is still a long way off — this sample should
        // be exact silence.
        let mid_gap = (0.1 * sample_rate as f64) as usize;
        assert_eq!(
            track[mid_gap], 0.0,
            "expected silence between clicks, found energy at t=0.1s"
        );
    }

    #[test]
    fn to_working_signal_produces_expected_rate() {
        let native = silence(1.0, 44_100);
        let sig = to_working_signal(native, 44_100);
        assert_eq!(sig.sample_rate, 11_025);
    }
}
