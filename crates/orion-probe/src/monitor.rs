//! Bucle de monitoreo (§4.1): sondea en paralelo cada `interval`, alimenta el motor y
//! publica los resultados por un canal. Lo usan igual `orion-cli monitor` y la app Tauri.

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Timelike;
use futures::future::join_all;
use orion_engine::{relay_id, Engine, Sample, TargetInfo, TargetKind, TickOutput, WifiInfo, GATEWAY_ID, ISP_HOP_ID};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use ts_rs::TS;

use crate::catalog::Catalog;
use crate::hops::{first_public_hop, trace, IspHop};
use crate::net::network_id;
use crate::{LocalNetwork, NetEnv, Prober};

#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub interval: Duration,
    pub timeout: Duration,
    pub watch_pops: Vec<String>,
    /// Destino del traceroute si el relay activo no responde ICMP.
    pub reference: Ipv4Addr,
    pub max_hops: u8,
    pub hops_every: Duration,
    pub net_check_every: Duration,
    /// Relays sondeados por segundo en el POP activo (en los demás, 1).
    pub relays_active_pop: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            timeout: Duration::from_millis(900),
            watch_pops: crate::catalog::DEFAULT_WATCH.iter().map(|s| s.to_string()).collect(),
            reference: Ipv4Addr::new(1, 1, 1, 1),
            max_hops: 8,
            hops_every: Duration::from_secs(600),
            net_check_every: Duration::from_secs(5),
            relays_active_pop: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetworkSnapshot {
    #[ts(type = "number")]
    pub network_id: i64,
    pub iface_label: String,
    pub iface_name: String,
    pub local_ipv4: Option<String>,
    pub gateway: Option<String>,
    pub isp_hop: Option<String>,
    pub isp_approximate: bool,
    pub wifi: Option<WifiInfo>,
    /// Datos simulados (modo DEMO de desarrollo).
    pub demo: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub enum MonitorEvent {
    Tick(Box<TickOutput>),
    Network(NetworkSnapshot),
    Log { level: LogLevel, message: String },
}

pub struct MonitorHandle {
    region_tx: watch::Sender<String>,
    task: JoinHandle<()>,
}

impl MonitorHandle {
    pub fn set_region(&self, pop: &str) {
        let _ = self.region_tx.send(pop.to_string());
    }

    pub fn stop(self) {
        self.task.abort();
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

pub struct MonitorDeps {
    pub prober: Arc<dyn Prober>,
    pub env: Arc<dyn NetEnv>,
    pub catalog: Catalog,
    pub engine: Arc<Mutex<Engine>>,
    pub demo: bool,
}

pub fn spawn(deps: MonitorDeps, cfg: MonitorConfig, region: String, tx: mpsc::UnboundedSender<MonitorEvent>) -> MonitorHandle {
    let (region_tx, region_rx) = watch::channel(region.clone());
    let runner = Runner {
        deps,
        cfg,
        tx,
        region_rx,
        active_pop: region,
        net: None,
        isp: None,
        relays: Vec::new(),
        last_net_check: Instant::now(),
        last_hops: Instant::now(),
    };
    MonitorHandle { region_tx, task: tokio::spawn(runner.run()) }
}

#[derive(Debug, Clone)]
struct RelayProbe {
    pop: String,
    addrs: Vec<Ipv4Addr>,
    blocked: bool,
}

struct Runner {
    deps: MonitorDeps,
    cfg: MonitorConfig,
    tx: mpsc::UnboundedSender<MonitorEvent>,
    region_rx: watch::Receiver<String>,
    active_pop: String,
    net: Option<(LocalNetwork, Option<WifiInfo>)>,
    isp: Option<IspHop>,
    relays: Vec<RelayProbe>,
    last_net_check: Instant,
    last_hops: Instant,
}

impl Runner {
    fn engine(&self) -> std::sync::MutexGuard<'_, Engine> {
        self.deps.engine.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn log(&self, level: LogLevel, message: impl Into<String>) {
        let message = message.into();
        match level {
            LogLevel::Info => tracing::info!("{message}"),
            LogLevel::Warn => tracing::warn!("{message}"),
            LogLevel::Error => tracing::error!("{message}"),
        }
        let _ = self.tx.send(MonitorEvent::Log { level, message });
    }

    async fn run(mut self) {
        self.check_network(true);
        let mut pops: Vec<String> = self.cfg.watch_pops.clone();
        if !pops.contains(&self.active_pop) {
            pops.push(self.active_pop.clone());
        }
        self.discover_relays(&pops).await;
        self.engine().set_active_relay(Some(relay_id(&self.active_pop)));
        self.refresh_isp().await;

        let mut iv = tokio::time::interval(self.cfg.interval);
        iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            iv.tick().await;

            if self.region_rx.has_changed().unwrap_or(false) {
                let pop = self.region_rx.borrow_and_update().clone();
                self.apply_region(pop).await;
            }
            if self.last_net_check.elapsed() >= self.cfg.net_check_every {
                self.last_net_check = Instant::now();
                if self.check_network(false) {
                    self.refresh_isp().await;
                }
            }
            if self.last_hops.elapsed() >= self.cfg.hops_every {
                let blocked: Vec<String> = self.relays.iter().filter(|r| r.blocked).map(|r| r.pop.clone()).collect();
                if !blocked.is_empty() {
                    self.discover_relays(&blocked).await;
                }
                self.refresh_isp().await;
            }
            self.probe_all().await;
        }
    }

    /// Revisa la red local. Devuelve `true` si cambió (otro gateway, otra interfaz u otro SSID).
    fn check_network(&mut self, first: bool) -> bool {
        let env = self.deps.env.clone();
        match env.local_network() {
            Ok(net) => {
                let wifi = env.wifi(&net);
                let id = network_id(net.gateway, net.gateway_mac.as_deref(), wifi.as_ref().map(|w| w.ssid_hash.as_str()));
                let changed = first || self.engine().network_id() != id;
                {
                    let mut e = self.engine();
                    e.set_network(id);
                    e.set_wifi(wifi.clone());
                    if changed {
                        e.upsert_target(TargetInfo {
                            id: GATEWAY_ID.into(),
                            label: format!("Router ({})", net.iface_label),
                            addr: net.gateway.to_string(),
                            kind: TargetKind::Gateway,
                            approximate: false,
                            icmp_blocked: false,
                        });
                    }
                }
                if changed {
                    if !first {
                        self.log(LogLevel::Info, format!("Cambio de red detectado: gateway {} en {}", net.gateway, net.iface_label));
                    }
                    self.net = Some((net, wifi));
                    self.emit_network();
                } else {
                    self.net = Some((net, wifi));
                }
                changed
            }
            Err(e) => {
                let had = self.net.take().is_some();
                if had || first {
                    self.engine().remove_target(GATEWAY_ID);
                    self.log(LogLevel::Error, format!("Sin red local: {e}"));
                    self.emit_network();
                }
                had
            }
        }
    }

    fn emit_network(&self) {
        let snap = NetworkSnapshot {
            network_id: self.engine().network_id(),
            iface_label: self.net.as_ref().map(|(n, _)| n.iface_label.clone()).unwrap_or_default(),
            iface_name: self.net.as_ref().map(|(n, _)| n.iface_name.clone()).unwrap_or_default(),
            local_ipv4: self.net.as_ref().and_then(|(n, _)| n.local_ipv4.map(|i| i.to_string())),
            gateway: self.net.as_ref().map(|(n, _)| n.gateway.to_string()),
            isp_hop: self.isp.as_ref().map(|h| h.addr.to_string()),
            isp_approximate: self.isp.as_ref().is_some_and(|h| h.approximate),
            wifi: self.net.as_ref().and_then(|(_, w)| w.clone()),
            demo: self.deps.demo,
        };
        let _ = self.tx.send(MonitorEvent::Network(snap));
    }

    async fn discover_relays(&mut self, pops: &[String]) {
        for code in pops {
            let Some(pop) = self.deps.catalog.pop(code).cloned() else {
                self.log(LogLevel::Warn, format!("El POP '{code}' no está en el catálogo de relays"));
                continue;
            };
            let prober = self.deps.prober.clone();
            let timeout = self.cfg.timeout;
            let results = join_all(pop.relays.iter().map(|&addr| {
                let prober = prober.clone();
                async move {
                    let a = prober.echo(addr, None, timeout).await.reply_rtt();
                    let b = prober.echo(addr, None, timeout).await.reply_rtt();
                    (addr, [a, b].into_iter().flatten().reduce(f64::min))
                }
            }))
            .await;
            let mut ok: Vec<(Ipv4Addr, f64)> = results.into_iter().filter_map(|(a, r)| r.map(|r| (a, r))).collect();
            ok.sort_by(|a, b| a.1.total_cmp(&b.1));
            let blocked = ok.is_empty();
            let addrs: Vec<Ipv4Addr> = if blocked {
                pop.relays.iter().take(2).copied().collect()
            } else {
                ok.iter().take(2).map(|(a, _)| *a).collect()
            };
            if blocked {
                self.log(LogLevel::Warn, format!("{}: ningún relay responde ICMP (icmp_bloqueado)", pop.desc));
            }
            self.engine().upsert_target(TargetInfo {
                id: relay_id(code),
                label: pop.desc.clone(),
                addr: addrs.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", "),
                kind: TargetKind::Relay { pop: code.clone() },
                approximate: false,
                icmp_blocked: blocked,
            });
            let entry = RelayProbe { pop: code.clone(), addrs, blocked };
            match self.relays.iter_mut().find(|r| &r.pop == code) {
                Some(r) => *r = entry,
                None => self.relays.push(entry),
            }
        }
    }

    async fn refresh_isp(&mut self) {
        self.last_hops = Instant::now();
        let dest = self
            .relays
            .iter()
            .find(|r| r.pop == self.active_pop && !r.blocked)
            .and_then(|r| r.addrs.first().copied())
            .unwrap_or(self.cfg.reference);
        let hops = trace(self.deps.prober.as_ref(), dest, self.cfg.max_hops, self.cfg.timeout).await;
        let isp = first_public_hop(&hops);
        // Si el primer público es el propio destino, no es un hop del ISP.
        let isp = isp.filter(|h| h.addr != dest);
        let changed = isp != self.isp;
        match &isp {
            Some(h) => {
                self.engine().upsert_target(TargetInfo {
                    id: ISP_HOP_ID.into(),
                    label: if h.approximate { "ISP (aproximado)".into() } else { "ISP (primer hop público)".into() },
                    addr: h.addr.to_string(),
                    kind: TargetKind::IspHop,
                    approximate: h.approximate,
                    icmp_blocked: false,
                });
            }
            None => {
                self.engine().remove_target(ISP_HOP_ID);
                if changed || self.isp.is_none() {
                    self.log(LogLevel::Warn, "No se pudo identificar el primer hop público del ISP; el tramo ISP queda sin datos.");
                }
            }
        }
        self.isp = isp;
        if changed {
            self.emit_network();
        }
    }

    async fn apply_region(&mut self, pop: String) {
        if pop == self.active_pop {
            return;
        }
        if !self.relays.iter().any(|r| r.pop == pop) {
            self.discover_relays(std::slice::from_ref(&pop)).await;
        }
        if self.deps.catalog.pop(&pop).is_none() {
            return;
        }
        self.active_pop = pop.clone();
        self.engine().set_active_relay(Some(relay_id(&pop)));
        self.log(LogLevel::Info, format!("Región activa: {pop}"));
        self.refresh_isp().await;
    }

    async fn probe_all(&mut self) {
        let mut targets: Vec<(String, Vec<Ipv4Addr>)> = Vec::new();
        if let Some((net, _)) = &self.net {
            targets.push((GATEWAY_ID.into(), vec![net.gateway]));
        }
        if let Some(h) = &self.isp {
            targets.push((ISP_HOP_ID.into(), vec![h.addr]));
        }
        for r in self.relays.iter().filter(|r| !r.blocked) {
            let n = if r.pop == self.active_pop { self.cfg.relays_active_pop } else { 1 };
            targets.push((relay_id(&r.pop), r.addrs.iter().take(n).copied().collect()));
        }

        let prober = self.deps.prober.clone();
        let timeout = self.cfg.timeout;
        let results = join_all(targets.into_iter().map(|(id, addrs)| {
            let prober = prober.clone();
            async move {
                let rtts = join_all(addrs.into_iter().map(|a| {
                    let prober = prober.clone();
                    async move { prober.echo(a, None, timeout).await.reply_rtt() }
                }))
                .await;
                // Por POP se toma el mejor RTT; si ninguno respondió, es pérdida.
                (id, rtts.into_iter().flatten().reduce(f64::min))
            }
        }))
        .await;

        let now = chrono::Local::now();
        let ts_ms = now.timestamp_millis();
        let hour = now.hour() as u8;
        let out = {
            let mut e = self.engine();
            for (target, rtt_ms) in results {
                e.ingest(&Sample { target, rtt_ms, ts_ms }, hour);
            }
            e.tick(ts_ms)
        };
        let _ = self.tx.send(MonitorEvent::Tick(Box::new(out)));
    }
}

#[cfg(all(test, feature = "mock"))]
mod tests {
    use super::*;
    use crate::mock::{MockEnv, MockProber, Scenario};
    use orion_engine::{EngineConfig, Readiness, Verdict};

    async fn run_scenario(scenario: Scenario, ticks: usize) -> Vec<TickOutput> {
        let engine = Arc::new(Mutex::new(Engine::new(EngineConfig::default())));
        let deps = MonitorDeps {
            prober: Arc::new(MockProber::new(scenario)),
            env: Arc::new(MockEnv { scenario }),
            catalog: Catalog::bundled(),
            engine,
            demo: true,
        };
        let cfg = MonitorConfig { interval: Duration::from_millis(1), ..Default::default() };
        let (tx, mut rx) = mpsc::unbounded_channel();
        let h = spawn(deps, cfg, "lim".into(), tx);
        let mut out = Vec::new();
        while out.len() < ticks {
            match rx.recv().await {
                Some(MonitorEvent::Tick(t)) => out.push(*t),
                Some(_) => {}
                None => break,
            }
        }
        h.stop();
        out
    }

    #[tokio::test]
    async fn calm_reaches_ready_with_isp_hop() {
        let ticks = run_scenario(Scenario::Calm, 30).await;
        let last = &ticks.last().unwrap().tick;
        assert_eq!(last.readiness, Readiness::Ready);
        assert_eq!(last.diagnosis.verdict, Verdict::Ok);
        let isp = last.targets.iter().find(|t| t.id == ISP_HOP_ID).expect("hop ISP registrado");
        assert_eq!(isp.addr, crate::mock::MOCK_ISP.to_string());
    }

    #[tokio::test]
    async fn wifi_scenario_is_local() {
        let ticks = run_scenario(Scenario::Wifi, 30).await;
        let last = &ticks.last().unwrap().tick;
        assert_eq!(last.diagnosis.verdict, Verdict::Local);
        assert!(last.recommendations.iter().any(|r| r.id == "wifi-24ghz"));
    }

    #[tokio::test]
    async fn route_scenario_degrades_route() {
        let ticks = run_scenario(Scenario::Route, 60).await;
        assert_eq!(ticks.last().unwrap().tick.diagnosis.verdict, Verdict::Route);
    }
}
