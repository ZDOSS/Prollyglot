# Experimental Windows visual-translation smoke

Use this short check only when testing **Translate Screen…**. It is separate from
the ordinary audio-caption smoke and should take roughly five minutes after the
models are installed. No screenshots, recordings, evidence folder, or formal
timing sheet are required.

## 1. Launch from the repository

Open PowerShell in the repository root:

```powershell
Set-Location C:\github\Prollyglot
git pull --ff-only origin main
pnpm --dir apps/desktop tauri dev
```

## 2. Install only the route you will test

1. Open **Settings** and search for **OCR**.
2. Expand **PP-OCRv6 Small · Multilingual** with its caret and choose
   **Download**. The one-time download is 30.4 MiB.
3. Clear the search. Under **Translation models**, expand and download the route
   needed for the source and target languages you intend to test. Japanese to
   English uses the compact 109.4 MiB route if it is not already installed.
4. Close Settings.

Each row should show its own progress and final **Installed** state. Downloads
are explicit; installing the OCR pack does not install every translator.

## 3. Translate visible text

1. Open a browser or media player containing visible foreign-language text.
   Burned-in video subtitles, a title card, menu, sign, or game HUD is suitable.
2. In Prollyglot, choose **Translate Screen…**.
3. Choose **Application window**, select the browser/player window, set **Text on
   screen** and **Translate to**, then choose **Start Screen Translation**.
4. Continue normal playback. The selected source is watched continuously; the
   region selector is only a live crop, not a screenshot. When readable text
   remains visible across two samples, confirm the original text and its
   translation appear near the recognized source region. The overlay should not
   repeatedly recognize its own labels.
5. Change scenes or move the selected window. Confirm stale labels disappear and
   current labels follow the source rather than creating an ever-growing queue.
6. Choose **Stop Screen Translation**. The app should return to setup and remain
   responsive.

If application-window capture is blank, stop and retry once with **Whole
display**. If only part of the screen matters, choose **Selected region**, draw a
box at least 80 × 60 pixels around it, and start again. The current slice uses
Windows Graphics Capture for all three choices; the planned DXGI display
fallback is not implemented yet.

Audio captions and screen translation intentionally do not run together in this
slice. Starting screen translation while captions are live should explicitly
stop captions first rather than silently competing for resources.

## Result to send back

A passing report can be one sentence: source type, language route, and whether
the labels were readable and stayed attached. For a failure, send the source
type, what was visible, and the exact message in the app. A screenshot or the
privacy-safe diagnostic log is useful only if we need it to troubleshoot that
specific problem.
