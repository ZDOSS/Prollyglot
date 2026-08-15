import type { AppDestination, AppViewMode } from "./app-store";

export type WorkspacePanel = Exclude<AppDestination, "captions">;

export interface WorkspaceNavigationState {
  viewMode: AppViewMode;
  destination: AppDestination;
  mountedPages: ReadonlySet<AppDestination>;
  compactPanel?: WorkspacePanel;
}

export type WorkspaceNavigationEvent =
  | { type: "navigate"; destination: AppDestination }
  | { type: "view-mode"; viewMode: AppViewMode };

export function initialWorkspaceNavigation(viewMode: AppViewMode): WorkspaceNavigationState {
  return {
    viewMode,
    destination: "captions",
    mountedPages: new Set<AppDestination>(["captions"])
  };
}

export function reduceWorkspaceNavigation(
  state: WorkspaceNavigationState,
  event: WorkspaceNavigationEvent
): WorkspaceNavigationState {
  if (event.type === "view-mode") {
    return {
      viewMode: event.viewMode,
      destination: "captions",
      mountedPages: new Set(state.mountedPages)
    };
  }

  const mountedPages = new Set(state.mountedPages);
  if (state.viewMode === "full") mountedPages.add(event.destination);
  return {
    viewMode: state.viewMode,
    destination: event.destination,
    mountedPages,
    compactPanel: state.viewMode === "compact" && event.destination !== "captions"
      ? event.destination
      : undefined
  };
}

interface PanelCopy {
  title: string;
  subtitle: string;
}

const PANEL_COPY: Record<WorkspacePanel, PanelCopy> = {
  transcript: {
    title: "Transcript",
    subtitle: "Follow the newest caption by default or scroll back without losing your place."
  },
  models: {
    title: "Models",
    subtitle: "Manage installed packs and choose compatible local models by language."
  },
  settings: {
    title: "Settings",
    subtitle: "Application, source, and privacy controls."
  },
  visual: {
    title: "Screen translation",
    subtitle: "Continuously recognize and translate text in a window, display, or selected region."
  },
  appearance: {
    title: "Appearance",
    subtitle: "Customize readable captions and preview changes as you make them."
  }
};

export interface WorkspaceNavigationElements {
  root: HTMLElement;
  dialog: HTMLDialogElement;
  dialogContent: HTMLElement;
  dialogTitle: HTMLElement;
  dialogSubtitle: HTMLElement;
  dialogClose: HTMLButtonElement;
}

export interface WorkspaceRenderContext {
  firstMount: boolean;
  forceLatest: boolean;
}

export type WorkspacePanelRenderer = (
  panel: WorkspacePanel,
  content: HTMLElement,
  context: WorkspaceRenderContext
) => void;

export interface WorkspaceNavigationActions {
  renderPanel: WorkspacePanelRenderer;
  onDestinationChange: (destination: AppDestination) => void;
}

export class WorkspaceNavigation {
  private state: WorkspaceNavigationState;
  private readonly elements: WorkspaceNavigationElements;
  private readonly actions: WorkspaceNavigationActions;
  private readonly pages = new Map<AppDestination, HTMLElement>();
  private readonly content = new Map<WorkspacePanel, HTMLElement>();
  private readonly lastFocus = new Map<AppDestination, HTMLElement>();
  private dialogOpener?: HTMLElement;
  private suppressDialogClose = false;

  constructor(
    elements: WorkspaceNavigationElements,
    actions: WorkspaceNavigationActions,
    viewMode: AppViewMode
  ) {
    this.elements = elements;
    this.actions = actions;
    this.state = initialWorkspaceNavigation(viewMode);
    for (const page of elements.root.querySelectorAll<HTMLElement>("[data-workspace-page]")) {
      const destination = destinationFrom(page.dataset.workspacePage);
      if (!destination) continue;
      this.pages.set(destination, page);
      if (destination !== "captions") {
        const panelContent = page.querySelector<HTMLElement>(`[data-workspace-content="${destination}"]`);
        if (!panelContent) throw new Error(`missing workspace content: ${destination}`);
        this.content.set(destination, panelContent);
      }
    }
    if (!this.pages.has("captions")) throw new Error("missing captions workspace");

    elements.dialogClose.addEventListener("click", () => elements.dialog.close());
    elements.dialog.addEventListener("click", (event) => {
      if (this.state.viewMode === "compact" && event.target === elements.dialog) {
        elements.dialog.close();
      }
    });
    elements.dialog.addEventListener("close", () => {
      if (this.suppressDialogClose) return;
      const opener = this.dialogOpener;
      this.dialogOpener = undefined;
      this.navigate("captions", { moveFocus: false });
      opener?.focus({ preventScroll: true });
    });
    this.applyPages("captions");
    this.applyNavigation("captions");
  }

