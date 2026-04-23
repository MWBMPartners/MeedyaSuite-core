// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// 4-level media classification system.
// From MeedyaManager mm-core/src/classify/mod.rs.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

/// Top-level media group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Display, EnumIter, EnumString)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum MediaGroup {
    Audio,
    Video,
    Image,
    Subtitle,
    Document,
}

/// Media format family within a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Display, EnumIter, EnumString)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum MediaFormat {
    // Audio formats
    Mpeg,
    Aac,
    Lossless,
    Compressed,
    Surround,
    Spatial,
    Speech,
    Raw,

    // Video formats
    Modern,
    Legacy,
    Professional,
    Stereoscopic,
    Web,

    // Image formats
    Raster,
    Vector,

    // Subtitle formats
    Text,
    Bitmap,
    ClosedCaption,
}

/// Quality class within a format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[derive(Display, EnumIter, EnumString)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum MediaClass {
    Lossy,
    Standard,
    High,
    Lossless,
    Master,
}

/// Quality tier for lossy content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[derive(Display, EnumIter, EnumString)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum MediaQuality {
    /// Low quality (< 128 kbps audio, < 480p video)
    Low,
    /// Standard quality (128-256 kbps audio, 480p-720p video)
    Standard,
    /// High quality (256-320 kbps audio, 1080p video)
    High,
    /// Very high quality (320+ kbps audio, 4K video)
    VeryHigh,
    /// Lossless / original quality
    Lossless,
}

/// Complete 4-level media classification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MediaClassification {
    pub group: MediaGroup,
    pub format: MediaFormat,
    pub class: MediaClass,
    pub quality: MediaQuality,
}

impl MediaClassification {
    pub fn new(
        group: MediaGroup,
        format: MediaFormat,
        class: MediaClass,
        quality: MediaQuality,
    ) -> Self {
        Self { group, format, class, quality }
    }
}

/// Classify an audio codec into the 4-level system.
pub fn classify_audio_codec(codec: crate::AudioCodec) -> MediaClassification {
    use crate::AudioCodec::*;

    let (format, class, quality) = match codec {
        Mp3 => (MediaFormat::Mpeg, MediaClass::Lossy, MediaQuality::Standard),
        AacLc => (MediaFormat::Aac, MediaClass::Lossy, MediaQuality::High),
        HeAac | HeAacV2 => (MediaFormat::Aac, MediaClass::Lossy, MediaQuality::Standard),
        XheAac => (MediaFormat::Aac, MediaClass::Lossy, MediaQuality::Low),
        Opus => (MediaFormat::Compressed, MediaClass::Lossy, MediaQuality::High),
        Vorbis => (MediaFormat::Compressed, MediaClass::Lossy, MediaQuality::Standard),
        Flac => (MediaFormat::Lossless, MediaClass::Lossless, MediaQuality::Lossless),
        Alac => (MediaFormat::Lossless, MediaClass::Lossless, MediaQuality::Lossless),
        Pcm | Aiff => (MediaFormat::Raw, MediaClass::Lossless, MediaQuality::Lossless),
        WavPack | Ape | Tak | TrueAudio => {
            (MediaFormat::Lossless, MediaClass::Lossless, MediaQuality::Lossless)
        }
        Dsd => (MediaFormat::Raw, MediaClass::Master, MediaQuality::Lossless),
        Ac3 => (MediaFormat::Surround, MediaClass::Lossy, MediaQuality::Standard),
        Eac3 => (MediaFormat::Surround, MediaClass::Lossy, MediaQuality::High),
        Eac3Atmos | TrueHdAtmos | DtsX | DtsXImax | MpegH3d | Sony360Ra | Iamf => {
            (MediaFormat::Spatial, MediaClass::High, MediaQuality::VeryHigh)
        }
        AacBinaural => (MediaFormat::Spatial, MediaClass::Lossy, MediaQuality::High),
        Dts => (MediaFormat::Surround, MediaClass::Lossy, MediaQuality::Standard),
        DtsHdMa => (MediaFormat::Surround, MediaClass::Lossless, MediaQuality::Lossless),
        DtsHdHr => (MediaFormat::Surround, MediaClass::High, MediaQuality::VeryHigh),
        TrueHd => (MediaFormat::Surround, MediaClass::Lossless, MediaQuality::Lossless),
        AacDownmix => (MediaFormat::Aac, MediaClass::Lossy, MediaQuality::Standard),
        Wma | WmaPro => (MediaFormat::Compressed, MediaClass::Lossy, MediaQuality::Standard),
        WmaLossless => (MediaFormat::Lossless, MediaClass::Lossless, MediaQuality::Lossless),
        AmrNb | AmrWb | Speex | G711 | G722 | G729 => {
            (MediaFormat::Speech, MediaClass::Lossy, MediaQuality::Low)
        }
        Musepack | Mp3Surround => {
            (MediaFormat::Compressed, MediaClass::Lossy, MediaQuality::Standard)
        }
    };

    MediaClassification::new(MediaGroup::Audio, format, class, quality)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioCodec;

    #[test]
    fn test_classify_lossless() {
        let c = classify_audio_codec(AudioCodec::Flac);
        assert_eq!(c.group, MediaGroup::Audio);
        assert_eq!(c.class, MediaClass::Lossless);
        assert_eq!(c.quality, MediaQuality::Lossless);
    }

    #[test]
    fn test_classify_lossy() {
        let c = classify_audio_codec(AudioCodec::Mp3);
        assert_eq!(c.class, MediaClass::Lossy);
    }

    #[test]
    fn test_classify_spatial() {
        let c = classify_audio_codec(AudioCodec::Eac3Atmos);
        assert_eq!(c.format, MediaFormat::Spatial);
    }
}
