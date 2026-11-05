import { useTranslation } from "react-i18next";

import type { NotificationKind } from "../lib/ipc";
import { useAppStore } from "../store/app";

const KIND_CLASS: Record<NotificationKind, string> = {
  info: "bg-accent",
  success: "bg-success",
  warning: "bg-warning",
  error: "bg-danger",
};

export function Notifications() {
  const { t } = useTranslation();
  const info = useAppStore((s) => s.info);
  const items = useAppStore((s) => s.notifications);
  const unread = useAppStore((s) => s.unread);
  const add = useAppStore((s) => s.addNotification);
  const markRead = useAppStore((s) => s.markRead);
  const markAllRead = useAppStore((s) => s.markAllRead);
  const remove = useAppStore((s) => s.removeNotification);
  const clear = useAppStore((s) => s.clearNotifications);

  const sendTest = () =>
    add({
      title: t("notifications.test_title", { app: info?.name ?? "" }),
      body: t("notifications.test_body"),
      kind: "success",
      native: true,
    });

  return (
    <>
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold">{t("notifications.title")}</h1>
          {unread > 0 && <div className="text-xs text-fg-muted">{t("notifications.unread", { count: unread })}</div>}
        </div>
        <div className="flex gap-2">
          <button type="button" className="btn" onClick={() => void sendTest()}>
            {t("notifications.send_test")}
          </button>
          <button type="button" className="btn" disabled={unread === 0} onClick={() => void markAllRead()}>
            {t("notifications.mark_all_read")}
          </button>
          <button type="button" className="btn btn-danger" disabled={items.length === 0} onClick={() => void clear()}>
            {t("notifications.clear")}
          </button>
        </div>
      </div>

      {items.length === 0 ? (
        <p className="card p-6 text-center text-fg-muted">{t("notifications.empty")}</p>
      ) : (
        <ul className="card divide-y divide-line">
          {items.map((n) => (
            <li key={n.id} className={`flex items-start gap-3 px-4 py-3 ${n.read ? "opacity-60" : ""}`}>
              <span aria-hidden className={`mt-2 size-2 rounded-full shrink-0 ${KIND_CLASS[n.kind]}`} />
              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium">{n.title}</div>
                {n.body && <div className="text-sm text-fg-muted">{n.body}</div>}
                <div className="text-xs text-fg-muted mt-1">{new Date(n.created_at).toLocaleString()}</div>
              </div>
              <div className="flex gap-2 shrink-0">
                {!n.read && (
                  <button type="button" className="btn" onClick={() => void markRead(n.id)}>
                    ✓
                  </button>
                )}
                <button
                  type="button"
                  className="btn btn-danger"
                  aria-label={`${t("notifications.delete")} ${n.title}`}
                  onClick={() => void remove(n.id)}
                >
                  ×
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
