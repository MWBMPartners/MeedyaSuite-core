// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// Coarse-grained spatial audio detection categories. This is distinct from
// [`crate::SpatialAudioFormat`], which is a fine-grained taxonomy (Atmos,
// DTS:X, MPEG-H, Auro-3D, ...). `SpatialType` is the bucket you get from
// probing tools like MediaInfo and is the level at which downstream apps
// usually branch.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpatialType {
    Stereo,
    DolbyDigital,
    DolbyAtmos,
}
