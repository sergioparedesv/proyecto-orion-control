# Metodología de medición

Este documento se incluirá en los informes exportados (F3). Describe exactamente cómo se obtiene cada número que muestra ORION.

## Sondeo

- **Protocolo:** eco ICMP de 32 bytes. En Windows, `IcmpSendEcho` de la IP Helper API (sin administrador). En Linux, socket ICMP DGRAM.
- **Frecuencia:** 1 eco por segundo a cada objetivo; timeout de 900 ms. Sin respuesta dentro del timeout = **pérdida**.
- **RTT:** tiempo entre el envío y la respuesta, medido con un reloj monotónico (resolución de microsegundos).
- **Objetivos:**
  - **Router:** el gateway IPv4 de la interfaz con ruta por defecto.
  - **ISP:** el primer salto con IP pública (excluye RFC1918, CGNAT 100.64.0.0/10, link-local) en un traceroute de 8 saltos hacia el relay activo. Si el salto anterior no respondía, se marca *aproximado*. Se recalcula al iniciar, al cambiar de red y cada 10 min.
  - **Relays de Valve:** IPs del catálogo público Steam Datagram Relay (`GetSDRConfig`, appid 570). Por POP se eligen los 2 relays que mejor responden; el POP activo se sondea con 2 relays (se toma el mejor RTT de cada segundo) y los demás con 1. Si ningún relay del POP responde ICMP, se marca **ICMP bloqueado** y no se inventa ningún valor.

## Métricas (ventana de 60 muestras)

| Métrica | Definición |
|---|---|
| último | RTT de la muestra más reciente (vacío si se perdió) |
| mediana, p95 | percentil por rango más cercano sobre las muestras válidas |
| pérdida | muestras perdidas / muestras totales de la ventana |
| jitter | RFC 3550: `J ← J + (|RTTᵢ − RTTᵢ₋₁| − J) / 16`, sobre muestras válidas consecutivas |

## Línea base estadística

- Clave: (red, objetivo, franja de 4 h). La red se identifica por el hash de IP y MAC del gateway más el hash del SSID; el SSID nunca se guarda en claro.
- Por cada muestra válida `x`: `z = (x − μ) / max(σ, 1 ms)` se calcula **antes** de actualizar; luego `δ = x − μ`, `μ ← μ + α·δ`, `var ← (1 − α)(var + α·δ²)` con `α = 0,05`.
- Se considera aprendida con 300 muestras. Desde entonces, las muestras con `z > 4` no actualizan la línea base (se registran como anomalía).
- Umbral dinámico: `μ + k·σ` (k = 3 por defecto, configurable). Puntaje de anomalía: `min(1, z/6)`.

## Diagnóstico por segmento (en orden)

1. **LAN:** pérdida al router > 1 %, o jitter al router > 5 ms, o p95 al router > 20 ms.
2. **ISP:** pérdida al hop del ISP > 1 %, o p95(ISP) − p95(router) > 30 ms.
3. **Ruta:** pérdida al relay > 1 %, o p95 del relay > umbral dinámico (80 ms mientras calibra).
4. Si nada de lo anterior: **OK**. Con menos de 10 muestras en todos los tramos: **sin datos**.

## Semáforo

| Estado | Condición sobre el relay activo |
|---|---|
| LISTO | p95 ≤ umbral, pérdida < 1 %, jitter < 10 ms |
| PRECAUCIÓN | p95 ≤ 1,5 × umbral, pérdida ≤ 3 %, jitter ≤ 20 ms |
| NO BUSCAR | cualquier valor peor |
| SIN DATOS | menos de 10 muestras válidas |

El umbral es el dinámico si la línea base convergió; si no, 80 ms (LISTO) y 120 ms (PRECAUCIÓN). Histéresis: 3 evaluaciones seguidas para empeorar y 10 para mejorar.

## Lo que ORION no hace

No lee memoria ni archivos del juego, no cambia opciones de Dota ni de Steam y no "acelera" la conexión. Diagnostica y sugiere.
