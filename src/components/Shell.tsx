import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type { View } from "../lib/events";
import { useAppStore } from "../store/app";
import { CrashBanner } from "./CrashBanner";
import { UpdateBanner } from "./UpdateBanner";

const NAV: { view: View; key: string }[] = [
  { view: "home", key: "nav.home" },
  { view: "notifications", key: "nav.notifications" },
  { view: "permissions", key: "nav.permissions" },
  { view: "settings", key: "nav.settings" },
];

/** App frame: sidebar navigation, global banners, error toast, content. */
export function Shell({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const view = useAppStore((s) => s.view);
  const setView = useAppStore((s) => s.setView);
  const info = useAppStore((s) => s.info);
  const unread = useAppStore((s) => s.unread);
  const health = useAppStore((s) => s.health);
  const error = useAppStore((s) => s.error);
  const clearError = useAppStore((s) => s.clearError);

  return (
    <div className="flex h-full">
      <nav
        aria-label="Primary"
        className="w-52 shrink-0 border-r border-line bg-surface-2/60 flex flex-col p-3 gap-1"
      >
        <div className="px-2 py-3 mb-2">
          <div className="font-semibold text-[15px] leading-5">{info?.name ?? "…"}</div>
          <div className="text-xs text-fg-muted">
            {info ? t("app.version", { version: info.version }) : ""}
          </div>
        </div>
        {NAV.map((item) => {
          const active = view === item.view;
          const badge =
            item.view === "notifications" && unread > 0
              ? unread
              : item.view === "permissions" && health && health.missing_required.length > 0
                ? "!"
                : null;
          return (
            <button
              key={item.view}
              type="button"
              aria-current={active ? "page" : undefined}
              onClick={() => setView(item.view)}
              className={`flex items-center justify-between px-3 py-2 rounded-control text-left text-sm cursor-pointer transition-colors ${
                active ? "bg-accent/12 text-fg font-medium" : "text-fg-muted hover:bg-surface-3 hover:text-fg"
              }`}
            >
              <span>{t(item.key)}</span>
              {badge !== null && (
                <span className="badge bg-accent text-accent-fg border-transparent">{badge}</span>
              )}
            </button>
          );
        })}
      </nav>

      <main className="flex-1 min-w-0 overflow-y-auto">
        <div className="max-w-3xl mx-auto p-8 flex flex-col gap-6">
          <UpdateBanner />
          <CrashBanner />
          {error && (
            <div
              role="alert"
              className="card border-danger/40 px-4 py-3 flex items-start justify-between gap-4 text-sm"
            >
              <span>
                <span className="font-medium text-danger">{t("common.error")}</span> — {error}
              </span>
              <button type="button" className="btn" onClick={clearError}>
                {t("common.close")}
              </button>
            </div>
          )}
          {children}
        </div>
      </main>
    </div>
  );
}
