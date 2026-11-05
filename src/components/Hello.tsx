import { useTranslation } from "react-i18next";

import { useAppStore } from "../store/app";

function Dot({ ok }: { ok: boolean }) {
  return (
    <span
      aria-hidden
      className={`inline-block size-2 rounded-full ${ok ? "bg-success" : "bg-danger"}`}
    />
  );
}

/** The "Hello Desktop" landing screen: identity, health, and where to go next. */
export function Hello() {
  const { t } = useTranslation();
  const info = useAppStore((s) => s.info);
  const health = useAppStore((s) => s.health);
  const setView = useAppStore((s) => s.setView);
  const refreshHealth = useAppStore((s) => s.refreshHealth);

  const problems = health
    ? [!health.db.ok, !health.keychain.ok, health.missing_required.length > 0].filter(Boolean).length
    : 0;

  return (
    <>
      <header className="flex flex-col gap-3">
        <div className="flex items-center gap-3">
          <div className="size-12 rounded-card bg-accent text-accent-fg grid place-items-center text-2xl font-bold select-none">
            {(info?.name ?? "H").slice(0, 1).toUpperCase()}
          </div>
          <div>
            <h1 className="text-2xl font-semibold leading-tight">{t("app.hello")}</h1>
            <div className="flex gap-2 text-xs text-fg-muted mt-1">
              {info && (
                <>
                  <span className="badge">{t("app.version", { version: info.version })}</span>
                  <span className="badge">{t("app.platform", { os: info.os, arch: info.arch })}</span>
                  {info.debug && <span className="badge text-warning">{t("app.debug")}</span>}
                </>
              )}
            </div>
          </div>
        </div>
        <p className="text-fg-muted max-w-prose">{t("app.tagline")}</p>
      </header>

      <section className="card p-5 flex flex-col gap-4" aria-labelledby="health-title">
        <div className="flex items-center justify-between">
          <h2 id="health-title" className="section-title">
            {t("health.title")}
          </h2>
          <button type="button" className="btn" onClick={() => void refreshHealth()}>
            {t("permissions.recheck")}
          </button>
        </div>
        {health ? (
          <>
            <p className={`text-sm font-medium ${health.ok ? "text-success" : "text-warning"}`}>
              {health.ok ? t("health.ok") : t("health.problems", { count: problems })}
            </p>
            <ul className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-sm">
              <li className="flex items-center gap-2">
                <Dot ok={health.db.ok} /> {t("health.db")}
                <span className="text-fg-muted">· {health.db.detail}</span>
              </li>
              <li className="flex items-center gap-2">
                <Dot ok={health.keychain.ok} /> {t("health.keychain")}
                <span className="text-fg-muted">· {health.keychain.detail}</span>
              </li>
              <li className="flex items-center gap-2">
                <Dot ok={health.missing_required.length === 0} /> {t("health.permissions")}
                <button
                  type="button"
                  className="text-accent underline-offset-2 hover:underline cursor-pointer"
                  onClick={() => setView("permissions")}
                >
                  {health.missing_required.length === 0
                    ? t("permissions.all_good")
                    : t("health.problems", { count: health.missing_required.length })}
                </button>
              </li>
              <li className="flex items-center gap-2">
                <Dot ok={health.updater_configured} /> {t("health.updater")}
                <span className="text-fg-muted">
                  · {health.updater_configured ? t("health.configured") : t("health.not_configured")}
                </span>
              </li>
              <li className="flex items-center gap-2">
                <Dot ok /> {t("health.telemetry")}
                <span className="text-fg-muted">
                  · {health.telemetry_enabled ? t("health.enabled") : t("health.disabled")}
                  {health.telemetry_sinks.length > 0 && ` (${health.telemetry_sinks.join(", ")})`}
                </span>
              </li>
              {health.pending_crashes > 0 && (
                <li className="flex items-center gap-2 text-warning">
                  <Dot ok={false} /> {t("health.crashes", { count: health.pending_crashes })}
                </li>
              )}
            </ul>
          </>
        ) : (
          <p className="text-fg-muted text-sm">{t("common.loading")}</p>
        )}
      </section>

      <section className="grid grid-cols-1 sm:grid-cols-3 gap-3">
        {(["settings", "notifications", "permissions"] as const).map((v) => (
          <button
            key={v}
            type="button"
            className="card p-4 text-left hover:border-accent/60 transition-colors cursor-pointer"
            onClick={() => setView(v)}
          >
            <div className="font-medium">{t(`nav.${v}`)}</div>
            <div className="text-xs text-fg-muted mt-1">
              {t(`${v === "permissions" ? "permissions" : v}.title`)}
            </div>
          </button>
        ))}
      </section>
    </>
  );
}
