//! Ventana deslizante de muestras por objetivo (§6.2.1).

use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WindowStats {
    pub samples: usize,
    pub valid: usize,
    pub last: Option<f64>,
    pub min: Option<f64>,
    pub median: Option<f64>,
    pub p95: Option<f64>,
    pub loss_pct: f64,
}

#[derive(Debug, Clone)]
pub struct SlidingWindow {
    cap: usize,
    buf: VecDeque<Option<f64>>,
}

impl SlidingWindow {
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "la ventana necesita al menos una muestra");
        Self { cap, buf: VecDeque::with_capacity(cap) }
    }

    pub fn push(&mut self, rtt_ms: Option<f64>) {
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(rtt_ms);
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn stats(&self) -> WindowStats {
        let samples = self.buf.len();
        let mut valid: Vec<f64> = self.buf.iter().flatten().copied().collect();
        valid.sort_by(|a, b| a.total_cmp(b));
        let lost = samples - valid.len();
        WindowStats {
            samples,
            valid: valid.len(),
            last: self.buf.back().copied().flatten(),
            min: valid.first().copied(),
            median: percentile(&valid, 50.0),
            p95: percentile(&valid, 95.0),
            loss_pct: if samples == 0 { 0.0 } else { lost as f64 * 100.0 / samples as f64 },
        }
    }
}

/// Percentil por rango más cercano sobre un slice **ordenado**.
pub fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    let idx = rank.clamp(1, sorted.len()) - 1;
    Some(sorted[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_window_has_no_data() {
        let w = SlidingWindow::new(60);
        let s = w.stats();
        assert_eq!(s.samples, 0);
        assert_eq!(s.p95, None);
        assert_eq!(s.loss_pct, 0.0);
    }

    #[test]
    fn evicts_oldest() {
        let mut w = SlidingWindow::new(3);
        for x in [1.0, 2.0, 3.0, 4.0] {
            w.push(Some(x));
        }
        let s = w.stats();
        assert_eq!(s.samples, 3);
        assert_eq!(s.min, Some(2.0));
        assert_eq!(s.last, Some(4.0));
    }

    #[test]
    fn percentiles_nearest_rank() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&v, 50.0), Some(50.0));
        assert_eq!(percentile(&v, 95.0), Some(95.0));
        assert_eq!(percentile(&[7.0], 95.0), Some(7.0));
    }

    #[test]
    fn loss_counts_none() {
        let mut w = SlidingWindow::new(10);
        for i in 0..10 {
            w.push(if i % 5 == 0 { None } else { Some(10.0) });
        }
        let s = w.stats();
        assert_eq!(s.valid, 8);
        assert!((s.loss_pct - 20.0).abs() < 1e-9);
        assert_eq!(s.last, Some(10.0));
    }
}
