// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// HDR format definitions.
// From MeedyaConverter MediaStream.swift HDRFormat enum (7 cases).

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

/// High Dynamic Range format.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumIter, EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum HdrFormat {
    /// HDR10 (static metadata, SMPTE ST 2084)
    Hdr10,
    /// HDR10+ (dynamic metadata, Samsung)
    Hdr10Plus,
    /// Dolby Vision
    DolbyVision,
    /// Dolby Vision + HDR10 dual-layer
    DolbyVisionHdr10,
    /// HLG (Hybrid Log-Gamma, BBC/NHK)
    Hlg,
    /// PQ10 (PQ transfer without metadata)
    Pq10,
    /// SDR (not HDR, but included for completeness in classification)
    Sdr,
}

impl HdrFormat {
    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Hdr10 => "HDR10",
            Self::Hdr10Plus => "HDR10+",
            Self::DolbyVision => "Dolby Vision",
            Self::DolbyVisionHdr10 => "Dolby Vision + HDR10",
            Self::Hlg => "HLG",
            Self::Pq10 => "PQ10",
            Self::Sdr => "SDR",
        }
    }

    /// Whether this is a true HDR format (not SDR).
    pub fn is_hdr(&self) -> bool {
        !matches!(self, Self::Sdr)
    }

    /// Whether this HDR format uses dynamic metadata (per-scene).
    pub fn is_dynamic(&self) -> bool {
        matches!(
            self,
            Self::Hdr10Plus | Self::DolbyVision | Self::DolbyVisionHdr10
        )
    }

    /// Whether this format requires proprietary licensing.
    pub fn requires_license(&self) -> bool {
        matches!(
            self,
            Self::DolbyVision | Self::DolbyVisionHdr10 | Self::Hdr10Plus
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hdr_classification() {
        assert!(HdrFormat::Hdr10.is_hdr());
        assert!(HdrFormat::DolbyVision.is_hdr());
        assert!(!HdrFormat::Sdr.is_hdr());
    }

    #[test]
    fn test_dynamic_metadata() {
        assert!(HdrFormat::DolbyVision.is_dynamic());
        assert!(HdrFormat::Hdr10Plus.is_dynamic());
        assert!(!HdrFormat::Hdr10.is_dynamic());
    }
}
