// Copyright (c) 2026 MWBMPartners
// Licensed under the MIT License.
//
// ReplayGain loudness analysis via FFmpeg EBU R128.
// Extracted from MeedyaDL replaygain_service.rs.
//
// Measures integrated loudness (LUFS) and true peak (dBFS) using
// FFmpeg's ebur128 audio filter, then calculates ReplayGain adjustments.
// Results are returned as structs — consumers write them to files using
// the appropriate tag format (MP4 atoms, Vorbis Comments, or ID3v2 TXXX).

use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::FingerprintError;

/// Default ReplayGain reference level in LUFS (EBU R128 standard).
pub const DEFAULT_REFERENCE_LEVEL: f64 = -18.0;

/// Result of a ReplayGain loudness analysis for a single track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayGainResult {
    /// Integrated loudness in LUFS (e.g., -14.2).
    pub integrated_loudness: f64,
    /// True peak in linear scale (e.g., 0.933254).
    pub true_peak: f64,
    /// Calculated gain adjustment in dB (e.g., -3.80).
    pub gain_db: f64,
    /// Reference level used for calculation (default: -18.0 LUFS).
    pub reference_level: f64,
}

impl ReplayGainResult {
    /// Format the gain as a ReplayGain-standard string (e.g., "-3.80 dB").
    pub fn gain_string(&self) -> String {
        format!("{:.2} dB", self.gain_db)
    }

    /// Format the peak as a ReplayGain-standard string (e.g., "0.933254").
    pub fn peak_string(&self) -> String {
        format!("{:.6}", self.true_peak)
    }

    /// Whether clipping would occur without gain reduction.
    pub fn would_clip(&self) -> bool {
        self.true_peak > 1.0 || (self.true_peak * 10f64.powf(self.gain_db / 20.0)) > 1.0
    }
}

/// Album-level ReplayGain result computed from multiple track results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumGainResult {
    /// Album-level integrated loudness (average across tracks).
    pub integrated_loudness: f64,
    /// Album-level true peak (maximum across tracks).
    pub true_peak: f64,
    /// Album-level gain adjustment in dB.
    pub gain_db: f64,
    /// Reference level used.
    pub reference_level: f64,
}

impl AlbumGainResult {
    /// Format the gain as a ReplayGain-standard string.
    pub fn gain_string(&self) -> String {
        format!("{:.2} dB", self.gain_db)
    }

    /// Format the peak as a ReplayGain-standard string.
    pub fn peak_string(&self) -> String {
        format!("{:.6}", self.true_peak)
    }
}

/// ReplayGain loudness analyzer using FFmpeg's EBU R128 filter.
pub struct ReplayGainAnalyzer {
    ffmpeg_path: String,
    reference_level: f64,
}

impl ReplayGainAnalyzer {
    /// Create a new analyzer with the given FFmpeg binary path.
    pub fn new(ffmpeg_path: impl Into<String>) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.into(),
            reference_level: DEFAULT_REFERENCE_LEVEL,
        }
    }

    /// Set a custom reference level (default: -18.0 LUFS).
    pub fn with_reference_level(mut self, level: f64) -> Self {
        self.reference_level = level;
        self
    }

    /// Analyze a single audio file for loudness.
    pub async fn analyze_track(
        &self,
        file_path: &Path,
    ) -> Result<ReplayGainResult, FingerprintError> {
        let output = Command::new(&self.ffmpeg_path)
            .args([
                "-i",
                file_path
                    .to_str()
                    .ok_or_else(|| FingerprintError::FfmpegError("Invalid file path".into()))?,
                "-af",
                "ebur128=peak=true",
                "-f",
                "null",
                "-",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    FingerprintError::FfmpegNotFound(self.ffmpeg_path.clone())
                } else {
                    FingerprintError::FfmpegError(e.to_string())
                }
            })?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        parse_ebur128_output(&stderr, self.reference_level)
    }

    /// Compute album-level gain from a set of track results.
    ///
    /// Album loudness is the energy-weighted average of all tracks.
    /// Album peak is the maximum peak across all tracks.
    pub fn compute_album_gain(&self, tracks: &[ReplayGainResult]) -> Option<AlbumGainResult> {
        if tracks.is_empty() {
            return None;
        }

        // Energy-weighted average: convert LUFS to linear, average, convert back
        let total_energy: f64 = tracks
            .iter()
            .map(|t| 10f64.powf(t.integrated_loudness / 10.0))
            .sum();
        let avg_loudness = 10.0 * (total_energy / tracks.len() as f64).log10();

        let max_peak = tracks.iter().map(|t| t.true_peak).fold(0.0f64, f64::max);

        let gain_db = self.reference_level - avg_loudness;

        Some(AlbumGainResult {
            integrated_loudness: avg_loudness,
            true_peak: max_peak,
            gain_db,
            reference_level: self.reference_level,
        })
    }
}

