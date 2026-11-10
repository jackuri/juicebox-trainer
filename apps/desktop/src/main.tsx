import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { AgreementGate } from "./components/AgreementGate";
import { Overlay } from "./components/Overlay";
import "./index.css";
import { applyTheme, loadTheme, watchTheme } from "./lib/theme";

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing");

// The overlay window loads the same page with #overlay.
const overlay = window.location.hash === "#overlay";
if (overlay) document.documentElement.classList.add("overlay");

// The saved look, before the first paint; a theme picked in another window is followed.
applyTheme(loadTheme());
watchTheme();

createRoot(root).render(
  <StrictMode>
    {overlay ? (
      <Overlay />
    ) : (
      <AgreementGate>
        <App />
      </AgreementGate>
    )}
  </StrictMode>,
);
