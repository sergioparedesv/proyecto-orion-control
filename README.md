# ORION NetControl v4.0

Herramienta de escritorio (Rust + Tauri 2) que mide de verdad la conexión hacia los relays de Valve, ubica el problema (LAN / ISP / ruta) y dice si conviene buscar partida. Diseño completo en [`docs/blueprint.md`](docs/blueprint.md); cómo se mide cada cosa en [`docs/metodologia.md`](docs/metodologia.md).

Estado: **F0–F2** (núcleo de medición, CLI y app con Telemetría, Diagnóstico, Línea base y Consola). Bufferbloat, historial, informes y acciones llegan en F3–F4.

## Descargar y ejecutar (usuarios)

Todo lo necesario para obtener la app está en la carpeta [`Release/`](Release/):

| Archivo | Para qué sirve |
|---|---|
| [`Release/INSTRUCCIONES.html`](Release/INSTRUCCIONES.html) | Guía paso a paso para instalar o compilar (ábrela en el navegador) |
| [`Release/construir.ps1`](Release/construir.ps1) | Script que compila la app en Windows y deja los instaladores en `Release/instaladores/` |
| [`Release/orion-netcontrol-v0.4.0-fuente.zip`](Release/orion-netcontrol-v0.4.0-fuente.zip) | Código fuente que usa el script |

Pasos rápidos en Windows 10/11 (64 bits), con Rust, Node.js LTS y Visual Studio Build Tools instalados:

```powershell
cd C:\orion\Release
powershell -ExecutionPolicy Bypass -File .\construir.ps1
```

Luego ejecuta `Release\instaladores\ORION NetControl_0.4.0_x64-setup.exe`. Los detalles y la solución de problemas están en `INSTRUCCIONES.html`.

## Estructura

```
crates/orion-engine   estadística y decisión (sin dependencias del SO, 100 % testeable)
crates/orion-probe    ICMP, gateway, traceroute, catálogo SDR, Wi-Fi, bucle de monitoreo, mocks
crates/orion-cli      spikes de F0 y monitor en consola
src-tauri/            app de escritorio: IPC, eventos, SQLite, consola con lista blanca
ui/                   frontend (Vite + TS + Tailwind local + Chart.js), estética de v3.0
docs/                 blueprint, metodología, resultados de F0, muestras, v3.0 de referencia
```

## Requisitos (Windows, solo para desarrollar)

1. Rust: `rustup` con `stable-x86_64-pc-windows-msvc`.
2. Visual Studio Build Tools con "Desarrollo de escritorio con C++".
3. Node.js LTS.
4. Tauri CLI: `cargo install tauri-cli --version "^2" --locked`.

> Desarrolla y ejecuta en **Windows nativo**, no en WSL: desde WSL se mediría la red virtual de la VM, no tu router.

## Comandos

```powershell
# pruebas del núcleo
cargo test

# spikes de F0 (ver docs/f0-resultados.md)
cargo run -p orion-cli -- spike icmp 1.1.1.1 -n 20
cargo run -p orion-cli -- spike gateway
cargo run -p orion-cli -- spike hops 155.133.244.35
cargo run -p orion-cli -- spike sdr -n 5
cargo run -p orion-cli -- spike wifi

# monitor en consola (real o DEMO)
cargo run -p orion-cli -- monitor --region lim
cargo run -p orion-cli -- monitor --mock spike

# app
cd ui; npm install; cd ..
cargo tauri dev
cargo tauri build          # instalador .msi / .exe

# regenerar los tipos TypeScript desde Rust (ts-rs → ui/src/bindings)
cargo test -p orion-engine -p orion-probe -p orion-netcontrol
```

## Principios

- Ningún número inventado: si algo no se puede medir, se muestra "sin datos" o "ICMP bloqueado".
- Sin servidor HTTP local: solo IPC de Tauri con capabilities mínimas y CSP estricta.
- No toca el juego (ni memoria, ni archivos, ni opciones de lanzamiento).
- Datos locales: SQLite en el directorio de datos de la app; el SSID solo como hash.
- El modo DEMO existe solo en compilaciones de desarrollo y se muestra con un aviso permanente.
