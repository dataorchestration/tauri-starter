// i18next setup. Locale JSON lives in ./locales and is shared with the Rust
// core (menu labels, native notifications), so one file per language covers
// the whole app. To add a language: add locales/<code>.json, import it here,
// include it in starter.config.json → i18n.supported, and `include_str!` it in
// src-tauri/src/i18n.rs.

import i18next from "i18next";
import { initReactI18next } from "react-i18next";

import { config } from "../lib/config";
import en from "./locales/en.json";
import es from "./locales/es.json";

export const resources = {
  en: { translation: en },
  es: { translation: es },
} as const;

export type Locale = keyof typeof resources;

export const LOCALE_NAMES: Record<string, string> = {
  en: "English",
  es: "Español",
};

export function isLocale(code: string): code is Locale {
  return code in resources;
}

let initialised = false;

export async function setupI18n(locale: string = config.i18n.default): Promise<void> {
  const lng = isLocale(locale) ? locale : config.i18n.default;
  if (initialised) {
    await i18next.changeLanguage(lng);
    return;
  }
  initialised = true;
  await i18next.use(initReactI18next).init({
    resources,
    lng,
    fallbackLng: config.i18n.default,
    interpolation: { escapeValue: false },
    returnNull: false,
  });
}

export default i18next;
