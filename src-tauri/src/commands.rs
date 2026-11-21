//! Comandos IPC (frontend → núcleo), §7.1.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use orion_engine::{relay_id, BaselineScope, Engine};
use orion_probe::catalog::Catalog;
use orion_probe::monitor::{self, LogLevel, MonitorConfig, MonitorDeps};
use orion_probe::{NetEnv, Prober, SystemEnv};
use tauri::{AppHandle, State};

use crate::cli::{self, CliOutput};
use crate::events::{emit_log, pump};
use crate::state::{lock, AppState, CatalogMeta, RelayInfo, Settings, StatusSnapshot, SETTINGS_KEY};

type CmdResult<T> = Result<T, String>;

fn demo_available() -> bool {
    cfg!(debug_assertions)
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> CmdResult<StatusSnapshot> {
    let monitoring = state.monitor.lock().await.as_ref().is_some_and(|h| !h.is_finished());
    Ok(StatusSnapshot {
        monitoring,
        demo: state.demo.load(Ordering::Relaxed),
        demo_available: demo_available(),
        region: lock(&state.settings).region.clone(),
        tick: lock(&state.last_tick).clone(),
        network: lock(&state.network).clone(),
        catalog: lock(&state.catalog).as_ref().map(|c| CatalogMeta { source: c.source, revision: c.revision }),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub async fn start_monitor(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let mut slot = state.monitor.lock().await;
    if slot.as_ref().is_some_and(|h| !h.is_finished()) {
        return Ok(());
    }
    let settings = lock(&state.settings).clone();

    let demo_scenario = if demo_available() { settings.demo_scenario.clone() } else { None };
    let (prober, env, catalog): (Arc<dyn Prober>, Arc<dyn NetEnv>, Catalog) = match &demo_scenario {
        Some(s) => {
            let sc: orion_probe::mock::Scenario = s.parse()?;
            emit_log(&app, LogLevel::Warn, format!("MODO DEMO ({s}): los datos son simulados, no son mediciones reales."));
            (
                Arc::new(orion_probe::mock::MockProber::new(sc)) as Arc<dyn Prober>,
                Arc::new(orion_probe::mock::MockEnv { scenario: sc }) as Arc<dyn NetEnv>,
                Catalog::bundled(),
            )
        }
        None => {
            let http = reqwest::Client::builder().user_agent("orion-netcontrol").build().map_err(|e| e.to_string())?;
            let (catalog, notes) = Catalog::load(Some(state.data_dir.as_path()), &http).await;
            for n in notes {
                emit_log(&app, LogLevel::Warn, n);
            }
            let prober = orion_probe::system_prober().map_err(|e| e.to_string())?;
            (prober, Arc::new(SystemEnv) as Arc<dyn NetEnv>, catalog)
        }
    };
    let demo = demo_scenario.is_some();

    // Motor nuevo en cada arranque: en DEMO sin historial; en real, con las líneas base guardadas.
    let mut engine = Engine::new(settings.engine_config());
    if !demo {
        match lock(&state.store).load_baselines() {
            Ok(items) => engine.load_baselines(items),
            Err(e) => emit_log(&app, LogLevel::Error, format!("No se pudieron cargar las líneas base: {e}")),
        }
    }
    *lock(&state.engine) = engine;
    state.demo.store(demo, Ordering::Relaxed);
    *lock(&state.catalog) = Some(catalog.clone());
    *lock(&state.last_tick) = None;
    *lock(&state.network) = None;

    let region = if catalog.pop(&settings.region).is_some() { settings.region.clone() } else { "lim".to_string() };
    let cfg = MonitorConfig { watch_pops: settings.watch_pops.clone(), ..Default::default() };
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let deps = MonitorDeps { prober, env, catalog, engine: state.engine.clone(), demo };
    *slot = Some(monitor::spawn(deps, cfg, region.clone(), tx));
    tauri::async_runtime::spawn(pump(app.clone(), rx));
    emit_log(&app, LogLevel::Info, format!("Monitoreo iniciado (región {region})."));
    Ok(())
}

#[tauri::command]
pub async fn stop_monitor(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(h) = state.monitor.lock().await.take() {
        h.stop();
        state.persist_baselines();
        emit_log(&app, LogLevel::Info, "Monitoreo detenido.");
    }
    Ok(())
}

#[tauri::command]
pub async fn set_region(pop: String, state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(c) = lock(&state.catalog).as_ref() {
        if c.pop(&pop).is_none() {
            return Err(format!("La región '{pop}' no está en el catálogo de relays."));
        }
    }
    let settings = {
        let mut s = lock(&state.settings);
        s.region = pop.clone();
        s.validate()?;
        s.clone()
    };
    save_settings(&state, &settings)?;
    if let Some(h) = state.monitor.lock().await.as_ref() {
        h.set_region(&pop);
    }
    Ok(())
}

#[tauri::command]
pub async fn list_relays(state: State<'_, AppState>) -> CmdResult<Vec<RelayInfo>> {
    let settings = lock(&state.settings).clone();
    let catalog = lock(&state.catalog).clone().unwrap_or_else(Catalog::bundled);
    let engine = lock(&state.engine);
    let active = engine.active_relay().map(str::to_string);
    Ok(catalog
        .pops
        .values()
        .filter(|p| settings.watch_pops.contains(&p.code) || p.code == settings.region)
        .map(|p| {
            let id = relay_id(&p.code);
            RelayInfo {
                pop: p.code.clone(),
                desc: p.desc.clone(),
                active: active.as_deref() == Some(id.as_str()),
                watched: settings.watch_pops.contains(&p.code),
                stats: engine.target_stats(&id),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn reset_baseline(scope: BaselineScope, app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let network_id = {
        let mut e = lock(&state.engine);
        e.reset_baseline(&scope);
        e.network_id()
    };
    if !state.demo.load(Ordering::Relaxed) {
        lock(&state.store).delete_baselines(&scope, network_id).map_err(|e| e.to_string())?;
    }
    emit_log(&app, LogLevel::Warn, format!("Línea base reiniciada ({scope:?})."));
    Ok(())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(lock(&state.settings).clone())
}

/// Guarda los ajustes. La región se aplica al instante; el resto, al reiniciar el monitoreo.
#[tauri::command]
pub async fn set_settings(mut settings: Settings, state: State<'_, AppState>) -> CmdResult<Settings> {
    if !demo_available() {
        settings.demo_scenario = None;
    }
    settings.validate()?;
    save_settings(&state, &settings)?;
    lock(&state.engine).set_config(settings.engine_config());
    *lock(&state.settings) = settings.clone();
    if let Some(h) = state.monitor.lock().await.as_ref() {
        h.set_region(&settings.region);
    }
    Ok(settings)
}

#[tauri::command]
pub async fn cli_exec(line: String, app: AppHandle, state: State<'_, AppState>) -> CmdResult<CliOutput> {
    Ok(cli::exec(&line, &app, &state).await)
}

fn save_settings(state: &AppState, s: &Settings) -> CmdResult<()> {
    let json = serde_json::to_string(s).map_err(|e| e.to_string())?;
    lock(&state.store).set_setting(SETTINGS_KEY, &json).map_err(|e| e.to_string())
}
