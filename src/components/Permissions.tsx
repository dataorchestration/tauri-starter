import { useTranslation } from "react-i18next";

import type { PermissionReport, PermissionState } from "../lib/ipc";
import { useAppStore } from "../store/app";

function stateClass(state: PermissionState): string {
  switch (state) {
    case "granted":
    case "not_applicable":
      return "text-success";
    case "denied":
      return "text-danger";
    default:
      return "text-warning";
  }
}

function PermissionRow({ report }: { report: PermissionReport }) {
  const { t } = useTranslation();
  const request = useAppStore((s) => s.requestPermission);
  const openSettings = useAppStore((s) => s.openPermissionSettings);
  const name = t(`permissions.names.${report.permission}`);
  return (
    <li className="flex items-center justify-between gap-4 py-3 border-b border-line last:border-b-0">
      <div>
        <div className="text-sm font-medium">{name}</div>
        <div className={`text-xs ${stateClass(report.state)}`}>{t(`permissions.${report.state}`)}</div>
      </div>
      <div className="flex gap-2">
        {report.can_request && (
          <button
            type="button"
            className="btn btn-primary"
            aria-label={`${t("permissions.request")} ${name}`}
            onClick={() => void request(report.permission)}
          >
            {t("permissions.request")}
          </button>
        )}
        {report.can_open_settings && !report.state.match(/granted|not_applicable/) && (
          <button
            type="button"
            className="btn"
            aria-label={`${t("permissions.open_settings")} ${name}`}
            onClick={() => void openSettings(report.permission)}
          >
            {t("permissions.open_settings")}
          </button>
        )}
      </div>
    </li>
  );
}

/** Permission health / onboarding. Required ones gate "Continue". */
export function Permissions() {
  const { t } = useTranslation();
  const info = useAppStore((s) => s.info);
  const permissions = useAppStore((s) => s.permissions);
  const health = useAppStore((s) => s.health);
  const refresh = useAppStore((s) => s.refreshHealth);
  const settings = useAppStore((s) => s.settings);
  const save = useAppStore((s) => s.saveSettings);
  const setView = useAppStore((s) => s.setView);

  const required = permissions.filter((p) => p.required);
  const optional = permissions.filter((p) => !p.required);
  const missing = health?.missing_required.length ?? 0;

  const finish = async () => {
    if (settings && !settings.onboarding_done) await save({ onboarding_done: true });
    setView("home");
  };

  return (
    <>
      <h1 className="text-xl font-semibold">{t("permissions.title")}</h1>
      <p className="text-fg-muted">
        {missing === 0 ? t("permissions.all_good") : t("permissions.intro", { app: info?.name ?? "" })}
      </p>

      {required.length > 0 && (
        <section className="card px-5 py-2">
          <h2 className="section-title pt-3">{t("permissions.required")}</h2>
          <ul>
            {required.map((r) => (
              <PermissionRow key={r.permission} report={r} />
            ))}
          </ul>
        </section>
      )}
      {optional.length > 0 && (
        <section className="card px-5 py-2">
          <h2 className="section-title pt-3">{t("permissions.optional")}</h2>
          <ul>
            {optional.map((r) => (
              <PermissionRow key={r.permission} report={r} />
            ))}
          </ul>
        </section>
      )}

      <div className="flex gap-2">
        <button type="button" className="btn" onClick={() => void refresh()}>
          {t("permissions.recheck")}
        </button>
        <button type="button" className="btn btn-primary" disabled={missing > 0} onClick={() => void finish()}>
          {t("permissions.continue")}
        </button>
      </div>
    </>
  );
}
