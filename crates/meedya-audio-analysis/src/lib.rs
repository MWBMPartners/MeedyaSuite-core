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

mod error;
pub mod onset;
pub mod signal;
pub mod stft;
#[cfg(test)]
mod test_signals;

pub use error::AnalysisError;
pub use signal::MonoSignal;

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
