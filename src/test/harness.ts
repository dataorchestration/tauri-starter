// Shared test doubles for the Tauri bridge. `setup.ts` wires these into
// `vi.mock` so every test file sees the same mocks; tests drive them through
// `mockBackend` (responses per command) and `emit` (core → UI events).

import { vi } from "vitest";

import type { AppInfo, AppSettings, HealthReport } from "../lib/ipc";

export const invoke = vi.fn();
export const relaunch = vi.fn().mockResolvedValue(undefined);
export const ask = vi.fn().mockResolvedValue(true);

export const listeners = new Map<string, (e: { payload: unknown }) => void>();
export function emit(event: string, payload: unknown) {
  listeners.get(event)?.({ payload });
}

export const appInfo: AppInfo = {
  name: "Hello Desktop",
  version: "0.1.0",
  identifier: "com.example.hellodesktop",
  os: "macos",
  arch: "aarch64",
  locale: "en",
  supported_locales: ["en", "es"],
  data_dir: "~/Library/Application Support/Hello Desktop",
  log_dir: "~/Library/Logs/Hello Desktop",
  debug: true,
  features: {
    tray: true,
    singleInstance: true,
    windowState: true,
    autostart: true,
    updater: false,
    notifications: true,
  },
};

export const settings: AppSettings = {
  locale: null,
  theme: "system",
  telemetry_enabled: false,
  autostart: false,
  device_id: "device-1",
  onboarding_done: true,
};

export const health: HealthReport = {
  ok: true,
  db: { ok: true, detail: "schema v1, encrypted" },
  keychain: { ok: true, detail: "key present" },
  permissions: [
    {
      permission: "notifications",
      state: "granted",
      required: false,
      can_request: false,
      can_open_settings: true,
    },
  ],
  missing_required: [],
  updater_configured: false,
  telemetry_enabled: false,
  telemetry_sinks: [],
  pending_crashes: 0,
  schema_version: 1,
};

type Handler = (args: Record<string, unknown> | undefined) => unknown;

const defaults: Record<string, Handler> = {
  app_info: () => appInfo,
  get_settings: () => settings,
  save_settings: (a) => a?.settings,
  health_check: () => health,
  list_notifications: () => [],
  pending_crashes: () => [],
  permission_report: () => health.permissions,
  log_event: () => null,
};

/** Install default responses for every command, overriding any given. A
 *  handler may throw to simulate an IPC error (`{ code, message }`). */
export function mockBackend(overrides: Record<string, Handler> = {}) {
  const table = { ...defaults, ...overrides };
  invoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    const h = table[cmd];
    if (!h) return Promise.resolve(null);
    try {
      return Promise.resolve(h(args));
    } catch (e) {
      return Promise.reject(e);
    }
  });
}

let initialStore: unknown;
export async function resetStore() {
  const { useAppStore } = await import("../store/app");
  initialStore ??= useAppStore.getState();
  useAppStore.setState(initialStore as never, true);
}
