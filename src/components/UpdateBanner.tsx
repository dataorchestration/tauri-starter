import { useTranslation } from "react-i18next";
import { relaunch } from "@tauri-apps/plugin-process";

import { useAppStore } from "../store/app";

/** Shown once the core has downloaded and staged a new version. */
export function UpdateBanner() {
  const { t } = useTranslation();
  const version = useAppStore((s) => s.updateReady);
  const setUpdateReady = useAppStore((s) => s.setUpdateReady);
  if (!version) return null;
  return (
    <div role="status" className="card border-accent/50 bg-accent/8 px-4 py-3 flex items-center justify-between gap-4">
      <span className="text-sm font-medium">{t("update.ready", { version })}</span>
      <div className="flex gap-2">
        <button type="button" className="btn" onClick={() => setUpdateReady(null)}>
          {t("update.later")}
        </button>
        <button type="button" className="btn btn-primary" onClick={() => void relaunch()}>
          {t("update.restart")}
        </button>
      </div>
    </div>
  );
}
