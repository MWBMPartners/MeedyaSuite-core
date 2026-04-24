// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Canonical subtitle/caption codec definitions.
// Merged from MeedyaConverter SubtitleFormat.swift (15 cases + ExtendedSubtitle).

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

/// Canonical subtitle codec/format identifier.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumIter, EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum SubtitleCodec {
    // — Text-based —
    /// SubRip (.srt)
    Srt,
    /// WebVTT (.vtt)
    WebVtt,
    /// Advanced SubStation Alpha (.ass)
    Ass,
    /// SubStation Alpha (.ssa)
    Ssa,
    /// TTML / SMPTE-TT (Timed Text Markup Language)
    Ttml,
    /// LRC (Lyrics, line-level sync)
    Lrc,
    /// Enhanced LRC (word-level sync)
    EnhancedLrc,
    /// CEA-608 Closed Captions
    Cea608,
    /// CEA-708 Closed Captions
    Cea708,
    /// EBU STL (European Broadcasting Union Subtitle)
    EbuStl,
    /// SCC (Scenarist Closed Captions)
    Scc,
    /// MCC (MacCaption)
    Mcc,
    /// Teletext subtitles
    Teletext,

    // — Bitmap-based —
    /// VobSub (DVD subtitles)
    VobSub,
    /// PGS / SUP (Blu-ray subtitles)
    Pgs,
    /// DVB Subtitle
    DvbSub,
}

impl SubtitleCodec {
    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Srt => "SubRip (SRT)",
            Self::WebVtt => "WebVTT",
            Self::Ass => "Advanced SubStation Alpha (ASS)",
            Self::Ssa => "SubStation Alpha (SSA)",
            Self::Ttml => "TTML / SMPTE-TT",
            Self::Lrc => "LRC",
            Self::EnhancedLrc => "Enhanced LRC (word-sync)",
            Self::Cea608 => "CEA-608 Closed Captions",
            Self::Cea708 => "CEA-708 Closed Captions",
            Self::EbuStl => "EBU STL",
            Self::Scc => "SCC",
            Self::Mcc => "MCC (MacCaption)",
            Self::Teletext => "Teletext",
            Self::VobSub => "VobSub (DVD)",
            Self::Pgs => "PGS / SUP (Blu-ray)",
            Self::DvbSub => "DVB Subtitle",
        }
    }

    /// Whether this is a bitmap-based subtitle format (as opposed to text).
    pub fn is_bitmap(&self) -> bool {
        matches!(self, Self::VobSub | Self::Pgs | Self::DvbSub)
    }

    /// Whether this is a text-based subtitle format.
    pub fn is_text(&self) -> bool {
        !self.is_bitmap()
    }

    /// Whether this format supports rich formatting (bold, italic, colors).
    pub fn supports_formatting(&self) -> bool {
        matches!(
            self,
            Self::Ass | Self::Ssa | Self::Ttml | Self::WebVtt | Self::Cea708
        )
    }

    /// Typical standalone file extension.
    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::WebVtt => "vtt",
            Self::Ass => "ass",
            Self::Ssa => "ssa",
            Self::Ttml => "ttml",
            Self::Lrc | Self::EnhancedLrc => "lrc",
            Self::Cea608 | Self::Cea708 => "scc",
            Self::EbuStl => "stl",
            Self::Scc => "scc",
            Self::Mcc => "mcc",
            Self::Teletext => "txt",
            Self::VobSub => "sub",
            Self::Pgs => "sup",
            Self::DvbSub => "sub",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_vs_text() {
        assert!(SubtitleCodec::Pgs.is_bitmap());
        assert!(SubtitleCodec::VobSub.is_bitmap());
        assert!(SubtitleCodec::Srt.is_text());
        assert!(SubtitleCodec::Ass.is_text());
    }

    #[test]
    fn test_formatting_support() {
        assert!(SubtitleCodec::Ass.supports_formatting());
        assert!(!SubtitleCodec::Srt.supports_formatting());
    }
}
