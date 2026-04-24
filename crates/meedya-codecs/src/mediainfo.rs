// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Adapted from interesting-mirzakhani. Uses the new `SpatialType` enum
// (coarse: Stereo/DolbyDigital/DolbyAtmos) and returns
// `Option<(AudioCodec, SpatialType)>` for unknown codecs rather than
// mirzakhani's `AudioCodec::Unknown` variant which doesn't exist on this tree.

use serde::Deserialize;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::audio_codec::AudioCodec;
use crate::spatial_type::SpatialType;

/// Result from MediaInfo codec detection.
#[derive(Debug, Clone)]
pub struct MediaInfoResult {
    pub codec: AudioCodec,
    pub spatial_type: SpatialType,
    pub format: String,
    pub additional_features: Option<String>,
    pub channels: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct MediaInfoOutput {
    media: Option<MediaInfoMedia>,
}

#[derive(Debug, Deserialize)]
struct MediaInfoMedia {
    track: Vec<MediaInfoTrack>,
}

#[derive(Debug, Deserialize)]
struct MediaInfoTrack {
    #[serde(rename = "@type")]
    track_type: String,
    #[serde(rename = "Format")]
    format: Option<String>,
    #[serde(rename = "Format_AdditionalFeatures")]
    format_additional_features: Option<String>,
    #[serde(rename = "Channels")]
    channels: Option<String>,
}

/// Run MediaInfo on a file and detect the audio codec.
///
/// Executes: `mediainfo --Output=JSON <file>`
///
/// MediaInfo is particularly useful for accurate Dolby Atmos detection
/// via the `Format_AdditionalFeatures: "JOC"` flag, which is more reliable
/// than FFprobe's channel-count heuristics.
pub async fn detect_codec(mediainfo_path: &Path, file_path: &Path) -> Option<MediaInfoResult> {
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new(mediainfo_path)
            .arg("--Output=JSON")
            .arg(file_path)
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    if !output.status.success() {
        warn!(
            "MediaInfo failed for {}: exit code {:?}",
            file_path.display(),
            output.status.code()
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_mediainfo_output(&stdout)
}

/// Parse MediaInfo JSON output into a codec detection result.
///
/// Returns `None` if the audio track is missing, or if the format doesn't
/// map to a known `AudioCodec` variant.
pub fn parse_mediainfo_output(json_str: &str) -> Option<MediaInfoResult> {
    let output: MediaInfoOutput = serde_json::from_str(json_str).ok()?;
    let media = output.media?;

    let audio_track = media.track.iter().find(|t| t.track_type == "Audio")?;

    let format = audio_track.format.clone().unwrap_or_default();
    let additional_features = audio_track.format_additional_features.clone();
    let channels = audio_track
        .channels
        .as_ref()
        .and_then(|c| c.parse::<u32>().ok());

    let (codec, spatial_type) = classify_codec(&format, additional_features.as_deref())?;

    debug!(
        "MediaInfo detected: format={format}, features={additional_features:?}, codec={codec:?}, spatial={spatial_type:?}"
    );

    Some(MediaInfoResult {
        codec,
        spatial_type,
        format,
        additional_features,
        channels,
    })
}

fn classify_codec(
    format: &str,
    additional_features: Option<&str>,
) -> Option<(AudioCodec, SpatialType)> {
    Some(match format {
        "E-AC-3" => {
            let has_joc = additional_features
                .map(|f| f.contains("JOC"))
                .unwrap_or(false);
            if has_joc {
                (AudioCodec::Eac3, SpatialType::DolbyAtmos)
            } else {
                (AudioCodec::Eac3, SpatialType::Stereo)
            }
        }
        "AC-3" => (AudioCodec::Ac3, SpatialType::DolbyDigital),
        "ALAC" => (AudioCodec::Alac, SpatialType::Stereo),
        "FLAC" => (AudioCodec::Flac, SpatialType::Stereo),
        "AAC" => {
            let codec = match additional_features {
                Some(f) if f.contains("SBR") || f.contains("HE") => AudioCodec::HeAac,
                _ => AudioCodec::AacLc,
            };
            (codec, SpatialType::Stereo)
        }
        "Opus" => (AudioCodec::Opus, SpatialType::Stereo),
        "Vorbis" => (AudioCodec::Vorbis, SpatialType::Stereo),
        "MPEG Audio" => (AudioCodec::Mp3, SpatialType::Stereo),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mediainfo_json(format: &str, features: Option<&str>, channels: &str) -> String {
        let features_field = match features {
            Some(f) => format!(r#""Format_AdditionalFeatures": "{f}","#),
            None => String::new(),
        };
        format!(
            r#"{{
                "media": {{
                    "track": [
                        {{ "@type": "General", "Format": "MPEG-4" }},
                        {{
                            "@type": "Audio",
                            "Format": "{format}",
                            {features_field}
                            "Channels": "{channels}"
                        }}
                    ]
                }}
            }}"#
        )
    }

    #[test]
    fn detect_dolby_atmos() {
        let json = make_mediainfo_json("E-AC-3", Some("JOC"), "6");
        let result = parse_mediainfo_output(&json).unwrap();
        assert_eq!(result.codec, AudioCodec::Eac3);
        assert_eq!(result.spatial_type, SpatialType::DolbyAtmos);
    }

    #[test]
    fn detect_eac3_without_atmos() {
        let json = make_mediainfo_json("E-AC-3", None, "6");
        let result = parse_mediainfo_output(&json).unwrap();
        assert_eq!(result.codec, AudioCodec::Eac3);
        assert_eq!(result.spatial_type, SpatialType::Stereo);
    }

    #[test]
    fn detect_ac3_dolby_digital() {
        let json = make_mediainfo_json("AC-3", None, "6");
        let result = parse_mediainfo_output(&json).unwrap();
        assert_eq!(result.codec, AudioCodec::Ac3);
        assert_eq!(result.spatial_type, SpatialType::DolbyDigital);
    }

    #[test]
    fn detect_alac() {
        let json = make_mediainfo_json("ALAC", None, "2");
        let result = parse_mediainfo_output(&json).unwrap();
        assert_eq!(result.codec, AudioCodec::Alac);
        assert!(result.codec.is_lossless());
    }

    #[test]
    fn detect_aac_lc() {
        let json = make_mediainfo_json("AAC", Some("LC"), "2");
        let result = parse_mediainfo_output(&json).unwrap();
        assert_eq!(result.codec, AudioCodec::AacLc);
    }

    #[test]
    fn detect_he_aac() {
        let json = make_mediainfo_json("AAC", Some("SBR"), "2");
        let result = parse_mediainfo_output(&json).unwrap();
        assert_eq!(result.codec, AudioCodec::HeAac);
    }

    #[test]
    fn detect_he_aac_via_he_feature() {
        let json = make_mediainfo_json("AAC", Some("HE-AAC"), "2");
        let result = parse_mediainfo_output(&json).unwrap();
        assert_eq!(result.codec, AudioCodec::HeAac);
    }

    #[test]
    fn parse_no_audio_track() {
        let json = r#"{"media": {"track": [{"@type": "General", "Format": "MPEG-4"}]}}"#;
        assert!(parse_mediainfo_output(json).is_none());
    }

    #[test]
    fn parse_invalid_json() {
        assert!(parse_mediainfo_output("not json").is_none());
    }

    #[test]
    fn parse_missing_media() {
        assert!(parse_mediainfo_output(r#"{}"#).is_none());
    }

    #[test]
    fn unknown_format_returns_none() {
        let json = make_mediainfo_json("WeirdFormat", None, "2");
        assert!(parse_mediainfo_output(&json).is_none());
    }
}
