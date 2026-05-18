// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Canonical audio codec definitions.
// Merged from MeedyaDL codec_registry.rs, MeedyaConverter AudioCodec.swift
// (42 cases + ExtendedAudioCodec), and MeedyaManager classify/mod.rs.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

/// Canonical audio codec identifier.
///
/// This enum is the single source of truth for audio codec identity across
/// all MeedyaSuite applications. Variants cover codecs from all three
/// projects: MeedyaDL, MeedyaConverter, and MeedyaManager.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumIter, EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum AudioCodec {
    // — Lossy codecs —
    /// AAC-LC (Advanced Audio Coding, Low Complexity)
    AacLc,
    /// HE-AAC v1 (High Efficiency AAC, SBR)
    HeAac,
    /// HE-AAC v2 (High Efficiency AAC, SBR + PS)
    HeAacV2,
    /// xHE-AAC (Extended High Efficiency AAC)
    XheAac,
    /// MP3 (MPEG-1/2 Audio Layer III)
    Mp3,
    /// Opus
    Opus,
    /// Vorbis (OGG Vorbis)
    Vorbis,
    /// AC-3 (Dolby Digital)
    Ac3,
    /// E-AC-3 (Dolby Digital Plus)
    Eac3,
    /// E-AC-3 with Dolby Atmos (JOC)
    Eac3Atmos,
    /// AAC with Binaural rendering
    AacBinaural,
    /// AAC Downmix
    AacDownmix,
    /// WMA (Windows Media Audio)
    Wma,
    /// WMA Pro
    WmaPro,
    /// WMA Lossless
    WmaLossless,
    /// AMR-NB (Adaptive Multi-Rate Narrowband)
    AmrNb,
    /// AMR-WB (Adaptive Multi-Rate Wideband)
    AmrWb,
    /// Musepack
    Musepack,
    /// Speex
    Speex,

    // — Lossless codecs —
    /// ALAC (Apple Lossless Audio Codec)
    Alac,
    /// FLAC (Free Lossless Audio Codec)
    Flac,
    /// WAV / PCM (Uncompressed)
    Pcm,
    /// AIFF (Audio Interchange File Format)
    Aiff,
    /// WavPack
    WavPack,
    /// APE (Monkey's Audio)
    Ape,
    /// TAK (Tom's lossless Audio Kompressor)
    Tak,
    /// TTA (True Audio)
    TrueAudio,
    /// DSD (Direct Stream Digital / SACD)
    Dsd,

    // — Surround / immersive codecs —
    /// DTS (Digital Theater Systems)
    Dts,
    /// DTS-HD Master Audio
    DtsHdMa,
    /// DTS-HD High Resolution
    DtsHdHr,
    /// DTS:X
    DtsX,
    /// DTS:X IMAX Enhanced
    DtsXImax,
    /// Dolby TrueHD
    TrueHd,
    /// Dolby TrueHD with Atmos
    TrueHdAtmos,
    /// MPEG-H 3D Audio
    MpegH3d,
    /// Sony 360 Reality Audio
    Sony360Ra,
    /// IAMF (Immersive Audio Model and Formats)
    Iamf,
    /// MP3 Surround
    Mp3Surround,

    // — Speech codecs —
    /// G.711 (PCM μ-law / A-law)
    G711,
    /// G.722
    G722,
    /// G.729
    G729,
}

