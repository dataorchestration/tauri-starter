import { useEffect } from "react";
import { useTranslation } from "react-i18next";

import { Hello } from "./components/Hello";
import { Notifications } from "./components/Notifications";
import { Permissions } from "./components/Permissions";
import { Settings } from "./components/Settings";
import { Shell } from "./components/Shell";
import i18n from "./i18n";
import { onLocaleChanged, onNavigate, onNotification, onUpdateReady } from "./lib/events";
import { useAppStore } from "./store/app";

export default function App() {
  const { t } = useTranslation();
  const ready = useAppStore((s) => s.ready);
  const view = useAppStore((s) => s.view);
  const bootstrap = useAppStore((s) => s.bootstrap);
  const setView = useAppStore((s) => s.setView);
  const setUpdateReady = useAppStore((s) => s.setUpdateReady);
  const loadNotifications = useAppStore((s) => s.loadNotifications);

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  // Core → UI events: menu/tray navigation, staged updates, language, new
  // notifications. One subscription each, torn down on unmount.
  useEffect(() => {
    const subs = [
      onNavigate(setView),
      onUpdateReady(setUpdateReady),
      onLocaleChanged((locale) => void i18n.changeLanguage(locale)),
      onNotification(() => void loadNotifications()),
    ];
    return () => {
      for (const s of subs) void s.then((un) => un());
    };
  }, [setView, setUpdateReady, loadNotifications]);

  if (!ready) {
    return (
      <div className="h-full grid place-items-center text-fg-muted" role="status">
        {t("common.loading")}
      </div>
    );
  }

  return (
    <Shell>
      {view === "home" && <Hello />}
      {view === "settings" && <Settings />}
      {view === "notifications" && <Notifications />}
      {view === "permissions" && <Permissions />}
    </Shell>
  );
}
