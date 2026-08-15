# Windows lifecycle and inference-resource soak

Use this focused run after changing session supervision, source recovery,
translation deadlines, inference ownership, or shutdown. It should take about
45–60 minutes after models are installed.

This is not a proof-gathering exercise. Do not make an evidence folder, take
pass screenshots, record the screen, or save a note for every action. Report
only a failure, a surprising recovery time, or material memory growth. At the
end, one command summarizes the privacy-safe application log.

Run on native Windows 11 from a normal, non-administrator account. WSL and
cross-compilation cannot satisfy this check.

## 1. Prepare once

Open PowerShell at the repository root—the directory containing `Cargo.toml`,
`Prollyglot.md`, and `scripts`:

```powershell
Set-Location C:\github\Prollyglot
git pull --ff-only origin main
.\scripts\check-windows.ps1
```

Close any old Prollyglot process. Start the current build in the same terminal:

```powershell
pnpm --dir apps/desktop tauri dev
```

Have these installed before the run:

- one speech model you can use repeatedly;
- Nemotron if its large-load cancellation is being checked;
- one translation route for known source text; and
- the Screen Text model.

Open Task Manager's **Details** page and show Prollyglot's Memory column. Record
one approximate starting value after the first normal session stops. That is the
only baseline note needed.

## 2. Exercise ten supervised sessions

Complete at least ten total Start attempts. Use the same ordinary speech source
for at least three comparable caption sessions so post-stop memory samples have
a meaningful baseline.

1. Select a normal speech model, start captions, play speech until the state is
   Live, press **Stop Captions once**, and wait for Stopped.
2. Repeat that normal start/live/one-click-stop cycle twice. The overlay must
   clear and no attempt may remain in Starting or Stopping.
3. Select Nemotron, start captions, and press Stop once while the UI says it is
   loading. The UI should acknowledge promptly; the eventual cleanup must not
   revive the session or show stale captions.
4. Start a translated-caption session. Let original and translated output both
   appear, then stop once. Original output must remain usable if translation is
   late or fails.
5. Start captions for **Only this application**. Close the selected application
   while it is speaking. Prollyglot should enter Waiting rather than bind an
   unrelated process. Restart the same application and play audio; it should
   resume under the same selection when exactly one safe match exists.
6. If convenient, open a second instance that resolves to the same identity.
   Prollyglot must remain Waiting and explain the ambiguity instead of choosing
   silently. Close the duplicate and confirm recovery.
7. With **Follow system default**, change Windows' default playback endpoint,
   play speech through the new endpoint, and confirm the same session recovers.
8. With a device pinned explicitly, disable or disconnect it, then restore it.
   Prollyglot should remain responsive and resume the same pinned source.
9. Start captions, put Windows to sleep, resume, and play speech. Recovery may
   pass through Waiting but must not require restarting Prollyglot.
10. Complete any remaining starts with the same normal speech model/profile used
    in steps 1–2. Stop each once. After the final stop, compare Task Manager with
    the baseline; note it only if memory keeps climbing materially across
    comparable stopped sessions.

Silence, an exited application, or a missing device may legitimately show
Waiting. A crash, contradictory active state, stale overlay from an old session,
permanent Starting/Stopping, or a required second Stop click is a failure.

## 3. Force a translation deadline

This uses a development-only delay that is inactive in production builds.
Close the current app and stop `tauri dev` with `Ctrl+C`, then run:

```powershell
$env:VITE_PROLLYGLOT_TRANSLATION_TEST_DELAY_MS = "3000"
pnpm --dir apps/desktop tauri dev
```

Use an already-installed compact Japanese-to-English, Spanish-to-English, or
multilingual-to-English route. Three seconds exceeds the live-caption deadline
but leaves a finalized caption enough time to complete. Start a caption session
and play known speech. Confirm:

- original text remains readable while translation is delayed;
- the delayed job times out instead of blocking newer work forever;
- later translation attempts proceed after the inference worker restarts; and
- one Stop click returns the session to Stopped and clears the overlay.

Close the delayed build, remove the variable, and relaunch normally:

```powershell
Remove-Item Env:VITE_PROLLYGLOT_TRANSLATION_TEST_DELAY_MS -ErrorAction SilentlyContinue
pnpm --dir apps/desktop tauri dev
```

## 4. Exercise visual-session cleanup

1. Start screen translation for a small region containing clear, static text.
   Wait for at least one translated label.
2. Press **Stop Screen Translation once**. The overlay should hide immediately
   while background cleanup finishes; the button must not require repeated
   clicks.
3. Start it again for a display or application, then move the source between
   monitors or display scales if the machine supports that. Labels must remain
   aligned and an old frame must not reappear after stopping.
4. Start and stop visual translation at least two more times so the log contains
   comparable OCR load/release samples.

Treat protected and unprotected sources alike. A blank protected surface does
not authorize hooks or bypass work, but a same-route OBS Display Capture success
and Prollyglot failure is a compatibility defect.

## 5. Check model and offline recovery

Choose one installed model that is safe to re-download:

1. Stop all sessions and remove that model through **Models**. A new session that
   requires it should fail clearly without leaving a loaded resource.
2. Reinstall it, start and stop one session, then close Prollyglot.
3. Disconnect the network, relaunch, and use an already installed speech and
   translation route. Startup and inference must not require an account or
   network access.

If deliberately testing a corrupt artifact, modify only a disposable downloaded
model file that you are prepared to remove and reinstall. Prollyglot must reject
it during verification rather than attempt inference. Do not corrupt the only
copy of a large model merely to complete this soak.

## 6. Audit the newest log

Close Prollyglot normally so the final cleanup is recorded, then run from the
repository root:

```powershell
node scripts/check-soak-log.mjs
```

The checker automatically selects the newest log under
`%LOCALAPPDATA%\com.prollyglot.desktop\logs`. It expects at least ten started
sessions, at least one Speech, Translation, and VisualOcr load, balanced unload
or forced-release records, post-stop memory samples, and no forbidden
media-content fields. It also compares repeated post-stop profiles and flags
more than 384 MiB of end-to-end growth by default.

If logs span midnight or another run became newest, pass the intended file:

```powershell
node scripts/check-soak-log.mjs "C:\path\to\prollyglot.log"
```

The threshold is a regression alarm, not a release memory budget. Investigate a
steady upward trend even if it remains below the threshold.

## Result to report

A passing report can be one sentence:

> Lifecycle soak passed: 10+ sessions, startup cancellation, app/device/sleep
> recovery, translation timeout recovery, visual one-click stop, bounded memory,
> and log audit passed.

For a failure, send the action, visible state, approximate wait time, and exact
error text. Include nearby log lines only when they help diagnose that failure;
do not create routine proof for yourself.
