// Pestaña Telemetría: tarjetas, gráfico de 3 series reales y tabla de relays.
import { Chart, Legend, LinearScale, LineController, LineElement, PointElement, Tooltip } from "chart.js";

import type { TargetStats } from "../bindings/TargetStats";
import type { TelemetryTick } from "../bindings/TelemetryTick";
import { fmtMs, fmtPct, fmtTime, h, icon, setText } from "../dom";
import { registerLogContainer } from "../components/feedback";

Chart.register(LineController, LineElement, PointElement, LinearScale, Tooltip, Legend);

const WINDOW_MS = 180_000;
const STATUS_LABEL: Record<TargetStats["status"], string> = {
  ok: "OK",
  degraded: "DEGRADADO",
  down: "CAÍDO",
  icmp_blocked: "ICMP BLOQUEADO",
  no_data: "SIN DATOS",
};

export function statusLabel(s: TargetStats["status"]): string {
  return STATUS_LABEL[s];
}

type Point = { x: number; y: number | null };

function card(title: string, iconCls: string) {
  const value = h("div", { class: "metric-value", text: "—" });
  const sub = h("div", { class: "text-[11px] font-mono text-gray-400 mt-1", text: "" });
  const el = h("div", { class: "hud-panel p-4" }, [h("div", { class: "metric-label flex items-center gap-2" }, [icon(iconCls), title]), value, sub]);
  return { el, value, sub };
}

export class TelemetryTab {
  readonly root: HTMLElement;
  private cards;
  private chart: Chart<"line", Point[]>;
  private relayRows: HTMLElement;
  private onPick: (pop: string) => void;

  constructor(onPick: (pop: string) => void) {
    this.onPick = onPick;
    this.cards = {
      relay: card("Relay activo (p95)", "fa-solid fa-server text-orion-gold"),
      jitter: card("Jitter RFC 3550", "fa-solid fa-wave-square text-orion-river"),
      loss: card("Pérdida (60 s)", "fa-solid fa-triangle-exclamation text-orion-dire"),
      local: card("Router / ISP (p95)", "fa-solid fa-house-signal text-orion-radiant"),
    };
    const canvas = h("canvas", { height: "110" });
    this.relayRows = h("tbody");
    const logBox = h("div", { class: "h-64 overflow-y-auto space-y-0.5 pr-1" });
    registerLogContainer(logBox);

    this.root = h("section", { class: "space-y-5" }, [
      h("div", { class: "grid grid-cols-1 md:grid-cols-4 gap-4" }, Object.values(this.cards).map((c) => c.el)),
      h("div", { class: "hud-panel p-4" }, [
        h("div", { class: "panel-title" }, [icon("fa-solid fa-chart-line"), "RTT MEDIDO — ÚLTIMOS 3 MINUTOS"]),
        canvas,
        h("p", { class: "text-[10px] font-mono text-gray-500 mt-2", text: "Eco ICMP cada 1 s. Los huecos son paquetes perdidos, no se rellenan." }),
      ]),
      h("div", { class: "grid grid-cols-1 lg:grid-cols-2 gap-4" }, [
        h("div", { class: "hud-panel p-4" }, [
          h("div", { class: "panel-title" }, [icon("fa-solid fa-globe"), "RELAYS DE VALVE (SDR) — CLIC PARA USAR"]),
          h("table", { class: "w-full text-xs font-mono" }, [
            h("thead", {}, [
              h("tr", { class: "text-gray-500 text-left" }, ["POP", "p95", "Jitter", "Pérdida", "Estado"].map((t) => h("th", { class: "pb-1", text: t }))),
            ]),
            this.relayRows,
          ]),
        ]),
        h("div", { class: "hud-panel p-4" }, [h("div", { class: "panel-title" }, [icon("fa-solid fa-list"), "REGISTRO DE EVENTOS"]), logBox]),
      ]),
    ]);

    const mk = (label: string, color: string): { label: string; data: Point[]; borderColor: string; backgroundColor: string; borderWidth: number; pointRadius: number; spanGaps: boolean; tension: number } => ({
      label,
      data: [],
      borderColor: color,
      backgroundColor: color,
      borderWidth: 2,
      pointRadius: 0,
      spanGaps: false,
      tension: 0.2,
    });
    this.chart = new Chart(canvas, {
      type: "line",
      data: { datasets: [mk("Router", "#00ffa3"), mk("ISP", "#00c3ff"), mk("Relay", "#ecc94b")] },
      options: {
        animation: false,
        parsing: false,
        normalized: true,
        responsive: true,
        interaction: { mode: "nearest", intersect: false },
        scales: {
          x: { type: "linear", ticks: { color: "#6b7280", callback: (v) => fmtTime(Number(v)), maxTicksLimit: 7 }, grid: { color: "rgba(212,175,55,0.08)" } },
          y: { type: "linear", beginAtZero: true, title: { display: true, text: "ms", color: "#9ca3af" }, ticks: { color: "#6b7280" }, grid: { color: "rgba(212,175,55,0.08)" } },
        },
        plugins: {
          legend: { labels: { color: "#d1d5db", font: { family: "Rajdhani", size: 12 } } },
          tooltip: { callbacks: { title: (items) => (items.length ? fmtTime(items[0].parsed.x) : "") } },
        },
      },
    });
  }

