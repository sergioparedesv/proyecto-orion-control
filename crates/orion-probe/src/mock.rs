//! Mocks deterministas (§10.2). Sustituyen al botón "Provocar spike" de v3.0:
//! solo se usan en pruebas y en el modo DEMO, que la interfaz etiqueta como tal.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

use orion_engine::{WifiBand, WifiInfo};

use crate::{EchoKind, EchoResult, LocalNetwork, NetEnv, ProbeError, Prober};

pub const MOCK_GATEWAY: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 1);
const MOCK_CGNAT: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 1);
/// 198.18.0.0/15 está reservado para pruebas de rendimiento: nunca es una IP real de un ISP.
pub const MOCK_ISP: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 1);
const MOCK_ISP2: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Todo estable.
    Calm,
    /// Picos de +150 ms hacia los relays durante 20 s cada 2 min.
    Spike,
    /// Wi-Fi 2,4 GHz con señal débil: jitter alto desde el gateway.
    Wifi,
    /// A partir de los 30 s el ISP añade 45 ms y pierde paquetes.
    Isp,
    /// A partir de los 30 s la ruta a Valve añade 90 ms.
    Route,
}

impl FromStr for Scenario {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "calm" => Scenario::Calm,
            "spike" => Scenario::Spike,
            "wifi" => Scenario::Wifi,
            "isp" => Scenario::Isp,
            "route" => Scenario::Route,
            _ => return Err(format!("escenario desconocido '{s}' (calm|spike|wifi|isp|route)")),
        })
    }
}

pub struct MockProber {
    scenario: Scenario,
    counters: Mutex<HashMap<Ipv4Addr, u64>>,
}

impl MockProber {
    pub fn new(scenario: Scenario) -> Self {
        Self { scenario, counters: Mutex::new(HashMap::new()) }
    }

    fn next_t(&self, addr: Ipv4Addr) -> u64 {
        let mut c = self.counters.lock().unwrap_or_else(|e| e.into_inner());
        let e = c.entry(addr).or_insert(0);
        *e += 1;
        *e
    }

    /// RTT del destino en el instante `t`, o `None` si el paquete se pierde.
    fn rtt(&self, addr: Ipv4Addr, t: u64) -> Option<f64> {
        let noise = noise(addr, t);
        let wifi_extra = if self.scenario == Scenario::Wifi { if t % 2 == 0 { 18.0 } else { 0.0 } } else { 0.0 };
        if addr == MOCK_GATEWAY {
            return Some(1.5 + noise * 0.2 + wifi_extra);
        }
        let is_isp = addr == MOCK_ISP || addr == MOCK_ISP2;
        let is_relay = addr != MOCK_CGNAT && !is_isp;
        let base = if addr == MOCK_CGNAT {
            4.0
        } else if is_isp {
            7.0
        } else {
            30.0 + (u32::from(addr) % 40) as f64
        };
        let mut extra = wifi_extra;
        match self.scenario {
            Scenario::Spike if is_relay && (40..60).contains(&(t % 120)) => extra += 150.0,
            Scenario::Isp if t >= 30 && addr != MOCK_CGNAT => {
                if t % 15 == 0 {
                    return None;
                }
                extra += 45.0;
            }
            Scenario::Route if is_relay && t >= 30 => extra += 90.0,
            _ => {}
        }
        Some(base + extra + noise)
    }
}

/// Ruido determinista en [-1, 1).
fn noise(addr: Ipv4Addr, t: u64) -> f64 {
    let mut x = (u32::from(addr) as u64) ^ t.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x % 2000) as f64 / 1000.0 - 1.0
}

#[async_trait::async_trait]
impl Prober for MockProber {
    async fn echo(&self, dest: Ipv4Addr, ttl: Option<u8>, _timeout: Duration) -> EchoResult {
        let path = [MOCK_GATEWAY, MOCK_CGNAT, MOCK_ISP, MOCK_ISP2];
        if let Some(ttl) = ttl {
            let i = ttl as usize;
            if i >= 1 && i <= path.len() && path[i - 1] != dest {
                let hop = path[i - 1];
                let t = self.next_t(hop);
                return match self.rtt(hop, t) {
                    Some(r) => EchoResult { kind: EchoKind::TtlExpired, rtt_ms: Some(r), from: Some(hop) },
                    None => EchoResult::lost(EchoKind::Timeout),
                };
            }
        }
        let t = self.next_t(dest);
        match self.rtt(dest, t) {
            Some(r) => EchoResult { kind: EchoKind::Reply, rtt_ms: Some(r), from: Some(dest) },
            None => EchoResult::lost(EchoKind::Timeout),
        }
    }
}

pub struct MockEnv {
    pub scenario: Scenario,
}

impl NetEnv for MockEnv {
    fn local_network(&self) -> Result<LocalNetwork, ProbeError> {
        Ok(LocalNetwork {
            iface_name: "demo0".into(),
            iface_label: "DEMO (simulado)".into(),
            iface_index: 0,
            local_ipv4: Some(Ipv4Addr::new(192, 168, 1, 50)),
            gateway: MOCK_GATEWAY,
            gateway_mac: Some("02:00:00:00:00:01".into()),
        })
    }

    fn wifi(&self, _net: &LocalNetwork) -> Option<WifiInfo> {
        (self.scenario == Scenario::Wifi).then(|| WifiInfo {
            ssid_hash: "demo".into(),
            rssi_dbm: Some(-74),
            signal_quality: Some(52),
            band: WifiBand::G2_4,
            channel: Some(6),
            link_mbps: Some(72.0),
        })
    }
}
