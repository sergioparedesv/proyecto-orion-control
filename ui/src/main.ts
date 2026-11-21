// ORION NetControl v4.0 — frontend. Solo muestra lo que mide el núcleo en Rust.
import "@fontsource/orbitron/500.css";
import "@fontsource/orbitron/700.css";
import "@fontsource/orbitron/900.css";
import "@fontsource/rajdhani/500.css";
import "@fontsource/rajdhani/600.css";
import "@fontsource/rajdhani/700.css";
import "@fontsource/share-tech-mono/400.css";
import "@fortawesome/fontawesome-free/css/fontawesome.min.css";
import "@fortawesome/fontawesome-free/css/solid.min.css";
import "./styles.css";

import { AudioEngine, type SoundPreset } from "./audio";
import { Background } from "./background";
import type { Readiness } from "./bindings/Readiness";
import type { Settings } from "./bindings/Settings";
import type { StatusSnapshot } from "./bindings/StatusSnapshot";
import { confirmModal, log, toast } from "./components/feedback";
import { byId, h, icon, setText } from "./dom";
import { api, errorText, events } from "./ipc";
import { BaselineTab } from "./tabs/baseline";
import { ConsoleTab } from "./tabs/console";
import { DiagnosisTab } from "./tabs/diagnosis";
import { TelemetryTab } from "./tabs/telemetry";

const READINESS_TEXT: Record<Readiness, string> = {
  ready: "LISTO — PUEDES BUSCAR PARTIDA",
  caution: "PRECAUCIÓN — CONEXIÓN INESTABLE",
  no_go: "NO BUSCAR — CONEXIÓN DEGRADADA",
  no_data: "SIN DATOS",
};

// ---------- preferencias de interfaz (localStorage solo para esto) ----------
const prefs = {
  get(key: string): string | null {
    try {
      return localStorage.getItem(`orion.ui.${key}`);
    } catch {
      return null;
    }
  },
  set(key: string, v: string): void {
    try {
      localStorage.setItem(`orion.ui.${key}`, v);
    } catch {
      /* sin almacenamiento: no pasa nada */
    }
  },
};

// ---------- estado ----------
let monitoring = false;
let matchMode = prefs.get("matchMode") === "1";
let settings: Settings | null = null;
let status: StatusSnapshot | null = null;

const audio = new AudioEngine();
audio.muted = prefs.get("muted") === "1";
const bg = new Background(byId<HTMLCanvasElement>("bg-canvas"), byId("wallpaper-image"), byId("wallpaper-overlay"));

// ---------- pestañas ----------
const telemetry = new TelemetryTab((pop) => void changeRegion(pop));
const diagnosis = new DiagnosisTab();
const baseline = new BaselineTab({ onReset: () => void resetBaseline(), onSaveSettings: (s) => void saveSettings(s) });
const consoleTab = new ConsoleTab(() => audio.play("blink"));

const upcoming = (title: string, text: string) =>
  h("section", {}, [
    h("div", { class: "hud-panel p-8 text-center space-y-2" }, [
      h("div", { class: "font-orbitron text-orion-gold", text: title }),
      h("p", { class: "text-sm text-gray-400", text }),
    ]),
  ]);

const TABS: { id: string; label: string; iconCls: string; el: HTMLElement; upcoming?: boolean }[] = [
  { id: "telemetry", label: "TELEMETRÍA", iconCls: "fa-solid fa-chart-line text-orion-gold", el: telemetry.root },
  { id: "diagnosis", label: "DIAGNÓSTICO", iconCls: "fa-solid fa-route text-orion-radiant", el: diagnosis.root },
  { id: "baseline", label: "LÍNEA BASE", iconCls: "fa-solid fa-chart-area text-orion-purple", el: baseline.root },
  { id: "console", label: "CONSOLA", iconCls: "fa-solid fa-terminal text-orion-steam", el: consoleTab.root },
  {
    id: "bufferbloat",
    label: "BUFFERBLOAT",
    iconCls: "fa-solid fa-gauge-high",
    upcoming: true,
    el: upcoming("PRÓXIMAMENTE (F3)", "Prueba real de latencia bajo carga (descarga y subida) con grado A+ a F. Esta versión no muestra ningún resultado simulado."),
  },
  {
    id: "actions",
    label: "ACCIONES",
    iconCls: "fa-solid fa-screwdriver-wrench",
    upcoming: true,
    el: upcoming("PRÓXIMAMENTE (F4)", "Purgar DNS, renovar DHCP y reiniciar el adaptador, con confirmación, privilegios explícitos y verificación posterior."),
  },
  {
    id: "history",
    label: "HISTORIAL",
    iconCls: "fa-solid fa-clock-rotate-left",
    upcoming: true,
    el: upcoming("PRÓXIMAMENTE (F3)", "Historial por día y franja horaria, y exportación de informes con metodología."),
  },
];

