// Pestaña Diagnóstico: PC → Router → ISP → Valve, con el estado y la evidencia de cada tramo.
import type { Evidence } from "../bindings/Evidence";
import type { NetworkSnapshot } from "../bindings/NetworkSnapshot";
import type { TargetStats } from "../bindings/TargetStats";
import type { TelemetryTick } from "../bindings/TelemetryTick";
import type { Verdict } from "../bindings/Verdict";
import { fmtMs, fmtPct, h, icon, setText } from "../dom";
import { statusLabel } from "./telemetry";

const VERDICT: Record<Verdict, { title: string; cls: string }> = {
  ok: { title: "TODO EN ORDEN", cls: "text-orion-radiant glow-radiant" },
  local: { title: "PROBLEMA EN TU RED LOCAL / WI-FI", cls: "text-orion-dire glow-dire" },
  isp: { title: "PROBLEMA EN TU PROVEEDOR (ISP)", cls: "text-orion-dire glow-dire" },
  route: { title: "RUTA HACIA VALVE DEGRADADA", cls: "text-orion-gold glow-gold" },
  no_data: { title: "RECOLECTANDO DATOS", cls: "text-gray-400" },
};

const METRIC: Record<string, string> = {
  loss_pct: "pérdida (%)",
  jitter_ms: "jitter (ms)",
  p95_ms: "p95 (ms)",
  p95_delta_ms: "p95 ISP − p95 router (ms)",
  rssi_dbm: "RSSI (dBm)",
  verdict_minutes: "minutos sostenido",
  alt_p95_ms: "p95 alternativo (ms)",
};

export function evidenceRow(e: Evidence): HTMLElement {
  return h("li", { class: "font-mono text-xs text-gray-300" }, [
    h("span", { class: "text-orion-gold", text: `${e.target} · ${METRIC[e.metric] ?? e.metric}: ` }),
    `${e.value.toFixed(1)} (umbral ${e.threshold.toFixed(1)})`,
  ]);
}

type Node = { el: HTMLElement; title: HTMLElement; line1: HTMLElement; line2: HTMLElement; status: HTMLElement };

function node(name: string, iconCls: string): Node {
  const title = h("div", { class: "text-[11px] font-mono text-gray-400 mt-1", text: "" });
  const line1 = h("div", { class: "font-orbitron text-lg text-white", text: "—" });
  const line2 = h("div", { class: "text-[11px] font-mono text-gray-400", text: "" });
  const status = h("div", { class: "text-[10px] font-mono mt-1", text: "" });
  const el = h("div", { class: "hud-panel seg-node" }, [
    h("div", { class: "text-2xl text-orion-gold" }, [icon(iconCls)]),
    h("div", { class: "font-orbitron text-xs font-bold tracking-wider text-white mt-1", text: name }),
    title,
    line1,
    line2,
    status,
  ]);
  return { el, title, line1, line2, status };
}

export class DiagnosisTab {
  readonly root: HTMLElement;
  private nodes;
  private links: HTMLElement[];
  private verdictTitle = h("div", { class: "font-orbitron text-xl font-bold", text: "—" });
  private verdictMsg = h("p", { class: "text-sm text-gray-300 mt-1", text: "" });
  private evidence = h("ul", { class: "space-y-1 mt-3" });
  private wifi = h("div", { class: "space-y-1 text-xs font-mono text-gray-300" });
  private recs = h("ul", { class: "space-y-2" });

  constructor() {
    this.nodes = { pc: node("TU PC", "fa-solid fa-desktop"), router: node("ROUTER", "fa-solid fa-wifi"), isp: node("ISP", "fa-solid fa-tower-broadcast"), valve: node("VALVE (SDR)", "fa-solid fa-server") };
    this.links = [0, 1, 2].map(() => h("div", { class: "seg-link", "data-state": "no_data" }));
    const chain = h("div", { class: "flex items-stretch gap-2 overflow-x-auto" }, [
      this.nodes.pc.el,
      this.links[0],
      this.nodes.router.el,
      this.links[1],
      this.nodes.isp.el,
      this.links[2],
      this.nodes.valve.el,
    ]);
    this.root = h("section", { class: "space-y-5" }, [
      h("div", { class: "hud-panel p-5" }, [h("div", { class: "panel-title" }, [icon("fa-solid fa-route"), "¿DÓNDE ESTÁ EL PROBLEMA?"]), chain]),
      h("div", { class: "grid grid-cols-1 lg:grid-cols-3 gap-4" }, [
        h("div", { class: "hud-panel p-5 lg:col-span-2" }, [
          h("div", { class: "panel-title" }, [icon("fa-solid fa-stethoscope"), "VEREDICTO"]),
          this.verdictTitle,
          this.verdictMsg,
          this.evidence,
        ]),
        h("div", { class: "hud-panel p-5" }, [h("div", { class: "panel-title" }, [icon("fa-solid fa-wifi"), "WI-FI"]), this.wifi]),
      ]),
      h("div", { class: "hud-panel p-5" }, [h("div", { class: "panel-title" }, [icon("fa-solid fa-lightbulb"), "RECOMENDACIONES (REGLAS CON EVIDENCIA)"]), this.recs]),
    ]);
    this.renderWifi(null);
  }

