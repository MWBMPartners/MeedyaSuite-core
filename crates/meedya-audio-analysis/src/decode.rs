// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// [feature "decode"] Decode an audio file straight to a decimated
// MonoSignal, without ever holding the whole native-rate file in memory.
//
// The decode loop below is ported from
// `meedya-fingerprint::chromaprint::generate_fingerprint` — same probe
// setup, same "first non-null track" selection, same packet-skip/
// UnexpectedEof/DecodeError handling — because that loop already solves
// "read an audio file with symphonia" correctly and there is no reason
// to solve it differently a second time in the same workspace. What's
// different here: samples go through `signal::mix_to_mono` and
// `signal::Decimator` PACKET BY PACKET as they come off the decoder
// (chromaprint.rs instead feeds a fingerprinter directly), and decoding
// stops early once a duration cap is reached rather than always running
// to end-of-stream — a 3-hour DJ mix handed to this crate by mistake
// should not spend minutes decoding audio nothing downstream will ever
// look at.
//
// A note on codec coverage, since it's easy to over-claim here:
// symphonia 0.5 has NO Opus decoder at all (unlike the optimistic gloss
// in the fingerprint crate's own comment, which describes Opus as
// supported — it is not, as of the 0.5 series this workspace pins).
// HE-AAC decodes via its AAC-LC core only, so a genuinely HE-AAC file
// analyses as if its SBR (spectral band replication) high-frequency
// extension were absent — phrase this as "expect reduced bandwidth",
// never as full HE-AAC support.

use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::error::AnalysisError;
use crate::signal::{mix_to_mono, Decimator};
use crate::MonoSignal;

/// Result of decoding a file: the decimated signal, plus enough
/// bookkeeping for the caller to fill in [`crate::AudioAnalysis`]'s
/// `analysed_seconds`/`truncated` fields without re-deriving them.
#[derive(Debug)]
pub struct DecodedAudio {
    /// The decoded, mixed-to-mono, decimated signal.
    pub signal: MonoSignal,
    /// How much of the source was actually decoded, in seconds —
    /// either the file's full duration, or `max_duration_seconds` if
    /// decoding stopped early because of the cap.
    pub analysed_seconds: f64,
    /// Whether decoding stopped early because of `max_duration_seconds`
    /// (as opposed to reaching the natural end of the stream).
    pub truncated: bool,
}

