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

The first launch after this update may rebuild the optimized visual-OCR crates
once. That one-time compile is separate from the app's capture-to-result speed.

## 2. Install only the route you will test

1. Open **Models** from the desktop sidebar (or compact bottom navigation).
2. Under **Add a model**, choose **Screen text**, select
   **PP-OCRv6 Small · Multilingual**, and choose **Download**. The one-time
   download is 30.4 MiB.
3. Choose **Translation**, select the source/target route, and download the
   compatible model you want to test. Japanese to English uses the compact
   109.4 MiB route if it is not already installed.
4. Close Models or return to **Screen translation**.

Each selected model should show its own progress and final **Installed** state,
and then appear under **Installed on this PC**. Downloads are explicit;
installing the OCR pack does not install every translator.

## 3. Translate visible text

1. Open a browser or media player containing visible foreign-language text.
   Burned-in video subtitles, a title card, menu, sign, or game HUD is suitable.
2. In Prollyglot, choose **Translate Screen…**.
3. Choose **Application window**, select the browser/player window, set **Text on
   screen** and **Translate to**, then choose **Start Screen Translation**.
4. Continue normal playback. The selected source is watched continuously; the
   region selector is only a live crop, not a screenshot. Leave **Detection** on
   **Prominent text** for the first pass. Confirm **Scanning for text…** appears
   promptly, then a high-confidence first OCR pass can translate near the
   original text without waiting for a second full inference. The translator
   loads before capture begins, so **Starting…** may last longer on the first
   route load, but model loading should not cover the source with pending
   labels. Once capture is active, a compact Japanese/Spanish-to-English result
   should normally begin appearing within two seconds. Dense **All detected
   text** sources fill progressively and only the one region actually being
   processed should say **Translating…**. A compact inference still running at
   3.5 seconds is removed while the local worker recovers; it must not leave
   every later label pending for minutes. Stacked title or sign lines should
   become one phrase rather than overlapping word fragments.
   The overlay should not repeatedly recognize its own labels or fill the screen
   with unrelated interface text. The active card separates **OCR regions** from
   **Overlay labels**: once text is visible, both should be nonzero. OCR regions
   above zero with Overlay labels stuck at zero is an overlay-delivery defect;
   both at zero means recognition did not produce a stable region.
5. Change scenes or move the selected window. Confirm current labels follow the
   source rather than creating an ever-growing queue. A recognition result that
   is already more than three seconds behind after a broad scene change should
   be discarded; a cursor, clock, video control, or small localized text change
   should not clear an otherwise useful result. The scanning indicator may
   return while the newest frame is processed. Newly
   disappeared text may remain readable for up to eight seconds; text that was
   already visible for twelve seconds or longer should clear as soon as its
   absence is recognized.
6. Choose **Stop Screen Translation once**. The overlay should clear at once,
   the button should enter **Stopping…**, and the app should return to setup
   without another click even if recognition was in progress.

If application-window capture is blank, stop and retry once with **Whole
display**. If only part of the screen matters, choose **Selected region**, draw a
box at least 80 × 60 pixels around it, and start again. The selector should be
translucent enough to see the target underneath it. The current slice uses
Windows Graphics Capture for all three choices; the planned DXGI display
fallback is not implemented yet. Ordinary screenshots should include the app,
overlay, and selector windows. Prollyglot filters its currently drawn translated
labels from later OCR observations instead of hiding those windows from capture.

Audio captions and screen translation intentionally do not run together in this
slice. Starting screen translation while captions are live should explicitly
stop captions first rather than silently competing for resources.

## Result to send back

A passing report can be one sentence: source type, language route, and whether
the labels were readable and stayed attached. For a failure, send the source
type, what was visible, and the exact message in the app. A screenshot or the
privacy-safe diagnostic log is useful only if we need it to troubleshoot that
specific problem.
