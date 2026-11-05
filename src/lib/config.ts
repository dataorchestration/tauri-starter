// Typed view of starter.config.json, injected at build time by vite.config.ts
// (`define: { __STARTER_CONFIG__ }`). Mirrors `StarterConfig` in
// src-tauri/src/config.rs — keep both in sync when adding a knob.

import type { PermissionId } from "./ipc";

export type ThemeId = "system" | "light" | "dark";

export interface StarterConfig {
  app: { org: string; homepage: string; supportUrl: string; docsUrl: string };
  features: {
    tray: boolean;
    singleInstance: boolean;
    windowState: boolean;
    autostart: boolean;
    updater: boolean;
    notifications: boolean;
  };
  permissions: { required: PermissionId[]; optional: PermissionId[] };
  telemetry: {
    defaultEnabled: boolean;
    sinks: {
      file: { enabled: boolean; maxFiles: number };
      sentry: { dsn: string };
      http: {
        endpoint: string;
        apiKeyHeader: string;
        apiKey: string;
        batchSize: number;
        flushIntervalSecs: number;
      };
    };
  };
  i18n: { default: string; supported: string[] };
  ui: { defaultTheme: ThemeId };
}

const FALLBACK: StarterConfig = {
  app: { org: "", homepage: "", supportUrl: "", docsUrl: "" },
  features: {
    tray: true,
    singleInstance: true,
    windowState: true,
    autostart: true,
    updater: false,
    notifications: true,
  },
  permissions: { required: [], optional: ["notifications"] },
  telemetry: {
    defaultEnabled: false,
    sinks: {
      file: { enabled: true, maxFiles: 7 },
      sentry: { dsn: "" },
      http: { endpoint: "", apiKeyHeader: "x-api-key", apiKey: "", batchSize: 20, flushIntervalSecs: 5 },
    },
  },
  i18n: { default: "en", supported: ["en"] },
  ui: { defaultTheme: "system" },
};

export const config: StarterConfig =
  typeof __STARTER_CONFIG__ !== "undefined" && __STARTER_CONFIG__ ? __STARTER_CONFIG__ : FALLBACK;
