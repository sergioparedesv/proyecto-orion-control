// Los comandos propios se declaran en el manifiesto de la app para que las capabilities
// (capabilities/main.json) decidan cuáles puede invocar cada ventana.
const COMMANDS: &[&str] = &[
    "get_status",
    "start_monitor",
    "stop_monitor",
    "set_region",
    "list_relays",
    "reset_baseline",
    "get_settings",
    "set_settings",
    "cli_exec",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("error en tauri-build");
}
