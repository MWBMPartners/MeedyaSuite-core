// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Spatial audio format definitions.
// From MeedyaManager issue #131 and MeedyaConverter SpatialAudioProcessor.swift.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

/// Spatial / immersive audio format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Display, EnumIter, EnumString)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum SpatialAudioFormat {
    /// Dolby Atmos (object-based, via E-AC-3 JOC or TrueHD)
    DolbyAtmos,
    /// DTS:X (object-based)
    DtsX,
    /// DTS:X IMAX Enhanced
    DtsXImax,
    /// MPEG-H 3D Audio (object + scene-based)
    MpegH3d,
    /// Sony 360 Reality Audio (object-based, MPEG-H)
    Sony360Ra,
    /// Apple Spatial Audio (binaural rendering of Atmos)
    AppleSpatial,
    /// Auro-3D (channel-based immersive)
    Auro3d,
    /// IAMF (Immersive Audio Model and Formats, open standard)
    Iamf,
    /// Ambisonics (scene-based, various orders)
    Ambisonics,
}

impl SpatialAudioFormat {
    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::DolbyAtmos => "Dolby Atmos",
            Self::DtsX => "DTS:X",
            Self::DtsXImax => "DTS:X IMAX Enhanced",
            Self::MpegH3d => "MPEG-H 3D Audio",
            Self::Sony360Ra => "Sony 360 Reality Audio",
            Self::AppleSpatial => "Apple Spatial Audio",
            Self::Auro3d => "Auro-3D",
            Self::Iamf => "IAMF",
            Self::Ambisonics => "Ambisonics",
        }
    }

    /// Whether this format uses object-based audio (as opposed to channel/scene).
    pub fn is_object_based(&self) -> bool {
        matches!(
            self,
            Self::DolbyAtmos
                | Self::DtsX
                | Self::DtsXImax
                | Self::MpegH3d
                | Self::Sony360Ra
                | Self::Iamf
        )
    }

    /// Whether this format uses scene-based audio (ambisonics etc.).
    pub fn is_scene_based(&self) -> bool {
        matches!(self, Self::Ambisonics | Self::MpegH3d)
    }

    /// Whether this format requires proprietary licensing.
    pub fn requires_license(&self) -> bool {
        matches!(
            self,
            Self::DolbyAtmos
                | Self::DtsX
                | Self::DtsXImax
                | Self::Sony360Ra
                | Self::Auro3d
        )
    }
}
