use crate::{AlbumReplayGain, ReplayGainResult};

/// Reference loudness level in dB (EBU R128 / ReplayGain 2.0).
const REFERENCE_LOUDNESS: f64 = -18.0;

/// Compute ReplayGain values from a list of per-sample RMS loudness values.
///
/// This is a simplified implementation. For production use, integrate with
/// a full EBU R128 loudness measurement library.
pub fn compute_track_gain(rms_loudness_db: f64, peak: f64) -> ReplayGainResult {
    let gain = REFERENCE_LOUDNESS - rms_loudness_db;
    ReplayGainResult {
        track_gain_db: (gain * 100.0).round() / 100.0,
        track_peak: peak.clamp(0.0, 1.0),
    }
}

/// Compute album-level ReplayGain from individual track results.
pub fn compute_album_gain(tracks: &[ReplayGainResult]) -> AlbumReplayGain {
    if tracks.is_empty() {
        return AlbumReplayGain {
            tracks: Vec::new(),
            album_gain_db: 0.0,
            album_peak: 0.0,
        };
    }

    // Album gain is the average of track gains (simplified)
    let avg_gain = tracks.iter().map(|t| t.track_gain_db).sum::<f64>() / tracks.len() as f64;
    let album_peak = tracks
        .iter()
        .map(|t| t.track_peak)
        .fold(0.0_f64, f64::max);

    AlbumReplayGain {
        tracks: tracks.to_vec(),
        album_gain_db: (avg_gain * 100.0).round() / 100.0,
        album_peak,
    }
}

/// Format a ReplayGain value as a standard string (e.g., "-6.50 dB").
pub fn format_gain(gain_db: f64) -> String {
    format!("{:.2} dB", gain_db)
}

/// Format a peak value as a standard string (e.g., "0.987654").
pub fn format_peak(peak: f64) -> String {
    format!("{:.6}", peak)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_gain_computation() {
        let result = compute_track_gain(-12.0, 0.95);
        assert_eq!(result.track_gain_db, -6.0); // -18 - (-12) = -6
        assert_eq!(result.track_peak, 0.95);
    }

    #[test]
    fn track_gain_quiet_track() {
        let result = compute_track_gain(-24.0, 0.5);
        assert_eq!(result.track_gain_db, 6.0); // -18 - (-24) = 6
    }

    #[test]
    fn album_gain_computation() {
        let tracks = vec![
            compute_track_gain(-12.0, 0.9),
            compute_track_gain(-14.0, 0.95),
            compute_track_gain(-16.0, 0.8),
        ];
        let album = compute_album_gain(&tracks);
        assert_eq!(album.tracks.len(), 3);
        assert_eq!(album.album_peak, 0.95);
        // Average: (-6 + -4 + -2) / 3 = -4.0
        assert_eq!(album.album_gain_db, -4.0);
    }

    #[test]
    fn album_gain_empty() {
        let album = compute_album_gain(&[]);
        assert_eq!(album.album_gain_db, 0.0);
        assert_eq!(album.album_peak, 0.0);
    }

    #[test]
    fn format_gain_string() {
        assert_eq!(format_gain(-6.5), "-6.50 dB");
        assert_eq!(format_gain(3.0), "3.00 dB");
    }

    #[test]
    fn format_peak_string() {
        assert_eq!(format_peak(0.95), "0.950000");
    }

    #[test]
    fn peak_clamped() {
        let result = compute_track_gain(-18.0, 1.5);
        assert_eq!(result.track_peak, 1.0);
    }
}
