// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// meedya-audio-analysis — tempo and musical key detection
// =========================================================
//
// Estimates two things from raw audio that nobody hands us for free:
// beats-per-minute, and the musical key (e.g. "A minor"). Both are
// computed from the audio itself, with nothing downloaded and nothing
// guessed from a filename or a tag that might already be wrong.
//
// ## What this crate does NOT do
//
// It never reads or writes tags. Analysis results come back as plain
// structs; writing them into a file's metadata (an M4A `tmpo` atom, an
// ID3 `TBPM` frame, a `TKEY`/`INITIALKEY` field, whatever the target
// format wants) is the consumer's job, the same boundary
// `meedya-fingerprint` draws around fingerprinting and loudness
// analysis. This split exists because different apps write to
// different tag formats, and because it keeps "detect a number" and
// "decide whether to overwrite a tag the user might have set on
// purpose" as two separate decisions made by two different pieces of
// code.
//
// That second point matters enough to say plainly: **this crate will
// happily re-analyse a file that already has a tempo or key tag, and
// it has no opinion on whether the result should overwrite what is
// already there.** The right policy — used by every consumer we know
// of — is "never overwrite an existing tag automatically." Enforcing
// that means reading the existing tag FIRST, and skipping the call
// into this crate entirely if a value is already present. That also
// saves the work: analysis is real CPU time (see `# Blocking` on
// [`AudioAnalyser::analyse_file`] and [`AudioAnalyser::analyse_samples`]
// below), so a consumer that checks the tag first, before analysing,
// avoids paying for an answer it was never going to use.
//
// ## Sync, not async
//
// Every public entry point here is a plain, blocking function, not
// `async fn`. That is a deliberate choice, not an oversight — an `async
// fn` that never actually awaits anything (because there is nothing to
// wait on; this is pure CPU work from the first sample to the last)
// just tells the caller "this is cheap to await" while it in fact
// blocks whatever thread runs it for the entire computation. On a tokio
// runtime that thread is a worker thread other tasks are waiting on, so
// the more honest and more useful shape is a sync function plus a
// `# Blocking` doc note telling the caller to hop onto a blocking
// thread pool (`tokio::task::spawn_blocking` or equivalent) if it's
// calling from inside an async context. `meedya-fingerprint`'s
// `chromaprint::generate_fingerprint` makes the same call for the same
// reason; this crate follows it on purpose so the workspace has one
// answer to "is audio analysis sync or async", not two.
//
// ## `MusicalKey`, not `String`
//
// Detected keys come back as [`meedya_tags_extended::MusicalKey`], the
// same type the tag writer already consumes and that already
// round-trips Camelot notation ("8A"), Open Key notation ("8d") and
// traditional notation ("Am") without losing information. Inventing a
// second key type here — even a "just for this crate" `String` — would
// only guarantee the two eventually drift out of sync with no compiler
// error to catch it.

pub mod chroma;
#[cfg(feature = "decode")]
pub mod decode;
mod error;
pub mod key;
pub mod onset;
pub mod signal;
pub mod stft;
pub mod tempo;
#[cfg(test)]
mod test_signals;

use serde::{Deserialize, Serialize};

pub use error::AnalysisError;
pub use key::{detect_key, KeyEstimate};
pub use signal::MonoSignal;
pub use tempo::{detect_tempo, TempoEstimate};

/// Default lower bound of the tempo search range, in beats per minute.
///
/// 60 BPM is a reasonable floor for anything commercially released as
/// "a song" — genuinely slower material (ambient drones, some classical
/// adagios) either has no clear beat at all, in which case a low
/// confidence score is the correct answer regardless of where the
/// search range starts, or is intentionally outside this crate's scope.
pub const DEFAULT_MIN_BPM: f64 = 60.0;

/// Default upper bound of the tempo search range, in beats per minute.
///
/// 200 BPM comfortably covers everything from ballads up through drum
/// and bass / hardcore territory. Faster genuine tempos exist, but
/// autocorrelation-based detection has an inherent ambiguity between a
/// tempo and its integer multiples/divisors (a 240 BPM track and a 120
/// BPM track built from the same note durations look identical to this
/// method) — widening the ceiling past this point buys very little for
/// how much more sub-octave ambiguity it invites.
pub const DEFAULT_MAX_BPM: f64 = 200.0;

