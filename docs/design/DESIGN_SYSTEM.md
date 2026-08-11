# Prollyglot design system

This file translates the accepted Milestone 1 visual concepts into implementation constraints. The concepts are references, not assets to display as application UI.

## Concept references

- `docs/design/concepts/main-window.png` — primary control window and default state.
- `docs/design/concepts/appearance-and-overlay.png` — appearance controls and live overlay preview.
- `assets/branding/prollyglot-mark.png` — transparent application mark generated to match the primary concept.
- `assets/branding/prollyglot-logo.png` — wide transparent logo supplied for the README header; keep the compact mark in constrained application surfaces.

## Visual direction

Prollyglot is a quiet accessibility utility: minimal by default, customizable by choice. Its interface is dark, open, typographically disciplined, and intentionally free of dashboard clutter. Mint indicates action, focus, and live state; it is not decorative ambient color.

The concepts use a true dark-neutral palette, not navy, brown, or softened off-white. Do not add gradients, glass blur, neon glow, nested cards, decorative waveforms, badges, or AI imagery.

## Tokens

```css
:root {
  --color-bg: #101518;
  --color-bg-deep: #0b0f12;
  --color-surface: #181e22;
  --color-surface-raised: #22282d;
  --color-surface-hover: #293137;
  --color-border: #3a4349;
  --color-border-strong: #566168;
  --color-text: #f4f6f5;
  --color-text-muted: #aeb7bd;
  --color-text-faint: #7f8a91;
  --color-accent: #86e3b0;
  --color-accent-strong: #68d79c;
  --color-accent-soft: #223c31;
  --color-on-accent: #07130d;
  --color-danger: #f27d7d;
  --color-warning: #e4bd72;

  --font-ui: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif;
  --font-caption: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif;

  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;
  --space-6: 32px;
  --space-7: 40px;

  --radius-sm: 8px;
  --radius-md: 12px;
  --radius-lg: 16px;

  --control-height: 52px;
  --border-width: 1px;
  --focus-width: 3px;
  --motion-fast: 120ms;
  --motion-standard: 180ms;
}
```

## Typography

- Product name: 25px, 650 weight, 1.15 line height.
- Screen title: 30px, 600 weight, 1.2 line height.
- Field label: 14px, 550 weight, muted color, 1.35 line height.
- Control value: 16px, 500 weight, 1.35 line height.
- Primary action: 16px, 650 weight.
- Utility action: 14px, 550 weight.
- Status: 14px, 600 weight; mint only for healthy Ready/Live states.
- Default overlay caption: 36px, 600 weight, 1.25 line height.
- Default post-speech reading time: 15 seconds after the newest final or translated text, followed by an 800ms fade.

App chrome uses the Windows system UI font. Caption font is user-selectable later and must never change the app chrome font.

## Window anatomy

### Main window

- Target logical size: 440 × 620px; resizable only when content would otherwise clip.
- One quiet title row with mark, product name, status, and native window controls.
- Open vertical field flow with 24px side gutters and 24-32px section rhythm.
- One full-width Start/Stop button.
- Bottom utility navigation is a single rail, not three cards.

### Appearance window

- Target logical size: 900 × 620px.
- Left control column around 320px; remaining width is a live preview canvas.
- Settings are labeled rows on the page background, not nested panels.
- Reset is secondary; Done is the only primary action.

### Overlay window

- Independent, borderless, transparent, always-on-top window.
- Default caption surface: 720px maximum width, two lines, bottom center, 36px text, 75% near-black background.
- Caption box uses 20px horizontal and 14px vertical padding with a 12px radius.
- No window chrome, status decoration, source label, waveform, or animation that competes with reading.
- Click-through must not take focus. Unlocking position temporarily restores interaction and exposes a clear drag affordance.

### Translate Screen surface

- Opens from a separate secondary action; it does not add capture/OCR controls to the audio Start/Stop path.
- Uses the same open field flow as the main window: source type, explicit source, text language, target language, required local models, then one Start/Stop action.
- Required OCR and translation packs appear as compact readiness rows with one explicit Download/Repair action each. The full catalog remains in Settings.
- Application window, whole display, and selected region are mutually exclusive source choices. Region selection uses a full-screen dimmed drag surface with visible dimensions, Escape, and Cancel.
- Active state replaces setup with the selected source, privacy-safe frame/label counters, recovery text, and one Stop action.
- Audio captions and screen translation cannot be active simultaneously in the experimental slice; the switching action names both effects before it runs.

