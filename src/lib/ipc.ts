// Typed wrapper over Tauri `invoke`. Every backend command has exactly one
// entry in `api` below; the types mirror the Rust structs in
// src-tauri/src/commands.rs (snake_case on the wire, same here).
//
// Errors from the core arrive as `{ code, message }` and are re-thrown as
// `IpcError` so callers can branch on `code` without string matching.

import { invoke } from "@tauri-apps/api/core";

export class IpcError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.name = "IpcError";
    this.code = code;
  }
}

/** Normalise anything thrown by `invoke` into an IpcError. */
export function toIpcError(e: unknown): IpcError {
  if (e instanceof IpcError) return e;
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    const { code, message } = e as { code: unknown; message: unknown };
    return new IpcError(String(code), String(message));
  }
  if (e instanceof Error) return new IpcError("unknown", e.message);
  return new IpcError("unknown", typeof e === "string" ? e : JSON.stringify(e));
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toIpcError(e);
  }
}

// ---------------------------------------------------------------- types

export type Theme = "system" | "light" | "dark";

export interface AppSettings {
  locale: string | null;
  theme: Theme;
  telemetry_enabled: boolean;
  autostart: boolean;
  device_id: string;
  onboarding_done: boolean;
}

export interface AppInfo {
  name: string;
  version: string;
  identifier: string;
  os: string;
  arch: string;
  locale: string;
  supported_locales: string[];
  data_dir: string;
  log_dir: string;
  debug: boolean;
  features: {
    tray: boolean;
    singleInstance: boolean;
    windowState: boolean;
    autostart: boolean;
    updater: boolean;
    notifications: boolean;
  };
}

export type PermissionId =
  | "notifications"
  | "accessibility"
  | "screen_recording"
  | "microphone"
  | "camera"
  | "input_monitoring";

export type PermissionState = "granted" | "denied" | "undetermined" | "not_applicable";

export interface PermissionReport {
  permission: PermissionId;
  state: PermissionState;
  required: boolean;
  can_request: boolean;
  can_open_settings: boolean;
}

export interface Check {
  ok: boolean;
  detail: string;
}

export interface HealthReport {
  ok: boolean;
  db: Check;
  keychain: Check;
  permissions: PermissionReport[];
  missing_required: PermissionId[];
  updater_configured: boolean;
  telemetry_enabled: boolean;
  telemetry_sinks: string[];
  pending_crashes: number;
  schema_version: number;
}

export type NotificationKind = "info" | "success" | "warning" | "error";

export interface AppNotification {
  id: number;
  title: string;
  body: string;
  kind: NotificationKind;
  read: boolean;
  created_at: number;
}

export interface NewNotification {
  title: string;
  body?: string;
  kind?: NotificationKind;
  native?: boolean;
}

export interface User {
  id: string;
  display_name: string;
  email: string | null;
  avatar_path: string | null;
  created_at: number;
  updated_at: number;
}

export interface NewUser {
  display_name: string;
  email?: string | null;
  avatar_path?: string | null;
}

export interface CrashReport {
  id: string;
  ts_ms: number;
  app_version: string;
  os: string;
  message: string;
  location: string | null;
  thread: string;
  backtrace: string;
}

export type LogLevel = "debug" | "info" | "warn" | "error";

// ---------------------------------------------------------------- commands

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  health: () => call<HealthReport>("health_check"),

  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<AppSettings>("save_settings", { settings }),

  listUsers: () => call<User[]>("list_users"),
  createUser: (user: NewUser) => call<User>("create_user", { user }),
  updateUser: (user: User) => call<User>("update_user", { user }),
  deleteUser: (id: string) => call<boolean>("delete_user", { id }),

  listNotifications: (limit?: number) => call<AppNotification[]>("list_notifications", { limit }),
  addNotification: (notification: NewNotification) =>
    call<AppNotification>("add_notification", { notification }),
  markNotificationRead: (id: number, read = true) =>
    call<boolean>("mark_notification_read", { id, read }),
  markAllNotificationsRead: () => call<number>("mark_all_notifications_read"),
  deleteNotification: (id: number) => call<boolean>("delete_notification", { id }),
  clearNotifications: () => call<number>("clear_notifications"),
  unreadCount: () => call<number>("unread_count"),

  permissionReport: () => call<PermissionReport[]>("permission_report"),
  requestPermission: (permission: PermissionId) =>
    call<PermissionState>("request_permission", { permission }),
  openPermissionSettings: (permission: PermissionId) =>
    call<boolean>("open_permission_settings", { permission }),

  /** Resolves to the staged version, or null when up to date. Throws
   *  `updater_not_configured` / `updater_failed`. */
  checkForUpdate: () => call<string | null>("check_for_update"),

  logEvent: (level: LogLevel, name: string, message: string, fields?: Record<string, unknown>) =>
    call<void>("log_event", { level, name, message, fields }),
  reportFrontendError: (err: { message: string; stack?: string; source?: string }) =>
    call<void>("report_frontend_error", { err }),

  pendingCrashes: () => call<CrashReport[]>("pending_crashes"),
  dismissCrash: (id: string) => call<boolean>("dismiss_crash", { id }),
  sendCrash: (id: string) => call<boolean>("send_crash", { id }),
  testCrash: () => call<void>("test_crash"),

  openLogsFolder: () => call<void>("open_logs_folder"),
  eraseAllData: () => call<void>("erase_all_data"),
};