/// Default cap on how much audio is analysed, in seconds, from the
/// start of the track.
///
/// Ten minutes is already far more than tempo or key needs (both are
/// designed to work from a handful of seconds), but it protects against
/// two real cases: a DJ mix or podcast accidentally handed to this
/// crate, and a track whose tempo genuinely changes partway through
/// (analysing only the first chunk gives a stable, explainable answer
/// instead of an average across two different songs).
pub const DEFAULT_MAX_ANALYSIS_SECONDS: f64 = 600.0;

/// Minimum amount of audio, in seconds, this crate is willing to say
/// anything about at all.
///
/// Below this, an autocorrelation-based tempo estimate has too few
/// candidate periods in its search range to distinguish a real beat
/// from noise, and a chroma average has too few STFT frames to be
/// anything but sample noise. This is a hard floor enforced once, up
/// front, by [`AudioAnalyser`] — [`tempo::detect_tempo`] and
/// [`key::detect_key`] themselves don't re-check it (they return
/// `Option`, not `Result`, and have their own, independent reasons to
/// return `None` — see each module's doc).
pub const MIN_ANALYSIS_SECONDS: f64 = 5.0;

/// The result of analysing one piece of audio.
///
/// `tempo`/`key` are independently `Option` — either can legitimately
/// come back `None` (ambient music has no clear tempo; a single
/// sustained drone has no clear key) without that saying anything about
/// the other.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AudioAnalysis {
    /// Tempo estimate, or `None` if [`AudioAnalyser::with_tempo`] was
    /// set to `false`, or if [`detect_tempo`] itself found nothing
    /// reportable.
    pub tempo: Option<TempoEstimate>,
    /// Key estimate, or `None` if [`AudioAnalyser::with_key`] was set to
    /// `false`, or if [`detect_key`] itself found nothing reportable.
    pub key: Option<KeyEstimate>,
    /// How much audio was actually analysed, in seconds — the full
    /// duration, or the configured max-duration cap if the input was
    /// longer than that.
    pub analysed_seconds: f64,
    /// Whether `analysed_seconds` is less than the input's actual full
    /// duration because the max-duration cap was hit.
    pub truncated: bool,
    /// The DECIMATED working sample rate analysis actually ran at (see
    /// `signal.rs`) — not the source's native sample rate. Exposed
    /// mainly for diagnostics; nothing about interpreting `tempo`/`key`
    /// requires knowing this.
    pub working_sample_rate: u32,
}

/// Builder for tempo/key analysis. `AudioAnalyser::new().analyse_file(path)`
/// (or [`AudioAnalyser::default`], identical) is the common case; the
/// `with_*` methods override defaults for callers that need to.
pub struct AudioAnalyser {
    max_analysis_seconds: f64,
    min_bpm: f64,
    max_bpm: f64,
    want_tempo: bool,
    want_key: bool,
}

impl Default for AudioAnalyser {
    fn default() -> Self {
        Self {
            max_analysis_seconds: DEFAULT_MAX_ANALYSIS_SECONDS,
            min_bpm: DEFAULT_MIN_BPM,
            max_bpm: DEFAULT_MAX_BPM,
            want_tempo: true,
            want_key: true,
        }
    }
}

impl AudioAnalyser {
    /// Equivalent to [`AudioAnalyser::default`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Override how much audio (from the start) is analysed. Default:
    /// [`DEFAULT_MAX_ANALYSIS_SECONDS`].
    pub fn with_max_duration(mut self, seconds: f64) -> Self {
        self.max_analysis_seconds = seconds;
        self
    }

    /// Override the tempo search range. Default: [`DEFAULT_MIN_BPM`] to
    /// [`DEFAULT_MAX_BPM`].
    pub fn with_tempo_range(mut self, min_bpm: f64, max_bpm: f64) -> Self {
        self.min_bpm = min_bpm;
        self.max_bpm = max_bpm;
        self
    }

