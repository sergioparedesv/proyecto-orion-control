# ORION NetControl — Blueprint v4.0 (Rust + Tauri)

> Documento de diseño para reconstruir **ORION DOTA NETCONTROL v3.0** como una herramienta real.
> La v3.0 es una maqueta HTML con métricas simuladas. La v4.0 mide de verdad, diagnostica de verdad y nunca reporta algo que no ocurrió.

- **Estado:** borrador de diseño
- **Fecha:** 2026-11-19
- **Plataforma objetivo:** Windows 10/11 (primaria), Linux (secundaria)
- **Stack:** Rust (núcleo) + Tauri 2 (app de escritorio) + frontend web (HTML/TS, reutilizando la estética de v3.0)
- **Tipo de aplicación:** app de escritorio instalable (tipo Electron, pero con Tauri). **No** usa Docker. Ver §15.
- **APIs externas:** ninguna de pago y ninguna API key. Ver §16.

---

## 1. Intención del proyecto

### 1.1 Qué se quiere lograr

Un jugador de Dota 2 en Latinoamérica quiere responder **antes y durante la partida** tres preguntas:

1. **¿Mi conexión está lista para buscar partida ahora?** Semáforo de *readiness*.
2. **Si va mal, ¿dónde está el problema?** En mi Wi-Fi o LAN, en mi ISP, o en la ruta hacia el servidor de Valve.
3. **¿Qué puedo hacer al respecto?** Acciones concretas y seguras, con su resultado verificado.

La v3.0 ya expresa esa intención: monitor de RTT, matriz de relays, prueba de bufferbloat, control del gateway, un motor que "aprende" la línea base y una consola. Lo que falta es que todo sea **real**.

### 1.2 Usuario objetivo

- Jugador de Dota 2 en LATAM (servidores Perú, Chile, Argentina, Brasil, US East).
- Usuario técnico medio: sabe qué es el ping, pero no sabe leer un traceroute.
- Juega en Windows, a menudo por Wi-Fi y con otros dispositivos consumiendo ancho de banda en casa.

### 1.3 Propuesta de valor (lo que v4.0 hace que otras herramientas no)

| Capacidad | Herramientas genéricas (speedtest, ping) | ORION v4.0 |
|---|---|---|
| Latencia a los relays de Valve de la región | No | Sí |
| Ubica el fallo por segmento (LAN / ISP / ruta) | No | Sí |
| Línea base personal por red y franja horaria | No | Sí |
| Bufferbloat medido contra el gateway y la ruta de juego | Parcial | Sí |
| Semáforo "buscar / no buscar partida" con histéresis | No | Sí |
| Acciones de remediación con verificación posterior | No | Sí |

---

## 2. Principios de diseño

1. **Honestidad ante todo.** Ningún valor fijo se presenta como medición. Si algo no se puede medir, la interfaz dice "sin datos" o "no disponible", nunca inventa un número. Cada acción reporta su resultado real, con el código de salida y el error.
2. **Seguridad por defecto.** No hay ningún puerto HTTP local abierto. La comunicación frontend↔núcleo va por el IPC de Tauri, con *capabilities* mínimas. No se usa `shell=true` en ninguna parte.
3. **Mínimo privilegio.** La app corre como usuario normal. La elevación (administrador) se pide **por acción**, con confirmación explícita y explicando qué se va a ejecutar.
4. **Cero contacto con el juego.** No se lee memoria del proceso, no se inyecta nada, no se modifican archivos ni opciones de lanzamiento de Dota. Solo se mide la red. Esto evita cualquier riesgo con VAC.
5. **Privacidad.** Todos los datos se quedan en el equipo. No hay telemetría hacia servidores propios.
6. **Bajo consumo.** Menos del 1 % de CPU en monitoreo continuo y un tráfico de sondeo despreciable (menos de 2 KB/s). El modo partida reduce todavía más la carga visual.

---

## 3. Alcance

### 3.1 MVP (v4.0)

- Monitoreo continuo de RTT, jitter y pérdida hacia: el gateway, el primer salto público del ISP y los relays de la región seleccionada.
- Diagnóstico por segmento (LAN / ISP / ruta).
- Semáforo de *readiness* con histéresis.
- Línea base estadística (EWMA) por red y por relay, con detección de anomalías.
- Prueba de bufferbloat real (descarga y subida con saturación + medición de latencia bajo carga).
- Información de Wi-Fi (RSSI, banda, canal, velocidad de enlace) en Windows.
- Acciones: purgar la caché DNS, renovar DHCP, reiniciar el adaptador (con elevación).
- Historial en SQLite e informes exportables (JSON, CSV y TXT) con datos reales y su metodología.
- Consola CLI dentro de la app (los mismos comandos que v3.0, pero reales).

### 3.2 Fases posteriores

- Reinicio del router mediante *drivers* (enchufe inteligente Shelly/Tasmota y APIs de fabricantes).
- Cambio asistido del DNS del sistema (con elevación) y prueba comparativa de resolvedores.
- Modo "pre-partida": un chequeo de 30 s con veredicto antes de entrar a la cola.
- Overlay mínimo siempre visible (ventana sin bordes) para mirar durante la partida.
- Soporte de Linux completo y, más adelante, macOS.

### 3.3 Fuera de alcance (no-objetivos)

- "Optimizar" la red con ajustes mágicos (MTU, DSCP, buffers de socket) que no tienen efecto medible en Dota. Si en algún momento se agregan, será solo con una medición antes/después real.
- Cualquier interacción con el cliente de Dota o de Steam (memoria, archivos, consola del juego).
- VPN o *routing* alternativo.
- Funciones de "IA" sin un modelo real detrás. El motor es estadístico y se presenta así.

---

## 4. Arquitectura

