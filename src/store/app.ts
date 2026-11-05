// Single zustand store for app-wide state. Views read slices with selectors;
// all backend calls go through `api` (src/lib/ipc.ts) so components never
// import `invoke` directly. Errors are captured into `error` for the shell to
// show; callers that need their own handling can `await` the action.

import { create } from "zustand";

import i18n, { setupI18n } from "../i18n";
import type { View } from "../lib/events";
import {
  api,
  toIpcError,
  type AppInfo,
  type AppNotification,
  type AppSettings,
  type CrashReport,
  type HealthReport,
  type NewNotification,
  type PermissionId,
  type PermissionReport,
} from "../lib/ipc";
import { log } from "../lib/log";
import { applyTheme } from "../lib/theme";

export interface AppStore {
  ready: boolean;
  error: string | null;
  view: View;

  info: AppInfo | null;
  health: HealthReport | null;
  settings: AppSettings | null;
  notifications: AppNotification[];
  unread: number;
  permissions: PermissionReport[];
  updateReady: string | null;
  crashes: CrashReport[];

  bootstrap: () => Promise<void>;
  setView: (view: View) => void;
  clearError: () => void;

  refreshHealth: () => Promise<void>;
  saveSettings: (patch: Partial<AppSettings>) => Promise<void>;

  loadNotifications: () => Promise<void>;
  addNotification: (n: NewNotification) => Promise<void>;
  markRead: (id: number) => Promise<void>;
  markAllRead: () => Promise<void>;
  removeNotification: (id: number) => Promise<void>;
  clearNotifications: () => Promise<void>;

  refreshPermissions: () => Promise<void>;
  requestPermission: (p: PermissionId) => Promise<void>;
  openPermissionSettings: (p: PermissionId) => Promise<void>;

  setUpdateReady: (version: string | null) => void;
  loadCrashes: () => Promise<void>;
  sendCrash: (id: string) => Promise<void>;
  dismissCrash: (id: string) => Promise<void>;
}

function fail(set: (p: Partial<AppStore>) => void, e: unknown, context: string) {
  const err = toIpcError(e);
  log.error(`${context}: ${err.message}`, { code: err.code });
  set({ error: err.message });
}

export const useAppStore = create<AppStore>((set, get) => ({
  ready: false,
  error: null,
  view: "home",

  info: null,
  health: null,
  settings: null,
  notifications: [],
  unread: 0,
  permissions: [],
  updateReady: null,
  crashes: [],

  async bootstrap() {
    try {
      const [info, settings] = await Promise.all([api.appInfo(), api.getSettings()]);
      await setupI18n(info.locale);
      applyTheme(settings.theme);
      set({ info, settings });
      await Promise.all([get().refreshHealth(), get().loadNotifications(), get().loadCrashes()]);
      // First run with unmet required permissions → start on the permissions view.
      const health = get().health;
      if (health && health.missing_required.length > 0 && !settings.onboarding_done) {
        set({ view: "permissions" });
      }
    } catch (e) {
      fail(set, e, "bootstrap");
    } finally {
      set({ ready: true });
    }
  },

  setView: (view) => set({ view }),
  clearError: () => set({ error: null }),

  async refreshHealth() {
    try {
      const health = await api.health();
      set({ health, permissions: health.permissions });
    } catch (e) {
      fail(set, e, "health");
    }
  },

  async saveSettings(patch) {
    const current = get().settings;
    if (!current) return;
    const next = { ...current, ...patch };
    try {
      const saved = await api.saveSettings(next);
      set({ settings: saved });
      if (patch.theme) applyTheme(saved.theme);
      if ("locale" in patch) {
        const info = await api.appInfo();
        await i18n.changeLanguage(info.locale);
        set({ info });
      }
      if ("telemetry_enabled" in patch) await get().refreshHealth();
    } catch (e) {
      fail(set, e, "save settings");
    }
  },

  async loadNotifications() {
    try {
      const notifications = await api.listNotifications(100);
      set({ notifications, unread: notifications.filter((n) => !n.read).length });
    } catch (e) {
      fail(set, e, "notifications");
    }
  },
  async addNotification(n) {
    try {
      await api.addNotification(n);
      await get().loadNotifications();
    } catch (e) {
      fail(set, e, "add notification");
    }
  },
  async markRead(id) {
    try {
      await api.markNotificationRead(id, true);
      await get().loadNotifications();
    } catch (e) {
      fail(set, e, "mark read");
    }
  },
  async markAllRead() {
    try {
      await api.markAllNotificationsRead();
      await get().loadNotifications();
    } catch (e) {
      fail(set, e, "mark all read");
    }
  },
  async removeNotification(id) {
    try {
      await api.deleteNotification(id);
      await get().loadNotifications();
    } catch (e) {
      fail(set, e, "delete notification");
    }
  },
  async clearNotifications() {
    try {
      await api.clearNotifications();
      await get().loadNotifications();
    } catch (e) {
      fail(set, e, "clear notifications");
    }
  },

  async refreshPermissions() {
    try {
      set({ permissions: await api.permissionReport() });
    } catch (e) {
      fail(set, e, "permissions");
    }
  },
  async requestPermission(p) {
    try {
      await api.requestPermission(p);
      await get().refreshHealth();
    } catch (e) {
      fail(set, e, "request permission");
    }
  },
  async openPermissionSettings(p) {
    try {
      await api.openPermissionSettings(p);
    } catch (e) {
      fail(set, e, "open settings");
    }
  },

  setUpdateReady: (version) => set({ updateReady: version }),

  async loadCrashes() {
    try {
      set({ crashes: await api.pendingCrashes() });
    } catch (e) {
      fail(set, e, "crashes");
    }
  },
  async sendCrash(id) {
    try {
      await api.sendCrash(id);
      await get().loadCrashes();
    } catch (e) {
      fail(set, e, "send crash");
    }
  },
  async dismissCrash(id) {
    try {
      await api.dismissCrash(id);
      await get().loadCrashes();
    } catch (e) {
      fail(set, e, "dismiss crash");
    }
  },
}));
