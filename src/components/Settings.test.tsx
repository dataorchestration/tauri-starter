import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import App from "../App";
import { appInfo, health, invoke, mockBackend, settings } from "../test/harness";

async function openSettings() {
  render(<App />);
  await screen.findByRole("heading", { name: "Hello, Desktop" });
  await userEvent.click(screen.getByRole("button", { name: "Settings" }));
  await screen.findByRole("heading", { name: "Settings" });
}

describe("Settings", () => {
  it("telemetry is off by default and opting in saves the setting", async () => {
    mockBackend();
    await openSettings();
    const toggle = screen.getByRole("switch", { name: "Share anonymous diagnostics" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    await userEvent.click(toggle);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: expect.objectContaining({ telemetry_enabled: true }),
      }),
    );
    expect(toggle).toHaveAttribute("aria-checked", "true");
  });

  it("changing the language re-renders the UI in that language", async () => {
    let locale = "en";
    mockBackend({
      app_info: () => ({ ...appInfo, locale }),
      save_settings: (a) => {
        const s = a?.settings as typeof settings;
        locale = s.locale ?? "en";
        return s;
      },
    });
    await openSettings();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Language" }), "es");
    expect(await screen.findByRole("heading", { name: "Ajustes" })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({ locale: "es" }),
    });
  });

  it("disables update checks until the updater is configured", async () => {
    mockBackend();
    await openSettings();
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled();
    expect(screen.getAllByText(/auto-updater is not configured/).length).toBeGreaterThan(0);
  });

  it("tells a failed check apart from being up to date", async () => {
    let fail = true;
    mockBackend({
      health_check: () => ({ ...health, updater_configured: true }),
      check_for_update: () => {
        if (fail) throw { code: "updater_failed", message: "offline" };
        return null;
      },
    });
    await openSettings();
    const btn = screen.getByRole("button", { name: "Check for updates" });
    await userEvent.click(btn);
    expect(await screen.findByText(/Couldn't check for updates/)).toBeInTheDocument();
    fail = false;
    await userEvent.click(btn);
    expect(await screen.findByText("You're on the latest version.")).toBeInTheDocument();
  });

  it("erasing data asks for confirmation first", async () => {
    mockBackend({ erase_all_data: () => null });
    const { ask } = await import("../test/harness");
    ask.mockResolvedValueOnce(false);
    await openSettings();
    await userEvent.click(screen.getByRole("button", { name: "Erase all local data" }));
    expect(ask).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("erase_all_data", undefined);
  });
});