```
┌───────────────────────────────────────────────────────────────┐
│                    Frontend (WebView Tauri)                   │
│  HTML/TS + Tailwind (build local) + Chart.js (versión fija)   │
│  Pestañas: Telemetría · Diagnóstico · Línea base · Bufferbloat│
│            · Acciones · Historial · Consola                   │
└───────────────▲───────────────────────────────┬───────────────┘
                │ eventos (emit)                │ comandos (invoke)
                │ telemetry://sample            │ start_monitor, run_bufferbloat,
                │ diagnosis://update            │ flush_dns, export_report, ...
┌───────────────┴───────────────────────────────▼───────────────┐
│                 src-tauri (capa de aplicación)                │
│  Commands · Estado compartido (Arc<AppState>) · Capabilities  │
└───────┬───────────────┬───────────────┬───────────────┬───────┘
        │               │               │               │
┌───────▼──────┐ ┌──────▼───────┐ ┌─────▼───────┐ ┌─────▼───────┐
│ orion-probe  │ │ orion-engine │ │orion-actions│ │ orion-store │
│ ICMP, hops,  │ │ métricas,    │ │ DNS, DHCP,  │ │ SQLite,     │
│ gateway,     │ │ línea base,  │ │ adaptador,  │ │ retención,  │
│ relays, Wi-Fi│ │ diagnóstico, │ │ elevación,  │ │ exportación │
│ bufferbloat  │ │ readiness    │ │ router (F2) │ │             │
└──────────────┘ └──────────────┘ └─────────────┘ └─────────────┘
```

### 4.1 Flujo de datos del monitoreo

1. El **scheduler** (`tokio::time::interval`, 1 s por defecto) dispara sondeos en paralelo hacia los objetivos activos: el gateway, el salto del ISP y N relays.
2. Cada sondeo produce un `ProbeResult { target, rtt: Option<Duration>, ts }`. Un `None` cuenta como pérdida.
3. El **engine** actualiza, por objetivo: una ventana deslizante (60 muestras), el jitter según RFC 3550, la pérdida en la ventana y la línea base EWMA.
4. El **diagnóstico** combina los segmentos y produce `Diagnosis` y `Readiness`.
5. Se emite el evento `telemetry://sample` al frontend y las muestras se agregan por minuto en SQLite.

---

## 5. Estructura del repositorio

```
orion-netcontrol/
├── Cargo.toml                    # workspace
├── crates/
│   ├── orion-probe/              # sondeos de red (sin dependencias de Tauri)
│   │   └── src/{lib.rs, icmp/, gateway.rs, hops.rs, relays.rs, wifi.rs, bufferbloat.rs}
│   ├── orion-engine/             # estadística y lógica de decisión (puro, 100 % testeable)
│   │   └── src/{lib.rs, window.rs, jitter.rs, baseline.rs, diagnosis.rs, readiness.rs, rules.rs}
│   ├── orion-actions/            # acciones del sistema con privilegios
│   │   └── src/{lib.rs, dns.rs, dhcp.rs, adapter.rs, elevation.rs, router/}
│   ├── orion-store/              # persistencia y exportación
│   │   └── src/{lib.rs, schema.sql, export.rs}
│   └── orion-cli/                # binario CLI opcional (headless, útil para pruebas y soporte)
├── src-tauri/
│   ├── tauri.conf.json
│   ├── capabilities/main.json
│   └── src/{main.rs, commands.rs, state.rs, events.rs}
├── ui/                           # frontend (migrado desde v3.0)
│   ├── index.html
│   ├── src/{main.ts, ipc.ts, tabs/, components/, audio.ts, background.ts}
│   └── tailwind.config.js
├── docs/
│   ├── blueprint.md              # este documento
│   └── metodologia.md            # cómo se mide cada métrica (se incluye en los informes)
└── .github/workflows/ci.yml      # build + tests en windows-latest y ubuntu-latest
```

**Regla de dependencias:** `orion-engine` no depende de nada del sistema operativo. `orion-probe` y `orion-actions` exponen *traits*, para poder sustituirlos por *mocks* en las pruebas.

---

## 6. Módulos del núcleo

### 6.1 `orion-probe` — Sondeos

#### 6.1.1 Trait principal

```rust
#[async_trait::async_trait]
pub trait Prober: Send + Sync {
    async fn probe(&self, target: &Target, timeout: Duration) -> ProbeResult;
}

pub struct Target {
    pub id: TargetId,          // "gateway", "isp-hop", "relay:lim-1"
    pub addr: IpAddr,
    pub kind: TargetKind,      // Gateway | IspHop | Relay { pop: String } | Reference
}

pub struct ProbeResult {
    pub target: TargetId,
    pub rtt: Option<Duration>, // None = timeout/pérdida
    pub ttl: Option<u8>,
    pub ts: SystemTime,
    pub error: Option<ProbeError>,
}
```

#### 6.1.2 Implementaciones ICMP

| Plataforma | Implementación | Privilegios |
|---|---|---|
| Windows | `IcmpSendEcho2` / `Icmp6SendEcho2` (crate `windows`, IP Helper API) | Usuario normal |
| Linux | `surge-ping` con socket ICMP DGRAM (`net.ipv4.ping_group_range`) | Usuario normal (si el sistema lo permite); si no, `CAP_NET_RAW` |

En Windows, `IcmpSendEcho2` es bloqueante: se ejecuta en `tokio::task::spawn_blocking` o con su variante de evento asíncrono. **Prohibido** invocar `ping.exe` y parsear su salida: depende del idioma del sistema, es lento y su precisión es pobre.

#### 6.1.3 Descubrimiento del gateway

- Windows: `GetBestRoute2` hacia 0.0.0.0 o lectura de la tabla de rutas (IP Helper).
- Multiplataforma: crate `netdev` (interfaz por defecto + gateway) como alternativa.
- Se vuelve a descubrir al detectar un cambio de red (otro SSID, cambio de adaptador o de IP local).

#### 6.1.4 Salto del ISP (primer hop público)

- Traceroute ligero por TTL incremental (1..=8) hacia un objetivo de referencia (el relay activo o 1.1.1.1).
- El "salto ISP" es el **primer hop con IP pública** (no RFC1918, no CGNAT 100.64.0.0/10).
- Se recalcula al arrancar, al cambiar de red y cada 10 minutos.
- Si ese hop no responde ICMP, se usa el siguiente que sí responda y se marca como `aproximado`.

#### 6.1.5 Catálogo de relays de Valve (SDR)

- **Fuente primaria (a validar en F0):** la configuración pública de Steam Datagram Relay para el appid 570, con los POP y las IP de sus relays. Se descarga al iniciar, se guarda en caché local durante 24 h y la última versión válida sirve como respaldo.
- **Fuente de respaldo:** un archivo `relays.json` empaquetado con la app, con los POP de LATAM y US East (Lima, Santiago, Buenos Aires, São Paulo, Virginia...).
- Por cada POP se sondean hasta 2 relays y se toma el mejor RTT.
- **Si un relay no responde ICMP**, se marca `icmp_bloqueado` y la interfaz lo muestra así. No se inventa el valor.