  reset(): void {
    for (const ds of this.chart.data.datasets) ds.data = [];
    this.chart.update();
  }

  update(tick: TelemetryTick): void {
    const find = (id: string | null) => (id ? tick.targets.find((t) => t.id === id) : undefined);
    const gw = find("gateway");
    const isp = find("isp-hop");
    const relay = find(tick.active_relay);

    setText(this.cards.relay.value, fmtMs(relay?.p95_ms, 0));
    setText(this.cards.relay.sub, relay ? `${relay.label} · último ${fmtMs(relay.last_ms)}` : "sin relay activo");
    setText(this.cards.jitter.value, fmtMs(relay?.jitter_ms));
    setText(this.cards.jitter.sub, `router ${fmtMs(gw?.jitter_ms)} · ISP ${fmtMs(isp?.jitter_ms)}`);
    setText(this.cards.loss.value, relay ? fmtPct(relay.loss_pct) : "—");
    setText(this.cards.loss.sub, `router ${gw ? fmtPct(gw.loss_pct) : "—"} · ISP ${isp ? fmtPct(isp.loss_pct) : "—"}`);
    setText(this.cards.local.value, fmtMs(gw?.p95_ms));
    setText(this.cards.local.sub, isp ? `ISP ${fmtMs(isp.p95_ms)}${isp.approximate ? " (aprox.)" : ""}` : "ISP: no identificado");

    const [dsGw, dsIsp, dsRelay] = this.chart.data.datasets;
    dsRelay.label = relay ? `Relay ${relay.label}` : "Relay";
    const push = (data: Point[], t: TargetStats | undefined) => {
      if (t) data.push({ x: tick.ts, y: t.last_ms });
      while (data.length && data[0].x < tick.ts - WINDOW_MS) data.shift();
    };
    push(dsGw.data, gw);
    push(dsIsp.data, isp);
    push(dsRelay.data, relay);
    this.chart.update("none");

    this.renderRelays(tick);
  }

  private renderRelays(tick: TelemetryTick): void {
    const relays = tick.targets.filter((t) => t.kind.type === "relay");
    const rows = relays.map((t) => {
      const active = t.id === tick.active_relay;
      const pop = t.kind.type === "relay" ? t.kind.pop : "";
      const tr = h("tr", { class: `cursor-pointer hover:bg-orion-gold/10 ${active ? "text-white" : "text-gray-300"}`, title: t.addr }, [
        h("td", { class: "py-1", text: `${active ? "▶ " : ""}${t.label}` }),
        h("td", { text: fmtMs(t.p95_ms, 0) }),
        h("td", { text: fmtMs(t.jitter_ms) }),
        h("td", { text: t.valid ? fmtPct(t.loss_pct) : "—" }),
        h("td", { class: `st-${t.status}`, text: statusLabel(t.status) }),
      ]);
      tr.addEventListener("click", () => this.onPick(pop));
      return tr;
    });
    this.relayRows.replaceChildren(...rows);
  }
}
