import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";

import { LOCALE_NAMES } from "../i18n";
import { config } from "../lib/config";
import { api, toIpcError, type Theme } from "../lib/ipc";
import { useAppStore } from "../store/app";

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 py-3 border-b border-line last:border-b-0">
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        {hint && <div className="text-xs text-fg-muted mt-0.5">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative inline-flex h-6 w-11 items-center rounded-full border transition-colors cursor-pointer ${
        checked ? "bg-accent border-accent" : "bg-surface-3 border-line"
      }`}
    >
      <span
        className={`inline-block size-5 rounded-full bg-white shadow transition-transform ${
          checked ? "translate-x-5" : "translate-x-0.5"
        }`}
      />
    </button>
  );
}

export function Settings() {
  const { t } = useTranslation();
  const info = useAppStore((s) => s.info);
  const settings = useAppStore((s) => s.settings);
  const health = useAppStore((s) => s.health);
  const save = useAppStore((s) => s.saveSettings);
  const updateReady = useAppStore((s) => s.updateReady);
  const setUpdateReady = useAppStore((s) => s.setUpdateReady);

  const [checking, setChecking] = useState(false);
  const [checkMsg, setCheckMsg] = useState<string | null>(null);

  if (!settings || !info) return <p className="text-fg-muted">{t("common.loading")}</p>;

  const appName = info.name;

  const checkUpdates = async () => {
    setChecking(true);
    setCheckMsg(null);
    try {
      const v = await api.checkForUpdate();
      if (v) {
        setUpdateReady(v);
        setCheckMsg(t("settings.update_staged", { version: v }));
      } else {
        setCheckMsg(t("settings.up_to_date"));
      }
    } catch (e) {
      const err = toIpcError(e);
      setCheckMsg(err.code === "updater_not_configured" ? t("settings.updater_disabled") : t("settings.check_failed"));
    } finally {
      setChecking(false);
    }
  };

  const erase = async () => {
    const ok = await ask(t("settings.reset_confirm"), { kind: "warning", title: appName });
    if (!ok) return;
    await api.eraseAllData();
    window.location.reload();
  };

  return (
    <>
      <h1 className="text-xl font-semibold">{t("settings.title")}</h1>

      <section className="card px-5 py-2">
        <h2 className="section-title pt-3">{t("settings.general")}</h2>
        <Row label={t("settings.language")}>
          <select
            aria-label={t("settings.language")}
            className="field w-44"
            value={settings.locale ?? ""}
            onChange={(e) => void save({ locale: e.target.value || null })}
          >
            <option value="">{t("settings.theme_system")}</option>
            {info.supported_locales.map((code) => (
              <option key={code} value={code}>
                {LOCALE_NAMES[code] ?? code}
              </option>
            ))}
          </select>
        </Row>
        <Row label={t("settings.theme")}>
          <select
            aria-label={t("settings.theme")}
            className="field w-44"
            value={settings.theme}
            onChange={(e) => void save({ theme: e.target.value as Theme })}
          >
            <option value="system">{t("settings.theme_system")}</option>
            <option value="light">{t("settings.theme_light")}</option>
            <option value="dark">{t("settings.theme_dark")}</option>
          </select>
        </Row>
        {config.features.autostart && (
          <Row label={t("settings.autostart")} hint={t("settings.autostart_hint", { app: appName })}>
            <Toggle
              label={t("settings.autostart")}
              checked={settings.autostart}
              onChange={(v) => void save({ autostart: v })}
            />
          </Row>
        )}
      </section>

      <section className="card px-5 py-2">
        <h2 className="section-title pt-3">{t("settings.privacy")}</h2>
        <Row label={t("settings.telemetry")} hint={t("settings.telemetry_hint")}>
          <Toggle
            label={t("settings.telemetry")}
            checked={settings.telemetry_enabled}
            onChange={(v) => void save({ telemetry_enabled: v })}
          />
        </Row>
      </section>

      <section className="card px-5 py-2">
        <h2 className="section-title pt-3">{t("settings.updates")}</h2>
        <Row label={t("app.version", { version: info.version })} hint={checkMsg ?? undefined}>
          {updateReady ? (
            <button type="button" className="btn btn-primary" onClick={() => void relaunch()}>
              {t("settings.restart")}
            </button>
          ) : (
            <button
              type="button"
              className="btn"
              disabled={checking || !health?.updater_configured}
              title={!health?.updater_configured ? t("settings.updater_disabled") : undefined}
              onClick={() => void checkUpdates()}
            >
              {checking ? t("settings.checking") : t("settings.check_now")}
            </button>
          )}
        </Row>
        {!health?.updater_configured && (
          <p className="text-xs text-fg-muted pb-3">{t("settings.updater_disabled")}</p>
        )}
      </section>

      <section className="card px-5 py-2">
        <h2 className="section-title pt-3">{t("settings.advanced")}</h2>
        <Row label={t("settings.open_logs")} hint={t("settings.data_location", { path: info.data_dir })}>
          <button type="button" className="btn" onClick={() => void api.openLogsFolder()}>
            {t("settings.open_logs")}
          </button>
        </Row>
        {info.debug && (
          <Row label={t("settings.test_crash")}>
            <button type="button" className="btn btn-danger" onClick={() => void api.testCrash()}>
              {t("settings.test_crash")}
            </button>
          </Row>
        )}
      </section>

      <section className="card px-5 py-2 border-danger/40">
        <h2 className="section-title pt-3 text-danger">{t("settings.danger")}</h2>
        <Row label={t("settings.reset")} hint={t("settings.reset_hint")}>
          <button type="button" className="btn btn-danger" onClick={() => void erase()}>
            {t("settings.reset")}
          </button>
        </Row>
      </section>
    </>
  );
}
