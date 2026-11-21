// Contrato IPC tipado (§7). Los tipos vienen de src/bindings (generados con ts-rs).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { BaselineScope } from "./bindings/BaselineScope";
import type { CliOutput } from "./bindings/CliOutput";
import type { Diagnosis } from "./bindings/Diagnosis";
import type { LogEvent } from "./bindings/LogEvent";
import type { NetworkSnapshot } from "./bindings/NetworkSnapshot";
import type { ReadinessChange } from "./bindings/ReadinessChange";
import type { RelayInfo } from "./bindings/RelayInfo";
import type { Settings } from "./bindings/Settings";
import type { StatusSnapshot } from "./bindings/StatusSnapshot";
import type { TelemetryTick } from "./bindings/TelemetryTick";

export const api = {
  getStatus: () => invoke<StatusSnapshot>("get_status"),
  startMonitor: () => invoke<void>("start_monitor"),
  stopMonitor: () => invoke<void>("stop_monitor"),
  setRegion: (pop: string) => invoke<void>("set_region", { pop }),
  listRelays: () => invoke<RelayInfo[]>("list_relays"),
  resetBaseline: (scope: BaselineScope) => invoke<void>("reset_baseline", { scope }),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  cliExec: (line: string) => invoke<CliOutput>("cli_exec", { line }),
};

function on<T>(event: string) {
  return (cb: (payload: T) => void): Promise<UnlistenFn> => listen<T>(event, (e) => cb(e.payload));
}

export const events = {
  sample: on<TelemetryTick>("telemetry://sample"),
  diagnosis: on<Diagnosis>("diagnosis://update"),
  readiness: on<ReadinessChange>("readiness://change"),
  network: on<NetworkSnapshot>("network://changed"),
  log: on<LogEvent>("log://event"),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
