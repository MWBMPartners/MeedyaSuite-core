// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.
//
// Short-Time Fourier Transform: slide a window across a signal and take
// the magnitude spectrum of each windowed chunk.
//
// Both tempo and key detection are built on this, but on two DIFFERENT
// STFTs of the same underlying signal, because they want opposite
// trade-offs from the same window/hop pair:
//
// - Tempo wants good TIME resolution (short window, small hop) so it can
//   see exactly when a transient happens — a click 12ms late is a
//   different tempo estimate.
// - Key wants good FREQUENCY resolution (long window, larger hop) so it
//   can tell a note from the semitone next to it — a bass note at C2
//   (65.4 Hz) and its neighbour C#2 (69.3 Hz) are only 3.9 Hz apart, and
//   a short window simply cannot resolve that.
//
// So `onset.rs` and `key.rs` each call this module with their own
// `n`/`hop`, rather than this module baking in one size for everyone.

use std::sync::Arc;

use rustfft::{num_complex::Complex32, Fft, FftPlanner};

/// Build a Hann window of length `n`.
///
/// `w[i] = 0.5 - 0.5*cos(2*pi*i/(n-1))` — zero at both edges, 1.0 at the
/// centre. Used to taper each frame before the FFT so that the frame's
/// hard edges don't leak energy across the whole spectrum (a rectangular
/// window's edges are themselves a sharp transient, which shows up as
/// spurious high-frequency content — "spectral leakage" — in the FFT of
/// anything periodic).
pub fn hann_window(n: usize) -> Vec<f32> {
    if n <= 1 {
        // A window of length 0 or 1 has no "shape" to taper; returning
        // an all-ones window of the right length avoids a divide by
        // zero in the formula below (which divides by `n - 1`) without
        // the caller needing to special-case tiny windows.
        return vec![1.0; n];
    }
    let denom = (n - 1) as f64;
    (0..n)
        .map(|i| {
            let v = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / denom).cos();
            v as f32
        })
        .collect()
}

/// An iterator over magnitude spectra of overlapping, Hann-windowed
/// frames of a real-valued signal.
///
/// Each item is a `Vec<f32>` of length `n/2 + 1` — the magnitude of bins
/// `0` (DC) through `n/2` (Nyquist) inclusive. A real-valued input's FFT
/// is conjugate-symmetric (bin `n-k` is the complex conjugate of bin
/// `k`), so the upper half carries no information the lower half doesn't
/// already have; keeping only the first half saves both the memory and
/// the confusion of exposing bins nobody downstream ever wants.
pub struct StftFrames<'a> {
    signal: &'a [f32],
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    n: usize,
    hop: usize,
    frame_index: usize,
    num_frames: usize,
}

impl<'a> StftFrames<'a> {
    /// Build a frame iterator over `signal` with FFT size `n` and hop
    /// `hop` (the number of samples the window advances between frames;
    /// `hop < n` means frames overlap).
    pub fn new(signal: &'a [f32], n: usize, hop: usize) -> Self {
        let window = hann_window(n);
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let num_frames = frame_count(signal.len(), n, hop);
        Self {
            signal,
            window,
            fft,
            n,
            hop,
            frame_index: 0,
            num_frames,
        }
    }

    /// Total number of frames this iterator will yield. Exposed
    /// separately from `Iterator::size_hint` so callers that need to
    /// pre-size a buffer (e.g. `onset.rs`'s spectral flux, which needs
    /// one flux value per frame) don't have to collect into a `Vec`
    /// first just to find out how big it needs to be.
    pub fn num_frames(&self) -> usize {
        self.num_frames
    }
}

/// Number of frames a signal of length `signal_len` yields at FFT size
/// `n` and hop `hop`: `(signal_len - n) / hop + 1` when the signal is at
/// least `n` samples long, else zero (there isn't even one full window).
fn frame_count(signal_len: usize, n: usize, hop: usize) -> usize {
    if signal_len < n {
        0
    } else {
        (signal_len - n) / hop + 1
    }
}