    /// Enable or disable tempo detection. Default: `true`. A caller that
    /// only wants the key (or vice versa) can skip the other analysis
    /// entirely rather than compute and discard it.
    pub fn with_tempo(mut self, enabled: bool) -> Self {
        self.want_tempo = enabled;
        self
    }

    /// Enable or disable key detection. Default: `true`.
    pub fn with_key(mut self, enabled: bool) -> Self {
        self.want_key = enabled;
        self
    }

    /// Decode and analyse the audio file at `path`.
    ///
    /// # Errors
    ///
    /// See [`decode::decode_file`] for the decode-side error cases
    /// ([`AnalysisError::Io`], [`AnalysisError::Decode`],
    /// [`AnalysisError::NoAudioTrack`],
    /// [`AnalysisError::UnsupportedSampleRate`]); additionally returns
    /// [`AnalysisError::TooShort`] if fewer than
    /// [`MIN_ANALYSIS_SECONDS`] of audio were found (after applying the
    /// max-duration cap, so this reflects real short input, not the cap
    /// itself — the cap can only ever shorten `analysed_seconds`, never
    /// lengthen it).
    ///
    /// # Blocking
    ///
    /// Synchronous and I/O + CPU bound (decode, two STFTs, autocorrelation,
    /// 24-way key correlation) — see the crate-level doc's "Sync, not
    /// async" section. Call from a blocking-safe context if invoking from
    /// inside an async runtime.
    #[cfg(feature = "decode")]
    pub fn analyse_file(&self, path: &std::path::Path) -> Result<AudioAnalysis, AnalysisError> {
        let decoded = decode::decode_file(path, self.max_analysis_seconds)?;
        self.finish(decoded.signal, decoded.analysed_seconds, decoded.truncated)
    }

    /// Analyse already-decoded PCM: `interleaved` samples, `channels`
    /// channels, at `sample_rate` Hz.
    ///
    /// Unlike [`AudioAnalyser::analyse_file`], the caller has already
    /// paid the cost of getting the audio into memory (however it did
    /// that) — this is the entry point for a caller that already
    /// decoded the file for some other reason and doesn't want to pay
    /// for a second decode pass just for analysis.
    ///
    /// # Errors
    ///
    /// [`AnalysisError::UnsupportedSampleRate`] if `sample_rate` is `0`;
    /// [`AnalysisError::TooShort`] under the same rule as
    /// [`AudioAnalyser::analyse_file`].
    ///
    /// # Blocking
    ///
    /// Synchronous and CPU-bound — see the crate-level doc's "Sync, not
    /// async" section.
    pub fn analyse_samples(
        &self,
        interleaved: &[f32],
        channels: u16,
        sample_rate: u32,
    ) -> Result<AudioAnalysis, AnalysisError> {
        if sample_rate == 0 {
            return Err(AnalysisError::UnsupportedSampleRate(0));
        }
        let channels_usize = channels.max(1) as usize;
        let native_frame_count = interleaved.len() / channels_usize;
        let native_seconds = native_frame_count as f64 / sample_rate as f64;

        // Apply the max-duration cap BEFORE decimating, so a caller that
        // hands us a huge in-memory buffer doesn't pay for decimating
        // audio past the point analysis will ever look at.
        let capped_frame_count = ((self.max_analysis_seconds * sample_rate as f64).round()
            as usize)
            .min(native_frame_count);
        let truncated = native_seconds > self.max_analysis_seconds;
        let capped = &interleaved[..capped_frame_count * channels_usize];

        let mono = signal::mix_to_mono(capped, channels);
        let mut decimator = signal::Decimator::new(sample_rate);
        let samples = decimator.process(&mono);
        let working_rate = decimator.working_rate();
        let analysed_seconds = capped_frame_count as f64 / sample_rate as f64;

        self.finish(
            MonoSignal {
                samples,
                sample_rate: working_rate,
            },
            analysed_seconds,
            truncated,
        )
    }