> ⚠️ **Lección de v3.0:** varias IP de la matriz no eran de Valve. Ninguna IP se escribe a mano en el código; todas salen del catálogo.

#### 6.1.6 Información de Wi-Fi (Windows)

- API nativa WLAN (`WlanQueryInterface`): SSID, RSSI o calidad de señal, banda (2,4, 5 o 6 GHz), canal y velocidad de enlace.
- Alimenta reglas del diagnóstico, por ejemplo "señal débil + jitter alto en el gateway → problema de Wi-Fi".
- El SSID solo se usa como clave de la línea base y se guarda como hash, no en claro.

#### 6.1.7 Bufferbloat

Procedimiento:

1. **Reposo (10 s):** RTT al gateway y a una referencia (relay activo o 1.1.1.1). Se registran la mediana y el p95.
2. **Carga de descarga (15 s):** 4 a 8 flujos HTTP paralelos con `reqwest` contra `https://speed.cloudflare.com/__down?bytes=N`. Se mide el RTT en paralelo y se registra el throughput.
3. **Carga de subida (15 s):** lo mismo, con POST de datos aleatorios a `https://speed.cloudflare.com/__up`.
   El endpoint es configurable. Si Cloudflare limita el uso, se cambia a un Worker propio (ver §16.3).
4. **Resultado:** el incremento de latencia es `p95_carga − mediana_reposo`, por separado para descarga y subida.

Calificación (escala aproximada a la de Waveform, configurable):

| Incremento p95 | Grado |
|---|---|
| < 5 ms | A+ |
| < 30 ms | A |
| < 60 ms | B |
| < 200 ms | C |
| < 400 ms | D |
| ≥ 400 ms | F |

- Se avisa antes de empezar que la prueba consume datos (se muestra una estimación en MB) y que satura la red durante unos 40 s.
- Se bloquea si hay una partida en curso (*readiness* en modo partida).
- La recomendación de SQM (CAKE / FQ_CoDel) **solo** aparece cuando el grado es B o peor.

### 6.2 `orion-engine` — Estadística y decisión

#### 6.2.1 Ventana deslizante por objetivo

- 60 muestras (1 minuto a 1 Hz), con `VecDeque`.
- Métricas: `last`, `min`, `median`, `p95`, `loss_pct` y `jitter`.

#### 6.2.2 Jitter (RFC 3550)

```
D = |RTT_i − RTT_{i−1}|
J = J + (D − J) / 16
```

El jitter se **deriva** de los RTT medidos. En v3.0 era un número aleatorio independiente.

#### 6.2.3 Línea base EWMA (sustituye al "motor AI" de v3.0)

Clave de la línea base: `(network_id, target_id, franja_horaria)`. La franja es de 4 horas (madrugada, mañana, tarde, noche).

Actualización por muestra (media y **varianza** EWMA correctas):

```
z      = (x − μ) / max(σ, σ_min)       # se calcula ANTES de actualizar
si z > 4 y la línea base ya convergió: no actualizar (outlier), solo marcar anomalía
δ      = x − μ
μ      = μ + α·δ
var    = (1 − α)·(var + α·δ²)
σ      = √var
```

- `α = 0.05` por defecto. `σ_min = 1 ms`, para no tener umbrales absurdos en redes muy estables.
- **Convergencia:** con al menos 300 muestras, la línea base se considera "aprendida". Antes de eso, la interfaz muestra "calibrando (n/300)" en lugar de un porcentaje de confianza inventado.
- **Umbral dinámico:** `μ + k·σ` con `k = 3` (configurable). La etiqueta de la interfaz muestra exactamente la `k` que usa el código.
- **Puntaje de anomalía:** `min(1, z / 6)`.
- Persistencia en SQLite (no en `localStorage`).
- En la interfaz se llama **"Línea base estadística"**, no "IA".

#### 6.2.4 Diagnóstico por segmento

Entrada: las métricas de la ventana de `gateway`, `isp-hop` y `relay activo`, más la información de Wi-Fi.

```
si gateway.loss > 1 % o gateway.jitter > 5 ms o gateway.p95 > 20 ms:
    → LOCAL   ("El problema está en tu red local / Wi-Fi")
si no, si isp.loss > 1 % o (isp.p95 − gateway.p95) > umbral_isp:
    → ISP     ("Tu red local está bien; la degradación empieza en tu proveedor")
si no, si relay.loss > 1 % o relay.p95 > baseline_relay.umbral:
    → RUTA    ("Tu ISP responde bien; la ruta hacia el servidor de Valve está degradada")
si no:
    → OK
```

- Cada veredicto lleva **evidencia**: las métricas que lo causaron, para mostrarlas en la interfaz y en el informe.
- Los umbrales son configurables y vienen con valores por defecto documentados en `metodologia.md`.

#### 6.2.5 Readiness (semáforo)

| Estado | Condición por defecto (relay activo, ventana de 60 s) |
|---|---|
| 🟢 LISTO | p95 ≤ umbral dinámico **y** loss < 1 % **y** jitter < 10 ms |
| 🟡 PRECAUCIÓN | p95 entre el umbral dinámico y +50 % **o** loss de 1 a 3 % **o** jitter de 10 a 20 ms |
| 🔴 NO BUSCAR | cualquier valor peor |
| ⚪ SIN DATOS | menos de 10 muestras válidas o el relay no responde |

**Histéresis** (corrige el spam de alertas de v3.0): se necesitan 3 evaluaciones seguidas para empeorar y 10 para mejorar. Las alertas sonoras se emiten **solo al cambiar de estado**, nunca en cada tick.

Si la línea base todavía no convergió, se usan umbrales absolutos: 80 ms para LISTO y 120 ms para PRECAUCIÓN.

#### 6.2.6 Motor de recomendaciones (sustituye a las "reglas auto-sintetizadas")

Las reglas son deterministas y cada una tiene evidencia y una acción sugerida. Ejemplos:

