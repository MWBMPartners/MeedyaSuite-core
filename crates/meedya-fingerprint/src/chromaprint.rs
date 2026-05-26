// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Chromaprint fingerprint generation — pure Rust, no external binaries.
// =====================================================================
//
// Ported verbatim from MeedyaDL's `acoustid_service::generate_fingerprint`
// for MeedyaDL#353 Phase 3. Lives under the `chromaprint` feature flag so
// consumers that only want the AcoustID HTTP client or the ReplayGain
// analyser don't pay the compile-time / binary-size cost of pulling in
// `rusty-chromaprint` + `symphonia`.
//
// ## Pipeline
//
// 1. Open the audio file and wrap it in a Symphonia `MediaSourceStream`.
// 2. Probe the format (M4A / FLAC / MP3 / OGG / Opus — anything Symphonia
//    can decode). The Symphonia `Hint` carries the file extension to
//    speed up probe.
// 3. Locate the first audio track, instantiate a decoder for it.
// 4. Decode every packet into interleaved `i16` PCM samples, feeding
//    each batch into `rusty_chromaprint::Fingerprinter::consume`.
// 5. Finalise the fingerprint, compress it with
//    `FingerprintCompressor`, encode the compressed bytes via
//    URL-safe base64 (no padding) — matches Chromaprint's reference
//    `fpcalc` output format and what the AcoustID API expects.
//
// ## Why pure Rust matters
//
// The original Chromaprint reference implementation is a C binary
// (`fpcalc`) distributed by acoustid.org. Pre-built `fpcalc` binaries
// only exist for x86_64 macOS, x86_64 Windows, and x86_64 Linux. ARM
// builds (Raspberry Pi 4/5, Apple Silicon servers, AWS Graviton)
// would need users to compile fpcalc themselves. By using
// `rusty-chromaprint` + `symphonia` we sidestep that entirely — the
// fingerprinter compiles with the rest of the consumer app for every
// target the consumer ships.
//
// ## What we don't preserve from `fpcalc`
//
// - **Locale-aware command-line parsing** — N/A, we're a library.
// - **The `-stats` output flag** — debug aid, not part of the API
//   contract.
// - **Streaming-from-stdin mode** — we always work from a file path.
//
// The fingerprint format and compression algorithm are byte-for-byte
// compatible with `fpcalc` output via `Configuration::preset_test2`
// (Chromaprint's default algorithm). An AcoustID lookup with a
// fingerprint produced here is indistinguishable from one produced by
// `fpcalc -raw -plain`.

use std::path::Path;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::error::FingerprintError;

