// Typed wrappers over Tauri events emitted by the core. Each returns the
// unlisten function so components can clean up in a `useEffect`.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AppNotification } from "./ipc";

export type View = "home" | "settings" | "notifications" | "permissions";

export const EVENTS = {
  navigate: "starter://navigate",
  updateReady: "starter://update-ready",
  localeChanged: "starter://locale-changed",
  notification: "starter://notification",
} as const;

export function onNavigate(cb: (view: View) => void): Promise<UnlistenFn> {
  return listen<{ view: View }>(EVENTS.navigate, (e) => cb(e.payload.view));
}

export function onUpdateReady(cb: (version: string) => void): Promise<UnlistenFn> {
  return listen<string>(EVENTS.updateReady, (e) => cb(e.payload));
}

export function onLocaleChanged(cb: (locale: string) => void): Promise<UnlistenFn> {
  return listen<string>(EVENTS.localeChanged, (e) => cb(e.payload));
}

export function onNotification(cb: (n: AppNotification) => void): Promise<UnlistenFn> {
  return listen<AppNotification>(EVENTS.notification, (e) => cb(e.payload));
}