/// Parse FFmpeg ebur128 filter output to extract loudness and peak values.
fn parse_ebur128_output(
    stderr: &str,
    reference_level: f64,
) -> Result<ReplayGainResult, FingerprintError> {
    // FFmpeg ebur128 summary line format:
    //   [Parsed_ebur128_0 @ 0x...] Summary:
    //     Integrated loudness:
    //       I:         -14.2 LUFS
    //     True peak:
    //       Peak:        -0.6 dBFS

    let integrated = parse_summary_value(stderr, "I:").ok_or_else(|| {
        FingerprintError::LoudnessParseError(
            "Could not find integrated loudness (I:) in FFmpeg output".into(),
        )
    })?;

    let peak_dbfs = parse_summary_value(stderr, "Peak:").ok_or_else(|| {
        FingerprintError::LoudnessParseError(
            "Could not find true peak (Peak:) in FFmpeg output".into(),
        )
    })?;

    // Convert dBFS to linear scale
    let true_peak = 10f64.powf(peak_dbfs / 20.0);
    let gain_db = reference_level - integrated;

    Ok(ReplayGainResult {
        integrated_loudness: integrated,
        true_peak,
        gain_db,
        reference_level,
    })
}

/// Extract a numeric value from FFmpeg ebur128 summary output.
fn parse_summary_value(output: &str, label: &str) -> Option<f64> {
    // Find the LAST occurrence (the summary, not per-frame measurements)
    let pos = output.rfind(label)?;
    let after = &output[pos + label.len()..];

    // Skip whitespace and extract the number
    let trimmed = after.trim_start();
    let end = trimmed
        .find(|c: char| !c.is_ascii_digit() && c != '-' && c != '.')
        .unwrap_or(trimmed.len());

    trimmed[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gain_string_format() {
        let result = ReplayGainResult {
            integrated_loudness: -14.2,
            true_peak: 0.933,
            gain_db: -3.80,
            reference_level: -18.0,
        };
        assert_eq!(result.gain_string(), "-3.80 dB");
        assert_eq!(result.peak_string(), "0.933000");
    }

    #[test]
    fn test_clipping_detection() {
        let loud = ReplayGainResult {
            integrated_loudness: -8.0,
            true_peak: 1.2,
            gain_db: -10.0,
            reference_level: -18.0,
        };
        assert!(loud.would_clip());

        let quiet = ReplayGainResult {
            integrated_loudness: -22.0,
            true_peak: 0.5,
            gain_db: 4.0,
            reference_level: -18.0,
        };
        assert!(!quiet.would_clip());
    }

    #[test]
    fn test_parse_ebur128_output() {
        let ffmpeg_output = r#"
[Parsed_ebur128_0 @ 0x7f9b0c] Summary:

  Integrated loudness:
    I:         -14.2 LUFS
    Threshold: -24.2 LUFS

  Loudness range:
    LRA:         7.3 LU
    Threshold:  -34.2 LUFS
    LRA low:   -18.5 LUFS
    LRA high:  -11.2 LUFS

  True peak:
    Peak:        -0.6 dBFS
"#;
        let result = parse_ebur128_output(ffmpeg_output, -18.0).unwrap();
        assert!((result.integrated_loudness - (-14.2)).abs() < 0.01);
        assert!((result.gain_db - (-3.8)).abs() < 0.01);
        // -0.6 dBFS = 10^(-0.6/20) ≈ 0.933
        assert!((result.true_peak - 0.933).abs() < 0.01);
    }

    #[test]
    fn test_album_gain_computation() {
        let analyzer = ReplayGainAnalyzer::new("ffmpeg");
        let tracks = vec![
            ReplayGainResult {
                integrated_loudness: -14.0,
                true_peak: 0.9,
                gain_db: -4.0,
                reference_level: -18.0,
            },
            ReplayGainResult {
                integrated_loudness: -16.0,
                true_peak: 0.8,
                gain_db: -2.0,
                reference_level: -18.0,
            },
        ];
        let album = analyzer.compute_album_gain(&tracks).unwrap();
        assert!(
            album.gain_db < 0.0,
            "Album with loud tracks should have negative gain"
        );
        assert_eq!(
            album.true_peak, 0.9,
            "Album peak should be max of track peaks"
        );
    }

    #[test]
    fn test_empty_album_returns_none() {
        let analyzer = ReplayGainAnalyzer::new("ffmpeg");
        assert!(analyzer.compute_album_gain(&[]).is_none());
    }
}
