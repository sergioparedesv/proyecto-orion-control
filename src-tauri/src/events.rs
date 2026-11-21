//! Bomba de eventos núcleo → frontend (§7.2).

use std::sync::atomic::Ordering;

use orion_probe::monitor::{LogLevel, MonitorEvent};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use crate::state::{lock, AppState, LogEvent};

pub const EV_SAMPLE: &str = "telemetry://sample";
pub const EV_DIAGNOSIS: &str = "diagnosis://update";
pub const EV_READINESS: &str = "readiness://change";
pub const EV_NETWORK: &str = "network://changed";
pub const EV_LOG: &str = "log://event";

/// Cada cuántos ticks se guardan las líneas base en disco.
const PERSIST_EVERY: u64 = 60;

pub fn emit_log(app: &AppHandle, level: LogLevel, message: impl Into<String>) {
    let ev = LogEvent { ts: chrono::Utc::now().timestamp_millis(), level, message: message.into() };
    let _ = app.emit(EV_LOG, &ev);
}

pub async fn pump(app: AppHandle, mut rx: mpsc::UnboundedReceiver<MonitorEvent>) {
    let mut ticks: u64 = 0;
    while let Some(ev) = rx.recv().await {
        let state = app.state::<AppState>();
        match ev {
            MonitorEvent::Tick(out) => {
                ticks += 1;
                let _ = app.emit(EV_SAMPLE, &out.tick);
                if let Some(d) = &out.diagnosis_changed {
                    let _ = app.emit(EV_DIAGNOSIS, d);
                }
                if let Some(c) = &out.readiness_change {
                    let _ = app.emit(EV_READINESS, c);
                    emit_log(&app, LogLevel::Info, format!("Semáforo: {} → {} ({})", c.from.label(), c.to.label(), c.reason));
                }
                *lock(&state.last_tick) = Some(out.tick);
                if ticks % PERSIST_EVERY == 0 {
                    state.persist_baselines();
                }
            }
            MonitorEvent::Network(n) => {
                if !state.demo.load(Ordering::Relaxed) && n.network_id != 0 {
                    let kind = if n.wifi.is_some() { "wifi" } else { "cable" };
                    let res = lock(&state.store).upsert_network(
                        n.network_id,
                        n.wifi.as_ref().map(|w| w.ssid_hash.as_str()),
                        n.gateway.as_deref(),
                        kind,
                        chrono::Utc::now().timestamp(),
                    );
                    if let Err(e) = res {
                        tracing::error!("no se pudo registrar la red: {e}");
                    }
                }
                let _ = app.emit(EV_NETWORK, &n);
                *lock(&state.network) = Some(n);
            }
            MonitorEvent::Log { level, message } => emit_log(&app, level, message),
        }
    }
}