/// Generate a Chromaprint fingerprint for an audio file.
///
/// Returns `Ok((fingerprint, duration_seconds))` where:
/// - `fingerprint` is the URL-safe-base64 encoded compressed
///   fingerprint (no padding) — the form the AcoustID API consumes.
/// - `duration_seconds` is the decoded duration of the audio track
///   in whole seconds, computed from the actual sample count (NOT
///   the container metadata, which can be wrong for re-muxed files).
///
/// # Errors
///
/// Returns a [`FingerprintError`] variant:
/// - [`FingerprintError::IoError`] — file open failed.
/// - [`FingerprintError::DecodeError`] — Symphonia probe or decode
///   failure (unsupported codec, malformed file, etc.).
/// - [`FingerprintError::FingerprintFailed`] — `rusty-chromaprint`
///   rejected the input (too short, malformed sample stream).
///
/// # Blocking
///
/// This is a **synchronous, CPU-bound** function. Consumers in async
/// contexts should wrap it in `tokio::task::spawn_blocking` to avoid
/// starving the runtime — decoding a typical 4-minute song takes
/// ~300-800ms on modern hardware.
pub fn generate_fingerprint(file_path: &Path) -> Result<(String, u32), FingerprintError> {
    // Open the audio file. We deliberately use `std::fs::File` here —
    // not `tokio::fs::File` — because Symphonia's reader is a
    // synchronous `io::Read` consumer. Async I/O would only add
    // overhead.
    let file = std::fs::File::open(file_path)?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    // Provide a file extension hint for format detection. Probe is
    // robust enough to detect the format from the leading magic bytes
    // even without the hint, but supplying it speeds the probe up.
    let mut hint = Hint::new();
    if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // Probe the format (auto-detect M4A / MP4 / FLAC / MP3 / OGG / Opus).
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| FingerprintError::DecodeError(format!("format probe: {e}")))?;

    let mut format_reader = probed.format;

    // Find the first audio track and capture its codec parameters.
    let track = format_reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| FingerprintError::DecodeError("no audio tracks found in file".into()))?;

    let codec_params = &track.codec_params;
    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| FingerprintError::DecodeError("audio track has no sample rate".into()))?;
    let channels = codec_params
        .channels
        .map(|ch| u32::try_from(ch.count()).unwrap_or(0))
        .ok_or_else(|| FingerprintError::DecodeError("audio track has no channel info".into()))?;
    let track_id = track.id;

    // Create a decoder for the audio track.
    let mut decoder = symphonia::default::get_codecs()
        .make(codec_params, &DecoderOptions::default())
        .map_err(|e| FingerprintError::DecodeError(format!("decoder init: {e}")))?;

    // Initialise the Chromaprint fingerprinter with `preset_test2` —
    // the same algorithm `fpcalc` uses by default.
    let config = Configuration::preset_test2();
    let mut printer = Fingerprinter::new(&config);
    printer
        .start(sample_rate, channels)
        .map_err(|e| FingerprintError::FingerprintFailed(format!("fingerprinter start: {e:?}")))?;

    // Decode every packet, feed interleaved i16 samples to the
    // fingerprinter. `total_samples` tracks the cumulative count
    // across packets so we can compute the true duration.
    let mut total_samples: u64 = 0;
    let mut sample_buf: Option<SampleBuffer<i16>> = None;

    loop {
        let packet = match format_reader.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break; // End of stream — clean termination.
            }
            Err(e) => return Err(FingerprintError::DecodeError(format!("next_packet: {e}"))),
        };

        // Skip packets from other tracks (unlikely for single-track
        // M4A, but defensive against multi-track sources).
        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                // Initialise or resize the sample buffer as needed.
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
                    let samples = buf.samples();
                    total_samples += samples.len() as u64;
                    printer.consume(samples);
                }
            }
            Err(SymphoniaError::DecodeError(_)) => {
                // Skip corrupted packets — Symphonia recovers and the
                // next packet should be fine. A single bad packet
                // shouldn't kill an otherwise valid fingerprint.
            }
            Err(e) => return Err(FingerprintError::DecodeError(format!("decode: {e}"))),
        }
    }

    // Finalise the fingerprint.
    printer.finish();
    let raw_fingerprint = printer.fingerprint();

    if raw_fingerprint.is_empty() {
        return Err(FingerprintError::FingerprintFailed(
            "generated empty fingerprint (file too short?)".into(),
        ));
    }

    // Compress the fingerprint using Chromaprint's internal
    // compression format, then encode in URL-safe base64 (no
    // padding) — matches `fpcalc`'s output and what the AcoustID
    // API consumes.
    let compressor = FingerprintCompressor::from(&config);
    let compressed = compressor.compress(raw_fingerprint);
    let encoded = URL_SAFE_NO_PAD.encode(&compressed);

    // Calculate duration from total decoded samples. u64::from()
    // does the lossless widening; try_from() guards against
    // truncation to u32 (which would only fire for tracks >
    // ~136 years long).
    let duration_seconds =
        u32::try_from(total_samples / u64::from(channels) / u64::from(sample_rate))
            .unwrap_or(u32::MAX);

    Ok((encoded, duration_seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_returns_io_error() {
        let err = generate_fingerprint(Path::new("/tmp/this/path/does/not/exist.m4a"))
            .expect_err("nonexistent path should fail");
        // `?` on a `std::fs::File::open` failure goes through the
        // `#[from] std::io::Error` impl on `FingerprintError::IoError`.
        assert!(
            matches!(err, FingerprintError::IoError(_)),
            "expected IoError, got: {err:?}"
        );
    }

    #[test]
    fn unrecognised_format_returns_decode_error() {
        use std::io::Write as _;
        // Write a file that's clearly not audio. Symphonia's probe
        // should fail; we want to ensure that surfaces as a
        // DecodeError rather than a panic.
        let dir = tempfile_dir();
        let path = dir.join("not_audio.m4a");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"this is not an audio file, it is plain text content")
            .unwrap();
        drop(f);

        let err = generate_fingerprint(&path).expect_err("non-audio should fail");
        assert!(
            matches!(err, FingerprintError::DecodeError(_)),
            "expected DecodeError, got: {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Helper: returns a process-unique temp dir created on demand.
    /// Avoids pulling `tempfile` as a dev-dep just for this single
    /// throwaway path.
    fn tempfile_dir() -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("meedya-fingerprint-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn extension_hint_is_optional() {
        // A file without an extension shouldn't crash the probe —
        // Symphonia falls back to magic-byte detection. We can't
        // assert success without a real audio fixture, but we CAN
        // assert that the failure mode is a clean DecodeError and
        // not a panic.
        use std::io::Write as _;
        let dir = tempfile_dir();
        let path = dir.join("noext"); // intentionally no extension
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"garbage").unwrap();
        drop(f);

        let err = generate_fingerprint(&path).expect_err("garbage should fail cleanly");
        assert!(matches!(err, FingerprintError::DecodeError(_)));
        let _ = std::fs::remove_file(&path);
    }

    // ----------------------------------------------------------
    // API surface pins
    // ----------------------------------------------------------

    #[test]
    fn rusty_chromaprint_api_surface_unchanged() {
        // Pins the bits of `rusty_chromaprint` this module depends on.
        // If upstream renames `Configuration::preset_test2` or removes
        // `FingerprintCompressor::from`, compilation here breaks first.
        let config = Configuration::preset_test2();
        let _printer = Fingerprinter::new(&config);
        let _compressor = FingerprintCompressor::from(&config);
    }

    #[test]
    fn symphonia_api_surface_unchanged() {
        // Same pin for symphonia: probe, codecs, MediaSourceStream.
        let _probe = symphonia::default::get_probe();
        let _codecs = symphonia::default::get_codecs();
        let _opts = MediaSourceStreamOptions::default();
    }
}