| Condición observada | Recomendación |
|---|---|
| Wi-Fi a 2,4 GHz + jitter del gateway > 5 ms | "Cámbiate a 5 GHz o usa cable" |
| RSSI < −70 dBm | "Señal Wi-Fi débil: acércate al router o usa cable" |
| Bufferbloat C o peor | "Activa SQM (CAKE/FQ_CoDel) en tu router o limita el ancho de banda de otros dispositivos" |
| Diagnóstico ISP sostenido > 5 min | "Problema en tu proveedor; exporta el informe para reclamar" |
| El relay de la región está degradado pero otro POP cercano está bien | "Prueba la región X (RTT medido: N ms)" |
| La línea base de esta franja es peor que la de otra franja | "Tu conexión suele estar mejor a las HH:MM" |

Las reglas se ejecutan, como mucho, una vez por minuto, y la interfaz muestra las que están activas.

### 6.3 `orion-actions` — Acciones del sistema

#### 6.3.1 Principios

- Toda acción usa `std::process::Command` con argumentos en **arreglo**, nunca una cadena interpretada por un shell.
- Cada acción devuelve `ActionResult { ok, exit_code, stdout, stderr, duration, verified }`.
- **Verificación posterior obligatoria:** tras la acción, se espera a que el gateway responda de nuevo y se mide un minuto de RTT. `verified = true` solo si hay conectividad.
- Cada acción tiene un **diálogo de confirmación** que dice qué se va a ejecutar, si necesita administrador y su impacto ("perderás la conexión unos 10 s").

#### 6.3.2 Catálogo y privilegios

| Acción | Implementación (Windows) | Privilegio esperado | Impacto |
|---|---|---|---|
| Purgar la caché DNS | `ipconfig /flushdns` | Usuario (confirmar en F0) | Ninguno |
| Renovar DHCP | `ipconfig /release <adaptador>` + `ipconfig /renew <adaptador>` | Confirmar en F0 | Corte de unos 5 s |
| Reiniciar el adaptador | `netsh interface set interface name="<nombre>" admin=disabled` y luego `enabled` | **Administrador** | Corte de unos 10 s |
| Cambiar el DNS del sistema (F2) | `netsh interface ip set dns ...` o la API WMI | **Administrador** | Ninguno |
| Reiniciar el router (F2) | Driver (ver 6.3.4) | Credenciales del dispositivo | Corte de 1 a 3 min |

El nombre del adaptador se obtiene de la API del sistema, nunca se escribe a mano (v3.0 tenía `'Wi-Fi'` fijo y mal entrecomillado).

#### 6.3.3 Elevación

- Windows: un ejecutable auxiliar mínimo, `orion-elevated.exe`, que acepta **solo** un conjunto cerrado de operaciones (enum serializado), se lanza con `ShellExecuteW` y el verbo `runas` (aparece el UAC) y termina al acabar. La app principal nunca corre como administrador.
- El auxiliar valida sus argumentos contra una lista blanca y rechaza cualquier otra cosa.
- Linux: `pkexec` con una política polkit específica.

#### 6.3.4 Drivers de router (fase 2)

```rust
#[async_trait::async_trait]
pub trait RouterDriver {
    fn name(&self) -> &str;
    async fn test_connection(&self) -> Result<()>;
    async fn reboot(&self) -> Result<()>;
}
```

- **Enchufe inteligente** (la opción más universal): Shelly (API HTTP local) y Tasmota (comando `Power`). Hace un ciclo de apagado, 10 s de espera y encendido.
- **Fabricantes:** se implementan bajo demanda (TP-Link, Huawei/ZTE de fibra GPON comunes en LATAM, MikroTik por API) y se documentan como experimentales.
- Las credenciales se guardan en el almacén del sistema (crate `keyring` → Windows Credential Manager), nunca en texto plano.
- Si no hay ningún driver configurado, el botón muestra instrucciones para reiniciar manualmente. **Nunca** simula un reinicio.

### 6.4 `orion-store` — Persistencia y exportación

#### 6.4.1 Esquema SQLite (`rusqlite` con la feature `bundled`)

```sql
CREATE TABLE networks   (id INTEGER PRIMARY KEY, ssid_hash TEXT, gateway_ip TEXT, kind TEXT, first_seen INTEGER);
CREATE TABLE targets    (id TEXT PRIMARY KEY, kind TEXT, addr TEXT, pop TEXT);
CREATE TABLE samples_1m (network_id INTEGER, target_id TEXT, ts_minute INTEGER,
                         n INTEGER, lost INTEGER, rtt_min REAL, rtt_med REAL, rtt_p95 REAL, jitter REAL,
                         PRIMARY KEY (network_id, target_id, ts_minute));
CREATE TABLE baselines  (network_id INTEGER, target_id TEXT, slot INTEGER,
                         mu REAL, var REAL, n INTEGER, updated INTEGER,
                         PRIMARY KEY (network_id, target_id, slot));
CREATE TABLE events     (ts INTEGER, level TEXT, kind TEXT, message TEXT, evidence_json TEXT);
CREATE TABLE bufferbloat_runs (ts INTEGER, network_id INTEGER, idle_med REAL,
                         down_p95 REAL, up_p95 REAL, down_mbps REAL, up_mbps REAL, grade TEXT);
CREATE TABLE actions_log (ts INTEGER, action TEXT, ok INTEGER, exit_code INTEGER, verified INTEGER, detail TEXT);
```

- Las muestras crudas solo viven en memoria. En disco se guardan **agregadas por minuto**.
- Retención: 30 días de `samples_1m` (configurable). Eventos y pruebas: 180 días.

#### 6.4.2 Informes

- Formatos: **JSON** (máquina), **CSV** (series) y **TXT/Markdown** (para enviar al soporte del ISP).
- Contenido: rango de fechas, red, objetivos, estadísticas reales, diagnósticos con su evidencia, pruebas de bufferbloat, acciones con su resultado y una sección de **metodología**.
- Ningún informe contiene un valor que no salga de la base de datos.

---

## 7. Contrato IPC (Tauri)

### 7.1 Comandos (frontend → núcleo)

