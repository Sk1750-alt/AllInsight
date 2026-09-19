import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

// The context menu belongs to a browser, not to a desktop utility. Text
// selection is still available where it is useful.
window.addEventListener("contextmenu", (event) => {
  const target = event.target as HTMLElement | null;
  const editable =
    target?.closest("input, textarea, [data-selectable]") !== null &&
    target?.closest("input, textarea, [data-selectable]") !== undefined;
  if (!editable) event.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