/// Decode `path` to a [`DecodedAudio`], analysing at most
/// `max_duration_seconds` of audio from the start.
///
/// # Errors
///
/// - [`AnalysisError::Io`] — the file could not be opened.
/// - [`AnalysisError::Decode`] — symphonia could not probe the
///   container, could not find/initialise a decoder for the track, or
///   hit a fatal (non-recoverable) decode error.
/// - [`AnalysisError::NoAudioTrack`] — the container has no track with
///   a real (non-null) codec.
/// - [`AnalysisError::UnsupportedSampleRate`] — the track reports a
///   sample rate of `0`.
///
/// # Blocking
///
/// Synchronous and I/O + CPU bound — see the crate-level doc's "Sync,
/// not async" section. Symphonia's decode path is a synchronous
/// `std::io::Read` consumer throughout (the same reason
/// `meedya-fingerprint`'s decode loop is synchronous), so there is no
/// async I/O to gain from doing this any other way. Call from a
/// blocking-safe context if invoking from inside an async runtime.
pub fn decode_file(path: &Path, max_duration_seconds: f64) -> Result<DecodedAudio, AnalysisError> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| AnalysisError::Decode(format!("format probe: {e}")))?;
    let mut format_reader = probed.format;

    let track = format_reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or(AnalysisError::NoAudioTrack)?;

    let codec_params = &track.codec_params;
    let native_rate = codec_params
        .sample_rate
        .ok_or_else(|| AnalysisError::Decode("audio track has no sample rate".into()))?;
    if native_rate == 0 {
        return Err(AnalysisError::UnsupportedSampleRate(0));
    }
    let channels: u16 = codec_params
        .channels
        .map(|c| u16::try_from(c.count()).unwrap_or(0))
        .unwrap_or(1)
        .max(1);
    let track_id = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(codec_params, &DecoderOptions::default())
        .map_err(|e| AnalysisError::Decode(format!("decoder init: {e}")))?;

    let mut decimator = Decimator::new(native_rate);
    let mut decimated_samples: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    // Frame budget for the duration cap. Checked once per PACKET rather
    // than trying to cut a packet short mid-decode — packets are small
    // (tens of milliseconds each), so overshooting the cap by at most
    // one packet's worth of audio is a negligible, unobservable amount
    // of extra work, and it keeps the loop simple.
    let max_frames = (max_duration_seconds * native_rate as f64).round() as u64;
    let mut frames_decoded: u64 = 0;
    let mut truncated = false;

    loop {
        if frames_decoded >= max_frames {
            truncated = true;
            break;
        }

        let packet = match format_reader.next_packet() {
            Ok(packet) => packet,
            // A clean end of stream surfaces as an I/O error deep in
            // symphonia's reader, not as a distinct "no more packets"
            // variant — this is the same pattern chromaprint.rs's
            // decode loop matches on, for the same reason.
            Err(SymphoniaError::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(AnalysisError::Decode(format!("next_packet: {e}"))),
        };

        // Skip packets belonging to any other track in the container
        // (e.g. a video track alongside the audio, or additional audio
        // tracks we didn't select).
        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let num_frames = decoded.frames();
                if sample_buf.is_none()
                    || sample_buf
                        .as_ref()
                        .is_some_and(|b| b.capacity() < num_frames)
                {
                    let spec = *decoded.spec();
                    sample_buf = Some(SampleBuffer::new(num_frames as u64, spec));
                }
                if let Some(ref mut buf) = sample_buf {
                    buf.copy_interleaved_ref(decoded);
                    let interleaved = buf.samples();

                    // Mix THIS PACKET's samples to mono and push them
                    // through the streaming decimator immediately — the
                    // native-rate interleaved buffer for this packet is
                    // dropped at the end of this scope rather than
                    // accumulated, which is what keeps a 10-minute
                    // 192kHz file from ever needing ~460MB in memory at
                    // once (see signal.rs's module doc).
                    let mono_chunk = mix_to_mono(interleaved, channels);
                    let dec_chunk = decimator.process(&mono_chunk);
                    decimated_samples.extend_from_slice(&dec_chunk);

                    frames_decoded += num_frames as u64;
                }
            }
            // A single corrupted packet shouldn't kill an otherwise
            // decodable file — symphonia recovers and the next packet
            // is typically fine. Same tolerance chromaprint.rs's decode
            // loop applies.
            Err(SymphoniaError::DecodeError(_)) => {}
            Err(e) => return Err(AnalysisError::Decode(format!("decode: {e}"))),
        }
    }

    let working_rate = decimator.working_rate();
    let analysed_seconds = frames_decoded as f64 / native_rate as f64;

    Ok(DecodedAudio {
        signal: MonoSignal {
            samples: decimated_samples,
            sample_rate: working_rate,
        },
        analysed_seconds,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_signals::{click_track, write_pcm16_wav};
    use crate::AudioAnalyser;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "meedya-audio-analysis-decode-test-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_file_returns_io_error() {
        let err = decode_file(Path::new("/tmp/this/path/does/not/exist.wav"), 60.0)
            .expect_err("nonexistent path should fail");
        assert!(
            matches!(err, AnalysisError::Io(_)),
            "expected Io, got: {err:?}"
        );
    }

    #[test]
    fn text_file_named_m4a_returns_decode_error() {
        use std::io::Write as _;
        let dir = temp_dir();
        let path = dir.join("not_audio.m4a");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"this is plain text, not an m4a container")
            .unwrap();
        drop(f);

        let err = decode_file(&path, 60.0).expect_err("non-audio should fail");
        assert!(
            matches!(err, AnalysisError::Decode(_)),
            "expected Decode, got: {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn analyse_file_matches_analyse_samples_for_a_generated_click_wav() {
        let native = click_track(120.0, 20.0, 4, 0.0, 44_100);
        let dir = temp_dir();
        let path = dir.join("click_120bpm.wav");
        write_pcm16_wav(&path, &native, 44_100).expect("write wav");

        let via_file = AudioAnalyser::new()
            .analyse_file(&path)
            .expect("analyse_file should succeed on a valid generated WAV");
        let via_samples = AudioAnalyser::new()
            .analyse_samples(&native, 1, 44_100)
            .expect("analyse_samples should succeed on the same audio in memory");

        let tempo_file = via_file
            .tempo
            .expect("expected a tempo estimate via analyse_file");
        let tempo_samples = via_samples
            .tempo
            .expect("expected a tempo estimate via analyse_samples");

        // The file path went through 16-bit PCM quantisation and back
        // out through symphonia's own decoder, so this isn't asserting
        // bit-identical MonoSignal data -- it's asserting the two paths
        // agree on what they heard, which is the property that actually
        // matters to a caller.
        assert!(
            (tempo_file.bpm - tempo_samples.bpm).abs() < 1.0,
            "analyse_file bpm={} vs analyse_samples bpm={}",
            tempo_file.bpm,
            tempo_samples.bpm
        );
        assert!(
            tempo_file.confidence >= 0.7,
            "analyse_file confidence={}",
            tempo_file.confidence
        );
        assert!(
            tempo_samples.confidence >= 0.7,
            "analyse_samples confidence={}",
            tempo_samples.confidence
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_long_file_is_truncated_at_the_configured_max_duration() {
        // 15 minutes of silence -- cheap to generate and write (mostly
        // zero bytes), and with with_max_duration(60) the decode loop
        // stops requesting packets after the first minute's worth, so
        // the remaining 14 minutes of file are never actually read.
        let seconds = 15.0 * 60.0;
        let sample_rate = 44_100u32;
        let samples = vec![0.0f32; (seconds * sample_rate as f64) as usize];
        let dir = temp_dir();
        let path = dir.join("fifteen_minutes_of_silence.wav");
        write_pcm16_wav(&path, &samples, sample_rate).expect("write wav");

        let result = AudioAnalyser::new()
            .with_max_duration(60.0)
            .analyse_file(&path);

        // Silence has no onset/chroma energy, so TooShort does not apply
        // here (60s comfortably clears the 5s floor) but tempo/key both
        // legitimately return None -- what this test actually checks is
        // the truncation bookkeeping.
        let analysis =
            result.expect("60s capped from 15 minutes of silence should not be TooShort");
        assert!(
            analysis.truncated,
            "expected truncated=true for a 15-minute file capped at 60s"
        );
        assert!(
            (analysis.analysed_seconds - 60.0).abs() < 1.0,
            "expected analysed_seconds close to 60.0, got {}",
            analysis.analysed_seconds
        );

        let _ = std::fs::remove_file(&path);
    }

    // ----------------------------------------------------------
    // API surface pin
    // ----------------------------------------------------------

    #[test]
    fn symphonia_api_surface_unchanged() {
        // Pins the specific symphonia calls this module depends on —
        // the same pin chromaprint.rs carries for the same reason: an
        // upstream rename breaks compilation here first, not silently
        // inside decode_file.
        let _probe = symphonia::default::get_probe();
        let _codecs = symphonia::default::get_codecs();
        let _opts = MediaSourceStreamOptions::default();
    }
}
