import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { Overlay } from "./components/Overlay";
import "./index.css";

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing");

// The overlay window loads the same page with #overlay.
const overlay = window.location.hash === "#overlay";
if (overlay) document.documentElement.classList.add("overlay");

createRoot(root).render(<StrictMode>{overlay ? <Overlay /> : <App />}</StrictMode>);
