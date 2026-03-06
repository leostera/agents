import React from "react";
import { createRoot } from "react-dom/client";
import "@borg/ui/index.css";
import "./app.css";
import { App } from "./App";

const root = document.getElementById("root");
if (!root) {
  throw new Error("Missing #root element");
}

createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