impl Iterator for StftFrames<'_> {
    type Item = Vec<f32>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.frame_index >= self.num_frames {
            return None;
        }
        let start = self.frame_index * self.hop;

        // Window the frame and lift it into the complex domain (a
        // real-valued FFT input still needs a complex buffer for
        // rustfft, which only implements complex-to-complex transforms;
        // the imaginary part is simply zero going in).
        let mut buffer: Vec<Complex32> = self.signal[start..start + self.n]
            .iter()
            .zip(self.window.iter())
            .map(|(&s, &w)| Complex32::new(s * w, 0.0))
            .collect();

        self.fft.process(&mut buffer);

        let bins = self.n / 2 + 1;
        let magnitudes = buffer[..bins].iter().map(|c| c.norm()).collect();

        self.frame_index += 1;
        Some(magnitudes)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.num_frames - self.frame_index;
        (remaining, Some(remaining))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_window_shape() {
        let w = hann_window(8);
        assert_eq!(w.len(), 8);
        // Zero at both edges.
        assert!(w[0].abs() < 1e-6, "first sample should be ~0, got {}", w[0]);
        assert!(w[7].abs() < 1e-6, "last sample should be ~0, got {}", w[7]);
        // Symmetric.
        for i in 0..4 {
            assert!(
                (w[i] - w[7 - i]).abs() < 1e-6,
                "window should be symmetric: w[{i}]={} vs w[{}]={}",
                w[i],
                7 - i,
                w[7 - i]
            );
        }
    }

    #[test]
    fn hann_window_handles_degenerate_lengths() {
        assert_eq!(hann_window(0), Vec::<f32>::new());
        assert_eq!(hann_window(1), vec![1.0]);
    }

    #[test]
    fn frame_count_matches_formula() {
        // (len - n) / hop + 1
        assert_eq!(frame_count(1024, 1024, 128), 1);
        assert_eq!(frame_count(1024 + 128, 1024, 128), 2);
        assert_eq!(frame_count(500, 1024, 128), 0, "shorter than one window");
        assert_eq!(frame_count(10_000, 1024, 128), (10_000 - 1024) / 128 + 1);
    }

    #[test]
    fn stft_frame_count_matches_iterator_output() {
        let signal = vec![0.0f32; 10_000];
        let frames = StftFrames::new(&signal, 1024, 128);
        let expected = frame_count(10_000, 1024, 128);
        assert_eq!(frames.num_frames(), expected);
        assert_eq!(frames.count(), expected);
    }

    #[test]
    fn stft_sine_peaks_in_the_right_bin() {
        // A 1kHz tone at 11025 Hz sampling, N=1024: bin spacing is
        // 11025/1024 ~= 10.77 Hz/bin, so 1000 Hz should land at bin
        // round(1000/10.77) = 93.
        let sample_rate = 11_025.0;
        let n = 1024usize;
        let freq = 1000.0;
        let seconds = 1.0;
        let len = (sample_rate * seconds) as usize;
        let signal: Vec<f32> = (0..len)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / sample_rate).sin() as f32)
            .collect();

        let mut frames = StftFrames::new(&signal, n, 128);
        let frame = frames.next().expect("expected at least one frame");

        let expected_bin = (freq * n as f64 / sample_rate).round() as usize;
        let peak_bin = frame
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        assert_eq!(
            peak_bin, expected_bin,
            "expected the 1kHz tone's peak at bin {expected_bin}, got {peak_bin}"
        );
    }

    // ----------------------------------------------------------
    // API surface pin
    // ----------------------------------------------------------

    #[test]
    fn rustfft_api_surface_unchanged() {
        // Pins the specific rustfft calls this module depends on. If
        // upstream renames `FftPlanner::new` or `plan_fft_forward`,
        // compilation here breaks first — before it breaks silently
        // inside `StftFrames::new`.
        let mut planner = FftPlanner::<f32>::new();
        let _fft = planner.plan_fft_forward(1024);
    }
}
