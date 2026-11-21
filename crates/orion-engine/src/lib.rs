//! ORION engine: estadística y decisión. No depende del sistema operativo ni del reloj:
//! quien lo usa le pasa las muestras, la marca de tiempo y la hora local.

pub mod baseline;
pub mod diagnosis;
pub mod jitter;
pub mod readiness;
pub mod rules;
pub mod types;
pub mod window;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use baseline::{Baseline, BaselineConfig, BaselineKey};
pub use diagnosis::{Diagnosis, DiagnosisConfig, Verdict};
pub use readiness::{Readiness, ReadinessChange, ReadinessConfig};
pub use rules::{Recommendation, RulesConfig};
pub use types::*;

use jitter::Jitter;
use readiness::Hysteresis;
use window::SlidingWindow;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EngineConfig {
    pub window: usize,
    pub baseline: BaselineConfig,
    pub diagnosis: DiagnosisConfig,
    pub readiness: ReadinessConfig,
    pub rules: RulesConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            window: 60,
            baseline: BaselineConfig::default(),
            diagnosis: DiagnosisConfig::default(),
            readiness: ReadinessConfig::default(),
            rules: RulesConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct TargetState {
    info: TargetInfo,
    window: SlidingWindow,
    jitter: Jitter,
    last_z: f64,
}

/// Qué parte de la línea base borrar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "scope", rename_all = "snake_case")]
#[ts(export)]
pub enum BaselineScope {
    All,
    Network,
    Target { target_id: String },
}

/// Resultado de un tick del motor.
#[derive(Debug, Clone)]
pub struct TickOutput {
    pub tick: TelemetryTick,
    /// `Some` si el veredicto cambió respecto al tick anterior.
    pub diagnosis_changed: Option<Diagnosis>,
    pub readiness_change: Option<ReadinessChange>,
}

#[derive(Debug)]
pub struct Engine {
    cfg: EngineConfig,
    network_id: i64,
    order: Vec<TargetId>,
    targets: HashMap<TargetId, TargetState>,
    baselines: HashMap<BaselineKey, Baseline>,
    active_relay: Option<TargetId>,
    wifi: Option<WifiInfo>,
    hysteresis: Hysteresis,
    last_verdict: Option<Verdict>,
    isp_since_ms: Option<i64>,
    last_rules_ms: Option<i64>,
    recommendations: Vec<Recommendation>,
    bufferbloat_grade: Option<String>,
    current_slot: u8,
}

impl Engine {
    pub fn new(cfg: EngineConfig) -> Self {
        Self {
            cfg,
            network_id: 0,
            order: Vec::new(),
            targets: HashMap::new(),
            baselines: HashMap::new(),
            active_relay: None,
            wifi: None,
            hysteresis: Hysteresis::default(),
            last_verdict: None,
            isp_since_ms: None,
            last_rules_ms: None,
            recommendations: Vec::new(),
            bufferbloat_grade: None,
            current_slot: 0,
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    pub fn set_config(&mut self, cfg: EngineConfig) {
        if cfg.window != self.cfg.window {
            for t in self.targets.values_mut() {
                t.window = SlidingWindow::new(cfg.window);
            }
        }
        self.cfg = cfg;
    }

    pub fn network_id(&self) -> i64 {
        self.network_id
    }

    /// Cambio de red: se vacían las ventanas y el semáforo; las líneas base se conservan por red.
    pub fn set_network(&mut self, network_id: i64) {
        if network_id == self.network_id {
            return;
        }
        self.network_id = network_id;
        for t in self.targets.values_mut() {
            t.window.clear();
            t.jitter.reset();
            t.last_z = 0.0;
        }
        self.hysteresis.reset();
        self.last_verdict = None;
        self.isp_since_ms = None;
    }

    pub fn upsert_target(&mut self, info: TargetInfo) {
        match self.targets.get_mut(&info.id) {
            Some(t) => {
                if t.info.addr != info.addr {
                    t.window.clear();
                    t.jitter.reset();
                }
                t.info = info;
            }
            None => {
                self.order.push(info.id.clone());
                self.targets.insert(
                    info.id.clone(),
                    TargetState { info, window: SlidingWindow::new(self.cfg.window), jitter: Jitter::new(), last_z: 0.0 },
                );
            }
        }
    }

    pub fn remove_target(&mut self, id: &str) {
        self.targets.remove(id);
        self.order.retain(|t| t != id);
        if self.active_relay.as_deref() == Some(id) {
            self.active_relay = None;
        }
    }

    pub fn target_ids(&self) -> Vec<TargetId> {
        self.order.clone()
    }

    pub fn target_info(&self, id: &str) -> Option<&TargetInfo> {
        self.targets.get(id).map(|t| &t.info)
    }

    pub fn set_active_relay(&mut self, id: Option<TargetId>) {
        if self.active_relay != id {
            self.active_relay = id;
            self.hysteresis.reset();
        }
    }

    pub fn active_relay(&self) -> Option<&str> {
        self.active_relay.as_deref()
    }

    pub fn set_wifi(&mut self, wifi: Option<WifiInfo>) {
        self.wifi = wifi;
    }

    pub fn set_bufferbloat_grade(&mut self, grade: Option<String>) {
        self.bufferbloat_grade = grade;
    }

    /// Registra una medición. `hour_local` (0-23) decide la franja de la línea base.
    pub fn ingest(&mut self, sample: &Sample, hour_local: u8) {
        let slot = baseline::slot_for_hour(hour_local);
        self.current_slot = slot;
        let Some(t) = self.targets.get_mut(&sample.target) else { return };
        t.window.push(sample.rtt_ms);
        t.jitter.update(sample.rtt_ms);
        if let Some(x) = sample.rtt_ms {
            let key = BaselineKey { network_id: self.network_id, target_id: sample.target.clone(), slot };
            let b = self.baselines.entry(key).or_default();
            t.last_z = b.update(x, &self.cfg.baseline).z;
        }
    }

    pub fn metrics(&self, id: &str) -> Option<Metrics> {
        let t = self.targets.get(id)?;
        let s = t.window.stats();
        Some(Metrics {
            samples: s.samples,
            valid: s.valid,
            p95: s.p95,
            median: s.median,
            jitter: t.jitter.value(),
            loss_pct: s.loss_pct,
        })
    }

    fn baseline_for(&self, id: &str) -> Option<&Baseline> {
        self.baselines.get(&BaselineKey { network_id: self.network_id, target_id: id.to_string(), slot: self.current_slot })
    }

    /// Umbral dinámico del objetivo, solo si su línea base convergió.
    pub fn dynamic_threshold(&self, id: &str) -> Option<f64> {
        self.baseline_for(id).filter(|b| b.converged(&self.cfg.baseline)).map(|b| b.threshold(&self.cfg.baseline))
    }

    pub fn baseline_view(&self, id: &str) -> Option<BaselineView> {
        let t = self.targets.get(id)?;
        let b = self.baseline_for(id)?;
        Some(b.view(id, self.current_slot, &self.cfg.baseline, t.last_z))
    }

    pub fn target_stats(&self, id: &str) -> Option<TargetStats> {
        let t = self.targets.get(id)?;
        let s = t.window.stats();
        let threshold = self.dynamic_threshold(id);
        let status = if t.info.icmp_blocked {
            TargetStatus::IcmpBlocked
        } else if s.samples == 0 {
            TargetStatus::NoData
        } else if s.samples >= 5 && s.valid == 0 {
            TargetStatus::Down
        } else if (s.valid as u32) < self.cfg.readiness.min_valid {
            TargetStatus::NoData
        } else if s.loss_pct > 1.0 || matches!((s.p95, threshold), (Some(p), Some(t)) if p > t) {
            TargetStatus::Degraded
        } else {
            TargetStatus::Ok
        };
        Some(TargetStats {
            id: t.info.id.clone(),
            label: t.info.label.clone(),
            addr: t.info.addr.clone(),
            kind: t.info.kind.clone(),
            approximate: t.info.approximate,
            last_ms: s.last,
            min_ms: s.min,
            median_ms: s.median,
            p95_ms: s.p95,
            jitter_ms: t.jitter.value(),
            loss_pct: s.loss_pct,
            samples: s.samples as u32,
            valid: s.valid as u32,
            status,
        })
    }

    /// Evalúa diagnóstico, semáforo y reglas. Llamar una vez por ciclo de sondeo.
    pub fn tick(&mut self, ts_ms: i64) -> TickOutput {
        let gateway = self.metrics(GATEWAY_ID);
        let isp = self.metrics(ISP_HOP_ID);
        let active = self.active_relay.clone();
        let relay = active.as_deref().and_then(|id| self.metrics(id));
        let relay_thr = active.as_deref().and_then(|id| self.dynamic_threshold(id));

        let diag = diagnosis::diagnose(
            &diagnosis::DiagnosisInput { gateway, isp, relay, relay_threshold_ms: relay_thr, wifi: self.wifi.clone() },
            &self.cfg.diagnosis,
        );
        let diagnosis_changed = (self.last_verdict != Some(diag.verdict)).then(|| diag.clone());
        self.last_verdict = Some(diag.verdict);
        if diag.verdict == Verdict::Isp {
            self.isp_since_ms.get_or_insert(ts_ms);
        } else {
            self.isp_since_ms = None;
        }

        let (raw, reason) = readiness::classify(relay.as_ref(), relay_thr, &self.cfg.readiness);
        let readiness_change = self.hysteresis.feed(raw, reason, &self.cfg.readiness);

        let due = self.last_rules_ms.map_or(true, |t| ts_ms - t >= self.cfg.rules.eval_every_ms);
        if due || diagnosis_changed.is_some() {
            self.last_rules_ms = Some(ts_ms);
            self.recommendations = self.evaluate_rules(ts_ms, gateway, diag.verdict);
        }

        let targets = self.order.iter().filter_map(|id| self.target_stats(id)).collect();
        let tick = TelemetryTick {
            ts: ts_ms,
            network_id: self.network_id,
            active_relay: active.clone(),
            targets,
            readiness: self.hysteresis.current(),
            readiness_reason: self.hysteresis.reason().to_string(),
            baseline: active.as_deref().and_then(|id| self.baseline_view(id)),
            diagnosis: diag,
            wifi: self.wifi.clone(),
            recommendations: self.recommendations.clone(),
        };
        TickOutput { tick, diagnosis_changed, readiness_change }
    }

    fn evaluate_rules(&self, ts_ms: i64, gateway: Option<Metrics>, verdict: Verdict) -> Vec<Recommendation> {
        let active = self.active_relay.as_deref();
        let active_relay = active.and_then(|id| {
            let m = self.metrics(id)?;
            let degraded = self
                .target_stats(id)
                .is_some_and(|s| matches!(s.status, TargetStatus::Degraded | TargetStatus::Down));
            Some((self.targets[id].info.label.clone(), m, degraded))
        });
        let other_relays = self
            .order
            .iter()
            .filter(|id| Some(id.as_str()) != active)
            .filter_map(|id| {
                let t = &self.targets[id];
                matches!(t.info.kind, TargetKind::Relay { .. })
                    .then(|| self.metrics(id).map(|m| (t.info.label.clone(), m)))
                    .flatten()
            })
            .filter(|(_, m)| m.valid as u32 >= self.cfg.readiness.min_valid)
            .collect();
        rules::evaluate(
            &rules::RulesInput {
                wifi: self.wifi.clone(),
                gateway,
                verdict: Some(verdict),
                isp_verdict_for_ms: self.isp_since_ms.map_or(0, |s| ts_ms - s),
                active_relay,
                other_relays,
                bufferbloat_grade: self.bufferbloat_grade.clone(),
            },
            &self.cfg.rules,
        )
    }

    // --- Persistencia de líneas base (la hace la capa de aplicación) ---

    pub fn baselines(&self) -> impl Iterator<Item = (&BaselineKey, &Baseline)> {
        self.baselines.iter()
    }

    pub fn load_baselines(&mut self, items: impl IntoIterator<Item = (BaselineKey, Baseline)>) {
        self.baselines.extend(items);
    }

    pub fn reset_baseline(&mut self, scope: &BaselineScope) {
        let net = self.network_id;
        match scope {
            BaselineScope::All => self.baselines.clear(),
            BaselineScope::Network => self.baselines.retain(|k, _| k.network_id != net),
            BaselineScope::Target { target_id } => {
                self.baselines.retain(|k, _| !(k.network_id == net && &k.target_id == target_id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, kind: TargetKind) -> TargetInfo {
        TargetInfo { id: id.into(), label: id.into(), addr: "192.0.2.1".into(), kind, approximate: false, icmp_blocked: false }
    }

    fn engine() -> Engine {
        let mut e = Engine::new(EngineConfig::default());
        e.set_network(1);
        e.upsert_target(info(GATEWAY_ID, TargetKind::Gateway));
        e.upsert_target(info(ISP_HOP_ID, TargetKind::IspHop));
        e.upsert_target(info("relay:lim", TargetKind::Relay { pop: "lim".into() }));
        e.set_active_relay(Some("relay:lim".into()));
        e
    }

    fn step(e: &mut Engine, ts: i64, gw: Option<f64>, isp: Option<f64>, relay: Option<f64>) -> TickOutput {
        for (id, v) in [(GATEWAY_ID, gw), (ISP_HOP_ID, isp), ("relay:lim", relay)] {
            e.ingest(&Sample { target: id.into(), rtt_ms: v, ts_ms: ts }, 20);
        }
        e.tick(ts)
    }

    #[test]
    fn full_flow_ready_then_spike_then_recover() {
        fn run(e: &mut Engine, ts: &mut i64, n: usize, relay: f64, changes: &mut Vec<(Readiness, Readiness)>) -> Verdict {
            let mut verdict = Verdict::NoData;
            for _ in 0..n {
                *ts += 1000;
                let out = step(e, *ts, Some(1.5), Some(6.0), Some(relay));
                verdict = out.tick.diagnosis.verdict;
                if let Some(c) = out.readiness_change {
                    changes.push((c.from, c.to));
                }
            }
            verdict
        }
        let mut e = engine();
        let mut changes = vec![];
        let mut ts = 0;
        assert_eq!(run(&mut e, &mut ts, 30, 40.0, &mut changes), Verdict::Ok);
        // spike sostenido: el p95 de la ventana supera 120 ms tras unas pocas muestras
        assert_eq!(run(&mut e, &mut ts, 20, 200.0, &mut changes), Verdict::Route);
        // vuelve a la normalidad: el p95 tarda en bajar hasta que salen los picos de la ventana
        assert_eq!(run(&mut e, &mut ts, 90, 40.0, &mut changes), Verdict::Ok);
        assert_eq!(
            changes,
            vec![(Readiness::NoData, Readiness::Ready), (Readiness::Ready, Readiness::NoGo), (Readiness::NoGo, Readiness::Ready)]
        );
    }

    #[test]
    fn gateway_loss_is_local() {
        let mut e = engine();
        for i in 0..30 {
            let gw = if i % 4 == 0 { None } else { Some(2.0) };
            step(&mut e, i * 1000, gw, Some(6.0), Some(40.0));
        }
        let out = e.tick(31_000);
        assert_eq!(out.tick.diagnosis.verdict, Verdict::Local);
    }

    #[test]
    fn network_change_resets_windows_but_keeps_baselines() {
        let mut e = engine();
        for i in 0..20 {
            step(&mut e, i * 1000, Some(2.0), Some(6.0), Some(40.0));
        }
        let n_before = e.baselines().count();
        e.set_network(2);
        assert_eq!(e.metrics(GATEWAY_ID).unwrap().samples, 0);
        assert_eq!(e.baselines().count(), n_before);
        e.reset_baseline(&BaselineScope::All);
        assert_eq!(e.baselines().count(), 0);
    }

    #[test]
    fn baseline_view_reports_calibration() {
        let mut e = engine();
        for i in 0..50 {
            step(&mut e, i * 1000, Some(2.0), Some(6.0), Some(40.0));
        }
        let v = e.tick(51_000).tick.baseline.unwrap();
        assert_eq!(v.samples, 50);
        assert!(!v.converged);
        assert_eq!(v.converge_after, 300);
        assert_eq!(v.k, 3.0);
    }
}
