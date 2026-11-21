//! Motor de recomendaciones determinista (§6.2.6).
//! Cada recomendación lleva la evidencia que la disparó.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::diagnosis::{Evidence, Verdict};
use crate::types::{Metrics, WifiBand, WifiInfo};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Recommendation {
    pub id: String,
    pub message: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RulesConfig {
    pub weak_rssi_dbm: f64,
    pub wifi_jitter_ms: f64,
    #[ts(type = "number")]
    pub isp_sustained_ms: i64,
    /// Un POP alternativo se sugiere si su p95 es al menos este porcentaje mejor.
    pub alt_pop_gain_pct: f64,
    #[ts(type = "number")]
    pub eval_every_ms: i64,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            weak_rssi_dbm: -70.0,
            wifi_jitter_ms: 5.0,
            isp_sustained_ms: 5 * 60 * 1000,
            alt_pop_gain_pct: 20.0,
            eval_every_ms: 60 * 1000,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RulesInput {
    pub wifi: Option<WifiInfo>,
    pub gateway: Option<Metrics>,
    pub verdict: Option<Verdict>,
    /// Cuánto tiempo lleva el diagnóstico en ISP de forma continua.
    pub isp_verdict_for_ms: i64,
    pub active_relay: Option<(String, Metrics, bool)>, // (label, métricas, degradado)
    pub other_relays: Vec<(String, Metrics)>,
    /// Grado de la última prueba de bufferbloat (F3).
    pub bufferbloat_grade: Option<String>,
}

fn ev(target: &str, metric: &str, value: f64, threshold: f64) -> Evidence {
    Evidence { target: target.into(), metric: metric.into(), value, threshold }
}

pub fn evaluate(input: &RulesInput, cfg: &RulesConfig) -> Vec<Recommendation> {
    let mut out = Vec::new();
    let gw_jitter = input.gateway.and_then(|g| g.jitter);

    if let Some(w) = &input.wifi {
        if let (WifiBand::G2_4, Some(j)) = (w.band, gw_jitter) {
            if j > cfg.wifi_jitter_ms {
                out.push(Recommendation {
                    id: "wifi-24ghz".into(),
                    message: "Estás en Wi-Fi de 2,4 GHz con jitter alto hacia el router: cámbiate a 5 GHz o usa cable.".into(),
                    evidence: vec![ev("gateway", "jitter_ms", j, cfg.wifi_jitter_ms)],
                });
            }
        }
        if let Some(rssi) = w.rssi_dbm {
            if (rssi as f64) < cfg.weak_rssi_dbm {
                out.push(Recommendation {
                    id: "wifi-weak".into(),
                    message: "Señal Wi-Fi débil: acércate al router o usa cable.".into(),
                    evidence: vec![ev("wifi", "rssi_dbm", rssi as f64, cfg.weak_rssi_dbm)],
                });
            }
        }
    }

    if input.verdict == Some(Verdict::Isp) && input.isp_verdict_for_ms >= cfg.isp_sustained_ms {
        out.push(Recommendation {
            id: "isp-sustained".into(),
            message: "La degradación en tu proveedor se mantiene hace más de 5 minutos: exporta el informe para reclamar.".into(),
            evidence: vec![ev(
                "isp-hop",
                "verdict_minutes",
                input.isp_verdict_for_ms as f64 / 60_000.0,
                cfg.isp_sustained_ms as f64 / 60_000.0,
            )],
        });
    }

    if let Some((_, active, true)) = &input.active_relay {
        if let Some(active_p95) = active.p95 {
            let best = input
                .other_relays
                .iter()
                .filter(|(_, m)| m.loss_pct < 1.0)
                .filter_map(|(label, m)| m.p95.map(|p| (label, p)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((label, p)) = best {
                let limit = active_p95 * (1.0 - cfg.alt_pop_gain_pct / 100.0);
                if p <= limit {
                    out.push(Recommendation {
                        id: "alt-pop".into(),
                        message: format!("La región activa está degradada. Prueba {label} (p95 medido: {p:.0} ms)."),
                        evidence: vec![ev("relay", "alt_p95_ms", p, limit)],
                    });
                }
            }
        }
    }

    if let Some(grade) = &input.bufferbloat_grade {
        if matches!(grade.as_str(), "C" | "D" | "F") {
            out.push(Recommendation {
                id: "bufferbloat".into(),
                message: "Bufferbloat alto: activa SQM (CAKE/FQ_CoDel) en tu router o limita el ancho de banda de otros dispositivos.".into(),
                evidence: vec![],
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn met(p95: f64, jitter: f64, loss: f64) -> Metrics {
        Metrics { samples: 60, valid: 60, p95: Some(p95), median: Some(p95), jitter: Some(jitter), loss_pct: loss }
    }

    fn wifi(band: WifiBand, rssi: i32) -> WifiInfo {
        WifiInfo { ssid_hash: "x".into(), rssi_dbm: Some(rssi), signal_quality: None, band, channel: None, link_mbps: None }
    }

    fn ids(r: &[Recommendation]) -> Vec<&str> {
        r.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn wifi_rules() {
        let cfg = RulesConfig::default();
        let input = RulesInput { wifi: Some(wifi(WifiBand::G2_4, -75)), gateway: Some(met(3.0, 8.0, 0.0)), ..Default::default() };
        assert_eq!(ids(&evaluate(&input, &cfg)), vec!["wifi-24ghz", "wifi-weak"]);
        let ok = RulesInput { wifi: Some(wifi(WifiBand::G5, -50)), gateway: Some(met(3.0, 8.0, 0.0)), ..Default::default() };
        assert!(evaluate(&ok, &cfg).is_empty());
    }

    #[test]
    fn isp_sustained_needs_five_minutes() {
        let cfg = RulesConfig::default();
        let mut input = RulesInput { verdict: Some(Verdict::Isp), isp_verdict_for_ms: 60_000, ..Default::default() };
        assert!(evaluate(&input, &cfg).is_empty());
        input.isp_verdict_for_ms = 6 * 60_000;
        assert_eq!(ids(&evaluate(&input, &cfg)), vec!["isp-sustained"]);
    }

    #[test]
    fn alternative_pop() {
        let cfg = RulesConfig::default();
        let input = RulesInput {
            active_relay: Some(("Lima".into(), met(120.0, 2.0, 0.0), true)),
            other_relays: vec![("Santiago".into(), met(70.0, 2.0, 0.0)), ("Virginia".into(), met(60.0, 2.0, 5.0))],
            ..Default::default()
        };
        let r = evaluate(&input, &cfg);
        assert_eq!(ids(&r), vec!["alt-pop"]);
        assert!(r[0].message.contains("Santiago"));
    }
}
