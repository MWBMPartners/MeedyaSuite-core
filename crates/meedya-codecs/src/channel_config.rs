// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.

use serde::{Deserialize, Serialize};

/// Channel configuration: number of channels plus a human-readable label.
///
/// Use [`ChannelConfig::from_count`] to construct from a raw channel count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub channels: u32,
    pub label: String,
}

impl ChannelConfig {
    pub fn from_count(channels: u32) -> Self {
        let label = match channels {
            1 => "1.0".to_string(),
            2 => "2.0".to_string(),
            6 => "5.1".to_string(),
            8 => "7.1".to_string(),
            n => format!("{n}ch"),
        };
        Self { channels, label }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_known_counts() {
        assert_eq!(ChannelConfig::from_count(1).label, "1.0");
        assert_eq!(ChannelConfig::from_count(2).label, "2.0");
        assert_eq!(ChannelConfig::from_count(6).label, "5.1");
        assert_eq!(ChannelConfig::from_count(8).label, "7.1");
    }

    #[test]
    fn labels_unknown_counts() {
        assert_eq!(ChannelConfig::from_count(4).label, "4ch");
        assert_eq!(ChannelConfig::from_count(12).label, "12ch");
    }
}