function switchTab(id: string): void {
  for (const t of TABS) {
    t.el.classList.toggle("hidden", t.id !== id);
    byId(`nav-${t.id}`).classList.toggle("active-tab", t.id === id);
  }
  prefs.set("tab", id);
  if (id === "console") consoleTab.focus();
}

const tabBar = byId("tab-bar");
const tabsRoot = byId("tabs");
TABS.forEach((t, i) => {
  const btn = h("button", { id: `nav-${t.id}`, class: `nav-tab-btn flex items-center gap-2${t.upcoming ? " upcoming" : ""}` }, [icon(t.iconCls), `[${i + 1}] ${t.label}`]);
  btn.addEventListener("click", () => switchTab(t.id));
  tabBar.append(btn);
  tabsRoot.append(t.el);
});
switchTab(TABS.some((t) => t.id === prefs.get("tab")) ? prefs.get("tab")! : "telemetry");

// ---------- cabecera ----------
const badge = byId("readiness-badge");
const readinessText = byId("readiness-text");
const regionSelect = byId<HTMLSelectElement>("region-select");
const monitorBtn = byId<HTMLButtonElement>("monitor-toggle");
const presetSelect = byId<HTMLSelectElement>("sound-preset");
const muteBtn = byId<HTMLButtonElement>("mute-toggle");
const matchBtn = byId<HTMLButtonElement>("match-mode");

function setReadiness(r: Readiness, reason: string): void {
  badge.dataset.state = r;
  setText(readinessText, READINESS_TEXT[r]);
  badge.title = reason;
}

function renderMonitorBtn(): void {
  monitorBtn.replaceChildren(icon(monitoring ? "fa-solid fa-stop" : "fa-solid fa-play"));
  monitorBtn.title = monitoring ? "Detener monitoreo" : "Iniciar monitoreo";
}

function renderMute(): void {
  muteBtn.replaceChildren(icon(audio.muted ? "fa-solid fa-volume-xmark" : "fa-solid fa-volume-high"));
}

function renderMatchMode(): void {
  document.body.classList.toggle("match-mode", matchMode);
  matchBtn.classList.toggle("on", matchMode);
  bg.setPaused(matchMode);
}

presetSelect.value = prefs.get("preset") ?? "ping";
presetSelect.addEventListener("change", () => prefs.set("preset", presetSelect.value));
byId("sound-test").addEventListener("click", () => audio.play(presetSelect.value as SoundPreset));
muteBtn.addEventListener("click", () => {
  audio.muted = !audio.muted;
  prefs.set("muted", audio.muted ? "1" : "0");
  renderMute();
});
matchBtn.addEventListener("click", () => {
  matchMode = !matchMode;
  prefs.set("matchMode", matchMode ? "1" : "0");
  renderMatchMode();
  toast(matchMode ? "Modo partida: fondo detenido, solo suena el cambio a NO BUSCAR" : "Modo partida desactivado");
});
byId("wallpaper-toggle").addEventListener("click", () => {
  if (!bg.toggle()) toast("Primero carga una imagen con el botón de subir", "warn");
});
byId<HTMLInputElement>("wallpaper-input").addEventListener("change", (e) => {
  const file = (e.target as HTMLInputElement).files?.[0];
  if (file) bg.setImage(URL.createObjectURL(file));
});
monitorBtn.addEventListener("click", () => void (monitoring ? stopMonitor() : startMonitor()));
regionSelect.addEventListener("change", () => void changeRegion(regionSelect.value));

// El audio solo puede arrancar tras un gesto del usuario.
const unlock = () => audio.unlock();
window.addEventListener("pointerdown", unlock, { once: true });
window.addEventListener("keydown", unlock, { once: true });

