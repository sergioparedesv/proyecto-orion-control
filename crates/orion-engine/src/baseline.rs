//! Línea base estadística EWMA (media + varianza) por red, objetivo y franja (§6.2.3).

use serde::{Deserialize, Serialize};

use crate::types::{BaselineView, TargetId};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BaselineConfig {
    pub alpha: f64,
    pub sigma_min_ms: f64,
    pub k: f64,
    pub converge_after: u32,
    /// z por encima del cual una muestra se considera outlier y no actualiza la línea base.
    pub outlier_z: f64,
}

impl Default for BaselineConfig {
    fn default() -> Self {
        Self { alpha: 0.05, sigma_min_ms: 1.0, k: 3.0, converge_after: 300, outlier_z: 4.0 }
    }
}

/// Franjas de 4 horas: 0 = 00-04, 1 = 04-08, ..., 5 = 20-24.
pub const SLOTS: u8 = 6;

pub fn slot_for_hour(hour_local: u8) -> u8 {
    (hour_local % 24) / 4
}

pub fn slot_label(slot: u8) -> &'static str {
    match slot {
        0 => "00:00-04:00",
        1 => "04:00-08:00",
        2 => "08:00-12:00",
        3 => "12:00-16:00",
        4 => "16:00-20:00",
        _ => "20:00-24:00",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BaselineKey {
    pub network_id: i64,
    pub target_id: TargetId,
    pub slot: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Baseline {
    pub mu: f64,
    pub var: f64,
    pub n: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaselineUpdate {
    /// z calculado **antes** de actualizar (0 en la primera muestra).
    pub z: f64,
    pub accepted: bool,
    pub anomaly: bool,
}

impl Baseline {
    pub fn sigma(&self) -> f64 {
        self.var.max(0.0).sqrt()
    }

    fn effective_sigma(&self, cfg: &BaselineConfig) -> f64 {
        self.sigma().max(cfg.sigma_min_ms)
    }

    pub fn converged(&self, cfg: &BaselineConfig) -> bool {
        self.n >= cfg.converge_after
    }

    pub fn threshold(&self, cfg: &BaselineConfig) -> f64 {
        self.mu + cfg.k * self.effective_sigma(cfg)
    }

    pub fn z(&self, x: f64, cfg: &BaselineConfig) -> f64 {
        if self.n == 0 {
            return 0.0;
        }
        (x - self.mu) / self.effective_sigma(cfg)
    }

    pub fn update(&mut self, x: f64, cfg: &BaselineConfig) -> BaselineUpdate {
        if self.n == 0 {
            self.mu = x;
            self.var = 0.0;
            self.n = 1;
            return BaselineUpdate { z: 0.0, accepted: true, anomaly: false };
        }
        let z = self.z(x, cfg);
        let converged = self.converged(cfg);
        if converged && z > cfg.outlier_z {
            return BaselineUpdate { z, accepted: false, anomaly: true };
        }
        let delta = x - self.mu;
        self.mu += cfg.alpha * delta;
        self.var = (1.0 - cfg.alpha) * (self.var + cfg.alpha * delta * delta);
        self.n = self.n.saturating_add(1);
        BaselineUpdate { z, accepted: true, anomaly: converged && z > cfg.k }
    }

    pub fn view(&self, target_id: &str, slot: u8, cfg: &BaselineConfig, last_z: f64) -> BaselineView {
        BaselineView {
            target_id: target_id.to_string(),
            slot,
            mu_ms: self.mu,
            sigma_ms: self.sigma(),
            k: cfg.k,
            threshold_ms: self.threshold(cfg),
            samples: self.n,
            converge_after: cfg.converge_after,
            converged: self.converged(cfg),
            anomaly_score: anomaly_score(last_z),
        }
    }
}

/// `min(1, z/6)`, acotado a [0, 1] (latencias por debajo de la media no son anomalía).
pub fn anomaly_score(z: f64) -> f64 {
    (z / 6.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generador determinista (LCG) para no depender de crates de azar.
    struct Lcg(u64);
    impl Lcg {
        fn next_f64(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        /// Normal aproximada (suma de 12 uniformes − 6).
        fn normal(&mut self) -> f64 {
            (0..12).map(|_| self.next_f64()).sum::<f64>() - 6.0
        }
    }

    #[test]
    fn converges_to_mean_and_variance() {
        let cfg = BaselineConfig { alpha: 0.01, ..Default::default() };
        let mut b = Baseline::default();
        let mut rng = Lcg(42);
        for _ in 0..20_000 {
            b.update(30.0 + 2.0 * rng.normal(), &cfg);
        }
        assert!((b.mu - 30.0).abs() < 0.5, "mu = {}", b.mu);
        assert!((b.sigma() - 2.0).abs() < 0.4, "sigma = {}", b.sigma());
        assert!(b.converged(&cfg));
    }

    #[test]
    fn outliers_do_not_contaminate_once_converged() {
        let cfg = BaselineConfig::default();
        let mut b = Baseline::default();
        let mut rng = Lcg(7);
        for _ in 0..400 {
            b.update(25.0 + rng.normal(), &cfg);
        }
        let before = b;
        for _ in 0..50 {
            let u = b.update(250.0, &cfg);
            assert!(!u.accepted);
            assert!(u.anomaly);
        }
        assert_eq!(b, before);
    }

    #[test]
    fn before_convergence_spikes_are_learned() {
        let cfg = BaselineConfig::default();
        let mut b = Baseline::default();
        for _ in 0..10 {
            b.update(20.0, &cfg);
        }
        let u = b.update(200.0, &cfg);
        assert!(u.accepted);
        assert!(b.mu > 20.0);
    }

    #[test]
    fn sigma_min_keeps_threshold_sane() {
        let cfg = BaselineConfig::default();
        let mut b = Baseline::default();
        for _ in 0..500 {
            b.update(10.0, &cfg);
        }
        assert_eq!(b.sigma(), 0.0);
        assert!((b.threshold(&cfg) - 13.0).abs() < 1e-9);
        assert!((b.z(12.0, &cfg) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn slots_cover_the_day() {
        assert_eq!(slot_for_hour(0), 0);
        assert_eq!(slot_for_hour(3), 0);
        assert_eq!(slot_for_hour(4), 1);
        assert_eq!(slot_for_hour(23), 5);
        assert_eq!(anomaly_score(12.0), 1.0);
        assert_eq!(anomaly_score(-3.0), 0.0);
        assert!((anomaly_score(3.0) - 0.5).abs() < 1e-12);
    }
}
