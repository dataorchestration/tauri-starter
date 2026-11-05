import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";

import { ask, invoke, listeners, relaunch, resetStore } from "./harness";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (e: { payload: unknown }) => void) => {
    listeners.set(event, handler);
    return Promise.resolve(() => listeners.delete(event));
  },
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: (...args: unknown[]) => relaunch(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: (...args: unknown[]) => ask(...args),
}));

beforeEach(async () => {
  invoke.mockReset();
  relaunch.mockClear();
  ask.mockClear();
  ask.mockResolvedValue(true);
  listeners.clear();
  await resetStore();
  const { setupI18n } = await import("../i18n");
  await setupI18n("en");
});

afterEach(() => {
  cleanup();
});
