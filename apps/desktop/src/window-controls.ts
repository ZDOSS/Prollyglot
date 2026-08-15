export type WindowControlAction = "minimize" | "maximize" | "close";
export type TitlebarOperation = "drag" | "maximize";

export interface WindowControlElements {
  root: HTMLElement;
  titlebar: HTMLElement;
}

export interface WindowControlActions {
  startDrag: () => Promise<void>;
  perform: (action: WindowControlAction) => Promise<void>;
  reportError: (action: string, error: unknown) => void;
}

export function titlebarOperation(
  mouseButton: number,
  clickCount: number,
  interactiveTarget: boolean
): TitlebarOperation | undefined {
  if (mouseButton !== 0 || interactiveTarget) return undefined;
  return clickCount === 2 ? "maximize" : "drag";
}

export function bindWindowControls(
  elements: WindowControlElements,
  actions: WindowControlActions
): () => void {
  const abort = new AbortController();
  const options = { signal: abort.signal };

  elements.titlebar.addEventListener("mousedown", (event) => {
    const target = event.target;
    const operation = titlebarOperation(
      event.button,
      event.detail,
      target instanceof Element && Boolean(target.closest("button, input, select, a"))
    );
    if (!operation) return;
    const request = operation === "maximize"
      ? actions.perform("maximize")
      : actions.startDrag();
    void request.catch((error: unknown) => actions.reportError(operation, error));
  }, options);

  for (const button of elements.root.querySelectorAll<HTMLButtonElement>("[data-window-action]")) {
    button.addEventListener("click", () => {
      const action = button.dataset.windowAction;
      if (action === "minimize" || action === "maximize" || action === "close") {
        void actions.perform(action).catch((error: unknown) => actions.reportError(action, error));
      }
    }, options);
  }

  return () => abort.abort();
}