| Comando | Parámetros | Respuesta |
|---|---|---|
| `get_status` | — | `StatusSnapshot` |
| `start_monitor` / `stop_monitor` | — | `()` |
| `set_region` | `pop: String` | `()` |
| `list_relays` | — | `Vec<RelayInfo>` (con RTT actual o `null`) |
| `run_bufferbloat` | `opts: BufferbloatOpts` | `BufferbloatResult` (con progreso por evento) |
| `run_action` | `action: ActionKind` | `ActionResult` |
| `reset_baseline` | `scope: BaselineScope` | `()` |
| `get_history` | `from, to, target?` | `Vec<MinuteSample>` |
| `export_report` | `format, from, to` | `PathBuf` (guardado mediante el diálogo del sistema) |
| `get_settings` / `set_settings` | `Settings` | `Settings` |
| `cli_exec` | `line: String` | `CliOutput` (parseado en Rust, con lista blanca de comandos) |

### 7.2 Eventos (núcleo → frontend)

| Evento | Payload | Frecuencia |
|---|---|---|
| `telemetry://sample` | `TelemetryTick` | 1 Hz |
| `diagnosis://update` | `Diagnosis` | Solo al cambiar |
| `readiness://change` | `{ from, to, reason }` | Solo al cambiar (dispara el sonido) |
| `bufferbloat://progress` | `{ phase, pct, live_rtt }` | 4 Hz durante la prueba |
| `network://changed` | `NetworkInfo` | Al cambiar de red |
| `log://event` | `LogEvent` | Por evento |

### 7.3 Tipos principales

```rust
#[derive(Serialize)]
pub struct TelemetryTick {
    pub ts: i64,
    pub targets: Vec<TargetStats>,     // gateway, isp-hop, relay activo
    pub readiness: Readiness,
    pub baseline: Option<BaselineView>,
}

#[derive(Serialize)]
pub struct TargetStats {
    pub id: String,
    pub label: String,
    pub last_ms: Option<f64>,
    pub median_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub loss_pct: f64,
    pub status: TargetStatus,          // Ok | Degraded | Down | IcmpBlocked | NoData
}

#[derive(Serialize)]
pub struct BaselineView {
    pub mu_ms: f64,
    pub sigma_ms: f64,
    pub k: f64,
    pub threshold_ms: f64,
    pub samples: u32,
    pub converged: bool,
    pub anomaly_score: f64,
}
```

Los tipos de TypeScript se generan desde Rust con `ts-rs` o `specta`, para que los dos lados no se desincronicen.

---

## 8. Frontend: migración desde v3.0

### 8.1 Qué se conserva

- Toda la identidad visual: paleta Radiant/Dire, tipografías, paneles HUD, scanlines y el canvas animado de fondo.
- El motor de audio Web Audio con los cinco presets.
- La estructura de pestañas, la consola, el registro de eventos y los toasts.

### 8.2 Cambios por pestaña

| Pestaña v3.0 | v4.0 | Cambios clave |
|---|---|---|
| [1] Telemetría y monitor RTT | **Telemetría** | Datos de `telemetry://sample`; tres series reales (gateway, ISP, relay). El eje del gráfico se basa en el tiempo real. Se elimina el "Inspector de tramas UDP" (datos ficticios: 64 Hz, buffer, DSCP). |
| — | **Diagnóstico** (nueva) | Vista de segmentos `PC → Router → ISP → Valve` con el estado de cada tramo y su evidencia. Es la pantalla central del producto. |
| [2] Dashboard de optimización | **Acciones** | Se eliminan los selectores MTU/QoS sin efecto. Se muestran las acciones reales con su privilegio y su resultado. Las recomendaciones salen del motor de reglas. |
| [3] Motor autónomo self-learning | **Línea base** | Se renombra. Muestra μ, σ, k, el umbral y las muestras/convergencia reales, más las recomendaciones activas. Se eliminan la "confianza 98,4 %" y las reglas fijas. |
| [4] Control gateway y router | (se fusiona en **Acciones**) | Reinicio del router solo si hay un driver configurado. El gateway se detecta solo, ya no es un selector manual. |
| [5] Bufferbloat | **Bufferbloat** | Prueba real con progreso en vivo, throughput y grado calculado. Historial de pruebas. |
| — | **Historial** (nueva) | Gráfico por día/hora, comparación entre franjas y botón de exportar. |

### 8.3 Correcciones obligatorias de la interfaz

- Canvas de fondo: al usar un wallpaper propio, **ocultar y limpiar** el canvas (en v3.0 lo tapaba). Usar un solo loop de `requestAnimationFrame` con bandera de cancelación.
- Tailwind compilado localmente (no el CDN). Chart.js con versión fija y empaquetado. CSP estricta en `tauri.conf.json`.
- Toda inserción de texto dinámico con `textContent`, nunca `innerHTML` con datos.
- `AudioContext.resume()` tras el primer gesto del usuario.
- Modo partida: el canvas se detiene, las partículas se apagan y solo suenan los cambios a 🔴.
- Estados vacíos explícitos: "Calibrando", "Sin datos" e "ICMP bloqueado por el relay".

---

## 9. Seguridad

| Riesgo (visto en v3.0 o potencial) | Mitigación en v4.0 |
|---|---|
| Endpoint HTTP local sin autenticación y con CORS `*` (CSRF desde cualquier web) | No hay servidor HTTP. Solo IPC de Tauri. |
| `shell=True` / inyección de comandos | `Command` con argumentos en arreglo; el nombre del adaptador se valida contra la lista del sistema. |
| La app completa corriendo como administrador | Auxiliar elevado de un solo uso, con una lista blanca de operaciones. |
| Credenciales del router en texto plano | `keyring` (Windows Credential Manager). |
| XSS en la WebView | CSP estricta, `textContent`, sin recursos remotos. |
| Frontend comprometido que invoca comandos peligrosos | Capabilities de Tauri 2 con permisos mínimos y confirmación nativa (no HTML) para las acciones con impacto. |
| Binario sin firmar (SmartScreen o el antivirus lo marcan) | Firma de código (Authenticode) antes de distribuir. |
| Fuga de datos de red | Sin telemetría externa. El SSID se guarda como hash. Los informes los exporta el usuario de forma explícita. |

---

## 10. Pruebas

### 10.1 Unitarias (`orion-engine`, sin red)

- Jitter según RFC 3550 contra vectores conocidos.
- La EWMA converge a la media y la varianza de una serie sintética. El *gating* de outliers no contamina la línea base.
- Histéresis del *readiness*: una secuencia de muestras produce exactamente la secuencia de estados esperada.
- Diagnóstico: una tabla de casos (gateway malo / ISP malo / relay malo / todo bien) con su veredicto y su evidencia.

### 10.2 Integración con *mocks*

