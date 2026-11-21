// Consola: los comandos se parsean y ejecutan en Rust (lista blanca); aquí solo se muestran.
import type { CliLineKind } from "../bindings/CliLineKind";
import { h, icon } from "../dom";
import { api, errorText } from "../ipc";

const CLS: Record<CliLineKind, string> = {
  info: "text-gray-300",
  ok: "text-orion-radiant",
  warn: "text-orion-gold",
  error: "text-orion-dire",
};

export class ConsoleTab {
  readonly root: HTMLElement;
  private out = h("div", { class: "h-[420px] overflow-y-auto font-mono text-xs space-y-0.5 whitespace-pre" });
  private input = h("input", { class: "flex-1 bg-black border border-orion-gold/40 rounded px-2 py-1 font-mono text-sm text-orion-radiant focus:outline-none", placeholder: "help", autocomplete: "off", spellcheck: "false" });
  private history: string[] = [];
  private hIdx = 0;

  constructor(private onCommand: () => void) {
    const form = h("form", { class: "flex gap-2 mt-3" }, [h("span", { class: "text-orion-gold font-mono", text: ">" }), this.input]);
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      void this.run(this.input.value);
    });
    this.input.addEventListener("keydown", (e) => {
      if (e.key === "ArrowUp" && this.hIdx > 0) this.input.value = this.history[--this.hIdx];
      else if (e.key === "ArrowDown" && this.hIdx < this.history.length) this.input.value = this.history[++this.hIdx] ?? "";
    });
    this.root = h("section", {}, [
      h("div", { class: "hud-panel p-4" }, [h("div", { class: "panel-title" }, [icon("fa-solid fa-terminal"), "CONSOLA ORION"]), this.out, form]),
    ]);
    this.line("Escribe 'help' para ver los comandos.", "info");
  }

  focus(): void {
    this.input.focus();
  }

  private line(text: string, kind: CliLineKind | "cmd"): void {
    this.out.append(h("div", { class: kind === "cmd" ? "text-white font-bold" : CLS[kind], text }));
    this.out.scrollTop = this.out.scrollHeight;
  }

  private async run(raw: string): Promise<void> {
    const line = raw.trim();
    if (!line) return;
    this.input.value = "";
    this.history.push(line);
    this.hIdx = this.history.length;
    this.onCommand();
    this.line(`> ${line}`, "cmd");
    try {
      const res = await api.cliExec(line);
      if (res.clear) this.out.replaceChildren();
      for (const l of res.lines) this.line(l.text, l.kind);
    } catch (e) {
      this.line(errorText(e), "error");
    }
  }
}
