// Toasts, registro de eventos y modal de confirmación.
import { byId, fmtTime, h } from "../dom";

export type Level = "info" | "warn" | "error";

export function toast(message: string, level: Level = "info"): void {
  const container = byId("toast-container");
  const color =
    level === "error" ? "border-orion-dire text-orion-dire" : level === "warn" ? "border-orion-gold text-orion-gold" : "border-orion-radiant text-orion-radiant";
  const el = h("div", { class: `hud-panel p-3 border ${color} text-xs font-mono rounded shadow-lg transition-all duration-300 opacity-0 translate-y-2`, text: message });
  container.append(el);
  requestAnimationFrame(() => el.classList.remove("opacity-0", "translate-y-2"));
  setTimeout(() => {
    el.classList.add("opacity-0");
    setTimeout(() => el.remove(), 300);
  }, 4000);
}

const MAX_LOG = 300;
const logTargets: HTMLElement[] = [];

export function registerLogContainer(el: HTMLElement): void {
  logTargets.push(el);
}

export function log(message: string, level: Level = "info", ts = Date.now()): void {
  const color = level === "error" ? "text-orion-dire" : level === "warn" ? "text-orion-gold" : "text-orion-radiant";
  for (const c of logTargets) {
    const row = h("div", { class: `${color} flex gap-2 text-[11px] font-mono` }, [
      h("span", { class: "text-gray-500", text: `[${fmtTime(ts)}]` }),
      h("span", { class: "font-bold", text: `[${level.toUpperCase()}]` }),
      h("span", { text: message }),
    ]);
    c.append(row);
    while (c.childElementCount > MAX_LOG) c.firstElementChild?.remove();
    c.scrollTop = c.scrollHeight;
  }
}

/** Modal de confirmación propio (las acciones con impacto en el sistema usarán diálogos nativos en F4). */
export function confirmModal(title: string, body: string, confirmLabel = "Confirmar"): Promise<boolean> {
  return new Promise((resolve) => {
    const root = byId("modal-root");
    const close = (v: boolean) => {
      overlay.remove();
      resolve(v);
    };
    const ok = h("button", { class: "ctl-btn", text: confirmLabel });
    const cancel = h("button", { class: "ctl-btn", text: "Cancelar" });
    ok.addEventListener("click", () => close(true));
    cancel.addEventListener("click", () => close(false));
    const overlay = h("div", { class: "fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4" }, [
      h("div", { class: "hud-panel p-6 max-w-md w-full space-y-4" }, [
        h("div", { class: "panel-title", text: title }),
        h("p", { class: "text-sm text-gray-300", text: body }),
        h("div", { class: "flex justify-end gap-2" }, [cancel, ok]),
      ]),
    ]);
    root.append(overlay);
    cancel.focus();
  });
}
