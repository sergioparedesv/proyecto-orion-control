// En release no se abre una consola junto a la ventana en Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod commands;
mod events;
mod state;
mod store;

use tauri::Manager;

use crate::state::AppState;
use crate::store::Store;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = match Store::open(&data_dir.join("orion.db")) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("no se pudo abrir la base de datos ({e}); se usa una en memoria");
                    Store::in_memory()?
                }
            };
            app.manage(AppState::new(store, data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::start_monitor,
            commands::stop_monitor,
            commands::set_region,
            commands::list_relays,
            commands::reset_baseline,
            commands::get_settings,
            commands::set_settings,
            commands::cli_exec,
        ])
        .build(tauri::generate_context!())
        .expect("error al construir la app");

    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(state) = handle.try_state::<AppState>() {
                state.persist_baselines();
            }
        }
    });
}
