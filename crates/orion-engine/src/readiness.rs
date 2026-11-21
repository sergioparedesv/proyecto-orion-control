//! Semáforo de readiness con histéresis (§6.2.5).
//!
//! Reglas de transición:
//! - Empeorar (LISTO → PRECAUCIÓN → NO BUSCAR) requiere `worsen_after` evaluaciones seguidas.
//! - Mejorar requiere `improve_after` evaluaciones seguidas.
//! - Entrar o salir de SIN DATOS cuenta como "empeorar" (se reacciona rápido).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::types::Metrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Readiness {
    Ready,
    Caution,
    NoGo,
    NoData,
}

impl Readiness {
    fn severity(self) -> Option<u8> {
        match self {
            Readiness::Ready => Some(0),
            Readiness::Caution => Some(1),
            Readiness::NoGo => Some(2),
            Readiness::NoData => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Readiness::Ready => "LISTO",
            Readiness::Caution => "PRECAUCIÓN",
            Readiness::NoGo => "NO BUSCAR",
            Readiness::NoData => "SIN DATOS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReadinessConfig {
    pub min_valid: u32,
    pub abs_ready_ms: f64,
    pub abs_caution_ms: f64,
    /// Factor sobre el umbral dinámico para el límite de PRECAUCIÓN (+50 % → 1.5).
    pub caution_factor: f64,
    pub ready_loss_pct: f64,
    pub caution_loss_pct: f64,
    pub ready_jitter_ms: f64,
    pub caution_jitter_ms: f64,
    pub worsen_after: u32,
    pub improve_after: u32,
}

impl Default for ReadinessConfig {
    fn default() -> Self {
        Self {
            min_valid: 10,
            abs_ready_ms: 80.0,
            abs_caution_ms: 120.0,
            caution_factor: 1.5,
            ready_loss_pct: 1.0,
            caution_loss_pct: 3.0,
            ready_jitter_ms: 10.0,
            caution_jitter_ms: 20.0,
            worsen_after: 3,
            improve_after: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReadinessChange {
    pub from: Readiness,
    pub to: Readiness,
    pub reason: String,
}

/// Clasificación instantánea (sin histéresis) del relay activo.
pub fn classify(relay: Option<&Metrics>, dynamic_threshold_ms: Option<f64>, cfg: &ReadinessConfig) -> (Readiness, String) {
    let Some(r) = relay else {
        return (Readiness::NoData, "No hay relay activo.".into());
    };
    if (r.valid as u32) < cfg.min_valid {
        return (Readiness::NoData, format!("Solo {} muestras válidas (mínimo {}).", r.valid, cfg.min_valid));
    }
    let Some(p95) = r.p95 else {
        return (Readiness::NoData, "El relay no responde.".into());
    };
    let (ready_max, caution_max, src) = match dynamic_threshold_ms {
        Some(t) => (t, t * cfg.caution_factor, "umbral dinámico"),
        None => (cfg.abs_ready_ms, cfg.abs_caution_ms, "umbral absoluto"),
    };
    let jitter = r.jitter.unwrap_or(0.0);
    let reason = format!(
        "p95 {p95:.1} ms ({src} {ready_max:.1} ms), pérdida {:.1} %, jitter {jitter:.1} ms",
        r.loss_pct
    );
    let state = if p95 <= ready_max && r.loss_pct < cfg.ready_loss_pct && jitter < cfg.ready_jitter_ms {
        Readiness::Ready
    } else if p95 <= caution_max && r.loss_pct <= cfg.caution_loss_pct && jitter <= cfg.caution_jitter_ms {
        Readiness::Caution
    } else {
        Readiness::NoGo
    };
    (state, reason)
}

#[derive(Debug, Clone)]
pub struct Hysteresis {
    current: Readiness,
    candidate: Readiness,
    count: u32,
    reason: String,
}

impl Default for Hysteresis {
    fn default() -> Self {
        Self { current: Readiness::NoData, candidate: Readiness::NoData, count: 0, reason: "Iniciando.".into() }
    }
}

impl Hysteresis {
    pub fn current(&self) -> Readiness {
        self.current
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Alimenta una evaluación instantánea. Devuelve `Some` solo si el estado publicado cambia.
    pub fn feed(&mut self, raw: Readiness, reason: String, cfg: &ReadinessConfig) -> Option<ReadinessChange> {
        if raw == self.current {
            self.candidate = raw;
            self.count = 0;
            self.reason = reason;
            return None;
        }
        if raw == self.candidate {
            self.count += 1;
        } else {
            self.candidate = raw;
            self.count = 1;
        }
        let worse = match (raw.severity(), self.current.severity()) {
            (Some(a), Some(b)) => a > b,
            _ => true,
        };
        let needed = if worse { cfg.worsen_after } else { cfg.improve_after };
        if self.count >= needed {
            let from = self.current;
            self.current = raw;
            self.count = 0;
            self.reason = reason.clone();
            Some(ReadinessChange { from, to: raw, reason })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Readiness::*;

    fn relay(p95: f64, loss: f64, jitter: f64) -> Metrics {
        Metrics { samples: 60, valid: 60, p95: Some(p95), median: Some(p95), jitter: Some(jitter), loss_pct: loss }
    }

    #[test]
    fn classify_absolute() {
        let c = ReadinessConfig::default();
        assert_eq!(classify(Some(&relay(40.0, 0.0, 2.0)), None, &c).0, Ready);
        assert_eq!(classify(Some(&relay(100.0, 0.0, 2.0)), None, &c).0, Caution);
        assert_eq!(classify(Some(&relay(40.0, 2.0, 2.0)), None, &c).0, Caution);
        assert_eq!(classify(Some(&relay(40.0, 0.0, 15.0)), None, &c).0, Caution);
        assert_eq!(classify(Some(&relay(130.0, 0.0, 2.0)), None, &c).0, NoGo);
        assert_eq!(classify(Some(&relay(40.0, 5.0, 2.0)), None, &c).0, NoGo);
        assert_eq!(classify(None, None, &c).0, NoData);
        let few = Metrics { valid: 5, ..relay(40.0, 0.0, 1.0) };
        assert_eq!(classify(Some(&few), None, &c).0, NoData);
    }

    #[test]
    fn classify_dynamic() {
        let c = ReadinessConfig::default();
        assert_eq!(classify(Some(&relay(40.0, 0.0, 2.0)), Some(35.0), &c).0, Caution);
        assert_eq!(classify(Some(&relay(60.0, 0.0, 2.0)), Some(35.0), &c).0, NoGo);
    }

    #[test]
    fn hysteresis_sequence() {
        let c = ReadinessConfig::default();
        let mut h = Hysteresis::default();
        let mut published = vec![];
        let seq: Vec<Readiness> = std::iter::repeat(Ready)
            .take(5)
            .chain([NoGo, Ready]) // spike aislado: no debe publicarse
            .chain(std::iter::repeat(NoGo).take(4))
            .chain(std::iter::repeat(Ready).take(12))
            .collect();
        for raw in seq {
            if let Some(ch) = h.feed(raw, String::new(), &c) {
                published.push((ch.from, ch.to));
            }
        }
        assert_eq!(published, vec![(NoData, Ready), (Ready, NoGo), (NoGo, Ready)]);
    }

    #[test]
    fn improving_needs_ten() {
        let c = ReadinessConfig::default();
        let mut h = Hysteresis::default();
        for _ in 0..3 {
            h.feed(NoGo, String::new(), &c);
        }
        assert_eq!(h.current(), NoGo);
        for i in 0..9 {
            assert!(h.feed(Ready, String::new(), &c).is_none(), "iteración {i}");
        }
        assert!(h.feed(Ready, String::new(), &c).is_some());
        assert_eq!(h.current(), Ready);
    }
}
