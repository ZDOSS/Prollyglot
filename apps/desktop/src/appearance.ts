import "./styles.css";

import { desktopBridge } from "./bridge";
import { AppearancePanel } from "./appearance-panel";
import { initializeConfiguration } from "./configuration";
import { icons } from "./icons";

const {
  closeAppearance,
  reportFrontendDiagnostic,
  startWindowDrag,
  windowAction
} = desktopBridge;

const root = document.querySelector<HTMLElement>("#appearance-app");
if (!root) throw new Error("missing appearance root");

root.innerHTML = `
  <section class="app-window appearance-window" aria-label="Caption appearance">
    <header class="titlebar compact-titlebar">
      <div class="brand compact-brand">
        <img class="brand-mark compact-mark" src="/branding/prollyglot-mark.png" alt="" />
        <span class="brand-name compact-name">Prollyglot</span>
      </div>
      <div class="window-controls" aria-label="Window controls">
        <button class="window-control" type="button" data-window-action="minimize" aria-label="Minimize">${icons.minimize}</button>
        <button class="window-control" type="button" data-window-action="maximize" aria-label="Maximize">${icons.maximize}</button>
        <button class="window-control close" type="button" data-window-action="close" aria-label="Close">${icons.close}</button>
      </div>
    </header>
    <div id="standalone-appearance-panel"></div>
  </section>
`;

let dismissing = false;

const configuration = await initializeConfiguration(
  desktopBridge,
  localStorage,
  (message) => {
    void reportFrontendDiagnostic("configuration", message);
  }
);

async function dismissAppearance(): Promise<void> {
  if (dismissing) return;
  dismissing = true;
  try {
    await closeAppearance();
  } catch (error) {
    console.error("Could not hide Appearance; closing the window instead.", error);
    try {
      await windowAction("close");
    } catch (closeError) {
      const message = closeError instanceof Error ? closeError.message : String(closeError);
      void reportFrontendDiagnostic("window-control", `close appearance: ${message}`);
    }
  } finally {
    dismissing = false;
  }
}

new AppearancePanel().render(
  document.querySelector<HTMLElement>("#standalone-appearance-panel")!,
  {
    settings: structuredClone(configuration.snapshot().config.overlay),
    onChange: (settings) => configuration.update((config) => {
      config.overlay = structuredClone(settings);
    }).then(() => undefined),
    showHeading: true,
    onDone: dismissAppearance
  }
);

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-window-action]")) {
  button.addEventListener("click", () => {
    const action = button.dataset.windowAction;
    if (action === "close") {
      void dismissAppearance();
    } else if (action === "minimize" || action === "maximize") {
      void windowAction(action).catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        void reportFrontendDiagnostic("window-control", `${action}: ${message}`);
      });
    }
  });
}

document.querySelector<HTMLElement>(".titlebar")!.addEventListener("mousedown", (event) => {
  if (event.button !== 0) return;
  const target = event.target;
  if (target instanceof Element && target.closest("button, input, select, a")) return;
  const action = event.detail === 2 ? "maximize" : "drag";
  const operation = event.detail === 2 ? windowAction("maximize") : startWindowDrag();
  void operation.catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    void reportFrontendDiagnostic("window-control", `${action}: ${message}`);
  });
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") void dismissAppearance();
});