  setNetwork(n: NetworkSnapshot): void {
    setText(this.nodes.pc.title, n.iface_label || "—");
    setText(this.nodes.pc.line1, n.local_ipv4 ?? "—");
    setText(this.nodes.pc.line2, n.demo ? "DEMO" : "");
    setText(this.nodes.router.title, n.gateway ?? "sin gateway");
    setText(this.nodes.isp.title, n.isp_hop ? `${n.isp_hop}${n.isp_approximate ? " (aprox.)" : ""}` : "no identificado");
    this.renderWifi(n);
  }

  update(tick: TelemetryTick): void {
    const t = (id: string | null) => (id ? tick.targets.find((x) => x.id === id) : undefined);
    this.fill(this.nodes.router, t("gateway"));
    this.fill(this.nodes.isp, t("isp-hop"));
    const relay = t(tick.active_relay);
    this.fill(this.nodes.valve, relay);
    if (relay) setText(this.nodes.valve.title, relay.label);

    tick.diagnosis.segments.forEach((s, i) => this.links[i]?.setAttribute("data-state", s.state));

    const v = VERDICT[tick.diagnosis.verdict];
    setText(this.verdictTitle, v.title);
    this.verdictTitle.className = `font-orbitron text-xl font-bold ${v.cls}`;
    setText(this.verdictMsg, tick.diagnosis.message);
    this.evidence.replaceChildren(...tick.diagnosis.evidence.map(evidenceRow));

    this.recs.replaceChildren(
      ...(tick.recommendations.length
        ? tick.recommendations.map((r) =>
            h("li", { class: "hud-panel-radiant p-3" }, [h("div", { class: "text-sm text-white", text: r.message }), h("ul", { class: "mt-1" }, r.evidence.map(evidenceRow))]),
          )
        : [h("li", { class: "text-xs font-mono text-gray-500", text: "Ninguna regla activa en este momento." })]),
    );
    if (tick.wifi) this.renderWifiInfo(tick.wifi);
  }

  private fill(n: Node, t: TargetStats | undefined): void {
    if (!t) {
      setText(n.line1, "—");
      setText(n.line2, "sin datos");
      setText(n.status, "");
      return;
    }
    setText(n.line1, fmtMs(t.p95_ms, 0));
    setText(n.line2, `jitter ${fmtMs(t.jitter_ms)} · pérdida ${t.valid ? fmtPct(t.loss_pct) : "—"}`);
    setText(n.status, statusLabel(t.status));
    n.status.className = `text-[10px] font-mono mt-1 st-${t.status}`;
  }

  private renderWifi(n: NetworkSnapshot | null): void {
    if (n?.wifi) return this.renderWifiInfo(n.wifi);
    this.wifi.replaceChildren(h("div", { class: "text-gray-500", text: n ? "Conexión por cable o Wi-Fi no disponible en esta plataforma." : "Sin datos de red todavía." }));
  }

  private renderWifiInfo(w: NonNullable<NetworkSnapshot["wifi"]>): void {
    const band = { "2.4ghz": "2,4 GHz", "5ghz": "5 GHz", "6ghz": "6 GHz", unknown: "desconocida" }[w.band];
    const row = (k: string, v: string) => h("div", { class: "flex justify-between gap-2" }, [h("span", { class: "text-gray-500", text: k }), h("span", { text: v })]);
    this.wifi.replaceChildren(
      row("Banda", band),
      row("Canal", w.channel?.toString() ?? "—"),
      row("RSSI", w.rssi_dbm !== null ? `${w.rssi_dbm} dBm` : "—"),
      row("Calidad", w.signal_quality !== null ? `${w.signal_quality} %` : "—"),
      row("Enlace", w.link_mbps !== null ? `${w.link_mbps.toFixed(0)} Mbps` : "—"),
      h("div", { class: "text-[10px] text-gray-500 pt-1", text: "El nombre de la red (SSID) no se guarda: solo un hash." }),
    );
  }
}
