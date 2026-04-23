// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Canonical container format definitions.
// Merged from MeedyaConverter ContainerFormat.swift (28 cases + ExtendedContainer),
// MeedyaManager filetype_registry.rs + filetypes.json5,
// and MeedyaDL codecs.toml.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

use crate::audio_codec::AudioCodec;
use crate::video_codec::VideoCodec;

/// Canonical container/file format identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Display, EnumIter, EnumString)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum ContainerFormat {
    // — Video containers —
    /// MPEG-4 Part 14 (.mp4, .m4v)
    Mp4,
    /// Matroska Video (.mkv)
    Mkv,
    /// WebM (.webm)
    WebM,
    /// QuickTime (.mov)
    Mov,
    /// Audio Video Interleave (.avi)
    Avi,
    /// MPEG Transport Stream (.ts, .mts, .m2ts)
    MpegTs,
    /// MPEG Program Stream (.mpg, .mpeg, .vob)
    MpegPs,
    /// Flash Video (.flv)
    Flv,
    /// 3GPP (.3gp)
    ThreeGp,
    /// Material eXchange Format (.mxf)
    Mxf,
    /// Advanced Systems Format / Windows Media (.asf, .wmv)
    Asf,
    /// NUT Open Container (.nut)
    Nut,

    // — Audio-only containers —
    /// MPEG-4 Audio (.m4a, .m4b, .m4r)
    M4a,
    /// FLAC (.flac)
    Flac,
    /// Ogg (.ogg, .oga)
    Ogg,
    /// Opus in Ogg (.opus)
    OpusOgg,
    /// WAV / RIFF (.wav)
    Wav,
    /// AIFF (.aiff, .aif)
    Aiff,
    /// MP3 (.mp3)
    Mp3,
    /// Matroska Audio (.mka)
    Mka,
    /// Windows Media Audio (.wma)
    Wma,
    /// WavPack (.wv)
    WavPack,
    /// Musepack (.mpc)
    Musepack,
    /// APE / Monkey's Audio (.ape)
    Ape,
    /// DSD Stream File (.dsf, .dff)
    Dsf,
    /// TAK (.tak)
    Tak,
    /// TTA (.tta)
    Tta,
    /// AAC raw stream (.aac)
    AacRaw,
    /// AC-3 raw stream (.ac3)
    Ac3Raw,
    /// Dolby Digital Plus raw stream (.eac3)
    Eac3Raw,
    /// DTS raw stream (.dts)
    DtsRaw,

    // — Image containers —
    /// JPEG (.jpg, .jpeg)
    Jpeg,
    /// PNG (.png)
    Png,
    /// WebP (.webp)
    WebP,
    /// GIF (.gif)
    Gif,
    /// TIFF (.tiff, .tif)
    Tiff,
    /// BMP (.bmp)
    Bmp,
}

/// Media category of a container format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaCategory {
    Audio,
    Video,
    Image,
}

