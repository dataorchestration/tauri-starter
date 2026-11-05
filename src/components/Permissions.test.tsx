import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import App from "../App";
import type { HealthReport, PermissionReport } from "../lib/ipc";
import { health, invoke, mockBackend } from "../test/harness";

const denied: PermissionReport = {
  permission: "screen_recording",
  state: "denied",
  required: true,
  can_request: false,
  can_open_settings: true,
};
const undetermined: PermissionReport = {
  permission: "microphone",
  state: "undetermined",
  required: false,
  can_request: true,
  can_open_settings: true,
};

function withPermissions(perms: PermissionReport[]): HealthReport {
  return {
    ...health,
    ok: false,
    permissions: perms,
    missing_required: perms.filter((p) => p.required && p.state !== "granted").map((p) => p.permission),
  };
}

describe("Permissions", () => {
  it("lists required and optional permissions with their state", async () => {
    mockBackend({ health_check: () => withPermissions([denied, undetermined]) });
    render(<App />);
    await screen.findByRole("heading", { name: "Hello, Desktop" });
    await userEvent.click(within(screen.getByRole("navigation", { name: "Primary" })).getByRole("button", { name: /Permissions/ }));

    expect(await screen.findByText("Required")).toBeInTheDocument();
    expect(screen.getByText("Optional")).toBeInTheDocument();
    expect(screen.getByText("Screen Recording")).toBeInTheDocument();
    expect(screen.getByText("Denied")).toBeInTheDocument();
    expect(screen.getByText("Microphone")).toBeInTheDocument();
    expect(screen.getByText("Not yet asked")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("requests a prompt-able permission and deep-links a denied one", async () => {
    mockBackend({
      health_check: () => withPermissions([denied, undetermined]),
      request_permission: () => "granted",
      open_permission_settings: () => true,
    });
    render(<App />);
    await screen.findByRole("heading", { name: "Hello, Desktop" });
    await userEvent.click(within(screen.getByRole("navigation", { name: "Primary" })).getByRole("button", { name: /Permissions/ }));
    await screen.findByText("Required");

    await userEvent.click(screen.getByRole("button", { name: "Request Microphone" }));
    expect(invoke).toHaveBeenCalledWith("request_permission", { permission: "microphone" });
    // Denied on macOS: no prompt possible, only System Settings.
    expect(screen.queryByRole("button", { name: "Request Screen Recording" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Open System Settings Screen Recording" }));
    expect(invoke).toHaveBeenCalledWith("open_permission_settings", { permission: "screen_recording" });
  });

  it("continue marks onboarding done once everything required is granted", async () => {
    mockBackend({
      health_check: () => withPermissions([{ ...denied, state: "granted" }]),
      get_settings: () => ({
        locale: null,
        theme: "system",
        telemetry_enabled: false,
        autostart: false,
        device_id: "d",
        onboarding_done: false,
      }),
    });
    render(<App />);
    await screen.findByRole("heading", { name: "Hello, Desktop" });
    await userEvent.click(within(screen.getByRole("navigation", { name: "Primary" })).getByRole("button", { name: /Permissions/ }));
    await screen.findByText("Everything this app needs is granted.");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({ onboarding_done: true }),
    });
    expect(await screen.findByRole("heading", { name: "Hello, Desktop" })).toBeInTheDocument();
  });
});
