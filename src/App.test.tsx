import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import App from "./App";
import { EVENTS } from "./lib/events";
import { emit, health, invoke, mockBackend, relaunch, settings } from "./test/harness";

describe("App shell", () => {
  it("boots, greets, and shows a healthy status", async () => {
    mockBackend();
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Hello, Desktop" })).toBeInTheDocument();
    expect(screen.getAllByText("Version 0.1.0").length).toBeGreaterThan(0);
    expect(screen.getByText("All checks passed")).toBeInTheDocument();
    expect(screen.getByText("macos · aarch64")).toBeInTheDocument();
    // Nothing was sent anywhere: only reads happened.
    expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
  });

  it("navigates from the sidebar and from a core navigate event", async () => {
    mockBackend();
    render(<App />);
    await screen.findByRole("heading", { name: "Hello, Desktop" });

    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();

    emit(EVENTS.navigate, { view: "notifications" });
    expect(await screen.findByRole("heading", { name: "Notifications" })).toBeInTheDocument();
  });

  it("shows the update banner on the core event and restarts", async () => {
    mockBackend();
    render(<App />);
    await screen.findByRole("heading", { name: "Hello, Desktop" });

    emit(EVENTS.updateReady, "0.2.0");
    expect(await screen.findByText("Update 0.2.0 is ready")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Restart now" }));
    expect(relaunch).toHaveBeenCalled();
  });

  it("offers to send a crash report left by the previous run", async () => {
    const report = {
      id: "c1",
      ts_ms: 1,
      app_version: "0.0.9",
      os: "macos aarch64",
      message: "index out of bounds",
      location: "src/x.rs:1:1",
      thread: "main",
      backtrace: "frame 0",
    };
    let pending = [report];
    mockBackend({
      pending_crashes: () => pending,
      send_crash: () => {
        pending = [];
        return true;
      },
    });
    render(<App />);
    expect(await screen.findByText(/quit unexpectedly last time/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Send report" }));
    expect(invoke).toHaveBeenCalledWith("send_crash", { id: "c1" });
    expect(await screen.findByText("Thanks — report sent.")).toBeInTheDocument();
  });

  it("starts on the permissions view when a required permission is missing", async () => {
    mockBackend({
      get_settings: () => ({ ...settings, onboarding_done: false }),
      health_check: () => ({
        ...health,
        ok: false,
        permissions: [
          {
            permission: "screen_recording",
            state: "denied",
            required: true,
            can_request: false,
            can_open_settings: true,
          },
        ],
        missing_required: ["screen_recording"],
      }),
    });
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Permissions" })).toBeInTheDocument();
    expect(screen.getByText("Screen Recording")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("surfaces an IPC error from the core", async () => {
    mockBackend({
      health_check: () => {
        throw { code: "db", message: "database is locked" };
      },
    });
    render(<App />);
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("database is locked"));
  });
});
