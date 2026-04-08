use rusty_chromaprint::{Configuration, Fingerprinter};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tracing::{debug, warn};

use crate::Fingerprint;

/// Generate a Chromaprint fingerprint from an audio file.
///
/// Uses symphonia for pure-Rust audio decoding (M4A, FLAC, MP3, WAV)
/// and rusty-chromaprint for fingerprint generation.
pub fn generate_fingerprint(path: &Path) -> Result<Fingerprint, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot open file: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("probe failed: {e}"))?;

    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or("no default audio track")?;

    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or("no sample rate")?;

    let channels = track
        .codec_params
        .channels
        .map(|c| c.count() as u32)
        .unwrap_or(2);

    let track_id = track.id;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("decoder init failed: {e}"))?;

    let config = Configuration::preset_test1();
    let mut printer = Fingerprinter::new(&config);
    printer
        .start(sample_rate, channels)
        .map_err(|e| format!("fingerprinter start failed: {e}"))?;

    let mut total_samples = 0u64;
    let max_duration_samples = (sample_rate as u64) * 120; // Max 120 seconds

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => {
                warn!("Packet read error: {e}");
                break;
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(e) => {
                warn!("Decode error: {e}");
                continue;
            }
        };

        let spec = *decoded.spec();
        let num_frames = decoded.frames();
        let mut sample_buf = SampleBuffer::<i16>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        printer.consume(sample_buf.samples());
        total_samples += num_frames as u64;

        if total_samples >= max_duration_samples {
            break;
        }
    }

    printer.finish();

    let raw_fp = printer.fingerprint();
    // Encode raw u32 fingerprint as base64-like string for AcoustID submission
    let fp_string = encode_fingerprint_raw(raw_fp);
    let duration = (total_samples / sample_rate as u64) as u32;

    debug!("Generated fingerprint for {}: duration={duration}s", path.display());

    Ok(Fingerprint {
        fingerprint: fp_string,
        duration,
    })
}

/// Encode a raw u32 fingerprint array as a hex string.
///
/// For AcoustID submission, the raw fingerprint is typically encoded.
/// This produces a compact hex representation.
fn encode_fingerprint_raw(data: &[u32]) -> String {
    data.iter()
        .map(|v| format!("{:08x}", v))
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_fingerprint_missing_file() {
        let result = generate_fingerprint(Path::new("/nonexistent/file.m4a"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot open file"));
    }
}
