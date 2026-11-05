// Frontend logging. Everything goes to the core (`log_event`), which writes the
// rolling file log and — only if the user opted in — forwards to the remote
// sinks. In dev it also mirrors to the devtools console. Fire-and-forget: a
// failing log call must never break the UI.

import { api, type LogLevel } from "./ipc";

function ship(level: LogLevel, name: string, message: string, fields?: Record<string, unknown>) {
  if (import.meta.env.DEV) {
    const fn = level === "debug" ? console.debug : level === "info" ? console.info : level === "warn" ? console.warn : console.error;
    fn(`[${name}] ${message}`, fields ?? "");
  }
  api.logEvent(level, name, message, fields).catch(() => {});
}

export const log = {
  debug: (message: string, fields?: Record<string, unknown>, name = "ui") => ship("debug", name, message, fields),
  info: (message: string, fields?: Record<string, unknown>, name = "ui") => ship("info", name, message, fields),
  warn: (message: string, fields?: Record<string, unknown>, name = "ui") => ship("warn", name, message, fields),
  error: (message: string, fields?: Record<string, unknown>, name = "ui") => ship("error", name, message, fields),
};

/** Route uncaught errors and unhandled promise rejections to the crash/error
 *  log in the core. Call once from main.tsx. */
export function installGlobalErrorHandlers(): void {
  window.addEventListener("error", (ev) => {
    api
      .reportFrontendError({
        message: ev.message || String(ev.error ?? "unknown error"),
        stack: ev.error instanceof Error ? ev.error.stack ?? "" : "",
        source: `${ev.filename ?? ""}:${ev.lineno ?? 0}:${ev.colno ?? 0}`,
      })
      .catch(() => {});
  });
  window.addEventListener("unhandledrejection", (ev) => {
    const reason = ev.reason;
    api
      .reportFrontendError({
        message: reason instanceof Error ? reason.message : String(reason),
        stack: reason instanceof Error ? reason.stack ?? "" : "",
        source: "unhandledrejection",
      })
      .catch(() => {});
  });
}
