//! Diagnóstico por segmento: LAN / ISP / ruta (§6.2.4).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::types::{Metrics, WifiInfo};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiagnosisConfig {
    pub min_samples: u32,
    pub gw_loss_pct: f64,
    pub gw_jitter_ms: f64,
    pub gw_p95_ms: f64,
    pub isp_loss_pct: f64,
    /// Incremento máximo aceptable de p95 entre el gateway y el hop del ISP.
    pub isp_delta_ms: f64,
    pub relay_loss_pct: f64,
    /// Umbral absoluto para el relay si la línea base todavía no convergió.
    pub relay_abs_p95_ms: f64,
}

impl Default for DiagnosisConfig {
    fn default() -> Self {
        Self {
            min_samples: 10,
            gw_loss_pct: 1.0,
            gw_jitter_ms: 5.0,
            gw_p95_ms: 20.0,
            isp_loss_pct: 1.0,
            isp_delta_ms: 30.0,
            relay_loss_pct: 1.0,
            relay_abs_p95_ms: 80.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Verdict {
    Ok,
    Local,
    Isp,
    Route,
    NoData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Segment {
    Lan,
    Isp,
    Route,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SegmentState {
    Ok,
    Degraded,
    NoData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Evidence {
    pub target: String,
    pub metric: String,
    pub value: f64,
    pub threshold: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SegmentStatus {
    pub segment: Segment,
    pub state: SegmentState,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Diagnosis {
    pub verdict: Verdict,
    pub message: String,
    pub evidence: Vec<Evidence>,
    pub segments: Vec<SegmentStatus>,
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosisInput {
    pub gateway: Option<Metrics>,
    pub isp: Option<Metrics>,
    pub relay: Option<Metrics>,
    /// Umbral dinámico de la línea base del relay activo, si convergió.
    pub relay_threshold_ms: Option<f64>,
    pub wifi: Option<WifiInfo>,
}

fn ev(target: &str, metric: &str, value: f64, threshold: f64) -> Evidence {
    Evidence { target: target.into(), metric: metric.into(), value, threshold }
}

fn has_data(m: &Option<Metrics>, cfg: &DiagnosisConfig) -> Option<Metrics> {
    m.filter(|m| m.samples as u32 >= cfg.min_samples)
}

fn check_lan(g: &Metrics, cfg: &DiagnosisConfig) -> Vec<Evidence> {
    let mut e = Vec::new();
    if g.loss_pct > cfg.gw_loss_pct {
        e.push(ev("gateway", "loss_pct", g.loss_pct, cfg.gw_loss_pct));
    }
    if let Some(j) = g.jitter.filter(|j| *j > cfg.gw_jitter_ms) {
        e.push(ev("gateway", "jitter_ms", j, cfg.gw_jitter_ms));
    }
    if let Some(p) = g.p95.filter(|p| *p > cfg.gw_p95_ms) {
        e.push(ev("gateway", "p95_ms", p, cfg.gw_p95_ms));
    }
    e
}

fn check_isp(i: &Metrics, g: Option<&Metrics>, cfg: &DiagnosisConfig) -> Vec<Evidence> {
    let mut e = Vec::new();
    if i.loss_pct > cfg.isp_loss_pct {
        e.push(ev("isp-hop", "loss_pct", i.loss_pct, cfg.isp_loss_pct));
    }
    if let (Some(ip), Some(gp)) = (i.p95, g.and_then(|g| g.p95)) {
        let delta = ip - gp;
        if delta > cfg.isp_delta_ms {
            e.push(ev("isp-hop", "p95_delta_ms", delta, cfg.isp_delta_ms));
        }
    }
    e
}

fn check_route(r: &Metrics, threshold: f64, cfg: &DiagnosisConfig) -> Vec<Evidence> {
    let mut e = Vec::new();
    if r.loss_pct > cfg.relay_loss_pct {
        e.push(ev("relay", "loss_pct", r.loss_pct, cfg.relay_loss_pct));
    }
    if let Some(p) = r.p95.filter(|p| *p > threshold) {
        e.push(ev("relay", "p95_ms", p, threshold));
    }
    e
}

fn seg(segment: Segment, data: bool, evidence: Vec<Evidence>) -> SegmentStatus {
    let state = if !data {
        SegmentState::NoData
    } else if evidence.is_empty() {
        SegmentState::Ok
    } else {
        SegmentState::Degraded
    };
    SegmentStatus { segment, state, evidence }
}

pub fn diagnose(input: &DiagnosisInput, cfg: &DiagnosisConfig) -> Diagnosis {
    let g = has_data(&input.gateway, cfg);
    let i = has_data(&input.isp, cfg);
    let r = has_data(&input.relay, cfg);
    let relay_thr = input.relay_threshold_ms.unwrap_or(cfg.relay_abs_p95_ms);

    let lan_e = g.as_ref().map(|g| check_lan(g, cfg)).unwrap_or_default();
    let isp_e = i.as_ref().map(|i| check_isp(i, g.as_ref(), cfg)).unwrap_or_default();
    let route_e = r.as_ref().map(|r| check_route(r, relay_thr, cfg)).unwrap_or_default();

    let segments = vec![
        seg(Segment::Lan, g.is_some(), lan_e.clone()),
        seg(Segment::Isp, i.is_some(), isp_e.clone()),
        seg(Segment::Route, r.is_some(), route_e.clone()),
    ];

    let (verdict, message, evidence) = if g.is_none() && i.is_none() && r.is_none() {
        (Verdict::NoData, "Recolectando muestras: todavía no hay datos suficientes.".to_string(), vec![])
    } else if !lan_e.is_empty() {
        let mut msg = "El problema está en tu red local / Wi-Fi.".to_string();
        if input.wifi.is_some() {
            msg.push_str(" Estás conectado por Wi-Fi: prueba con cable para confirmarlo.");
        }
        (Verdict::Local, msg, lan_e)
    } else if !isp_e.is_empty() {
        (Verdict::Isp, "Tu red local está bien; la degradación empieza en tu proveedor (ISP).".into(), isp_e)
    } else if !route_e.is_empty() {
        (Verdict::Route, "Tu ISP responde bien; la ruta hacia el servidor de Valve está degradada.".into(), route_e)
    } else if r.is_none() {
        (
            Verdict::NoData,
            "Red local e ISP sin problemas, pero aún no hay datos suficientes del relay.".into(),
            vec![],
        )
    } else {
        (Verdict::Ok, "Todos los tramos responden dentro de lo normal.".into(), vec![])
    };

    Diagnosis { verdict, message, evidence, segments }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p95: f64, jitter: f64, loss: f64) -> Option<Metrics> {
        Some(Metrics { samples: 60, valid: 60, p95: Some(p95), median: Some(p95), jitter: Some(jitter), loss_pct: loss })
    }

    fn run(g: Option<Metrics>, i: Option<Metrics>, r: Option<Metrics>) -> Diagnosis {
        diagnose(&DiagnosisInput { gateway: g, isp: i, relay: r, ..Default::default() }, &DiagnosisConfig::default())
    }

    #[test]
    fn table() {
        let cases = [
            (m(2.0, 0.5, 0.0), m(8.0, 1.0, 0.0), m(40.0, 2.0, 0.0), Verdict::Ok),
            (m(2.0, 9.0, 0.0), m(8.0, 1.0, 0.0), m(40.0, 2.0, 0.0), Verdict::Local),
            (m(35.0, 1.0, 0.0), m(40.0, 1.0, 0.0), m(80.0, 2.0, 0.0), Verdict::Local),
            (m(2.0, 0.5, 0.0), m(8.0, 1.0, 5.0), m(40.0, 2.0, 0.0), Verdict::Isp),
            (m(2.0, 0.5, 0.0), m(60.0, 1.0, 0.0), m(70.0, 2.0, 0.0), Verdict::Isp),
            (m(2.0, 0.5, 0.0), m(8.0, 1.0, 0.0), m(140.0, 2.0, 0.0), Verdict::Route),
            (m(2.0, 0.5, 0.0), m(8.0, 1.0, 0.0), m(40.0, 2.0, 4.0), Verdict::Route),
            (None, None, None, Verdict::NoData),
            (m(2.0, 0.5, 0.0), None, None, Verdict::NoData),
        ];
        for (idx, (g, i, r, want)) in cases.into_iter().enumerate() {
            let d = run(g, i, r);
            assert_eq!(d.verdict, want, "caso {idx}: {d:?}");
            let needs_evidence = matches!(want, Verdict::Local | Verdict::Isp | Verdict::Route);
            assert_eq!(!d.evidence.is_empty(), needs_evidence, "caso {idx}");
        }
    }

    #[test]
    fn isp_hop_missing_skips_isp_check() {
        let d = run(m(2.0, 0.5, 0.0), None, m(40.0, 2.0, 0.0));
        assert_eq!(d.verdict, Verdict::Ok);
        assert_eq!(d.segments[1].state, SegmentState::NoData);
    }

    #[test]
    fn dynamic_threshold_is_used() {
        let input = DiagnosisInput {
            gateway: m(2.0, 0.5, 0.0),
            isp: m(8.0, 1.0, 0.0),
            relay: m(50.0, 2.0, 0.0),
            relay_threshold_ms: Some(45.0),
            wifi: None,
        };
        let d = diagnose(&input, &DiagnosisConfig::default());
        assert_eq!(d.verdict, Verdict::Route);
        assert_eq!(d.evidence[0].threshold, 45.0);
    }

    #[test]
    fn gateway_down_is_local() {
        let down = Some(Metrics { samples: 30, valid: 0, p95: None, median: None, jitter: None, loss_pct: 100.0 });
        let d = run(down, None, None);
        assert_eq!(d.verdict, Verdict::Local);
    }
}