- `MockProber` con guiones (latencia, pérdida, spikes) para probar el flujo completo, eventos incluidos, de forma determinista. Esto reemplaza el botón "Provocar spike" de v3.0 por un modo demo **explícitamente etiquetado**, disponible solo en compilaciones de desarrollo.

### 10.3 Pruebas en el sistema real

- CI en `windows-latest`: compilación, pruebas unitarias e ICMP contra 127.0.0.1 y el gateway del runner.
- Matriz manual antes de cada versión:

| Escenario | Resultado esperado |
|---|---|
| Cable, red en reposo | 🟢, diagnóstico OK |
| Wi-Fi 2,4 GHz lejos del router | Diagnóstico LOCAL, recomendación Wi-Fi |
| Descarga pesada en otro equipo | Bufferbloat degradado, 🟡/🔴, diagnóstico LOCAL o ISP |
| Desconectar el cable del ISP del router | Gateway OK, ISP caído → diagnóstico ISP |
| Cambio de red Wi-Fi | `network://changed`, línea base distinta |
| Acción "reiniciar adaptador" sin aceptar el UAC | Resultado `ok=false` con el mensaje claro |

### 10.4 Criterios de rendimiento

- CPU < 1 % en monitoreo (medido en una laptop de gama media).
- RAM < 150 MB, incluida la WebView.
- Ejecutable instalado < 15 MB.

---

## 11. Roadmap

| Fase | Entregable | Criterio de aceptación |
|---|---|---|
| **F0 — Validación técnica** (1 semana) | Spike en `orion-cli`: ICMP en Windows sin administrador, gateway, primer hop público, catálogo de relays SDR | Lista real de relays LATAM con RTT medido. Se documenta qué relays responden ICMP. Se confirman los privilegios de `flushdns` y `release/renew`. |
| **F1 — Núcleo de medición** | `orion-probe` + `orion-engine` + pruebas | `orion-cli monitor` imprime en consola las métricas y el diagnóstico en vivo. Las pruebas unitarias pasan. |
| **F2 — App Tauri MVP** | UI migrada: Telemetría, Diagnóstico y Línea base | Semáforo con histéresis funcionando. Ningún valor simulado en la UI. |
| **F3 — Bufferbloat + Historial** | Prueba real, SQLite, exportación | Informe exportado con la sección de metodología. Grado reproducible (±1 grado entre corridas). |
| **F4 — Acciones** | DNS, DHCP, adaptador, auxiliar elevado | Cada acción con confirmación, resultado real y verificación posterior. |
| **F5 — Distribución** | Instalador firmado, autoactualización (plugin updater de Tauri) | Se instala sin alertas de SmartScreen en una máquina limpia. |
| **F6 — Extensiones** | Drivers de router, overlay, cambio de DNS, Linux | Según demanda. |

---

## 12. Riesgos y preguntas abiertas

| # | Riesgo o pregunta | Impacto | Plan |
|---|---|---|---|
| R1 | Los relays SDR pueden no responder a ICMP o filtrarlo | Alto: es la métrica estrella | F0 lo mide. Si muchos lo bloquean: usar el último hop que responda en la ruta hacia el relay como aproximación, etiquetado como tal. Investigar si existe un protocolo de ping documentado. |
| R2 | `GetSDRConfig` no está documentado oficialmente: puede cambiar o desaparecer | Medio | Caché local + `relays.json` empaquetado + aviso en la UI si el catálogo está desactualizado. |
| R2b | Los endpoints de Cloudflare `__down`/`__up` son públicos, pero no son una API con garantías para terceros | Bajo | URL configurable + Worker propio como respaldo (§16.3). |
| R3 | Los privilegios reales de `ipconfig` varían según la versión de Windows | Bajo | Confirmar en F0. Si piden administrador, pasan al auxiliar elevado. |
| R4 | El antivirus marca el sondeo ICMP o el auxiliar elevado | Medio | Firma de código, lista blanca mínima y la API oficial de Windows (no sockets raw). |
| R5 | La prueba de bufferbloat consume datos en conexiones medidas | Bajo | Aviso + estimación en MB + duración configurable. |
| R6 | **Marca registrada:** "Dota" y sus elementos visuales son de Valve | Medio si se distribuye | Pregunta abierta: ¿nombre de producto sin "Dota" (p. ej. "ORION NetControl — para Dota 2")? No usar logos ni assets oficiales; la estética actual es original. |
| R7 | Interpretación de los usuarios ("el programa me baja el ping") | Medio | Mensajes claros: la herramienta **diagnostica** y sugiere; no acelera la conexión. |
| P1 | ¿Distribución gratuita, *freemium* o parte de la oferta de iaciberseguridad? | — | Define si hacen falta licencias, cuentas o actualizaciones de pago. |
| P2 | ¿La CLI (`orion-cli`) se publica también como herramienta independiente? | — | Útil para usuarios avanzados y soporte técnico. |

---

## 13. Mapa v3.0 → v4.0 (resumen)

| Elemento de v3.0 | Destino |
|---|---|
| RTT, jitter y loss con `Math.random()` | → Medición ICMP real + jitter RFC 3550 + pérdida en ventana |
| IP de relays fijas (algunas no eran de Valve) | → Catálogo SDR con caché |
| "Optimización" MTU/DNS/QoS sin efecto | → Eliminada; sustituida por acciones reales y recomendaciones con evidencia |
| Antes/después fijo (34→21 ms) | → Comparación real entre periodos desde el historial |
| Bufferbloat que siempre da A+ | → Prueba real con saturación |
| "Motor AI" con confianza fija y reglas fijas | → Línea base EWMA (media + varianza), z-score previo, gating de outliers, reglas deterministas |
| Alertas en cada tick | → Histéresis + sonido solo al cambiar de estado |
| Backend FastAPI con `stroke=True`, CORS `*` y sin autenticación | → IPC Tauri + auxiliar elevado con lista blanca |
| "Reinicio de router" que solo animaba | → Drivers reales o instrucciones manuales; jamás una simulación |
| `localStorage` | → SQLite con agregación y retención |
| Inspector UDP (64 Hz, DSCP, buffer) | → Eliminado: datos ficticios e incorrectos (Dota corre a 30 Hz) |
| Estética HUD, audio y consola | → Se conservan |

---

## 14. Dependencias previstas (fijar versiones al iniciar F0)

