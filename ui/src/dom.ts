// Utilidades DOM. Todo texto dinámico entra con textContent, nunca con innerHTML.

type Child = Node | string | number | null | undefined | false;
type Attrs = { class?: string; text?: string } & Record<string, string | undefined>;

export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attrs = {}, children: Child[] = []): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined) continue;
    if (k === "class") el.className = v;
    else if (k === "text") el.textContent = v;
    else el.setAttribute(k, v);
  }
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    el.append(typeof c === "string" || typeof c === "number" ? document.createTextNode(String(c)) : c);
  }
  return el;
}

/** Icono de Font Awesome (clases fijas, nunca datos externos). */
export function icon(cls: string): HTMLElement {
  return h("i", { class: cls, "aria-hidden": "true" });
}

export function byId<T extends HTMLElement = HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`falta #${id}`);
  return el as T;
}

export function setText(el: HTMLElement, text: string): void {
  if (el.textContent !== text) el.textContent = text;
}

export function fmtMs(v: number | null | undefined, digits = 1): string {
  return v === null || v === undefined ? "—" : `${v.toFixed(digits)} ms`;
}

export function fmtPct(v: number | null | undefined): string {
  return v === null || v === undefined ? "—" : `${v.toFixed(1)} %`;
}

export function fmtTime(tsMs: number): string {
  return new Date(tsMs).toLocaleTimeString("es", { hour12: false });
}