### Visual translation overlay

- Independent, borderless, transparent, always-on-top, and click-through.
- Each recognized source region owns one stable original/translation pair; partial updates must not displace another region's translation.
- Original text is smaller and neutral; translated text uses the configured translation color and appears above the source when possible, otherwise below it.
- Labels wrap and clamp to the captured bounds. They must not use ellipses that hide either language.
- Prollyglot windows request exclusion from display capture to prevent an OCR feedback loop.

## Component families

- `SelectField`: label, full-width value surface, custom chevron, hover/focus/disabled states.
- `PrimaryButton`: mint fill, dark label, strong focus ring, busy and Stop variants.
- `UtilityNav`: three text actions with consistent 20px outline icons.
- `StatusLabel`: Ready, Live, Waiting, and Error variants without a pill container.
- `SettingRow`: label/value alignment for select, slider, color, and toggle controls.
- `ModelDisclosure`: searchable grouped list row with a plain-text state,
  rotating outline caret, collapsed name/language/size summary, and expanded
  facts plus model-specific actions. It is a disclosure list, not a card grid.
- `Toggle`: compact track and thumb with a visible focus ring.
- `CaptionSurface`: provisional/final text treatment inside the overlay.
- `VisualReadinessRow`: model name, installed/download size state, and one route-specific action without exposing the entire catalog.
- `VisualLabel`: positioned original/translation pair anchored to one recognized screen region.

## Icon inventory

- Brand: generated Prollyglot speech-caption waveform mark.
- Select chevron: 16px, 1.75px stroke, rounded caps and joins.
- Transcript: outline speech rectangle with two short text lines.
- Appearance: outline brush.
- Settings: outline gear.
- Window controls: use native Tauri/Windows behavior instead of custom decorative glyphs where possible.

All utility icons use the same 1.75px optical stroke and `currentColor`. Do not mix filled and outline icon families.

## Visible-copy lock

Primary screen copy:

- Prollyglot
- Ready
- Audio source
- Everything I hear
- Playback device
- Speakers (Realtek(R) Audio)
- Spoken language
- English
- Translate to
- Off · original language
- Caption output
- Original language
- Start Captions
- Transcript
- Appearance
- Settings

Appearance screen copy:

- Appearance
- Caption style
- Font
- Inter
- Size
- 36 px
- Text color
- Background opacity
- 75%
- Width
- 720 px
- Maximum lines
- 2
- Keep after speech
- 15 seconds
- Fade out
- Gentle · 0.8 sec
- Position
- Bottom center
- Click-through
- Reset
- Done
- We should be there in about ten minutes.

Runtime source names and status/error messages may replace sample values. Do not invent explanatory marketing copy above the primary action.

## Interaction and accessibility

- Every control has a visible `:focus-visible` state using the mint focus color and a minimum 3px outer ring.
- Minimum pointer target is 40 × 40px; primary controls use the 52px control height.
- Text and essential control boundaries meet WCAG AA contrast.
- All icons have text labels or accessible names; the brand mark is decorative beside the visible product name.
- Model disclosure buttons expose expanded state and panel ownership; progress
  updates retain the open rows, scroll position, and a usable focus target.
- Search filters every model family by model name, language, route, and state;
  an empty result explains how to clear or broaden the query.
- A visual source-language choice describes translation routing accurately; it
  must not claim to tune OCR unless the selected OCR backend actually uses it.
- Motion only clarifies state and respects `prefers-reduced-motion`.
- Error states use text plus color. Ready/Live state must not rely on a green dot alone.

## Fidelity constraints

- Preserve the open layout and restrained border system.
- Preserve dark-neutral background and mint accent semantics.
- Do not convert fields or settings into a card grid.
- Large model catalogs use search, semantic grouping, and disclosure rows rather
  than permanently expanded model cards or a dense matrix of download buttons.
- Do not add a sidebar, onboarding carousel, account menu, cloud status, fake audio waveform, or model-performance metrics.
- Keep customization out of the primary Start/Stop flow.
