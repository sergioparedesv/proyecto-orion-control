//! Sondeos de red de ORION (§6.1).
//!
//! Todo lo que toca el sistema operativo está detrás de dos traits:
//! - [`Prober`]: envía un eco ICMP (opcionalmente con TTL, para traceroute).
//! - [`NetEnv`]: describe la red local (gateway, interfaz) y el Wi-Fi.
//!
//! Así el bucle de [`monitor`] es idéntico con el sistema real y con los mocks.

pub mod catalog;
pub mod gateway;
pub mod hops;
pub mod icmp;
#[cfg(feature = "mock")]
pub mod mock;
pub mod monitor;
pub mod net;
pub mod wifi;

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

pub use orion_engine;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("no se encontró una interfaz de red con ruta por defecto: {0}")]
    NoDefaultRoute(String),
    #[error("la interfaz por defecto no tiene gateway IPv4")]
    NoGateway,
    #[error("error del sistema: {0}")]
    System(String),
    #[error("no soportado en esta plataforma: {0}")]
    Unsupported(&'static str),
    #[error("error de red: {0}")]
    Http(#[from] reqwest::Error),
    #[error("error de E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("formato inválido: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub enum EchoKind {
    /// El destino respondió.
    Reply,
    /// Un router intermedio respondió "TTL excedido" (traceroute).
    TtlExpired,
    Timeout,
    Unreachable,
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EchoResult {
    pub kind: EchoKind,
    /// RTT medido. Solo `Some` para `Reply` y `TtlExpired`.
    pub rtt_ms: Option<f64>,
    /// Quién respondió (el destino o el hop intermedio).
    pub from: Option<Ipv4Addr>,
}

impl EchoResult {
    pub fn lost(kind: EchoKind) -> Self {
        Self { kind, rtt_ms: None, from: None }
    }

    /// RTT si el **destino** respondió; cualquier otra cosa cuenta como pérdida.
    pub fn reply_rtt(&self) -> Option<f64> {
        (self.kind == EchoKind::Reply).then_some(self.rtt_ms).flatten()
    }
}

#[async_trait::async_trait]
pub trait Prober: Send + Sync {
    async fn echo(&self, dest: Ipv4Addr, ttl: Option<u8>, timeout: Duration) -> EchoResult;
}

/// Red local vista por la interfaz con ruta por defecto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalNetwork {
    pub iface_name: String,
    pub iface_label: String,
    pub iface_index: u32,
    pub local_ipv4: Option<Ipv4Addr>,
    pub gateway: Ipv4Addr,
    pub gateway_mac: Option<String>,
}

pub trait NetEnv: Send + Sync {
    fn local_network(&self) -> Result<LocalNetwork, ProbeError>;
    /// Wi-Fi del adaptador activo; `None` si se usa cable o la plataforma no lo soporta.
    fn wifi(&self, net: &LocalNetwork) -> Option<orion_engine::WifiInfo>;
}

/// Entorno del sistema real.
pub struct SystemEnv;

impl NetEnv for SystemEnv {
    fn local_network(&self) -> Result<LocalNetwork, ProbeError> {
        gateway::discover()
    }

    fn wifi(&self, net: &LocalNetwork) -> Option<orion_engine::WifiInfo> {
        wifi::query(net)
    }
}

/// Prober ICMP nativo de la plataforma.
pub fn system_prober() -> Result<Arc<dyn Prober>, ProbeError> {
    icmp::system_prober()
}