impl ContainerFormat {
    /// Primary file extension for this container.
    pub fn primary_extension(&self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::WebM => "webm",
            Self::Mov => "mov",
            Self::Avi => "avi",
            Self::MpegTs => "ts",
            Self::MpegPs => "mpg",
            Self::Flv => "flv",
            Self::ThreeGp => "3gp",
            Self::Mxf => "mxf",
            Self::Asf => "asf",
            Self::Nut => "nut",
            Self::M4a => "m4a",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::OpusOgg => "opus",
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Mp3 => "mp3",
            Self::Mka => "mka",
            Self::Wma => "wma",
            Self::WavPack => "wv",
            Self::Musepack => "mpc",
            Self::Ape => "ape",
            Self::Dsf => "dsf",
            Self::Tak => "tak",
            Self::Tta => "tta",
            Self::AacRaw => "aac",
            Self::Ac3Raw => "ac3",
            Self::Eac3Raw => "eac3",
            Self::DtsRaw => "dts",
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
            Self::Gif => "gif",
            Self::Tiff => "tiff",
            Self::Bmp => "bmp",
        }
    }

    /// All recognized file extensions for this container.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Mp4 => &["mp4", "m4v"],
            Self::Mkv => &["mkv"],
            Self::WebM => &["webm"],
            Self::Mov => &["mov", "qt"],
            Self::Avi => &["avi"],
            Self::MpegTs => &["ts", "mts", "m2ts"],
            Self::MpegPs => &["mpg", "mpeg", "vob"],
            Self::Flv => &["flv"],
            Self::ThreeGp => &["3gp", "3g2"],
            Self::Mxf => &["mxf"],
            Self::Asf => &["asf", "wmv"],
            Self::Nut => &["nut"],
            Self::M4a => &["m4a", "m4b", "m4r"],
            Self::Flac => &["flac"],
            Self::Ogg => &["ogg", "oga"],
            Self::OpusOgg => &["opus"],
            Self::Wav => &["wav"],
            Self::Aiff => &["aiff", "aif"],
            Self::Mp3 => &["mp3"],
            Self::Mka => &["mka"],
            Self::Wma => &["wma"],
            Self::WavPack => &["wv"],
            Self::Musepack => &["mpc"],
            Self::Ape => &["ape"],
            Self::Dsf => &["dsf", "dff"],
            Self::Tak => &["tak"],
            Self::Tta => &["tta"],
            Self::AacRaw => &["aac"],
            Self::Ac3Raw => &["ac3"],
            Self::Eac3Raw => &["eac3"],
            Self::DtsRaw => &["dts"],
            Self::Jpeg => &["jpg", "jpeg"],
            Self::Png => &["png"],
            Self::WebP => &["webp"],
            Self::Gif => &["gif"],
            Self::Tiff => &["tiff", "tif"],
            Self::Bmp => &["bmp"],
        }
    }

    /// MIME type for this container.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Mp4 => "video/mp4",
            Self::Mkv => "video/x-matroska",
            Self::WebM => "video/webm",
            Self::Mov => "video/quicktime",
            Self::Avi => "video/x-msvideo",
            Self::MpegTs => "video/mp2t",
            Self::MpegPs => "video/mpeg",
            Self::Flv => "video/x-flv",
            Self::ThreeGp => "video/3gpp",
            Self::Mxf => "application/mxf",
            Self::Asf => "video/x-ms-asf",
            Self::Nut => "video/x-nut",
            Self::M4a => "audio/mp4",
            Self::Flac => "audio/flac",
            Self::Ogg => "audio/ogg",
            Self::OpusOgg => "audio/opus",
            Self::Wav => "audio/wav",
            Self::Aiff => "audio/aiff",
            Self::Mp3 => "audio/mpeg",
            Self::Mka => "audio/x-matroska",
            Self::Wma => "audio/x-ms-wma",
            Self::WavPack => "audio/x-wavpack",
            Self::Musepack => "audio/x-musepack",
            Self::Ape => "audio/x-ape",
            Self::Dsf => "audio/x-dsf",
            Self::Tak => "audio/x-tak",
            Self::Tta => "audio/x-tta",
            Self::AacRaw => "audio/aac",
            Self::Ac3Raw => "audio/ac3",
            Self::Eac3Raw => "audio/eac3",
            Self::DtsRaw => "audio/x-dts",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::WebP => "image/webp",
            Self::Gif => "image/gif",
            Self::Tiff => "image/tiff",
            Self::Bmp => "image/bmp",
        }
    }

    /// FFmpeg format name for muxing/demuxing.
    pub fn ffmpeg_format_name(&self) -> &'static str {
        match self {
            Self::Mp4 | Self::M4a => "mp4",
            Self::Mkv | Self::Mka => "matroska",
            Self::WebM => "webm",
            Self::Mov => "mov",
            Self::Avi => "avi",
            Self::MpegTs => "mpegts",
            Self::MpegPs => "mpeg",
            Self::Flv => "flv",
            Self::ThreeGp => "3gp",
            Self::Mxf => "mxf",
            Self::Asf => "asf",
            Self::Nut => "nut",
            Self::Flac => "flac",
            Self::Ogg | Self::OpusOgg => "ogg",
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Mp3 => "mp3",
            Self::Wma => "asf",
            Self::WavPack => "wv",
            Self::Musepack => "mpc",
            Self::Ape => "ape",
            Self::Dsf => "dsf",
            Self::Tak => "tak",
            Self::Tta => "tta",
            Self::AacRaw => "adts",
            Self::Ac3Raw => "ac3",
            Self::Eac3Raw => "eac3",
            Self::DtsRaw => "dts",
            Self::Jpeg => "image2",
            Self::Png => "image2",
            Self::WebP => "webp",
            Self::Gif => "gif",
            Self::Tiff => "image2",
            Self::Bmp => "image2",
        }
    }

    /// Media category for this container.
    pub fn category(&self) -> MediaCategory {
        match self {
            Self::Mp4 | Self::Mkv | Self::WebM | Self::Mov | Self::Avi
            | Self::MpegTs | Self::MpegPs | Self::Flv | Self::ThreeGp
            | Self::Mxf | Self::Asf | Self::Nut => MediaCategory::Video,

            Self::M4a | Self::Flac | Self::Ogg | Self::OpusOgg | Self::Wav
            | Self::Aiff | Self::Mp3 | Self::Mka | Self::Wma | Self::WavPack
            | Self::Musepack | Self::Ape | Self::Dsf | Self::Tak | Self::Tta
            | Self::AacRaw | Self::Ac3Raw | Self::Eac3Raw | Self::DtsRaw => {
                MediaCategory::Audio
            }

            Self::Jpeg | Self::Png | Self::WebP | Self::Gif | Self::Tiff
            | Self::Bmp => MediaCategory::Image,
        }
    }

    /// Whether this container supports HDR video.
    pub fn supports_hdr(&self) -> bool {
        matches!(
            self,
            Self::Mp4 | Self::Mkv | Self::WebM | Self::Mov | Self::MpegTs | Self::Mxf
        )
    }

    /// Whether this container supports Dolby Vision.
    pub fn supports_dolby_vision(&self) -> bool {
        matches!(self, Self::Mp4 | Self::Mkv | Self::Mov | Self::MpegTs)
    }

    /// Whether this container supports embedded subtitles.
    pub fn supports_subtitles(&self) -> bool {
        matches!(
            self,
            Self::Mp4 | Self::Mkv | Self::WebM | Self::Mov | Self::Avi
            | Self::MpegTs | Self::MpegPs | Self::Mxf | Self::Asf
        )
    }

    /// Whether this container supports chapter markers.
    pub fn supports_chapters(&self) -> bool {
        matches!(
            self,
            Self::Mp4 | Self::Mkv | Self::WebM | Self::Mov | Self::M4a | Self::Ogg
        )
    }

    /// Check if a given audio codec can be muxed into this container.
    pub fn supports_audio_codec(&self, codec: AudioCodec) -> bool {
        match self {
            Self::Mp4 | Self::M4a | Self::Mov => matches!(
                codec,
                AudioCodec::AacLc | AudioCodec::HeAac | AudioCodec::HeAacV2
                | AudioCodec::XheAac | AudioCodec::Alac | AudioCodec::Ac3
                | AudioCodec::Eac3 | AudioCodec::Eac3Atmos | AudioCodec::Flac
                | AudioCodec::Opus | AudioCodec::AacBinaural | AudioCodec::AacDownmix
                | AudioCodec::TrueHd | AudioCodec::TrueHdAtmos
            ),
            Self::Mkv | Self::Mka => true, // Matroska accepts virtually anything
            Self::WebM => matches!(
                codec,
                AudioCodec::Opus | AudioCodec::Vorbis
            ),
            Self::Ogg => matches!(
                codec,
                AudioCodec::Vorbis | AudioCodec::Flac | AudioCodec::Opus
            ),
            Self::OpusOgg => matches!(codec, AudioCodec::Opus),
            Self::Flac => matches!(codec, AudioCodec::Flac),
            Self::Wav => matches!(codec, AudioCodec::Pcm),
            Self::Aiff => matches!(codec, AudioCodec::Pcm | AudioCodec::Aiff),
            Self::Mp3 => matches!(codec, AudioCodec::Mp3),
            Self::Avi => matches!(
                codec,
                AudioCodec::Mp3 | AudioCodec::AacLc | AudioCodec::Pcm
                | AudioCodec::Ac3 | AudioCodec::Dts
            ),
            Self::MpegTs | Self::MpegPs => matches!(
                codec,
                AudioCodec::AacLc | AudioCodec::HeAac | AudioCodec::Mp3
                | AudioCodec::Ac3 | AudioCodec::Eac3 | AudioCodec::Eac3Atmos
                | AudioCodec::Dts | AudioCodec::DtsHdMa | AudioCodec::Pcm
                | AudioCodec::TrueHd | AudioCodec::TrueHdAtmos
            ),
            Self::Asf | Self::Wma => matches!(
                codec,
                AudioCodec::Wma | AudioCodec::WmaPro | AudioCodec::WmaLossless
                | AudioCodec::Pcm
            ),
            _ => false,
        }
    }

    /// Check if a given video codec can be muxed into this container.
    pub fn supports_video_codec(&self, codec: VideoCodec) -> bool {
        match self {
            Self::Mp4 | Self::Mov => matches!(
                codec,
                VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Av1
                | VideoCodec::Mpeg4 | VideoCodec::ProRes | VideoCodec::MvHevc
                | VideoCodec::MvH264 | VideoCodec::Jpeg2000 | VideoCodec::Mjpeg
                | VideoCodec::Vp9
            ),
            Self::Mkv => true, // Matroska accepts virtually anything
            Self::WebM => matches!(
                codec,
                VideoCodec::Vp8 | VideoCodec::Vp9 | VideoCodec::Av1
            ),
            Self::Avi => matches!(
                codec,
                VideoCodec::H264 | VideoCodec::Mpeg4 | VideoCodec::Mjpeg
                | VideoCodec::Huffyuv | VideoCodec::DnxHd
            ),
            Self::MpegTs => matches!(
                codec,
                VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Mpeg2
                | VideoCodec::Av1
            ),
            Self::MpegPs => matches!(
                codec,
                VideoCodec::Mpeg2 | VideoCodec::Mpeg4
            ),
            Self::Flv => matches!(
                codec,
                VideoCodec::H264 | VideoCodec::Vp8
            ),
            Self::Mxf => matches!(
                codec,
                VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Mpeg2
                | VideoCodec::ProRes | VideoCodec::DnxHd | VideoCodec::Jpeg2000
            ),
            Self::Asf => matches!(codec, VideoCodec::Vc1 | VideoCodec::Mpeg4),
            _ => false,
        }
    }

    /// Look up a container format by file extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext_lower = ext.to_ascii_lowercase();
        let ext_lower = ext_lower.strip_prefix('.').unwrap_or(&ext_lower);

        use strum::IntoEnumIterator;
        for fmt in Self::iter() {
            if fmt.extensions().iter().any(|e| *e == ext_lower) {
                return Some(fmt);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_extension() {
        assert_eq!(ContainerFormat::from_extension("mp4"), Some(ContainerFormat::Mp4));
        assert_eq!(ContainerFormat::from_extension(".flac"), Some(ContainerFormat::Flac));
        assert_eq!(ContainerFormat::from_extension("MKV"), Some(ContainerFormat::Mkv));
        assert_eq!(ContainerFormat::from_extension("m4a"), Some(ContainerFormat::M4a));
        assert_eq!(ContainerFormat::from_extension("xyz"), None);
    }

    #[test]
    fn test_category() {
        assert_eq!(ContainerFormat::Mp4.category(), MediaCategory::Video);
        assert_eq!(ContainerFormat::Flac.category(), MediaCategory::Audio);
        assert_eq!(ContainerFormat::Png.category(), MediaCategory::Image);
    }

    #[test]
    fn test_codec_container_compatibility() {
        assert!(ContainerFormat::Mp4.supports_audio_codec(AudioCodec::AacLc));
        assert!(ContainerFormat::Mp4.supports_audio_codec(AudioCodec::Alac));
        assert!(!ContainerFormat::WebM.supports_audio_codec(AudioCodec::AacLc));
        assert!(ContainerFormat::WebM.supports_audio_codec(AudioCodec::Opus));
    }

    #[test]
    fn test_hdr_support() {
        assert!(ContainerFormat::Mkv.supports_hdr());
        assert!(!ContainerFormat::Avi.supports_hdr());
    }
}
