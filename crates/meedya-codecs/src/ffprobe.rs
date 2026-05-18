// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Adapted from interesting-mirzakhani. Returns `Option<AudioCodec>` where
// mirzakhani used an `AudioCodec::Unknown` variant; the codec enum on this
// tree doesn't have one. `pcm_*` codec names map to `AudioCodec::Pcm`.

use serde::Deserialize;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::audio_codec::AudioCodec;
use crate::channel_config::ChannelConfig;

/// Audio information extracted by FFprobe.
#[derive(Debug, Clone)]
pub struct FfprobeAudioInfo {
    pub codec_name: String,
    pub profile: Option<String>,
    pub channels: u32,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub channel_config: ChannelConfig,
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    streams: Vec<FfprobeStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_name: Option<String>,
    profile: Option<String>,
    channels: Option<u32>,
    sample_rate: Option<String>,
    bits_per_raw_sample: Option<String>,
}

/// Run FFprobe on a file and extract audio stream information.
///
/// Executes: `ffprobe -v quiet -print_format json -show_streams -select_streams a:0 <file>`
pub async fn detect_audio_info(ffprobe_path: &Path, file_path: &Path) -> Option<FfprobeAudioInfo> {
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new(ffprobe_path)
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_streams",
                "-select_streams",
                "a:0",
            ])
            .arg(file_path)
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    if !output.status.success() {
        warn!(
            "FFprobe failed for {}: exit code {:?}",
            file_path.display(),
            output.status.code()
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ffprobe_output(&stdout)
}

/// Parse FFprobe JSON output into structured audio info.
pub fn parse_ffprobe_output(json_str: &str) -> Option<FfprobeAudioInfo> {
    let output: FfprobeOutput = serde_json::from_str(json_str).ok()?;
    let stream = output.streams.first()?;

    let codec_name = stream.codec_name.clone().unwrap_or_default();
    let channels = stream.channels.unwrap_or(2);
    let sample_rate = stream
        .sample_rate
        .as_ref()
        .and_then(|s| s.parse::<u32>().ok());
    let bit_depth = stream
        .bits_per_raw_sample
        .as_ref()
        .and_then(|s| s.parse::<u32>().ok());

    debug!(
        "FFprobe detected: codec={codec_name}, channels={channels}, sample_rate={sample_rate:?}"
    );

    Some(FfprobeAudioInfo {
        codec_name,
        profile: stream.profile.clone(),
        channels,
        sample_rate,
        bit_depth,
        channel_config: ChannelConfig::from_count(channels),
    })
}

/// Resolve the actual audio codec from FFprobe info.
///
/// Returns `None` for codecs that don't map to a known `AudioCodec` variant.
pub fn resolve_codec(info: &FfprobeAudioInfo) -> Option<AudioCodec> {
    Some(match info.codec_name.as_str() {
        "alac" => AudioCodec::Alac,
        "flac" => AudioCodec::Flac,
        "eac3" => AudioCodec::Eac3,
        "ac3" => AudioCodec::Ac3,
        "opus" => AudioCodec::Opus,
        "vorbis" => AudioCodec::Vorbis,
        "mp3" => AudioCodec::Mp3,
        "pcm_s16le" | "pcm_s24le" | "pcm_s32le" | "pcm_f32le" => AudioCodec::Pcm,
        "aac" => match info.profile.as_deref() {
            Some("HE-AAC") | Some("HE-AACv2") => AudioCodec::HeAac,
            _ => AudioCodec::AacLc,
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_FFPROBE_AAC: &str = r#"{
        "streams": [{
            "codec_name": "aac",
            "profile": "LC",
            "channels": 2,
            "sample_rate": "44100",
            "bits_per_raw_sample": "16"
        }]
    }"#;

    const SAMPLE_FFPROBE_ALAC: &str = r#"{
        "streams": [{
            "codec_name": "alac",
            "channels": 2,
            "sample_rate": "96000",
            "bits_per_raw_sample": "24"
        }]
    }"#;

    const SAMPLE_FFPROBE_EAC3: &str = r#"{
        "streams": [{
            "codec_name": "eac3",
            "channels": 6,
            "sample_rate": "48000"
        }]
    }"#;

    const SAMPLE_FFPROBE_HE_AAC: &str = r#"{
        "streams": [{
            "codec_name": "aac",
            "profile": "HE-AAC",
            "channels": 2,
            "sample_rate": "44100"
        }]
    }"#;

    #[test]
    fn parse_aac_lc() {
        let info = parse_ffprobe_output(SAMPLE_FFPROBE_AAC).unwrap();
        assert_eq!(info.codec_name, "aac");
        assert_eq!(info.channels, 2);
        assert_eq!(info.sample_rate, Some(44100));
        assert_eq!(info.channel_config.label, "2.0");
        assert_eq!(resolve_codec(&info), Some(AudioCodec::AacLc));
    }

    #[test]
    fn parse_alac() {
        let info = parse_ffprobe_output(SAMPLE_FFPROBE_ALAC).unwrap();
        assert_eq!(info.codec_name, "alac");
        assert_eq!(info.sample_rate, Some(96000));
        assert_eq!(info.bit_depth, Some(24));
        assert!(resolve_codec(&info).unwrap().is_lossless());
    }

    #[test]
    fn parse_eac3_surround() {
        let info = parse_ffprobe_output(SAMPLE_FFPROBE_EAC3).unwrap();
        assert_eq!(info.channels, 6);
        assert_eq!(info.channel_config.label, "5.1");
        assert_eq!(resolve_codec(&info), Some(AudioCodec::Eac3));
    }

    #[test]
    fn parse_he_aac() {
        let info = parse_ffprobe_output(SAMPLE_FFPROBE_HE_AAC).unwrap();
        assert_eq!(resolve_codec(&info), Some(AudioCodec::HeAac));
    }

    #[test]
    fn parse_empty_streams() {
        let json = r#"{"streams": []}"#;
        assert!(parse_ffprobe_output(json).is_none());
    }

    #[test]
    fn parse_invalid_json() {
        assert!(parse_ffprobe_output("not json").is_none());
    }

    #[test]
    fn unknown_codec_returns_none() {
        let json = r#"{"streams": [{"codec_name": "weirdcodec", "channels": 2}]}"#;
        let info = parse_ffprobe_output(json).unwrap();
        assert_eq!(resolve_codec(&info), None);
    }
}
