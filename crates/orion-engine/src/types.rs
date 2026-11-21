//! Tipos compartidos entre el núcleo, la app Tauri y el frontend.
//! Los que llevan `#[ts(export)]` se generan en `ui/src/bindings/` con `cargo test`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::diagnosis::Diagnosis;
use crate::readiness::Readiness;
use crate::rules::Recommendation;

/// Identificador estable de un objetivo: "gateway", "isp-hop", "relay:lim".
pub type TargetId = String;

pub const GATEWAY_ID: &str = "gateway";
pub const ISP_HOP_ID: &str = "isp-hop";

pub fn relay_id(pop: &str) -> TargetId {
    format!("relay:{pop}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum TargetKind {
    Gateway,
    IspHop,
    Relay { pop: String },
    Reference,
}

/// Descripción de un objetivo registrado en el motor.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TargetInfo {
    pub id: TargetId,
    pub label: String,
    pub addr: String,
    pub kind: TargetKind,
    /// El objetivo es una aproximación (p. ej. el hop del ISP no respondía y se usó el siguiente).
    pub approximate: bool,
    /// Se comprobó que el objetivo no responde ICMP.
    pub icmp_blocked: bool,
}

/// Una medición individual. `rtt_ms = None` significa pérdida/timeout.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub target: TargetId,
    pub rtt_ms: Option<f64>,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TargetStatus {
    Ok,
    Degraded,
    Down,
    IcmpBlocked,
    NoData,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TargetStats {
    pub id: String,
    pub label: String,
    pub addr: String,
    pub kind: TargetKind,
    pub approximate: bool,
    pub last_ms: Option<f64>,
    pub min_ms: Option<f64>,
    pub median_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub loss_pct: f64,
    /// Muestras en la ventana (válidas + perdidas).
    pub samples: u32,
    pub valid: u32,
    pub status: TargetStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BaselineView {
    pub target_id: String,
    pub slot: u8,
    pub mu_ms: f64,
    pub sigma_ms: f64,
    pub k: f64,
    pub threshold_ms: f64,
    pub samples: u32,
    pub converge_after: u32,
    pub converged: bool,
    pub anomaly_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WifiBand {
    #[serde(rename = "2.4ghz")]
    G2_4,
    #[serde(rename = "5ghz")]
    G5,
    #[serde(rename = "6ghz")]
    G6,
    Unknown,
}

/// Información del enlace Wi-Fi (solo Windows en el MVP).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WifiInfo {
    /// El SSID nunca viaja en claro fuera del módulo de sondeo: solo su hash.
    pub ssid_hash: String,
    pub rssi_dbm: Option<i32>,
    /// Calidad de señal 0-100 tal como la reporta el sistema.
    pub signal_quality: Option<u32>,
    pub band: WifiBand,
    pub channel: Option<u32>,
    pub link_mbps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TelemetryTick {
    #[ts(type = "number")]
    pub ts: i64,
    #[ts(type = "number")]
    pub network_id: i64,
    pub active_relay: Option<String>,
    pub targets: Vec<TargetStats>,
    pub readiness: Readiness,
    pub readiness_reason: String,
    pub baseline: Option<BaselineView>,
    pub diagnosis: Diagnosis,
    pub wifi: Option<WifiInfo>,
    pub recommendations: Vec<Recommendation>,
}

/// Métricas de ventana resumidas, entrada del diagnóstico y del semáforo.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Metrics {
    pub samples: usize,
    pub valid: usize,
    pub p95: Option<f64>,
    pub median: Option<f64>,
    pub jitter: Option<f64>,
    pub loss_pct: f64,
}
