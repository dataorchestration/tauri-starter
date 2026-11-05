import { describe, expect, it } from "vitest";

import { api, IpcError, toIpcError } from "./ipc";
import { invoke } from "../test/harness";

describe("toIpcError", () => {
  it("keeps the core's code/message shape", () => {
    const e = toIpcError({ code: "db_wrong_key", message: "bad key" });
    expect(e).toBeInstanceOf(IpcError);
    expect(e.code).toBe("db_wrong_key");
    expect(e.message).toBe("bad key");
  });

  it("wraps plain errors and strings", () => {
    expect(toIpcError(new Error("boom")).code).toBe("unknown");
    expect(toIpcError("nope").message).toBe("nope");
    expect(toIpcError({ odd: 1 }).message).toContain("odd");
  });

  it("passes IpcError through unchanged", () => {
    const e = new IpcError("x", "y");
    expect(toIpcError(e)).toBe(e);
  });
});

describe("api", () => {
  it("rethrows invoke failures as IpcError", async () => {
    invoke.mockRejectedValueOnce({ code: "keychain", message: "locked" });
    await expect(api.getSettings()).rejects.toMatchObject({ code: "keychain", message: "locked" });
  });

  it("passes arguments with the Rust parameter names", async () => {
    invoke.mockResolvedValue(true);
    await api.markNotificationRead(7);
    expect(invoke).toHaveBeenCalledWith("mark_notification_read", { id: 7, read: true });
    await api.requestPermission("camera");
    expect(invoke).toHaveBeenCalledWith("request_permission", { permission: "camera" });
  });
});
