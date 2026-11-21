//! Consola dentro de la app (§3.1): se parsea en Rust con una lista blanca de comandos.
//! Ningún comando ejecuta procesos del sistema; las acciones con impacto llegan en F4
//! con su diálogo de confirmación nativo.

use std::sync::atomic::Ordering;
use std::time::Duration;

use orion_engine::{BaselineScope, TargetStats, ISP_HOP_ID};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use ts_rs::TS;

use crate::state::{lock, AppState};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CliLineKind {
    Info,
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CliLine {
    pub kind: CliLineKind,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CliOutput {
    pub lines: Vec<CliLine>,
    /// El frontend debe limpiar la consola.
    pub clear: bool,
}

impl CliOutput {
    fn push(&mut self, kind: CliLineKind, text: impl Into<String>) {
        self.lines.push(CliLine { kind, text: text.into() });
    }
    fn info(&mut self, t: impl Into<String>) {
        self.push(CliLineKind::Info, t)
    }
    fn ok(&mut self, t: impl Into<String>) {
        self.push(CliLineKind::Ok, t)
    }
    fn warn(&mut self, t: impl Into<String>) {
        self.push(CliLineKind::Warn, t)
    }
    fn err(&mut self, t: impl Into<String>) {
        self.push(CliLineKind::Error, t)
    }
}

fn ms(v: Option<f64>) -> String {
    v.map_or("—".into(), |v| format!("{v:.1} ms"))
}

fn target_line(t: &TargetStats) -> String {
    format!(
        "{:<28} {:<16} último {:>9} | p95 {:>9} | jitter {:>9} | pérdida {:>5.1} % | {:?}",
        t.label,
        t.addr,
        ms(t.last_ms),
        ms(t.p95_ms),
        ms(t.jitter_ms),
        t.loss_pct,
        t.status
    )
}

const HELP: &[(&str, &str)] = &[
    ("help", "Lista de comandos"),
    ("status", "Semáforo, diagnóstico y métricas actuales"),
    ("ping", "4 ecos ICMP reales al relay activo"),
    ("relays", "RTT medido de los POP vigilados"),
    ("region <pop>", "Cambia la región activa (p. ej. region scl)"),
    ("baseline", "Estado de la línea base estadística"),
    ("baseline reset --yes", "Borra la línea base de la red actual"),
    ("diag", "Diagnóstico por segmento con su evidencia"),
    ("clear", "Limpia la consola"),
];

pub async fn exec(line: &str, app: &AppHandle, state: &AppState) -> CliOutput {
    let mut out = CliOutput::default();
    let line = line.trim();
    if line.len() > 200 {
        out.err("Comando demasiado largo.");
        return out;
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    let Some(&cmd) = parts.first() else { return out };
    let cmd = cmd.to_ascii_lowercase();

    match cmd.as_str() {
        "help" => {
            out.info("Comandos disponibles:");
            for (c, d) in HELP {
                out.info(format!("  {c:<22} {d}"));
            }
        }
        "clear" => out.clear = true,
        "status" => {
            let tick = lock(&state.last_tick).clone();
            match tick {
                None => out.warn("Sin datos todavía: ¿está iniciado el monitoreo?"),
                Some(t) => {
                    if state.demo.load(Ordering::Relaxed) {
                        out.warn("MODO DEMO: datos simulados.");
                    }
                    out.ok(format!("Semáforo: {} — {}", t.readiness.label(), t.readiness_reason));
                    out.info(format!("Diagnóstico: {:?} — {}", t.diagnosis.verdict, t.diagnosis.message));
                    let active = t.active_relay.as_deref();
                    for s in t.targets.iter().filter(|s| s.id == "gateway" || s.id == ISP_HOP_ID || Some(s.id.as_str()) == active) {
                        out.info(target_line(s));
                    }
                }
            }
        }
        "ping" => {
            if state.demo.load(Ordering::Relaxed) {
                out.warn("En MODO DEMO no se envían ecos reales.");
                return out;
            }
            let addr = {
                let e = lock(&state.engine);
                e.active_relay().and_then(|id| e.target_info(id)).and_then(|i| i.addr.split(", ").next()?.parse::<std::net::Ipv4Addr>().ok())
            };
            let Some(addr) = addr else {
                out.err("No hay un relay activo con dirección conocida.");
                return out;
            };
            let prober = match orion_probe::system_prober() {
                Ok(p) => p,
                Err(e) => {
                    out.err(format!("ICMP no disponible: {e}"));
                    return out;
                }
            };
            out.info(format!("Eco ICMP (32 bytes) a {addr}:"));
            let mut ok = 0;
            for i in 1..=4 {
                let r = prober.echo(addr, None, Duration::from_millis(1000)).await;
                match r.reply_rtt() {
                    Some(rtt) => {
                        ok += 1;
                        out.ok(format!("  #{i} respuesta de {addr}: {rtt:.1} ms"));
                    }
                    None => out.warn(format!("  #{i} sin respuesta ({:?})", r.kind)),
                }
                if i < 4 {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
            out.info(format!("  enviados 4, recibidos {ok}, pérdida {} %", (4 - ok) * 25));
        }
        "relays" => {
            let e = lock(&state.engine);
            let active = e.active_relay().map(str::to_string);
            for id in e.target_ids().into_iter().filter(|id| id.starts_with("relay:")) {
                if let Some(s) = e.target_stats(&id) {
                    let mark = if active.as_deref() == Some(id.as_str()) { "*" } else { " " };
                    out.info(format!("{mark} {}", target_line(&s)));
                }
            }
            if out.lines.is_empty() {
                out.warn("Todavía no hay relays medidos.");
            }
        }
        "region" => match parts.get(1) {
            None => out.info(format!("Región activa: {}", lock(&state.settings).region)),
            Some(pop) => {
                let pop = pop.to_ascii_lowercase();
                let known = lock(&state.catalog).as_ref().map(|c| c.pop(&pop).is_some());
                if known == Some(false) {
                    out.err(format!("'{pop}' no está en el catálogo de relays."));
                    return out;
                }
                let json = {
                    let mut s = lock(&state.settings);
                    let prev = s.region.clone();
                    s.region = pop.clone();
                    if let Err(e) = s.validate() {
                        s.region = prev;
                        out.err(e);
                        return out;
                    }
                    serde_json::to_string(&*s).unwrap_or_default()
                };
                let _ = lock(&state.store).set_setting(crate::state::SETTINGS_KEY, &json);
                if let Some(h) = state.monitor.lock().await.as_ref() {
                    h.set_region(&pop);
                }
                out.ok(format!("Región activa: {pop}"));
            }
        },
        "baseline" => {
            if parts.get(1) == Some(&"reset") {
                if parts.get(2) != Some(&"--yes") {
                    out.warn("Esto borra lo aprendido para la red actual. Repite con: baseline reset --yes");
                    return out;
                }
                let net = {
                    let mut e = lock(&state.engine);
                    e.reset_baseline(&BaselineScope::Network);
                    e.network_id()
                };
                if !state.demo.load(Ordering::Relaxed) {
                    if let Err(e) = lock(&state.store).delete_baselines(&BaselineScope::Network, net) {
                        out.err(format!("Error al borrar en disco: {e}"));
                        return out;
                    }
                }
                crate::events::emit_log(app, orion_probe::monitor::LogLevel::Warn, "Línea base de la red actual reiniciada desde la consola.");
                out.ok("Línea base de la red actual reiniciada.");
                return out;
            }
            match lock(&state.last_tick).as_ref().and_then(|t| t.baseline.clone()) {
                None => out.warn("Sin línea base todavía para el relay activo."),
                Some(b) => {
                    if b.converged {
                        out.ok(format!(
                            "μ = {:.1} ms, σ = {:.1} ms, k = {}, umbral = {:.1} ms, anomalía = {:.2} ({} muestras)",
                            b.mu_ms, b.sigma_ms, b.k, b.threshold_ms, b.anomaly_score, b.samples
                        ));
                    } else {
                        out.info(format!("Calibrando ({}/{}). μ provisional = {:.1} ms", b.samples, b.converge_after, b.mu_ms));
                    }
                }
            }
        }
        "diag" => match lock(&state.last_tick).as_ref() {
            None => out.warn("Sin datos todavía."),
            Some(t) => {
                out.ok(format!("{:?}: {}", t.diagnosis.verdict, t.diagnosis.message));
                for s in &t.diagnosis.segments {
                    out.info(format!("  {:?}: {:?}", s.segment, s.state));
                    for e in &s.evidence {
                        out.info(format!("    {} {} = {:.1} (umbral {:.1})", e.target, e.metric, e.value, e.threshold));
                    }
                }
            }
        },
        "flushdns" | "router-reboot" | "export-report" | "bufferbloat" => {
            out.warn(format!("'{cmd}' llegará en una versión posterior (F3/F4), con confirmación y verificación real."));
        }
        "optimize" => out.warn("'optimize' se eliminó: los ajustes de MTU/QoS de v3.0 no tenían efecto medible."),
        _ => out.err(format!("Comando '{cmd}' no reconocido. Escribe 'help'.")),
    }
    out
}