  snapshot(): WorkspaceNavigationState {
    return {
      ...this.state,
      mountedPages: new Set(this.state.mountedPages)
    };
  }

  navigate(
    destination: AppDestination,
    options: { opener?: HTMLElement; moveFocus?: boolean } = {}
  ): void {
    this.rememberFocus(this.state.destination);
    const previousMounted = this.state.mountedPages.has(destination);
    this.state = reduceWorkspaceNavigation(this.state, { type: "navigate", destination });

    if (this.state.viewMode === "full") {
      this.closeDialogSilently();
      this.applyPages(destination);
      if (destination !== "captions" && !previousMounted) {
        this.actions.renderPanel(destination, this.requireContent(destination), {
          firstMount: true,
          forceLatest: destination === "transcript"
        });
      }
      if (options.moveFocus !== false) this.restoreFocus(destination);
    } else {
      this.applyPages("captions");
      if (destination === "captions") {
        this.closeDialogSilently();
      } else {
        this.openCompactPanel(destination, options.opener);
      }
    }

    this.applyNavigation(destination);
    this.actions.onDestinationChange(destination);
  }

  setViewMode(viewMode: AppViewMode): void {
    this.rememberFocus(this.state.destination);
    this.closeDialogSilently();
    this.state = reduceWorkspaceNavigation(this.state, { type: "view-mode", viewMode });
    this.applyPages("captions");
    this.applyNavigation("captions");
    this.actions.onDestinationChange("captions");
  }

  refresh(panel: WorkspacePanel, forceLatest = false): void {
    if (this.state.mountedPages.has(panel)) {
      this.actions.renderPanel(panel, this.requireContent(panel), {
        firstMount: false,
        forceLatest
      });
    }
    if (
      this.state.viewMode === "compact"
      && this.elements.dialog.open
      && this.state.compactPanel === panel
    ) {
      this.actions.renderPanel(panel, this.elements.dialogContent, {
        firstMount: false,
        forceLatest
      });
    }
  }

  isMounted(panel: WorkspacePanel): boolean {
    return this.state.mountedPages.has(panel);
  }

  isVisible(panel: WorkspacePanel): boolean {
    return this.state.destination === panel;
  }

  private openCompactPanel(panel: WorkspacePanel, opener?: HTMLElement): void {
    const copy = PANEL_COPY[panel];
    this.dialogOpener = opener ?? (
      document.activeElement instanceof HTMLElement ? document.activeElement : undefined
    );
    this.elements.dialog.dataset.panel = panel;
    this.elements.dialogTitle.textContent = copy.title;
    this.elements.dialogSubtitle.textContent = copy.subtitle;
    this.actions.renderPanel(panel, this.elements.dialogContent, {
      firstMount: true,
      forceLatest: panel === "transcript"
    });
    if (!this.elements.dialog.open) this.elements.dialog.showModal();
  }

  private requireContent(panel: WorkspacePanel): HTMLElement {
    const content = this.content.get(panel);
    if (!content) throw new Error(`missing persistent page content: ${panel}`);
    return content;
  }

  private applyPages(destination: AppDestination): void {
    for (const [name, page] of this.pages) {
      const visible = name === destination;
      page.hidden = !visible;
      page.inert = !visible;
      if (visible) page.removeAttribute("aria-hidden");
      else page.setAttribute("aria-hidden", "true");
    }
  }

  private applyNavigation(destination: AppDestination): void {
    for (const button of this.elements.root.querySelectorAll<HTMLButtonElement>(".desktop-nav-action")) {
      const selected = button.dataset.destination === destination;
      button.classList.toggle("is-active", selected);
      if (selected) button.setAttribute("aria-current", "page");
      else button.removeAttribute("aria-current");
    }
  }

  private rememberFocus(destination: AppDestination): void {
    const active = document.activeElement;
    const page = this.pages.get(destination);
    if (active instanceof HTMLElement && page?.contains(active)) {
      this.lastFocus.set(destination, active);
    }
  }

  private restoreFocus(destination: AppDestination): void {
    const page = this.pages.get(destination);
    if (!page) return;
    const previous = this.lastFocus.get(destination);
    const target = previous?.isConnected && page.contains(previous)
      ? previous
      : page.querySelector<HTMLElement>("h1");
    target?.focus({ preventScroll: true });
  }

  private closeDialogSilently(): void {
    if (!this.elements.dialog.open) return;
    this.suppressDialogClose = true;
    this.elements.dialog.close();
    this.suppressDialogClose = false;
    this.dialogOpener = undefined;
    this.state = { ...this.state, compactPanel: undefined };
  }
}

export function destinationFrom(value: string | undefined): AppDestination | undefined {
  if (
    value === "captions"
    || value === "visual"
    || value === "transcript"
    || value === "models"
    || value === "appearance"
    || value === "settings"
  ) return value;
  return undefined;
}
