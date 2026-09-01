// Copyright (c) 2026 MeedyaSuite
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
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::FingerprintError;

/// Default ReplayGain reference level in LUFS (EBU R128 standard).
pub const DEFAULT_REFERENCE_LEVEL: f64 = -18.0;

/// Default timeout for a single FFmpeg loudness analysis pass.
///
/// This is intentionally much longer than the 30s timeout used by the
/// probe-style helpers in `meedya-codecs` (ffprobe/mediainfo). Those just
/// read container metadata; this decodes the ENTIRE audio file through the
/// `ebur128` filter to measure loudness, which is inherently proportional
/// to track length and decode speed. A long DJ mix analyzed on a slow
/// external volume can legitimately take several minutes — 30s would
/// produce false-positive timeouts on perfectly healthy runs. Ten minutes
/// still bounds a genuine hang (stuck process, corrupt file that makes
/// FFmpeg spin) to a well-defined worst case.
pub const DEFAULT_ANALYSIS_TIMEOUT: Duration = Duration::from_secs(600);

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
    timeout: Duration,
}

impl ReplayGainAnalyzer {
    /// Create a new analyzer with the given FFmpeg binary path.
    pub fn new(ffmpeg_path: impl Into<String>) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.into(),
            reference_level: DEFAULT_REFERENCE_LEVEL,
            timeout: DEFAULT_ANALYSIS_TIMEOUT,
        }
    }

    /// Set a custom reference level (default: -18.0 LUFS).
    pub fn with_reference_level(mut self, level: f64) -> Self {
        self.reference_level = level;
        self
    }

    /// Set a custom analysis timeout (default: [`DEFAULT_ANALYSIS_TIMEOUT`], 10 minutes).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Analyze a single audio file for loudness.
    ///
    /// The FFmpeg subprocess is bounded by `self.timeout` (10 minutes by
    /// default — see [`DEFAULT_ANALYSIS_TIMEOUT`]). On timeout the child is
    /// killed (`kill_on_drop(true)`) rather than left running, and this
    /// returns `FingerprintError::FfmpegTimeout`.
    pub async fn analyze_track(
        &self,
        file_path: &Path,
    ) -> Result<ReplayGainResult, FingerprintError> {
        let output = tokio::time::timeout(
            self.timeout,
            Command::new(&self.ffmpeg_path)
                .arg("-i")
                .arg(file_path)
                .args(["-af", "ebur128=peak=true", "-f", "null", "-"])
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                // Dropping the `.output()` future on timeout does NOT kill the
                // child by itself — only `kill_on_drop` does that. Without this,
                // a timed-out analysis leaves a still-running FFmpeg behind.
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_elapsed| FingerprintError::FfmpegTimeout {
            seconds: self.timeout.as_secs(),
        })?
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

    #[test]
    fn test_with_timeout_overrides_default() {
        let analyzer = ReplayGainAnalyzer::new("ffmpeg");
        assert_eq!(analyzer.timeout, DEFAULT_ANALYSIS_TIMEOUT);

        let analyzer = analyzer.with_timeout(Duration::from_secs(5));
        assert_eq!(analyzer.timeout, Duration::from_secs(5));
    }

    // Points `ffmpeg_path` at a shell script that sleeps far longer than the
    // configured timeout, so a passing run proves the timeout — not the
    // script finishing — is what ends `analyze_track`. `kill_on_drop(true)`
    // means the sleeping child is killed the moment the timeout elapses, so
    // this resolves in well under a second rather than waiting out the sleep.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_timeout_is_deterministic_and_fast() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let script_path =
            std::env::temp_dir().join(format!("meedya_rg_test_sleep_{}.sh", std::process::id()));
        fs::write(&script_path, "#!/bin/sh\nsleep 5\n").expect("write sleep script");
        let mut perms = fs::metadata(&script_path)
            .expect("stat sleep script")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod sleep script");

        let analyzer = ReplayGainAnalyzer::new(script_path.to_string_lossy().into_owned())
            .with_timeout(Duration::from_millis(200));

        let start = std::time::Instant::now();
        let result = analyzer
            .analyze_track(Path::new("/tmp/meedya-rg-test-input.wav"))
            .await;
        let elapsed = start.elapsed();

        let _ = fs::remove_file(&script_path);

        assert!(
            matches!(result, Err(FingerprintError::FfmpegTimeout { .. })),
            "expected FfmpegTimeout, got {result:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "timeout should short-circuit the sleep, took {elapsed:?}"
        );
    }

    // Non-UTF-8 paths must reach `Command` as an `OsStr`/`Path`, not be
    // rejected up front by a `to_str()` check. The chosen ffmpeg binary
    // doesn't exist, so the call still fails — the assertion is only that it
    // does NOT fail with the old "Invalid file path" error.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_non_utf8_path_is_not_rejected_up_front() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let bad_name = OsStr::from_bytes(&[b't', b'r', 0xFF, 0xFE, b'.', b'w', b'a', b'v']);
        let path = std::env::temp_dir().join(bad_name);

        let analyzer = ReplayGainAnalyzer::new("meedya-fingerprint-test-nonexistent-ffmpeg")
            .with_timeout(Duration::from_millis(500));

        let result = analyzer.analyze_track(&path).await;

        if let Err(FingerprintError::FfmpegError(msg)) = &result {
            assert!(
                !msg.contains("Invalid file path"),
                "non-UTF-8 path should not trigger the removed path-validation error, got: {msg}"
            );
        }
        // Any other outcome (e.g. FfmpegNotFound, since the binary above
        // doesn't exist) is fine — this test only guards the removed check.
    }
}
