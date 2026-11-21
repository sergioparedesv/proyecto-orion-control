// Pestaña Línea base: EWMA real (μ, σ, k, umbral, convergencia) + ajustes.
import type { Settings } from "../bindings/Settings";
import type { TelemetryTick } from "../bindings/TelemetryTick";
import { fmtMs, h, icon, setText } from "../dom";

const SLOTS = ["00-04 h", "04-08 h", "08-12 h", "12-16 h", "16-20 h", "20-24 h"];
const KNOWN_POPS: [string, string][] = [
  ["lim", "Lima"],
  ["scl", "Santiago"],
  ["eze", "Buenos Aires"],
  ["gru", "São Paulo"],
  ["iad", "Virginia"],
  ["atl", "Atlanta"],
  ["dfw", "Dallas"],
  ["ord", "Chicago"],
  ["lax", "Los Ángeles"],
  ["sea", "Seattle"],
];

export type BaselineHandlers = {
  onReset: () => void;
  onSaveSettings: (s: Settings) => void;
};

function stat(label: string) {
  const v = h("div", { class: "metric-value", text: "—" });
  return { el: h("div", { class: "hud-panel p-4" }, [h("div", { class: "metric-label", text: label }), v]), v };
}

export class BaselineTab {
  readonly root: HTMLElement;
  private s = { mu: stat("μ (media EWMA)"), sigma: stat("σ (desv. EWMA)"), thr: stat("Umbral μ + k·σ"), anomaly: stat("Puntaje de anomalía") };
  private status = h("div", { class: "font-orbitron text-sm text-white", text: "Sin datos" });
  private bar = h("div", { class: "h-2 bg-orion-gold rounded transition-all", style: "width:0%" });
  private detail = h("div", { class: "text-xs font-mono text-gray-400 mt-2", text: "" });
  private settingsBox = h("div", { class: "space-y-3" });
  private settings: Settings | null = null;
  private demoAvailable = false;

  constructor(private handlers: BaselineHandlers) {
    const reset = h("button", { class: "ctl-btn" }, [icon("fa-solid fa-rotate-left"), " Reiniciar línea base de esta red"]);
    reset.addEventListener("click", () => this.handlers.onReset());
    this.root = h("section", { class: "space-y-5" }, [
      h("div", { class: "grid grid-cols-2 md:grid-cols-4 gap-4" }, [this.s.mu.el, this.s.sigma.el, this.s.thr.el, this.s.anomaly.el]),
      h("div", { class: "hud-panel p-5" }, [
        h("div", { class: "panel-title" }, [icon("fa-solid fa-chart-area"), "LÍNEA BASE ESTADÍSTICA DEL RELAY ACTIVO"]),
        this.status,
        h("div", { class: "w-full h-2 bg-gray-800 rounded mt-2" }, [this.bar]),
        this.detail,
        h("p", { class: "text-xs text-gray-400 mt-4 leading-relaxed", text: "Media y varianza móviles exponenciales (α = 0,05) por red, por objetivo y por franja de 4 horas. Cada muestra se compara con la línea base antes de actualizarla (z-score); una vez aprendida, las muestras con z > 4 se marcan como anomalía y no la contaminan. No es inteligencia artificial: es estadística, y todos los valores mostrados salen de las mediciones." }),
        h("div", { class: "mt-4" }, [reset]),
      ]),
      h("div", { class: "hud-panel p-5" }, [h("div", { class: "panel-title" }, [icon("fa-solid fa-gear"), "AJUSTES"]), this.settingsBox]),
    ]);
  }

  update(tick: TelemetryTick): void {
    const b = tick.baseline;
    if (!b) {
      setText(this.status, "Sin datos del relay activo todavía.");
      for (const x of Object.values(this.s)) setText(x.v, "—");
      return;
    }
    setText(this.s.mu.v, fmtMs(b.mu_ms));
    setText(this.s.sigma.v, fmtMs(b.sigma_ms));
    setText(this.s.thr.v, b.converged ? fmtMs(b.threshold_ms) : "—");
    setText(this.s.anomaly.v, b.converged ? b.anomaly_score.toFixed(2) : "—");
    const pct = Math.min(100, (b.samples / b.converge_after) * 100);
    this.bar.style.width = `${pct}%`;
    setText(this.status, b.converged ? `Aprendida (${b.samples} muestras)` : `Calibrando (${b.samples}/${b.converge_after})`);
    setText(
      this.detail,
      `Franja ${SLOTS[b.slot] ?? b.slot} · k = ${b.k} · ${b.converged ? "el semáforo usa el umbral dinámico" : "mientras calibra, el semáforo usa umbrales absolutos (80 / 120 ms)"}`,
    );
  }

  setSettings(s: Settings, demoAvailable: boolean): void {
    this.settings = s;
    this.demoAvailable = demoAvailable;
    this.renderSettings();
  }

  private renderSettings(): void {
    const s = this.settings;
    if (!s) return;
    const k = h("input", { type: "number", min: "1", max: "6", step: "0.5", value: String(s.baseline_k), class: "ctl-select w-20" });
    const pops = KNOWN_POPS.map(([code, name]) => {
      const cb = h("input", { type: "checkbox", value: code });
      cb.checked = s.watch_pops.includes(code);
      return { cb, el: h("label", { class: "flex items-center gap-1 text-xs font-mono" }, [cb, `${name} (${code})`]) };
    });
    const demo = h("select", { class: "ctl-select" }, [
      h("option", { value: "", text: "Desactivado (mediciones reales)" }),
      ...["calm", "spike", "wifi", "isp", "route"].map((x) => h("option", { value: x, text: `DEMO: ${x}` })),
    ]);
    demo.value = s.demo_scenario ?? "";
    const save = h("button", { class: "ctl-btn" }, [icon("fa-solid fa-floppy-disk"), " Guardar y reiniciar monitoreo"]);
    save.addEventListener("click", () => {
      const next: Settings = {
        ...s,
        baseline_k: Number(k.value),
        watch_pops: pops.filter((p) => p.cb.checked).map((p) => p.cb.value),
        demo_scenario: this.demoAvailable && demo.value ? demo.value : null,
      };
      this.handlers.onSaveSettings(next);
    });
    const rows: Node[] = [
      h("label", { class: "flex items-center gap-2 text-xs font-mono" }, ["k del umbral dinámico (1-6):", k]),
      h("div", { class: "text-xs font-mono text-gray-400", text: "POP vigilados (para sugerir regiones alternativas):" }),
      h("div", { class: "grid grid-cols-2 md:grid-cols-5 gap-2" }, pops.map((p) => p.el)),
    ];
    if (this.demoAvailable) {
      rows.push(h("label", { class: "flex items-center gap-2 text-xs font-mono text-orion-dire" }, ["Modo DEMO (solo compilación de desarrollo):", demo]));
    }
    rows.push(save);
    this.settingsBox.replaceChildren(...rows);
  }
}