    /// Shared tail end of both entry points: enforce the minimum-length
    /// floor, then run whichever of tempo/key detection was asked for.
    fn finish(
        &self,
        signal: MonoSignal,
        analysed_seconds: f64,
        truncated: bool,
    ) -> Result<AudioAnalysis, AnalysisError> {
        if analysed_seconds < MIN_ANALYSIS_SECONDS {
            return Err(AnalysisError::TooShort {
                seconds: analysed_seconds,
            });
        }

        let tempo = if self.want_tempo {
            tempo::detect_tempo(&signal, self.min_bpm, self.max_bpm)
        } else {
            None
        };
        let key = if self.want_key {
            key::detect_key(&signal)
        } else {
            None
        };

        Ok(AudioAnalysis {
            tempo,
            key,
            analysed_seconds,
            truncated,
            working_sample_rate: signal.sample_rate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_signals::click_track;

    // The brief this crate implements lists "3s of clicks -> Err(TooShort)"
    // under its tempo test table, but detect_tempo itself returns
    // Option, not Result -- TooShort is necessarily enforced here, at the
    // AudioAnalyser level, which is the only place in the crate that
    // knows how much audio it was actually asked to analyse before any
    // module-specific "found nothing" logic runs.
    #[test]
    fn three_seconds_of_audio_is_too_short() {
        let native = click_track(120.0, 3.0, 4, 0.0, 44_100);
        let err = AudioAnalyser::new()
            .analyse_samples(&native, 1, 44_100)
            .expect_err("3 seconds should be rejected as too short");
        assert!(
            matches!(err, AnalysisError::TooShort { seconds } if (seconds - 3.0).abs() < 0.1),
            "expected TooShort{{seconds: ~3.0}}, got {err:?}"
        );
    }

    #[test]
    fn five_seconds_clears_the_too_short_floor() {
        // Exactly at MIN_ANALYSIS_SECONDS should NOT be rejected -- the
        // floor is inclusive (`< MIN_ANALYSIS_SECONDS`, not `<=`).
        let native = click_track(120.0, MIN_ANALYSIS_SECONDS, 4, 0.0, 44_100);
        let result = AudioAnalyser::new().analyse_samples(&native, 1, 44_100);
        assert!(
            result.is_ok(),
            "expected 5.0s of audio to clear the too-short floor, got {result:?}"
        );
    }

    #[test]
    fn zero_sample_rate_is_rejected() {
        let native = vec![0.0f32; 1000];
        let err = AudioAnalyser::new()
            .analyse_samples(&native, 1, 0)
            .expect_err("a zero sample rate should be rejected");
        assert!(matches!(err, AnalysisError::UnsupportedSampleRate(0)));
    }

    #[test]
    fn with_tempo_false_skips_tempo_detection() {
        let native = click_track(120.0, 20.0, 4, 0.0, 44_100);
        let analysis = AudioAnalyser::new()
            .with_tempo(false)
            .analyse_samples(&native, 1, 44_100)
            .expect("expected a successful analysis");
        assert!(
            analysis.tempo.is_none(),
            "tempo detection should have been skipped"
        );
    }

    #[test]
    fn with_key_false_skips_key_detection() {
        let native = click_track(120.0, 20.0, 4, 0.0, 44_100);
        let analysis = AudioAnalyser::new()
            .with_key(false)
            .analyse_samples(&native, 1, 44_100)
            .expect("expected a successful analysis");
        assert!(
            analysis.key.is_none(),
            "key detection should have been skipped"
        );
    }

    #[test]
    fn analyse_samples_truncates_at_the_configured_max_duration() {
        let native = click_track(120.0, 30.0, 4, 0.0, 44_100);
        let analysis = AudioAnalyser::new()
            .with_max_duration(10.0)
            .analyse_samples(&native, 1, 44_100)
            .expect("expected a successful analysis");
        assert!(analysis.truncated);
        assert!((analysis.analysed_seconds - 10.0).abs() < 0.1);
    }

    #[test]
    fn analyse_samples_does_not_report_truncated_when_it_is_not() {
        let native = click_track(120.0, 20.0, 4, 0.0, 44_100);
        let analysis = AudioAnalyser::new()
            .analyse_samples(&native, 1, 44_100)
            .expect("expected a successful analysis");
        assert!(!analysis.truncated);
        assert!((analysis.analysed_seconds - 20.0).abs() < 0.1);
    }
}