impl AudioCodec {
    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::AacLc => "AAC-LC",
            Self::HeAac => "HE-AAC",
            Self::HeAacV2 => "HE-AAC v2",
            Self::XheAac => "xHE-AAC",
            Self::Mp3 => "MP3",
            Self::Opus => "Opus",
            Self::Vorbis => "Vorbis",
            Self::Ac3 => "AC-3 (Dolby Digital)",
            Self::Eac3 => "E-AC-3 (Dolby Digital Plus)",
            Self::Eac3Atmos => "E-AC-3 Atmos (Dolby Atmos)",
            Self::AacBinaural => "AAC Binaural",
            Self::AacDownmix => "AAC Downmix",
            Self::Wma => "WMA",
            Self::WmaPro => "WMA Pro",
            Self::WmaLossless => "WMA Lossless",
            Self::AmrNb => "AMR-NB",
            Self::AmrWb => "AMR-WB",
            Self::Musepack => "Musepack",
            Self::Speex => "Speex",
            Self::Alac => "ALAC",
            Self::Flac => "FLAC",
            Self::Pcm => "PCM / WAV",
            Self::Aiff => "AIFF",
            Self::WavPack => "WavPack",
            Self::Ape => "APE (Monkey's Audio)",
            Self::Tak => "TAK",
            Self::TrueAudio => "TTA (True Audio)",
            Self::Dsd => "DSD",
            Self::Dts => "DTS",
            Self::DtsHdMa => "DTS-HD Master Audio",
            Self::DtsHdHr => "DTS-HD High Resolution",
            Self::DtsX => "DTS:X",
            Self::DtsXImax => "DTS:X IMAX",
            Self::TrueHd => "Dolby TrueHD",
            Self::TrueHdAtmos => "Dolby TrueHD Atmos",
            Self::MpegH3d => "MPEG-H 3D Audio",
            Self::Sony360Ra => "Sony 360 Reality Audio",
            Self::Iamf => "IAMF",
            Self::Mp3Surround => "MP3 Surround",
            Self::G711 => "G.711",
            Self::G722 => "G.722",
            Self::G729 => "G.729",
        }
    }

    /// FFmpeg encoder name (if FFmpeg can encode this codec).
    pub fn ffmpeg_encoder(&self) -> Option<&'static str> {
        match self {
            Self::AacLc => Some("aac"),
            Self::Mp3 => Some("libmp3lame"),
            Self::Opus => Some("libopus"),
            Self::Vorbis => Some("libvorbis"),
            Self::Ac3 => Some("ac3"),
            Self::Eac3 | Self::Eac3Atmos => Some("eac3"),
            Self::Flac => Some("flac"),
            Self::Pcm => Some("pcm_s16le"),
            Self::Alac => Some("alac"),
            Self::Dts => Some("dca"),
            Self::TrueHd | Self::TrueHdAtmos => Some("truehd"),
            Self::WavPack => Some("wavpack"),
            Self::Speex => Some("libspeex"),
            _ => None,
        }
    }

    /// FFmpeg decoder name.
    pub fn ffmpeg_decoder(&self) -> Option<&'static str> {
        match self {
            Self::AacLc | Self::HeAac | Self::HeAacV2 | Self::XheAac => Some("aac"),
            Self::Mp3 | Self::Mp3Surround => Some("mp3"),
            Self::Opus => Some("libopus"),
            Self::Vorbis => Some("vorbis"),
            Self::Ac3 => Some("ac3"),
            Self::Eac3 | Self::Eac3Atmos => Some("eac3"),
            Self::Flac => Some("flac"),
            Self::Pcm => Some("pcm_s16le"),
            Self::Alac => Some("alac"),
            Self::Dts | Self::DtsHdMa | Self::DtsHdHr => Some("dca"),
            Self::TrueHd | Self::TrueHdAtmos => Some("truehd"),
            Self::WavPack => Some("wavpack"),
            Self::Ape => Some("ape"),
            Self::TrueAudio => Some("tta"),
            Self::Wma => Some("wmav2"),
            Self::WmaPro => Some("wmapro"),
            Self::WmaLossless => Some("wmalossless"),
            Self::AmrNb => Some("amrnb"),
            Self::AmrWb => Some("amrwb"),
            Self::Musepack => Some("mpc7"),
            Self::Speex => Some("libspeex"),
            _ => None,
        }
    }

    /// Whether this codec is lossless.
    pub fn is_lossless(&self) -> bool {
        matches!(
            self,
            Self::Alac
                | Self::Flac
                | Self::Pcm
                | Self::Aiff
                | Self::WavPack
                | Self::Ape
                | Self::Tak
                | Self::TrueAudio
                | Self::Dsd
                | Self::DtsHdMa
                | Self::TrueHd
                | Self::TrueHdAtmos
                | Self::WmaLossless
        )
    }

    /// Whether this codec supports spatial/immersive audio.
    pub fn is_spatial(&self) -> bool {
        matches!(
            self,
            Self::Eac3Atmos
                | Self::TrueHdAtmos
                | Self::DtsX
                | Self::DtsXImax
                | Self::MpegH3d
                | Self::Sony360Ra
                | Self::Iamf
                | Self::AacBinaural
        )
    }

    /// Whether this codec uses object-based audio (as opposed to channel-based).
    pub fn is_object_based(&self) -> bool {
        matches!(
            self,
            Self::Eac3Atmos
                | Self::TrueHdAtmos
                | Self::DtsX
                | Self::DtsXImax
                | Self::MpegH3d
                | Self::Sony360Ra
                | Self::Iamf
        )
    }

    /// Maximum channel count supported by this codec (0 = unlimited/varies).
    pub fn max_channels(&self) -> u8 {
        match self {
            Self::AacLc | Self::HeAac => 48,
            Self::HeAacV2 => 2,
            Self::Mp3 => 2,
            Self::Opus => 255,
            Self::Vorbis => 8,
            Self::Ac3 => 6,
            Self::Eac3 | Self::Eac3Atmos => 16,
            Self::TrueHd | Self::TrueHdAtmos => 16,
            Self::Dts => 6,
            Self::DtsHdMa | Self::DtsHdHr => 8,
            Self::DtsX | Self::DtsXImax => 32,
            Self::Alac => 8,
            Self::Flac => 8,
            Self::Pcm | Self::Aiff => 0, // unlimited
            Self::MpegH3d | Self::Sony360Ra | Self::Iamf => 0,
            _ => 2,
        }
    }

    /// Whether this codec supports variable bitrate encoding.
    pub fn supports_vbr(&self) -> bool {
        matches!(
            self,
            Self::AacLc | Self::Mp3 | Self::Opus | Self::Vorbis | Self::Musepack
        )
    }

    /// Typical file extension when this codec is stored standalone.
    pub fn typical_extension(&self) -> &'static str {
        match self {
            Self::AacLc | Self::HeAac | Self::HeAacV2 | Self::XheAac => "m4a",
            Self::Mp3 | Self::Mp3Surround => "mp3",
            Self::Opus => "opus",
            Self::Vorbis => "ogg",
            Self::Flac => "flac",
            Self::Alac => "m4a",
            Self::Pcm => "wav",
            Self::Aiff => "aiff",
            Self::WavPack => "wv",
            Self::Ape => "ape",
            Self::Dsd => "dsf",
            Self::Ac3 => "ac3",
            Self::Eac3 | Self::Eac3Atmos => "eac3",
            Self::Wma | Self::WmaPro | Self::WmaLossless => "wma",
            Self::Musepack => "mpc",
            Self::Speex => "spx",
            Self::Tak => "tak",
            Self::TrueAudio => "tta",
            _ => "mka",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_roundtrip_serialization() {
        let codec = AudioCodec::Eac3Atmos;
        let serialized = serde_json::to_string(&codec).unwrap();
        let deserialized: AudioCodec = serde_json::from_str(&serialized).unwrap();
        assert_eq!(codec, deserialized);
    }

    #[test]
    fn test_from_str() {
        let codec = AudioCodec::from_str("alac").unwrap();
        assert_eq!(codec, AudioCodec::Alac);
    }

    #[test]
    fn test_lossless_classification() {
        assert!(AudioCodec::Flac.is_lossless());
        assert!(AudioCodec::Alac.is_lossless());
        assert!(!AudioCodec::AacLc.is_lossless());
        assert!(!AudioCodec::Mp3.is_lossless());
    }

    #[test]
    fn test_spatial_classification() {
        assert!(AudioCodec::Eac3Atmos.is_spatial());
        assert!(AudioCodec::DtsX.is_spatial());
        assert!(!AudioCodec::Ac3.is_spatial());
        assert!(!AudioCodec::Flac.is_spatial());
    }

    #[test]
    fn test_object_based_is_subset_of_spatial() {
        use strum::IntoEnumIterator;
        for codec in AudioCodec::iter() {
            if codec.is_object_based() {
                assert!(
                    codec.is_spatial(),
                    "{:?} is object-based but not spatial",
                    codec
                );
            }
        }
    }
}
