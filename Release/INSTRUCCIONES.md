# ORION NetControl v0.4.0 — Instrucciones

ORION NetControl mide tu conexión hacia tu router, tu proveedor de internet (ISP) y los servidores relay de Valve. Se diseñó para Dota 2, pero también sirve para otros juegos (ver [¿Sirve para otros juegos?](#sirve-para-otros-juegos)). Te dice si conviene buscar partida y dónde está el problema cuando la conexión va mal.

- Solo **mide**: no acelera la conexión y no toca el juego (ni memoria, ni archivos, ni opciones de lanzamiento).
- No necesita cuentas, claves ni internet aparte del que ya usas. Los datos se quedan en tu equipo.
- Sistema: **Windows 10 u 11 (64 bits)**.

## Contenido de esta carpeta

| Archivo | Para qué sirve |
|---|---|
| `INSTRUCCIONES.md` | Este documento |
| `orion-netcontrol-v0.4.0-fuente.zip` | Código fuente mínimo para compilar la app |
| `construir.ps1` | Script que compila la app y deja el instalador en `instaladores\` |
| `instaladores\` | Aparece después de compilar. Contiene el `.msi`, el `.exe` de instalación, un `.exe` portable y `orion-cli.exe` |

---

## Opción A — Ya tienes la carpeta `instaladores\`

1. Abre `instaladores\` y ejecuta **`ORION NetControl_0.4.0_x64-setup.exe`** (o el `.msi`).
2. Si Windows muestra "Windows protegió su PC", pulsa **Más información → Ejecutar de todas formas**. Aparece porque el instalador todavía no está firmado.
3. Abre **ORION NetControl** desde el menú Inicio.

Para usarla sin instalar, ejecuta `orion-netcontrol-portable.exe`.

> La app usa **Microsoft Edge WebView2**, que ya viene con Windows 11 y con Windows 10 actualizado. Si no se abre ninguna ventana, instálalo desde <https://developer.microsoft.com/microsoft-edge/webview2/> ("Evergreen Bootstrapper").

## Opción B — Compilar la app (la primera vez)

Esto se hace **una sola vez** en un equipo con Windows. Después puedes copiar la carpeta `instaladores\` a cualquier otro equipo y seguir la opción A.

### 1. Instalar los requisitos (unos 20-30 minutos, ~6 GB de disco)

1. **Visual Studio Build Tools**: <https://visualstudio.microsoft.com/es/visual-cpp-build-tools/>. En el instalador marca **"Desarrollo para el escritorio con C++"** y pulsa Instalar.
2. **Rust**: descarga y ejecuta `rustup-init.exe` desde <https://rustup.rs>. Acepta la opción por defecto (`1`).
3. **Node.js LTS**: <https://nodejs.org> (instalador `.msi`, opciones por defecto).
4. **Cierra y vuelve a abrir** PowerShell para que reconozca los programas nuevos.

Para comprobarlo, cada uno de estos comandos debe mostrar una versión:

```powershell
cargo --version
node --version
```

### 2. Compilar

1. Copia la carpeta `Release` a una ruta **corta y sin espacios raros**, por ejemplo `C:\orion\Release`.
2. Abre PowerShell en esa carpeta (en el Explorador: clic derecho en un espacio vacío → **Abrir en Terminal**).
3. Ejecuta:

```powershell
powershell -ExecutionPolicy Bypass -File .\construir.ps1
```

El script:
- comprueba los requisitos e instala Tauri CLI si falta;
- descomprime el código;
- instala las dependencias del frontend;
- compila la app.

La primera vez tarda entre 10 y 20 minutos y necesita internet para bajar las dependencias. Al terminar, los instaladores quedan en `Release\instaladores\`.

Opciones del script:

| Comando | Qué hace |
|---|---|
| `.\construir.ps1 -Test` | Corre además las pruebas automáticas antes de compilar |
| `.\construir.ps1 -Dev` | Abre la app en modo desarrollo, sin generar instalador |

---

## Uso rápido

1. Al abrir la app, el monitoreo arranca solo.
2. Arriba a la izquierda, elige tu **región** (Lima, Santiago, Buenos Aires, São Paulo, Virginia...): la misma que usas en Dota.
3. Mira el **semáforo** de la parte superior:
   - 🟢 **LISTO**: puedes buscar partida.
   - 🟡 **PRECAUCIÓN**: la conexión está inestable.
   - 🔴 **NO BUSCAR**: la conexión está degradada.
   - ⚪ **SIN DATOS**: todavía está midiendo (unos 10 segundos al inicio).
4. La pestaña **Diagnóstico** muestra el camino `Tu PC → Router → ISP → Valve` y en qué tramo está el problema, con recomendaciones.
5. La **línea base** aprende cómo es normalmente tu conexión. Tarda unos 5 minutos en calibrarse para cada red y cada franja horaria. Mientras calibra, el semáforo usa límites fijos (80 / 120 ms).
6. El botón **PARTIDA** detiene las animaciones y solo suena cuando el semáforo pasa a 🔴.

Las pestañas **Bufferbloat**, **Acciones** e **Historial** aparecen como "Próximamente": son de una versión posterior y no muestran datos inventados.

## ¿Sirve para otros juegos?

La app mide tres tramos: **tu PC → router**, **router → ISP** y **ISP → relays de Valve** (la red Steam Datagram Relay). Los dos primeros afectan a cualquier juego online. El tercero solo corresponde a los juegos que pasan por la red de relays de Valve.

### Compatibilidad completa: juegos que usan los relays de Valve

Los tres tramos y el semáforo valen tal cual. Elige en la app la misma región que usas en el juego.

| Juego | Nota |
|---|---|
| **Dota 2** | Para el que se diseñó la app |
| **Counter-Strike 2** | El matchmaking oficial va por los mismos relays de Valve |
| **Deadlock** | Igual que CS2 |
| **Team Fortress 2** | Solo en los servidores oficiales de Valve (casual/competitivo). Los servidores comunitarios no usan relays |

### Compatibilidad parcial: cualquier otro juego online

Por ejemplo **Valorant, League of Legends, Fortnite, Apex Legends, Rocket League, Overwatch 2, Call of Duty o Free Fire en emulador**.

- **Sí sirve** para saber si el problema está **en tu casa** (Wi-Fi débil, banda de 2,4 GHz, router saturado) o **en tu proveedor (ISP)**. Esa es la causa más habitual del lag.
- **No mide** los servidores de esos juegos: el tramo "Valve" y su latencia no corresponden a los servidores de Riot, Epic, etc. Usa el panel de red del propio juego para ver tu ping real.
- El semáforo usa la latencia hacia Valve, así que tómalo solo como referencia. Para estos juegos, fíjate sobre todo en los tramos **Router** e **ISP** de la pestaña **Diagnóstico**.

## Línea de comandos (opcional, para soporte técnico)

`orion-cli.exe` está en `instaladores\`. Úsalo desde PowerShell en esa carpeta:

```powershell
.\orion-cli.exe monitor --region lim     # monitoreo en la consola
.\orion-cli.exe spike sdr -n 5           # qué relays de Valve responden y con qué latencia
.\orion-cli.exe spike hops 155.133.244.35
```

---

## Problemas frecuentes

| Síntoma | Solución |
|---|---|
| `No se encontro 'cargo'` | Instala Rust y **reabre** PowerShell |
| `linker 'link.exe' not found` | Falta Visual Studio Build Tools con "Desarrollo para el escritorio con C++" |
| `no se puede cargar el archivo ... la ejecución de scripts está deshabilitada` | Usa el comando completo con `-ExecutionPolicy Bypass` |
| Un relay aparece como **ICMP BLOQUEADO** | Ese servidor no responde al ping. Es una limitación del servidor, no un fallo de tu red |
| El tramo **ISP** dice "no identificado" | Tu proveedor no responde al traceroute. El resto del diagnóstico sigue funcionando |
| El antivirus avisa sobre la app | Ocurre con programas sin firmar que envían ping. La app solo usa la API oficial de Windows y no pide permisos de administrador |
| La compilación falla con otro error | Ejecuta `.\construir.ps1 -Test` y envía la salida completa a quien te pasó el proyecto |

## Dónde guarda los datos

`%APPDATA%\net.orion.netcontrol\` contiene:
- `orion.db`: la línea base aprendida y tus ajustes;
- `sdr_config.json`: la caché de la lista de servidores de Valve.

Para empezar de cero, borra esa carpeta con la app cerrada.

---

ORION NetControl no está afiliado a Valve. "Dota 2" es una marca de Valve Corporation.
