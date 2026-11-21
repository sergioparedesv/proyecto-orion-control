# F0 — Resultados de la validación técnica

Estado: **parcial**. Lo marcado ✅ ya se verificó; lo marcado ⏳ hay que correrlo en Windows con `orion-cli` y pegar aquí la salida.

## 1. Catálogo SDR (`GetSDRConfig`, appid 570) ✅

- Descargado el 2026-11-20 → `docs/samples/sdr_config.json` (revisión `1794268433`, 70 POP).
- El esquema coincide con el §16.2 del blueprint. Campos extra presentes: `certs`, `p2p_share_ip`, `relay_public_key`, `revoked_keys`, `typical_pings`, `success`, y por POP `partners`, `tier`, `aliases`. Se ignoran.
- Muchos POP de partners (`m*1`, `ams4`, `dfwm`, `pvg`, ...) **no traen relays**: el parser los descarta.
- `geo` es `[lon, lat]` en casi todos los POP, pero hay excepciones (`dfra` trae `[lat, lon]`). No se usa para nada crítico.
- POP de LATAM y EE. UU. con relays: `lim` (4), `scl` (2), `eze` (4), `gru` (6), `iad` (4), `atl` (4), `dfw` (4), `ord` (4), `lax` (4), `sea` (2). **No hay POP en Bogotá ni en México.**
- `crates/orion-probe/assets/relays.json` se generó desde esa respuesta (no a mano).

## 2. ¿Responden ICMP los relays? ⏳ (riesgo R1)

```powershell
cargo run -p orion-cli -- spike sdr -n 5
```

Pegar aquí la tabla Markdown que imprime. Si la mayoría de los relays no responde, activar el plan B del R1 (último hop que responda en la ruta, etiquetado como aproximado).

## 3. ICMP sin administrador en Windows ⏳

```powershell
cargo run -p orion-cli -- spike icmp 1.1.1.1 -n 20
ping -n 20 1.1.1.1
```

Criterio: misma pérdida y mediana dentro de ±2 ms respecto de `ping.exe`.

## 4. Gateway y primer hop público ⏳

```powershell
cargo run -p orion-cli -- spike gateway      # comparar con: ipconfig
cargo run -p orion-cli -- spike hops 155.133.244.35
tracert -d -h 12 155.133.244.35
```

Anotar: ¿hay CGNAT (100.64.0.0/10)? ¿qué TTL tiene el primer hop público? ¿responde ICMP?

## 5. Wi-Fi ⏳

```powershell
cargo run -p orion-cli -- spike wifi        # comparar con: netsh wlan show interfaces
```

Validar RSSI, canal y velocidad. Pendiente: distinguir 6 GHz (el canal solo no basta).

## 6. Privilegios de `ipconfig` ⏳ (para F4)

Desde una consola **sin** administrador: `ipconfig /flushdns`, `ipconfig /release` y `ipconfig /renew`. Anotar cuáles piden elevación.

## Notas para el blueprint

- §6.2.3 dice "franja de 4 horas (madrugada, mañana, tarde, noche)": son 6 franjas de 4 h, no 4. Se implementaron 6 (`baseline::slot_for_hour`).
- §6.1.3: en F1 el gateway se obtiene con `netdev`; `GetBestRoute2` queda como alternativa si `netdev` falla en algún equipo.
- §6.1.2: se usa `IcmpSendEcho` (síncrono) dentro de `spawn_blocking`, equivalente a `IcmpSendEcho2` sin evento. El RTT se mide con `Instant` (µs); el `RoundTripTime` de Windows (1 ms de resolución) se usa solo como control.
