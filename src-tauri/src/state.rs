//! Estado compartido de la app y tipos del contrato IPC (§7).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use orion_engine::{Engine, EngineConfig, TargetStats, TelemetryTick};
use orion_probe::catalog::{Catalog, CatalogSource, DEFAULT_WATCH};
use orion_probe::monitor::{MonitorHandle, NetworkSnapshot};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::store::Store;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Settings {
    /// POP activo (código SDR: lim, scl, eze, gru, iad, ...).
    pub region: String,
    /// POP vigilados además del activo (para sugerir alternativas).
    pub watch_pops: Vec<String>,
    /// k del umbral dinámico μ + k·σ.
    pub baseline_k: f64,
    /// Solo compilaciones de desarrollo: escenario simulado (calm|spike|wifi|isp|route).
    pub demo_scenario: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            region: "lim".into(),
            watch_pops: DEFAULT_WATCH.iter().map(|s| s.to_string()).collect(),
            baseline_k: 3.0,
            demo_scenario: None,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        let pop_ok = |p: &str| (2..=6).contains(&p.len()) && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        if !pop_ok(self.region.as_str()) {
            return Err(format!("código de región inválido: '{}'", self.region));
        }
        if self.watch_pops.len() > 12 || !self.watch_pops.iter().all(|p| pop_ok(p.as_str())) {
            return Err("lista de POP vigilados inválida (máx. 12 códigos)".into());
        }
        if !(1.0..=6.0).contains(&self.baseline_k) {
            return Err("k debe estar entre 1 y 6".into());
        }
        if let Some(s) = &self.demo_scenario {
            if !["calm", "spike", "wifi", "isp", "route"].contains(&s.as_str()) {
                return Err(format!("escenario DEMO desconocido: '{s}'"));
            }
        }
        Ok(())
    }

    pub fn engine_config(&self) -> EngineConfig {
        let mut cfg = EngineConfig::default();
        cfg.baseline.k = self.baseline_k;
        cfg
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CatalogMeta {
    pub source: CatalogSource,
    #[ts(type = "number | null")]
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatusSnapshot {
    pub monitoring: bool,
    pub demo: bool,
    /// El modo DEMO solo existe en compilaciones de desarrollo.
    pub demo_available: bool,
    pub region: String,
    pub tick: Option<TelemetryTick>,
    pub network: Option<NetworkSnapshot>,
    pub catalog: Option<CatalogMeta>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RelayInfo {
    pub pop: String,
    pub desc: String,
    pub active: bool,
    pub watched: bool,
    pub stats: Option<TargetStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogEvent {
    #[ts(type = "number")]
    pub ts: i64,
    pub level: orion_probe::monitor::LogLevel,
    pub message: String,
}

pub struct AppState {
    pub engine: Arc<Mutex<Engine>>,
    pub monitor: tokio::sync::Mutex<Option<MonitorHandle>>,
    pub settings: Mutex<Settings>,
    pub store: Mutex<Store>,
    pub catalog: Mutex<Option<Catalog>>,
    pub last_tick: Mutex<Option<TelemetryTick>>,
    pub network: Mutex<Option<NetworkSnapshot>>,
    pub demo: AtomicBool,
    pub data_dir: PathBuf,
}

/// `lock()` que no entra en pánico si otro hilo murió con el mutex tomado.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub const SETTINGS_KEY: &str = "settings.v1";

impl AppState {
    pub fn new(store: Store, data_dir: PathBuf) -> Self {
        let settings = store
            .get_setting(SETTINGS_KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .filter(|s| s.validate().is_ok())
            .unwrap_or_default();
        let engine = Engine::new(settings.engine_config());
        Self {
            engine: Arc::new(Mutex::new(engine)),
            monitor: tokio::sync::Mutex::new(None),
            settings: Mutex::new(settings),
            store: Mutex::new(store),
            catalog: Mutex::new(None),
            last_tick: Mutex::new(None),
            network: Mutex::new(None),
            demo: AtomicBool::new(false),
            data_dir,
        }
    }

    /// Guarda las líneas base del motor (nunca en modo DEMO).
    pub fn persist_baselines(&self) {
        if self.demo.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let now = chrono::Utc::now().timestamp();
        let engine = lock(&self.engine);
        if let Err(e) = lock(&self.store).save_baselines(engine.baselines(), now) {
            tracing::error!("no se pudieron guardar las líneas base: {e}");
        }
    }
}
