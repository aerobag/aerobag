import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { debugLog } from "./domain/debugLog";
import "./styles.css";

debugLog("START", {
  href: typeof window !== "undefined" ? window.location.href : "",
  userAgent: typeof navigator !== "undefined" ? navigator.userAgent : "",
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
