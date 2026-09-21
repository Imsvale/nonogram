import "./style.css";
import { startApp } from "./ui/app";

const root = document.getElementById("app");
if (!root) throw new Error("#app missing");

const app = startApp(root);

// Debug/test hook: lets browser tests read the layout and game without
// reaching into module internals. Read-only by convention.
(window as unknown as { __nonogram: unknown }).__nonogram = {
  layout: () => app.gridView.layout(),
  game: () => app.game(),
  settings: () => app.settings(),
  gridView: app.gridView,
};