| Crate | Uso |
|---|---|
| `tokio` | Runtime asíncrono, intervalos y tareas |
| `tauri` (v2) + `tauri-plugin-dialog` / `-updater` | Aplicación de escritorio, IPC, diálogos y actualización |
| `windows` | IP Helper (ICMP, rutas), WLAN API, `ShellExecuteW` |
| `surge-ping` | ICMP en Linux |
| `netdev` | Interfaz y gateway por defecto, multiplataforma |
| `reqwest` | Catálogo SDR, flujos de bufferbloat |
| `serde` / `serde_json` | Serialización IPC e informes |
| `rusqlite` (`bundled`) | Persistencia |
| `keyring` | Credenciales de los drivers de router |
| `ts-rs` o `specta` | Generación de tipos TypeScript |
| `tracing` + `tracing-subscriber` | Logs estructurados |
| `thiserror` / `anyhow` | Manejo de errores |
| `async-trait` | Traits asíncronos (`Prober`, `RouterDriver`) |

---

## 15. Tipo de aplicación: ¿Docker, Electron o web?

### 15.1 Decisión: app de escritorio nativa con Tauri 2

ORION v4.0 es una **aplicación de escritorio instalable**, como las que se hacen con Electron (Discord, VS Code). Se distribuye como un instalador `.msi` o `.exe` para Windows. El usuario la instala, la abre y funciona: no necesita Docker, Python, Node ni nada más.

Se elige **Tauri en lugar de Electron** por estas razones:

| Criterio | Tauri 2 | Electron |
|---|---|---|
| Tamaño del instalador | ~5-15 MB (usa el WebView2 que ya trae Windows) | ~80-150 MB (incluye Chromium completo) |
| RAM en reposo | ~50-150 MB | ~200-400 MB |
| Lenguaje del backend | Rust (el núcleo de medición vive en el mismo proceso) | Node.js (las mediciones ICMP precisas necesitarían módulos nativos en C++/Rust de todos modos) |
| Seguridad | *Capabilities* por comando; sin acceso al sistema desde el frontend por defecto | El frontend puede tener acceso a Node si se configura mal |
| Reutilizar el HTML de v3.0 | Sí | Sí |

Para un monitor que se deja abierto mientras se juega, consumir poca RAM y CPU es un requisito, no un lujo. Esa es la razón principal.

### 15.2 Por qué **no** Docker

Docker es para empaquetar servicios de servidor, no aplicaciones de escritorio que diagnostican la red del propio equipo. En este caso sería contraproducente:

1. **Mediría la red equivocada.** Un contenedor sale a internet a través de la red virtual NAT de Docker, no por el adaptador real. El "gateway" que vería es el de Docker (p. ej. `172.17.0.1`), no tu router.
2. **No ve el Wi-Fi.** Desde un contenedor no se puede leer el RSSI, la banda ni el canal del adaptador del equipo.
3. **No puede actuar sobre el sistema.** Purgar el DNS, renovar el DHCP o reiniciar el adaptador de Windows es imposible desde un contenedor Linux.
4. **En Windows, Docker Desktop añade una VM (WSL2/Hyper-V).** Eso suma otra capa de red y de latencia justo en lo que queremos medir, y consume varios GB de RAM.
5. **Fricción para el usuario.** Un jugador no va a instalar Docker Desktop para ver su ping.

**Dónde sí podría usarse Docker (opcional, no necesario):** para compilar la versión Linux en CI de forma reproducible, o para alojar un servidor propio de prueba de velocidad (§16.3). GitHub Actions ya ofrece runners de Windows y Linux, así que en la práctica no hace falta.

### 15.3 Por qué no una página web pura

Un navegador no puede enviar ICMP, ni leer la tabla de rutas, ni ver el Wi-Fi, ni ejecutar acciones del sistema. Una web solo podría medir latencia HTTP hacia servidores con CORS, que no son los relays de Valve. Ese límite fue justamente el que llevó a v3.0 a simular los datos.

### 15.4 Qué se instala para **desarrollar** (solo tú, no el usuario final)

| Herramienta | Para qué | Dónde |
|---|---|---|
| Rust (vía `rustup`, toolchain `stable-x86_64-pc-windows-msvc`) | Compilar el núcleo | rustup.rs |
| Microsoft C++ Build Tools ("Desarrollo de escritorio con C++") | Enlazador MSVC que necesita Rust en Windows | visualstudio.microsoft.com |
| WebView2 Runtime | Motor web de Tauri (Windows 11 ya lo trae) | Microsoft |
| Node.js LTS + npm/pnpm | Compilar el frontend (Vite, Tailwind) | nodejs.org |
| Tauri CLI | `cargo install tauri-cli` o `npm create tauri-app@latest` | v2.tauri.app |
| VS Code + `rust-analyzer` | Editor | — |

Comandos clave:

```bash
cargo tauri dev      # app en modo desarrollo con recarga en caliente
cargo tauri build    # genera el instalador .msi / .exe en src-tauri/target/release/bundle/
cargo test --workspace
```

### 15.5 Distribución al usuario final

- **Instalador:** `.msi` (WiX) o `.exe` (NSIS), ambos los genera `cargo tauri build`.
- **Firma de código:** un certificado Authenticode evita la alerta de SmartScreen ("Windows protegió su PC"). Es el **único costo** previsto del proyecto: un certificado de firma de código es de pago y se renueva cada año. Para el MVP y las pruebas internas se puede distribuir sin firmar.
- **Actualizaciones:** plugin `updater` de Tauri. Los instaladores van firmados con una clave propia (gratis, generada con `cargo tauri signer generate`) y se publican en GitHub Releases.
- **Canal:** GitHub Releases (gratis) o la propia web del proyecto.

---

## 16. APIs y servicios externos

### 16.1 Resumen: no necesitas comprar ni registrar ninguna API

Todo lo que necesita el MVP es **gratuito y no requiere API key**. Casi todo son APIs del propio sistema operativo, a las que se accede desde Rust con el crate `windows`.

