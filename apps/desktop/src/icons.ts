const icon = (paths: string, className = "icon") =>
  `<svg class="${className}" viewBox="0 0 24 24" aria-hidden="true" focusable="false">${paths}</svg>`;

export const icons = {
  chevronDown: icon('<path d="m7 9.5 5 5 5-5"/>', "select-chevron"),
  disclosure: icon('<path d="m9 6 6 6-6 6"/>', "model-caret"),
  search: icon('<circle cx="11" cy="11" r="6.5"/><path d="m16 16 4 4"/>', "search-icon"),
  transcript: icon('<path d="M5 4.5h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-6l-4 3v-3H5a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2Z"/><path d="M7 9h10M7 13h7"/>'),
  appearance: icon('<path d="m14.5 4.5 5 5M13.5 5.5l2-2a2.1 2.1 0 0 1 3 0l2 2a2.1 2.1 0 0 1 0 3l-8.6 8.6"/><path d="M11.7 17.3a4.5 4.5 0 1 1-5-5 4.3 4.3 0 0 0 5 5Z"/>'),
  settings: icon('<circle cx="12" cy="12" r="3"/><path d="M12 2.5v3M12 18.5v3M21.5 12h-3M5.5 12h-3M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1M18.7 18.7l-2.1-2.1M7.4 7.4 5.3 5.3"/>'),
  refresh: icon('<path d="M20 7v5h-5"/><path d="M18.2 17A8 8 0 1 1 20 12"/>'),
  minimize: icon('<path d="M5 12h14"/>', "window-icon"),
  maximize: icon('<rect x="5" y="5" width="14" height="14" rx="1"/>', "window-icon"),
  close: icon('<path d="m6 6 12 12M18 6 6 18"/>', "window-icon")
};
