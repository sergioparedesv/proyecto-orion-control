//! Jitter según RFC 3550 §6.4.1, derivado de los RTT medidos (§6.2.2).
//!
//! `D = |RTT_i − RTT_{i−1}|`, `J = J + (D − J) / 16`.
//! Las pérdidas no se usan: la diferencia se toma contra la última muestra válida.

#[derive(Debug, Clone, Default)]
pub struct Jitter {
    j: f64,
    prev: Option<f64>,
    updates: u32,
}

impl Jitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, rtt_ms: Option<f64>) {
        let Some(x) = rtt_ms else { return };
        if let Some(p) = self.prev {
            let d = (x - p).abs();
            self.j += (d - self.j) / 16.0;
            self.updates += 1;
        }
        self.prev = Some(x);
    }

    /// `None` hasta tener al menos dos muestras válidas.
    pub fn value(&self) -> Option<f64> {
        (self.updates > 0).then_some(self.j)
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_rtt_has_zero_jitter() {
        let mut j = Jitter::new();
        for _ in 0..50 {
            j.update(Some(30.0));
        }
        assert_eq!(j.value(), Some(0.0));
    }

    #[test]
    fn needs_two_samples() {
        let mut j = Jitter::new();
        assert_eq!(j.value(), None);
        j.update(Some(10.0));
        assert_eq!(j.value(), None);
        j.update(None);
        assert_eq!(j.value(), None);
    }

    #[test]
    fn known_vector() {
        // 10, 20 → D=10 → J=0.625 ; 20, 10 → D=10 → J=0.625+(10-0.625)/16=1.2109375
        let mut j = Jitter::new();
        j.update(Some(10.0));
        j.update(Some(20.0));
        assert!((j.value().unwrap() - 0.625).abs() < 1e-12);
        j.update(None); // la pérdida no rompe la cadena
        j.update(Some(10.0));
        assert!((j.value().unwrap() - 1.2109375).abs() < 1e-12);
    }

    #[test]
    fn alternating_converges_to_difference() {
        let mut j = Jitter::new();
        for i in 0..2000 {
            j.update(Some(if i % 2 == 0 { 20.0 } else { 30.0 }));
        }
        assert!((j.value().unwrap() - 10.0).abs() < 1e-6);
    }
}
