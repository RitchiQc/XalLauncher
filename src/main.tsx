import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter } from "react-router-dom";
import App from "./App";
import { LauncherProvider } from "./state/LauncherContext";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode><HashRouter><LauncherProvider><App /></LauncherProvider></HashRouter></React.StrictMode>,
);
