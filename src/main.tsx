import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { setupI18n } from "./i18n";
import { config } from "./lib/config";
import { installGlobalErrorHandlers } from "./lib/log";
import { applyTheme } from "./lib/theme";
import "./index.css";

installGlobalErrorHandlers();
// Paint with sensible defaults immediately; the store re-applies the user's
// saved locale/theme once it has loaded them from the core.
applyTheme(config.ui.defaultTheme);

void setupI18n().then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