| # | API / servicio | Para qué | ¿Key? | Costo | Dónde está la documentación |
|---|---|---|---|---|---|
| 1 | **IP Helper API** de Windows (`IcmpSendEcho2`, `GetBestRoute2`, `GetAdaptersAddresses`) | Ping ICMP sin administrador, gateway, lista de adaptadores | No | Gratis (parte de Windows) | Microsoft Learn → "IP Helper" |
| 2 | **Native Wifi API** de Windows (`WlanOpenHandle`, `WlanQueryInterface`) | RSSI, banda, canal, SSID | No | Gratis | Microsoft Learn → "Native Wifi" |
| 3 | **`ShellExecuteW`** con verbo `runas` | Pedir elevación (UAC) para el auxiliar | No | Gratis | Microsoft Learn → "ShellExecuteW" |
| 4 | **Credential Manager** de Windows (vía crate `keyring`) | Guardar credenciales de router/enchufe | No | Gratis | docs.rs → `keyring` |
| 5 | **Steam SDR Config**: `https://api.steampowered.com/ISteamApps/GetSDRConfig/v1/?appid=570` | Lista de POP y relays de Valve con sus IP | No | Gratis | No documentada oficialmente (ver 16.2) |
| 6 | **Cloudflare speed endpoints**: `speed.cloudflare.com/__down?bytes=N` y `/__up` | Generar carga para la prueba de bufferbloat | No | Gratis | Repositorio `cloudflare/speedtest` en GitHub |
| 7 | **Tauri 2** (framework) | App de escritorio, IPC, instalador, updater | No | Gratis (MIT/Apache-2.0) | v2.tauri.app |
| 8 | **Shelly** (API HTTP local) — fase 2 | Reinicio del router con enchufe inteligente | No (usuario/clave del dispositivo, opcional) | Gratis (hay que tener el enchufe) | shelly-api-docs.shelly.cloud |
| 9 | **Tasmota** (comando `Power` por HTTP) — fase 2 | Ídem, para enchufes con firmware Tasmota | No | Gratis | tasmota.github.io/docs/Commands |

**Lo que NO necesitas:**

- **Steam Web API key:** `GetSDRConfig` no la pide. Las APIs de Steam que sí piden key de publicador son para desarrolladores de juegos y no aplican aquí.
- **APIs de IA** (OpenAI, Gemini, etc.): el motor de línea base es estadístico y corre localmente.
- **Servidor propio o backend en la nube:** todo corre en el equipo del usuario.
- **Base de datos externa:** SQLite va embebido en la app.

### 16.2 Steam SDR Config (relays de Valve)

- **Qué es:** la misma configuración que usa el cliente de Steam para conocer los puntos de presencia (POP) de Steam Datagram Relay. Para cada POP incluye un código (p. ej. `lim`, `scl`, `gru`, `iad`) y la lista de relays con su IPv4.
- **Cómo se usa:** un `GET` sin autenticación al iniciar la app → se parsea con `serde` → se guarda en caché 24 h en SQLite.
- **Riesgo:** no aparece en la documentación oficial del Web API de Steam. Funciona y la usa la comunidad (herramientas de selección de servidores de CS/TF2), pero Valve podría cambiarla sin aviso. Por eso existe el respaldo `relays.json` empaquetado (§6.1.5).
- **Tarea de F0:** descargarla para el appid 570, verificar qué POP de LATAM trae y probar cuáles de sus relays responden a ICMP.

Esquema mínimo esperado (a confirmar en F0):

```rust
#[derive(Deserialize)]
struct SdrConfig {
    revision: u64,
    pops: HashMap<String, Pop>,          // clave: código del POP ("lim", "scl", ...)
}

#[derive(Deserialize)]
struct Pop {
    desc: Option<String>,                // "Lima (Peru)"
    geo: Option<[f64; 2]>,
    relays: Option<Vec<Relay>>,          // algunos POP no tienen relays públicos
}

#[derive(Deserialize)]
struct Relay {
    ipv4: String,
    port_range: Option<[u16; 2]>,
}
```

Todos los campos van como `Option` para que un cambio de formato no tumbe la app; si el parseo falla, se usa el respaldo.

### 16.3 Endpoints de prueba de velocidad (bufferbloat)

- **Primario:** los endpoints públicos que usa `speed.cloudflare.com`. `GET /__down?bytes=N` devuelve N bytes, y `POST /__up` recibe y descarta los datos. No piden key.
- **Consideración:** son públicos, pero no son una API con SLA para terceros. Hay que usarlos con moderación: una prueba de unos 40 s cada vez que el usuario la lanza, nunca de forma automática y continua.
- **Respaldo propio (opcional):** Cloudflare publica la plantilla `worker-speedtest-template`, que implementa `GET /down?bytes=N` y `POST /up` en un Cloudflare Worker. Se despliega en una cuenta gratuita de Cloudflare y su URL se configura en la app. Así el proyecto no depende de un endpoint de terceros.
- **Implementación:** la URL base va en `Settings` (`bufferbloat.endpoint`), con el valor de Cloudflare por defecto.

### 16.4 Política de uso de red de la app

| Tráfico | Destino | Frecuencia | Volumen |
|---|---|---|---|
| ICMP de monitoreo | Gateway, salto del ISP, 1-2 relays por POP | 1 Hz | < 2 KB/s |
| Traceroute ligero | Relay activo | Al arrancar, al cambiar de red y cada 10 min | Despreciable |
| Catálogo SDR | `api.steampowered.com` | 1 vez cada 24 h | < 100 KB |
| Bufferbloat | Endpoint configurado | Solo cuando el usuario lo lanza | Decenas a cientos de MB, según la velocidad de la conexión (se estima antes) |
| Actualizaciones | GitHub Releases | Al arrancar (configurable) | < 1 KB (manifiesto); el instalador solo si el usuario acepta |

Ningún otro tráfico sale del equipo.

### 16.5 Tareas de F0 relacionadas con APIs

- [ ] Descargar `GetSDRConfig` para el appid 570 y guardar una muestra en `docs/samples/sdr_config.json`.
- [ ] Listar los POP de LATAM y US East, y medir qué relays responden a ICMP desde Colombia.
- [ ] Prototipo de `IcmpSendEcho2` en Rust sin administrador, comparado contra `ping.exe` como referencia.
- [ ] Prototipo de `WlanQueryInterface` para leer el RSSI y la banda.
- [ ] Prueba de carga contra `speed.cloudflare.com/__down` y `/__up`, midiendo el RTT al gateway en paralelo.
- [ ] (Opcional) Desplegar `worker-speedtest-template` en una cuenta gratuita de Cloudflare como respaldo.
