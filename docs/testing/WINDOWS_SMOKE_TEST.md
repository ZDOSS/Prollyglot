# Windows development smoke test

Use this short check for ordinary pre-release builds on the Windows 11 test machine. It should take about five minutes once the model is installed.

This is not a release-certification run. Do not create an evidence folder, generate speech fixtures, record the screen, take pass screenshots, run OBS comparisons, interrupt a model download, or perform a soak test. If something fails, report what happened in plain language; collect a screenshot or log only when it would help troubleshoot that specific failure.

## 1. Launch the current build

Open PowerShell and enter the repository root. For the owner's current clone:

```powershell
Set-Location C:\github\Prollyglot
git pull --ff-only origin main
pnpm --dir apps/desktop tauri dev
```

Run `pnpm --dir apps/desktop install --frozen-lockfile` first only when dependencies have changed or pnpm says packages are missing.

The app should open without a UAC prompt or blank window. No subtitle preview should appear merely because the app launched.

Open **Appearance** once. Its sample stays inside the Appearance window rather than opening a second always-on-top preview over the controls. Confirm both **Done** and the title-bar **Close** return to the main window; no screenshot is required.

If the selected model is missing, download it normally and wait for it to finish. Fast is the smallest English default; Balanced and Enhanced are optional English choices under **Settings**. Nemotron is a much larger optional multilingual trial and does not need to be downloaded for an ordinary English smoke. Interrupted-download recovery is deferred to a later hardening pass and is not part of this smoke test.

## 2. Smoke-test Everything I hear

1. Choose **Everything I hear** and **Follow system default** (or the playback device you are actually using).
2. Start captions and play any ordinary English speech through that device.
3. Confirm the state becomes **Live** while audio is audible and the overlay shows changing captions.
4. Watch a few changing partial captions. Text should occupy one readable surface; words and lines must not stack or paint over one another.
5. Play a short exchange with a few brief remarks. Finalized utterances should remain as separate recent lines while the newest line updates. Older context may be dimmer; it must not be presented as identified speakers.
6. Pause the speech briefly, resume it, then stop captions. The app should remain responsive, retain the final context for several seconds, and then clear the overlay.

Recognizing the audible selected-device output is enough to pass this pre-release capture smoke. Exact wording and punctuation do not need to match perfectly.

## 3. Smoke-test Settings

Open **Settings** and select **Refresh audio sources**. The dialog should immediately say it is refreshing, then report how many playback devices and applications it found. Newly opened audio applications should appear in the main source list after closing Settings.

Settings should list Fast, Balanced, Enhanced, and Multilingual model cards with download sizes and one clearly marked **In use**. To compare recognition, stop captions, download another model, select **Use model**, and replay the same difficult speech. A simple note about which model sounded better is enough; no recording, reference transcript, or screenshot is required for this pre-release check.

Model removal is not required on every build. When specifically checking it, stop captions, choose **Remove**, and confirm that Settings reports success. Removing an unselected model returns it to **Optional**. Removing the selected model returns first-run setup and disables **Start Captions** until that model is reinstalled or another installed model is selected.

When specifically spot-checking multilingual captions, choose **Spanish** or **Japanese** under **Spoken language**. Prollyglot should select Nemotron and ask for its one-time 650.6 MiB download if needed. Play familiar speech, report whether the wording feels usable and how much delay you notice, then repeat with the other language if desired. **Automatic detection** is available for experimentation but may add delay or choose the wrong language. This build captions the original language only; it does not translate. No fixtures, recordings, or screenshots are required.

## 4. Briefly check one application source

1. Start an application that can play speech before refreshing audio sources.
2. Refresh sources, close Settings, and choose **Only _application_**.
3. Start captions and play speech in that application.
4. Confirm its speech is captioned. If convenient, play unrelated speech in another app and confirm it is not captioned.

If the application does not appear or capture, report the application name and what the UI showed. That is a product defect to investigate, not a request for the tester to build a proof package.

## Result to send back

For a pass, a short message such as “launch clean; device capture, overlay, Refresh, and app capture passed” is sufficient. For a failure, send the action, visible result, and exact error text if there was one. Logs or screenshots are follow-up diagnostics, not routine proof.

If captions report that transcription fell behind or stop updating unexpectedly, the newest privacy-safe diagnostic log can be read from any PowerShell directory while the app is open or after it closes:

```powershell
$LogRoot = Join-Path $env:LOCALAPPDATA "com.prollyglot.desktop\logs"
$LatestLog = Get-ChildItem $LogRoot -Filter *.log |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1
Get-Content $LatestLog.FullName -Tail 200
```

The log includes queue-drop and recovery counts in current builds, but no captured audio or transcript text.

The longer [Windows release and hardening plan](WINDOWS_TEST_PLAN.md) remains available for formal milestone acceptance, installer/release candidates, latency work, routing edge cases, and sustained reliability testing.