// ---------- acciones ----------
async function refreshRelays(): Promise<void> {
  try {
    const relays = await api.listRelays();
    const region = settings?.region ?? status?.region ?? "lim";
    regionSelect.replaceChildren(...relays.map((r) => h("option", { value: r.pop, text: `${r.desc} (${r.pop})` })));
    regionSelect.value = region;
  } catch (e) {
    log(`No se pudo leer la lista de relays: ${errorText(e)}`, "error");
  }
}

async function startMonitor(): Promise<void> {
  monitorBtn.disabled = true;
  try {
    telemetry.reset();
    await api.startMonitor();
    monitoring = true;
    status = await api.getStatus();
    byId("demo-banner").classList.toggle("hidden", !status.demo);
    if (status.catalog && status.catalog.source !== "remote" && status.catalog.source !== "cache") {
      toast("Catálogo de relays sin actualizar: se usa una copia local", "warn");
    }
    await refreshRelays();
  } catch (e) {
    toast(`No se pudo iniciar el monitoreo: ${errorText(e)}`, "error");
    log(`Inicio fallido: ${errorText(e)}`, "error");
  } finally {
    monitorBtn.disabled = false;
    renderMonitorBtn();
  }
}

async function stopMonitor(): Promise<void> {
  await api.stopMonitor().catch((e) => log(errorText(e), "error"));
  monitoring = false;
  renderMonitorBtn();
  setReadiness("no_data", "Monitoreo detenido");
}

async function changeRegion(pop: string): Promise<void> {
  if (!pop) return;
  try {
    await api.setRegion(pop);
    if (settings) settings.region = pop;
    regionSelect.value = pop;
    telemetry.reset();
    toast(`Región activa: ${pop}`);
  } catch (e) {
    toast(errorText(e), "error");
  }
}

async function resetBaseline(): Promise<void> {
  const ok = await confirmModal("Reiniciar línea base", "Se borrará lo aprendido para la red actual. El semáforo volverá a usar umbrales absolutos hasta recalibrar (unos 5 minutos).", "Reiniciar");
  if (!ok) return;
  try {
    await api.resetBaseline({ scope: "network" });
    toast("Línea base reiniciada");
  } catch (e) {
    toast(errorText(e), "error");
  }
}

async function saveSettings(next: Settings): Promise<void> {
  try {
    settings = await api.setSettings(next);
    baseline.setSettings(settings, status?.demo_available ?? false);
    toast("Ajustes guardados");
    if (monitoring) {
      await api.stopMonitor();
      monitoring = false;
      await startMonitor();
    } else {
      await refreshRelays();
    }
  } catch (e) {
    toast(errorText(e), "error");
  }
}

// ---------- eventos del núcleo ----------
async function wireEvents(): Promise<void> {
  await events.sample((tick) => {
    if (!monitoring) return;
    setReadiness(tick.readiness, tick.readiness_reason);
    telemetry.update(tick);
    diagnosis.update(tick);
    baseline.update(tick);
  });
  await events.network((n) => {
    diagnosis.setNetwork(n);
    byId("demo-banner").classList.toggle("hidden", !n.demo);
    log(`Red: ${n.iface_label || "—"} · gateway ${n.gateway ?? "—"} · ISP ${n.isp_hop ?? "no identificado"}`);
  });
  await events.readiness((c) => {
    // Sonido solo al cambiar de estado; en modo partida, solo al pasar a NO BUSCAR.
    if (c.to === "no_go") audio.play("alert");
    else if (!matchMode && c.to !== "no_data") audio.play(presetSelect.value as SoundPreset);
    if (!matchMode || c.to === "no_go") toast(READINESS_TEXT[c.to], c.to === "no_go" ? "error" : c.to === "caution" ? "warn" : "info");
  });
  await events.diagnosis((d) => log(`Diagnóstico: ${d.message}`, d.verdict === "ok" || d.verdict === "no_data" ? "info" : "warn"));
  await events.log((e) => log(e.message, e.level, e.ts));
}

async function boot(): Promise<void> {
  renderMute();
  renderMatchMode();
  renderMonitorBtn();
  await wireEvents();
  try {
    status = await api.getStatus();
    settings = await api.getSettings();
    setText(byId("app-version"), `v${status.version}`);
    baseline.setSettings(settings, status.demo_available);
    if (status.network) diagnosis.setNetwork(status.network);
  } catch (e) {
    log(`No se pudo leer el estado: ${errorText(e)}`, "error");
  }
  log("ORION NetControl iniciado. Todas las cifras son mediciones reales (o están marcadas como DEMO).");
  await startMonitor();
}

void boot();
