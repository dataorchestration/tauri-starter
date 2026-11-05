import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import App from "../App";
import type { AppNotification } from "../lib/ipc";
import { invoke, mockBackend } from "../test/harness";

const items: AppNotification[] = [
  { id: 2, title: "Build finished", body: "All green", kind: "success", read: false, created_at: 2 },
  { id: 1, title: "Welcome", body: "", kind: "info", read: true, created_at: 1 },
];

async function openNotifications() {
  render(<App />);
  await screen.findByRole("heading", { name: "Hello, Desktop" });
  await userEvent.click(within(screen.getByRole("navigation", { name: "Primary" })).getByRole("button", { name: /^Notifications/ }));
  await screen.findByRole("heading", { name: "Notifications" });
}

describe("Notifications", () => {
  it("lists notifications newest first with the unread count", async () => {
    mockBackend({ list_notifications: () => items });
    await openNotifications();
    expect(screen.getByText("Build finished")).toBeInTheDocument();
    expect(screen.getByText("Welcome")).toBeInTheDocument();
    expect(screen.getByText("1 unread")).toBeInTheDocument();
  });

  it("marks all read, deletes one, and sends a native test notification", async () => {
    let list = [...items];
    mockBackend({
      list_notifications: () => list,
      mark_all_notifications_read: () => {
        list = list.map((n) => ({ ...n, read: true }));
        return 1;
      },
      delete_notification: (a) => {
        list = list.filter((n) => n.id !== a?.id);
        return true;
      },
      add_notification: (a) => {
        const n = a?.notification as { title: string; body?: string };
        const created: AppNotification = {
          id: 3,
          title: n.title,
          body: n.body ?? "",
          kind: "success",
          read: false,
          created_at: 3,
        };
        list = [created, ...list];
        return created;
      },
    });
    await openNotifications();

    await userEvent.click(screen.getByRole("button", { name: "Mark all as read" }));
    expect(invoke).toHaveBeenCalledWith("mark_all_notifications_read", undefined);
    await waitFor(() => expect(screen.queryByText("1 unread")).not.toBeInTheDocument());

    await userEvent.click(screen.getByRole("button", { name: "Delete Welcome" }));
    expect(invoke).toHaveBeenCalledWith("delete_notification", { id: 1 });
    await waitFor(() => expect(screen.queryByText("Welcome")).not.toBeInTheDocument());

    await userEvent.click(screen.getByRole("button", { name: "Send a test notification" }));
    expect(invoke).toHaveBeenCalledWith("add_notification", {
      notification: expect.objectContaining({ title: "Hello from Hello Desktop", native: true }),
    });
    expect(await screen.findByText("Hello from Hello Desktop")).toBeInTheDocument();
  });

  it("shows an empty state", async () => {
    mockBackend();
    await openNotifications();
    expect(screen.getByText("No notifications yet.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear all" })).toBeDisabled();
  });
});
