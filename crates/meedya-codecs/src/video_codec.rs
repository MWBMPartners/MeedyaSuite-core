// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Canonical video codec definitions.
// Merged from MeedyaConverter VideoCodec.swift (21 cases + ExtendedVideoCodec)
// and MeedyaManager classify/mod.rs.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

/// Canonical video codec identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Display, EnumIter, EnumString)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum VideoCodec {
    // — Modern codecs —
    /// H.264 / AVC
    H264,
    /// H.265 / HEVC
    H265,
    /// H.266 / VVC (Versatile Video Coding)
    H266,
    /// AV1 (AOMedia Video 1)
    Av1,
    /// VP8
    Vp8,
    /// VP9
    Vp9,

    // — Legacy codecs —
    /// MPEG-2
    Mpeg2,
    /// MPEG-4 Part 2 (DivX, Xvid)
    Mpeg4,
    /// VC-1 (Windows Media Video 9)
    Vc1,
    /// Theora
    Theora,

    // — Professional / lossless codecs —
    /// Apple ProRes (422, 4444, etc.)
    ProRes,
    /// DNxHD / DNxHR
    DnxHd,
    /// CineForm
    CineForm,
    /// FFV1 (FF Video Codec 1, lossless archival)
    Ffv1,
    /// JPEG 2000
    Jpeg2000,
    /// Huffyuv (lossless)
    Huffyuv,

    // — Stereoscopic / immersive —
    /// MV-HEVC (Multiview HEVC, Apple Vision Pro)
    MvHevc,
    /// MV-H264 (Multiview H.264)
    MvH264,

    // — Still image codecs (in video containers) —
    /// MJPEG (Motion JPEG)
    Mjpeg,
    /// PNG (for embedded cover art / stills)
    Png,
    /// WebP
    WebP,
}

impl VideoCodec {
    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::H264 => "H.264 / AVC",
            Self::H265 => "H.265 / HEVC",
            Self::H266 => "H.266 / VVC",
            Self::Av1 => "AV1",
            Self::Vp8 => "VP8",
            Self::Vp9 => "VP9",
            Self::Mpeg2 => "MPEG-2",
            Self::Mpeg4 => "MPEG-4",
            Self::Vc1 => "VC-1",
            Self::Theora => "Theora",
            Self::ProRes => "Apple ProRes",
            Self::DnxHd => "DNxHD / DNxHR",
            Self::CineForm => "CineForm",
            Self::Ffv1 => "FFV1",
            Self::Jpeg2000 => "JPEG 2000",
            Self::Huffyuv => "Huffyuv",
            Self::MvHevc => "MV-HEVC",
            Self::MvH264 => "MV-H264",
            Self::Mjpeg => "Motion JPEG",
            Self::Png => "PNG",
            Self::WebP => "WebP",
        }
    }

    /// FFmpeg encoder name (if available).
    pub fn ffmpeg_encoder(&self) -> Option<&'static str> {
        match self {
            Self::H264 => Some("libx264"),
            Self::H265 => Some("libx265"),
            Self::Av1 => Some("libsvtav1"),
            Self::Vp8 => Some("libvpx"),
            Self::Vp9 => Some("libvpx-vp9"),
            Self::Mpeg2 => Some("mpeg2video"),
            Self::Mpeg4 => Some("mpeg4"),
            Self::ProRes => Some("prores_ks"),
            Self::DnxHd => Some("dnxhd"),
            Self::Ffv1 => Some("ffv1"),
            Self::Jpeg2000 => Some("libopenjpeg"),
            Self::Huffyuv => Some("huffyuv"),
            Self::Mjpeg => Some("mjpeg"),
            Self::Theora => Some("libtheora"),
            _ => None,
        }
    }

    /// FFmpeg decoder name.
    pub fn ffmpeg_decoder(&self) -> Option<&'static str> {
        match self {
            Self::H264 => Some("h264"),
            Self::H265 | Self::MvHevc => Some("hevc"),
            Self::Av1 => Some("libdav1d"),
            Self::Vp8 => Some("libvpx"),
            Self::Vp9 => Some("libvpx-vp9"),
            Self::Mpeg2 => Some("mpeg2video"),
            Self::Mpeg4 => Some("mpeg4"),
            Self::Vc1 => Some("vc1"),
            Self::ProRes => Some("prores"),
            Self::DnxHd => Some("dnxhd"),
            Self::CineForm => Some("cfhd"),
            Self::Ffv1 => Some("ffv1"),
            Self::Jpeg2000 => Some("libopenjpeg"),
            Self::Huffyuv => Some("huffyuv"),
            Self::MvH264 => Some("h264"),
            Self::Mjpeg => Some("mjpeg"),
            Self::Theora => Some("theora"),
            Self::Png => Some("png"),
            Self::WebP => Some("webp"),
            Self::H266 => None,
        }
    }

    /// Whether this codec is lossless.
    pub fn is_lossless(&self) -> bool {
        matches!(
            self,
            Self::Ffv1 | Self::Huffyuv | Self::Png
        )
    }

    /// Whether this codec supports HDR content.
    pub fn supports_hdr(&self) -> bool {
        matches!(
            self,
            Self::H265
                | Self::Av1
                | Self::Vp9
                | Self::H266
                | Self::MvHevc
                | Self::ProRes
        )
    }

    /// Whether this codec can use hardware acceleration (VideoToolbox on macOS).
    pub fn supports_videotoolbox(&self) -> bool {
        matches!(
            self,
            Self::H264 | Self::H265 | Self::ProRes | Self::MvHevc
        )
    }

    /// Whether this codec supports stereoscopic 3D content.
    pub fn is_stereoscopic(&self) -> bool {
        matches!(self, Self::MvHevc | Self::MvH264)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_roundtrip() {
        let codec = VideoCodec::H265;
        let s = serde_json::to_string(&codec).unwrap();
        let back: VideoCodec = serde_json::from_str(&s).unwrap();
        assert_eq!(codec, back);
    }

    #[test]
    fn test_from_str() {
        assert_eq!(VideoCodec::from_str("av1").unwrap(), VideoCodec::Av1);
    }

    #[test]
    fn test_hdr_support() {
        assert!(VideoCodec::H265.supports_hdr());
        assert!(VideoCodec::Av1.supports_hdr());
        assert!(!VideoCodec::H264.supports_hdr());
    }
}
