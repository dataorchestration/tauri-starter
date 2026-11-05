import { useState } from "react";
import { useTranslation } from "react-i18next";

import { useAppStore } from "../store/app";

/** Offers to send / dismiss crash reports left by a previous run. */
export function CrashBanner() {
  const { t } = useTranslation();
  const info = useAppStore((s) => s.info);
  const crashes = useAppStore((s) => s.crashes);
  const send = useAppStore((s) => s.sendCrash);
  const dismiss = useAppStore((s) => s.dismissCrash);
  const [sent, setSent] = useState(false);

  if (crashes.length === 0) {
    return sent ? (
      <p role="status" className="text-sm text-success">
        {t("crash.sent")}
      </p>
    ) : null;
  }

  const sendAll = async () => {
    for (const c of crashes) await send(c.id);
    setSent(true);
  };
  const dismissAll = async () => {
    for (const c of crashes) await dismiss(c.id);
  };

  return (
    <div role="alert" className="card border-warning/50 px-4 py-3 flex flex-col gap-2">
      <div className="text-sm font-medium">{t("crash.title", { app: info?.name ?? "" })}</div>
      <p className="text-sm text-fg-muted">{t("crash.body")}</p>
      <details className="text-xs">
        <summary className="cursor-pointer text-fg-muted">
          {crashes.length} × {crashes[0].message}
        </summary>
        <pre className="mt-2 max-h-40 overflow-auto bg-surface-3 rounded-control p-2 font-mono whitespace-pre-wrap">
          {crashes[0].location}
          {"\n"}
          {crashes[0].backtrace}
        </pre>
      </details>
      <div className="flex gap-2">
        <button type="button" className="btn btn-primary" onClick={() => void sendAll()}>
          {t("crash.send")}
        </button>
        <button type="button" className="btn" onClick={() => void dismissAll()}>
          {t("crash.dismiss")}
        </button>
      </div>
    </div>
  );
}
