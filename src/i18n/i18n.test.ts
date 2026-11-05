import { describe, expect, it } from "vitest";

import { config } from "../lib/config";
import i18n, { isLocale, resources, setupI18n } from "./index";

function keys(obj: unknown, prefix = ""): string[] {
  if (obj && typeof obj === "object") {
    return Object.entries(obj as Record<string, unknown>).flatMap(([k, v]) =>
      keys(v, prefix ? `${prefix}.${k}` : k),
    );
  }
  return [prefix];
}

describe("locales", () => {
  it("every supported locale has every key the default locale has", () => {
    const base = keys(resources.en.translation);
    expect(base.length).toBeGreaterThan(50);
    for (const code of Object.keys(resources)) {
      const have = new Set(keys(resources[code as keyof typeof resources].translation));
      const missing = base.filter((k) => !have.has(k));
      expect(missing, `locale ${code} is missing keys`).toEqual([]);
    }
  });

  it("config lists only locales that exist", () => {
    for (const code of config.i18n.supported) expect(isLocale(code)).toBe(true);
    expect(isLocale(config.i18n.default)).toBe(true);
  });

  it("switches language and interpolates", async () => {
    await setupI18n("es");
    expect(i18n.t("menu.quit", { app: "Demo" })).toBe("Salir de Demo");
    await setupI18n("en");
    expect(i18n.t("menu.quit", { app: "Demo" })).toBe("Quit Demo");
    // Unknown locale falls back to the default instead of throwing.
    await setupI18n("zz");
    expect(i18n.language).toBe(config.i18n.default);
  });
});
