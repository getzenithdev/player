import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { bootstrapPreferences } from "./preferences";
import "./Preferences.css";

void bootstrapPreferences();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
