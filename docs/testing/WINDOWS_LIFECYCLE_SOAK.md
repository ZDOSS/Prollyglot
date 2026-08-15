# What to test on Windows before structural sign-off

Use Prollyglot normally for roughly half an hour. Play media you understand,
change modes when you feel like it, and be a little rough with Start and Stop.
There is no required order, evidence folder, screenshot, recording, or form to
fill out.

We are answering one practical question: after repeated use and ordinary Windows
interruptions, does Prollyglot keep telling the truth about what it is doing,
recover cleanly, and release work that is no longer needed?

Run the current `main` build from the repository root:

```powershell
git pull --ff-only origin main
.\scripts\check-windows.ps1
pnpm --dir apps/desktop tauri dev
```

## What needs to get exercised

Cover these areas in any order. You do not need a special video or exact words.

- **Ordinary captions:** Start and stop captions several times with the same
  model and source. Stop once while a model is still loading, once while speech
  is live, and once after silence. One click should always be enough. The app
  must not remain stuck on Starting or Stopping, and old captions must not return
  after Stop.

- **Application capture recovery:** Caption one application, close that
  application, and reopen it. Prollyglot should wait while it is gone and resume
  the same application when it returns. It must not quietly start captioning a
  different program. If two matching instances are open and Windows cannot
  distinguish them safely, waiting is the correct result.

- **Windows interruptions:** While captions are running, change the default
  playback device if you have another one available. Disable and restore a
  pinned device if that is convenient. Let the computer sleep and wake once.
  Prollyglot may briefly say Waiting, but it should recover without restarting
  the app. Skip hardware cases your machine cannot perform.

- **Translated captions:** Use any installed translation route. Original text
  should continue appearing even when translation is slower, and translated
  text must not block Stop or revive an older caption. Use it long enough to see
  several updates rather than judging one sentence.

- **Screen translation:** Start it on a region, application, or display that has
  clear text. Move or change the source if possible, then Stop once. Labels
  should follow current content, disappear promptly after Stop, and never require
  repeated clicks. Moving between monitors or display scales is useful when the
  machine supports it, but not mandatory on a single-monitor setup.

- **Missing and offline models:** Remove a small model you can easily reinstall
  and try the feature that needs it. The app should explain what is missing and
  remain usable. Reinstall it, then launch once without a network connection and
  use already-installed models. Do not corrupt a large model by hand.

Those actions naturally create the repeated sessions needed to check cleanup.
There is no magic total count. The log audit only asks that one equivalent setup
was started and stopped at least three times, and that speech, translation, and
screen OCR were each actually loaded once.

## One deliberate slow-translation check

Translation timeout/recovery is the only behavior that is hard to produce by
normal use. For one development launch, close the current app and run:

```powershell
$env:VITE_PROLLYGLOT_TRANSLATION_TEST_DELAY_MS = "3000"
pnpm --dir apps/desktop tauri dev
```

Use translated captions normally for a minute. Live translations are now
deliberately slower than their deadline. Original captions should keep moving,
later finalized translation should still recover, and one Stop click should end
the session. Then close that build and clear the switch:

```powershell
Remove-Item Env:VITE_PROLLYGLOT_TRANSLATION_TEST_DELAY_MS -ErrorAction SilentlyContinue
```

You do not need to time the timeout or prove which internal worker restarted.
The visible requirement is simply that slow translation cannot freeze captions
or Stop.

## What counts as a failure

Tell me if you see any of these:

- Start or Stop needs repeated clicks;
- the UI stays in Starting, Stopping, or a false Live state;
- captions or screen labels from a stopped/older session reappear;
- reopening an application captions the wrong program or never recovers;
- changing devices or waking Windows permanently kills the session;
- slow or failed translation prevents original captions from continuing;
- screen translation keeps running after Stop;
- the app crashes, becomes permanently unresponsive, or becomes progressively
  slower as sessions are repeated; or
- memory continues climbing after equivalent sessions have stopped.

Waiting is not itself a failure when a source is silent, closed, disconnected,
or ambiguous. A clear missing-model message is also correct; becoming stuck is
not.

## Let the log check the internal cleanup

When you are finished, close Prollyglot normally and run one command from the
repository root:

```powershell
node scripts/check-soak-log.mjs
```

It reads the newest privacy-safe Prollyglot log automatically. It checks that
speech, translation, and screen OCR were exercised; that a comparable setup was
repeated; that no inference resource remained owned after Stop; that stopped
memory did not show a large upward trend; and that no caption, OCR, translation,
audio, or frame content appeared in diagnostics.

Send me that command's output and describe anything that felt wrong. If nothing
felt wrong and it says PASS, that is the entire report.
