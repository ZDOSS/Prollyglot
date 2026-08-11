# Prollyglot Windows 11 release and hardening plan

This is the exhaustive validation runbook for formal milestone acceptance, hardening passes, and release candidates. It combines the Milestone 1 capture checks and the Milestone 2 live-caption checks so a planned validation session can produce comparable evidence.

> [!IMPORTANT]
> Do not use this document for ordinary pre-release checks. Use the five-minute [Windows development smoke test](WINDOWS_SMOKE_TEST.md), which does not require screenshots, recordings, fixtures, or an evidence bundle for passing behavior. Run this longer plan only when the project is deliberately closing a milestone or validating a release candidate.

Run it on native Windows 11, from an ordinary non-administrator account. Do not run the application from WSL. Installing prerequisites may require administrator approval, but Prollyglot itself must launch and caption without elevation, a virtual audio device, an account, or a cloud transcription service.

Allow roughly two to four hours for the first complete run, including dependency downloads and the 30-minute soak. The representative model comparison takes additional time because its audio samples must have trustworthy transcripts.

## What this run accepts

The current gate covers:

- playback-device capture using **Everything I hear**;
- process-tree capture using **Only _application_**;
- local English model installation, interruption recovery, removal, and offline reuse;
- incremental and finalized captions, transcript behavior, silence handling, and restart behavior;
- overlay placement, customization, click-through, focus, and multi-monitor behavior;
- current OBS device/application-capture parity, including protected-media sources the tester can lawfully play;
- partial-caption latency, sustained resource use, and model-comparison evidence; and
- the absence of persisted raw audio or caption text in diagnostic logs.

Installer behavior, tray controls, shortcuts, transcript export, and Ubuntu packaging are not part of this run. They belong to later milestones. Record an observation if you encounter one of those areas, but do not fail this Windows caption slice because an unreleased feature is absent.

Use exactly one result for every test case:

- **PASS** — the observed behavior matches the expected result.
- **FAIL** — the test could be run and the behavior did not match.
- **BLOCKED** — required hardware, media, or another external condition was unavailable.
- **N/A** — the conditional case does not apply to this machine.

Do not turn a product failure into **BLOCKED**. For example, if OBS captures a source under equivalent routing and Prollyglot does not, that is **FAIL**.

## 1. Prepare the Windows machine once

### 1.1 Record the hardware you will use

Before installing or changing anything, write down:

- Windows edition, display version, and OS build;
- CPU model and installed RAM;
- GPU model;
- every playback endpoint you intend to test;
- monitor count, resolution, and scale percentage for each monitor;
- whether the machine is on AC power or battery; and
- whether Bluetooth, USB, HDMI/DisplayPort, or virtual playback endpoints are present.

A second playback endpoint and a second monitor are strongly useful. If either is unavailable, mark only the corresponding conditional cases **BLOCKED**; the primary one-device/one-monitor path can still pass.

### 1.2 Install the build prerequisites

Install the following on Windows, not inside WSL:

1. Install [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/). In the installer, select **Desktop development with C++** and a current Windows 11 SDK.
2. Confirm Microsoft Edge WebView2 is installed. Windows 11 normally includes it; use the [WebView2 Evergreen Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) only if the Prollyglot window is blank or WebView2 is reported missing.
3. Install Git, Rust, the current Node.js LTS release, OBS Studio, and VLC. The commands below use Windows Package Manager and can be run one at a time in PowerShell:

   ```powershell
   winget install --exact --id Git.Git
   winget install --exact --id Rustlang.Rustup
   winget install --exact --id OpenJS.NodeJS.LTS
   winget install --exact --id OBSProject.OBSStudio
   winget install --exact --id VideoLAN.VLC
   ```

4. Close every terminal, open a new **Windows PowerShell** window, and install/configure the remaining tools:

   ```powershell
   rustup default stable-msvc
   rustup update stable
   rustup component add rustfmt clippy
   npm install --global pnpm@11
   ```

5. Verify the toolchain:

   ```powershell
   git --version
   rustup show active-toolchain
   rustc --version
   cargo --version
   node --version
   pnpm --version
   ```

Expected:

- the active Rust toolchain ends in `pc-windows-msvc`;
- `rustc` is 1.88 or newer;
- Node.js is 22.12 or newer; and
- every command prints a version instead of “not recognized.”

The upstream setup references are [Tauri's Windows prerequisites](https://v2.tauri.app/start/prerequisites/), [Rust installation](https://www.rust-lang.org/tools/install), and [pnpm installation](https://pnpm.io/installation).

## 2. Create a clean evidence folder

Use **Windows PowerShell**, not an administrator shell. Run:

```powershell
$RunId = Get-Date -Format "yyyyMMdd-HHmmss"
$EvidenceRoot = Join-Path ([Environment]::GetFolderPath("MyDocuments")) "Prollyglot-Test-$RunId"
$FixtureRoot = Join-Path $EvidenceRoot "fixtures"
New-Item -ItemType Directory -Path $EvidenceRoot, $FixtureRoot -Force | Out-Null
Start-Transcript -Path (Join-Path $EvidenceRoot "powershell-transcript.txt")

Get-ComputerInfo |
  Select-Object WindowsProductName, WindowsDisplayVersion, OsBuildNumber, OsArchitecture, CsSystemType, CsProcessors, CsTotalPhysicalMemory |
  Format-List |
  Out-File (Join-Path $EvidenceRoot "machine.txt")

Get-CimInstance Win32_VideoController |
  Select-Object Name, DriverVersion, AdapterRAM |
  Format-Table -AutoSize |
  Out-File (Join-Path $EvidenceRoot "gpu.txt")

Get-CimInstance Win32_SoundDevice |
  Select-Object Name, Manufacturer, Status |
  Format-Table -AutoSize |
  Out-File (Join-Path $EvidenceRoot "sound-devices.txt")

explorer.exe $EvidenceRoot
```

Keep this PowerShell window open. Save screenshots, latency notes, benchmark output, and copied logs under `$EvidenceRoot`. Do not put private media or transcripts in the repository.

In `machine.txt` or a separate note, manually add the monitor resolutions/scales and AC/battery state; Windows' generic hardware inventory does not reliably capture per-monitor scale.

## 3. Generate three recognizable local speech fixtures

These fixtures make routing and application-isolation failures obvious. Run the following in **Windows PowerShell 5.1**. If `Add-Type -AssemblyName System.Speech` fails in PowerShell 7, open the built-in app named **Windows PowerShell** and run this section there.

```powershell
Add-Type -AssemblyName System.Speech

function New-ProllyglotSpeechFixture {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Text
  )

  $Voice = New-Object System.Speech.Synthesis.SpeechSynthesizer
  try {
    $Voice.Rate = -1
    $Voice.Volume = 100
    $Voice.SetOutputToWaveFile($Path)
    $Voice.Speak($Text)
  }
  finally {
    $Voice.Dispose()
  }
}

New-ProllyglotSpeechFixture `
  -Path (Join-Path $FixtureRoot "system-device.wav") `
  -Text "Blue windows open above the garden. Prollyglot should caption this playback device."

New-ProllyglotSpeechFixture `
  -Path (Join-Path $FixtureRoot "selected-application.wav") `
  -Text "Silver lanterns guide the quiet river. This sentence belongs to the selected application."

New-ProllyglotSpeechFixture `
  -Path (Join-Path $FixtureRoot "unrelated-application.wav") `
  -Text "Orange bicycles circle the station. This sentence belongs to the unrelated application."

Get-ChildItem $FixtureRoot -Filter *.wav | Select-Object Name, Length
```

Expected: all three WAV files exist and have nonzero sizes. Play each once to confirm it is audible. Exact punctuation is not important during caption tests; the unusual anchor words (`BLUE WINDOWS`, `SILVER LANTERNS`, and `ORANGE BICYCLES`) are what distinguish the routes.

## 4. Synchronize and validate the source tree

### 4.1 Clone once, or enter the existing clone

For a new clone:

```powershell
New-Item -ItemType Directory -Path C:\src -Force | Out-Null
Set-Location C:\src
git clone https://github.com/ZDOSS/Prollyglot.git
Set-Location C:\src\Prollyglot
```

For an existing clone, replace the example path and enter it:

```powershell
Set-Location C:\path\to\Prollyglot
```

Then run:

```powershell
git status --short --branch
git switch main
git pull --ff-only origin main
git rev-parse HEAD | Tee-Object -FilePath (Join-Path $EvidenceRoot "tested-commit.txt")
git status --short | Tee-Object -FilePath (Join-Path $EvidenceRoot "worktree-before-test.txt")
```

Expected: the branch is `main`, the pull fast-forwards or reports that it is current, and `worktree-before-test.txt` is empty. If it is not empty, do not delete or reset the files; record the changes because they make the run non-reproducible.

### 4.2 Install locked frontend dependencies

```powershell
pnpm --dir apps/desktop install --frozen-lockfile
```

Expected: exit code 0 and no lockfile rewrite.

### 4.3 Run the local validation loop

```powershell
.\scripts\check-windows.ps1
```

If PowerShell blocks the script because of local execution policy, do not change the machine-wide policy. Run this one process instead:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\check-windows.ps1
```

Do not merge all output streams into `Tee-Object` around this script. Cargo writes ordinary progress messages such as `Updating crates.io index` to stderr; under the script's strict PowerShell error handling, stream merging can incorrectly surface that progress as a `NativeCommandError` even though Cargo has not failed.

Expected: formatting, Rust tests, the frontend TypeScript/Vite build, and Clippy all finish successfully. Record **WIN-BUILD-01 = PASS** only when the complete script exits successfully. If it fails, copy the actual failing command and error output for troubleshooting; manual behavior from a genuinely failing build is not milestone evidence.

This check is local and consumes no GitHub Actions minutes.

## 5. Launch the development build

Open a second ordinary Windows PowerShell window, enter the repository, and run:

```powershell
Set-Location C:\path\to\Prollyglot
pnpm --dir apps/desktop tauri dev
```

Leave this terminal open. The first Rust build can take several minutes. Expected:

- a full desktop workspace titled **Prollyglot** opens, with a title-bar control
  available for compact mode;
- there is no administrator/UAC prompt from the application;
- the window is not blank;
- the initial status is **Ready**; and
- if no model is installed, a **Local model required** card is visible and **Start Captions** is disabled.

Record **WIN-LAUNCH-01** and save a screenshot named `launch.png` under `$EvidenceRoot`.

If the app crashes, preserve the entire development-terminal output and copy the current Prollyglot log before relaunching. Do not repeatedly retry a deterministic crash more than twice.

## 6. Test the first-run model lifecycle

Perform this section before normal caption testing. If Fast was installed by an earlier run, open **Models**, expand **Installed on this PC**, make **English Streaming Small** the selected model if necessary, press its **Remove** action, close Prollyglot, stop `tauri dev` with `Ctrl+C`, and launch it again.

### WIN-MODEL-01 — missing-model state

1. Confirm the card says **Fast English captions** and describes a one-time local download.
2. Confirm the button shows approximately **43.1 MB**.
3. Confirm **Start Captions** cannot be pressed.

Expected: all three checks pass.

### WIN-MODEL-02 — interrupted download recovery

1. Press **Download model**.
2. Confirm the percentage changes and the window remains responsive.
3. At roughly 10–50%, close the Prollyglot window.
4. In the development terminal, press `Ctrl+C` and wait for the process to end.
5. Run `pnpm --dir apps/desktop tauri dev` again.
6. Press **Retry download** or **Download model**, whichever is shown.

Expected: the partial download is never reported as ready, the retry starts normally, and there is no manual file cleanup requirement.

### WIN-MODEL-03 — verified completion

1. Let the download finish.
2. Confirm the model card disappears.
3. Confirm **Start Captions** becomes available.
4. Open **Models**, then expand **Installed on this PC**.
5. Confirm **English Streaming Small** is marked **In use** and reports approximately 43.1 MB. Under **Add a model → Speech recognition → English**, Balanced and Enhanced should remain available but not installed.

Expected: all checks pass. Save `model-installed.png`.

### WIN-MODEL-04 — offline reuse

1. Close Prollyglot and stop `tauri dev`.
2. Disable Wi-Fi and unplug Ethernet.
3. Relaunch with `pnpm --dir apps/desktop tauri dev`.
4. Confirm the model is still installed and **Start Captions** remains available.
5. Select **Everything I hear** and **Follow system default**, then press **Start Captions**.
6. Play `system-device.wav` in VLC and confirm recognizable live captions appear.
7. Stop captions and restore the network.

Expected: an already-installed model loads and captions locally with no account or network connection.

### WIN-MODEL-05 — remove and reinstall

1. Start captions and open **Models**.
2. Confirm model selection and removal controls are disabled while the session is active and the model remains installed.
3. Press **Stop Captions** and wait for **Ready**.
4. Open **Models → Installed on this PC** and press **Remove** on English Streaming Small.
5. Confirm the missing-model card returns and **Start Captions** becomes unavailable.
6. Reinstall the model and leave it installed for the remaining tests.

Expected: removal is blocked while captions are active, succeeds after Stop, and the reinstalled model returns to **In use**.

## 7. Run the pinned ASR sanity check

In the evidence PowerShell window, from the repository root:

```powershell
$ReferenceWav = Join-Path $FixtureRoot "pinned-reference-0.wav"
Invoke-WebRequest `
  "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/d42f2d9f7ca24806fb667456a18a9f1b60f70d16/test_wavs/0.wav" `
  -OutFile $ReferenceWav

$env:PROLLYGLOT_TEST_WAV = $ReferenceWav
cargo test --release --locked -p prollyglot-asr-sherpa `
  transcribes_the_pinned_models_reference_speech -- --ignored --nocapture *>&1 |
  Tee-Object -FilePath (Join-Path $EvidenceRoot "asr-reference-test.txt")
```

Expected: the ignored test runs and reports `ok`. It verifies that the pinned runtime produces incremental text and recognizes key content including “after early nightfall,” “yellow lamps,” and “squalid quarter.” Record **WIN-ASR-01**.

This is a runtime sanity check, not the representative quality benchmark in section 16.

## 8. Establish the Windows routing baseline

1. Open **Settings → System → Sound** and record the exact current default output device name.
2. Open `system-device.wav` in VLC, play it for at least one second, then pause it. This creates an active Windows audio session.
3. Open `unrelated-application.wav` in Microsoft Edge with `Ctrl+O`, play it for at least one second, then pause it.
4. In Prollyglot, open **Settings** and press **Refresh audio sources**.
5. Inspect **Audio source** and **Playback device**.

Expected:

- **Follow system default — _device name_** names the same default endpoint as Windows Settings;
- the playback list contains the available endpoints you expect;
- **Audio source** contains an entry for VLC and an entry for Edge, each prefixed with **Only**; and
- multiple child audio processes are represented as one user-facing application root rather than a list of indistinguishable child processes.

Record **WIN-SOURCES-01**. If an application is missing, play it again, use **Refresh audio sources**, and retry once. A source that remains absent is **FAIL**, not a reason to reinstall the model.

## 9. Test “Everything I hear”

### WIN-SYSTEM-01 — selected default playback device

1. Set **Audio source** to **Everything I hear**.
2. Set **Playback device** to **Follow system default — _device name_**.
3. Press **Start Captions**.
4. Confirm the status progresses through **Starting** to **Waiting** or **Live**.
5. Play `system-device.wav` in VLC.
6. Watch the overlay and the main-window activity treatment.
7. Open **Transcript** after the phrase completes.

Expected:

- status becomes **Live** while speech is present;
- useful partial text appears before the whole phrase has finished;
- the overlay contains recognizable words from the phrase, ideally the `BLUE WINDOWS` anchor;
- finalized text appears once in Transcript with an increasing timestamp; and
- no account, microphone permission, or virtual-cable setup is requested.

ASR wording does not need to be letter-perfect for this routing test. It must be recognizably derived from the played phrase.

### WIN-SYSTEM-02 — silence and recovery

1. Pause all playback for at least five seconds.
2. Watch the status and overlay.
3. Resume `system-device.wav`.

Expected: status settles to **Waiting**, inference remains quiet, the final overlay text clears after its short hold, and playback returns the session to **Live** without pressing Stop/Start.

### WIN-SYSTEM-03 — transcript clear

1. Open **Transcript**.
2. Confirm finalized segments are not duplicated and a current partial, if present, is visibly labeled **Live**.
3. Press **Clear**.

Expected: the current session transcript becomes empty immediately. Clearing the transcript must not stop capture.

### WIN-SYSTEM-04 — ten start/stop cycles

Stop the current session and wait for **Ready**. Then, for cycles 1 through 10:

1. Play `system-device.wav`.
2. Press **Start Captions** if stopped.
3. Wait for **Live** and at least one useful partial.
4. Press **Stop Captions**.
5. Wait for **Ready** and confirm the overlay disappears.
6. Record the result for that cycle.

| Cycle | Reached Live | Returned to Ready | Overlay closed | Duplicate final/orphan text | Result |
| ---: | --- | --- | --- | --- | --- |
| 1 |  |  |  |  |  |
| 2 |  |  |  |  |  |
| 3 |  |  |  |  |  |
| 4 |  |  |  |  |  |
| 5 |  |  |  |  |  |
| 6 |  |  |  |  |  |
| 7 |  |  |  |  |  |
| 8 |  |  |  |  |  |
| 9 |  |  |  |  |  |
| 10 |  |  |  |  |  |

Expected: no cycle remains stuck in **Starting** or **Stopping**, crashes, duplicates a final, or leaves an orphan overlay.

## 10. Test device isolation and recovery

If the machine has only one playback endpoint, mark `WIN-DEVICE-02` through `WIN-DEVICE-04` **BLOCKED — second endpoint unavailable** and continue.

### WIN-DEVICE-01 — pinned current endpoint

1. In Windows Settings, note endpoint A as the current default.
2. In Prollyglot, choose the explicit endpoint A entry ending in **Pin current default**, not **Follow system default**.
3. Start captions and play `system-device.wav` through endpoint A.

Expected: captions work through the pinned endpoint.

### WIN-DEVICE-02 — endpoint isolation

1. Connect/enable endpoint B.
2. Pin endpoint B in Prollyglot and start captions.
3. In **Settings → System → Sound → Volume mixer**, route VLC to endpoint A.
4. Play `system-device.wav`.
5. Route VLC to endpoint B and play it again.

Expected: audio routed only to A produces no meaningful Prollyglot activity or captions while B is pinned; audio routed to B produces **Live** status and captions.

### WIN-DEVICE-03 — follow-default switch

1. Set VLC back to the Windows default output.
2. Select **Follow system default** in Prollyglot and start captions.
3. Confirm speech captions on endpoint A.
4. While capture remains active, change the Windows default output to endpoint B.
5. Confirm the fixture is audibly routed to B; restart VLC playback if the player retains its previous route.

Expected: Prollyglot may briefly wait/recover, then captures endpoint B without a new caption session.

### WIN-DEVICE-04 — endpoint removal and reconnection

1. Explicitly pin a removable USB/Bluetooth/HDMI endpoint.
2. Start captions and confirm **Live** speech.
3. Disconnect or disable that endpoint while capture is active.
4. Wait up to 30 seconds and confirm the control window remains usable.
5. Reconnect/re-enable the same endpoint and play the fixture again.

Expected: the app reports/waits through the loss and resumes the same session after the endpoint returns. Record the exact message and recovery time. A crash, permanent UI hang, or required application restart is **FAIL**.

## 11. Test “Only this application” isolation

Use VLC as the selected application and Edge as the unrelated application first. Keep `selected-application.wav` open in VLC and `unrelated-application.wav` open in Edge. Play each briefly, pause both, then press **Settings → Refresh audio sources**.

### WIN-APP-01 — unrelated application excluded

1. Select **Only VLC** (the displayed executable/friendly name may vary).
2. Press **Start Captions**.
3. Leave VLC paused.
4. Play `unrelated-application.wav` in Edge twice.

Expected: Prollyglot settles to **Waiting** and does not produce meaningful text from Edge. In particular, `ORANGE BICYCLES` or a recognizable approximation must not appear.

### WIN-APP-02 — selected application included

1. Pause Edge.
2. Play `selected-application.wav` in VLC twice.

Expected: Prollyglot becomes **Live** and produces partial/final text recognizable as the selected phrase, ideally including `SILVER LANTERNS`.

### WIN-APP-03 — simultaneous isolation

1. Start both fixture files as close together as practical.
2. Let both complete.
3. Inspect the overlay and Transcript.

Expected: the transcript follows VLC's selected phrase and does not incorporate Edge's distinctive unrelated phrase.

### WIN-APP-04 — browser process tree

1. Stop captions.
2. Select **Only Microsoft Edge** (the displayed name may vary).
3. Start captions.
4. Play `unrelated-application.wav` in the already-open Edge tab.
5. Play `selected-application.wav` only in VLC.

Expected: Edge tab audio is included even when rendered by a child process, while VLC audio is excluded.

### WIN-APP-05 — selected process exits

1. With Edge selected and speaking, close every Edge window so the selected application root exits.
2. Wait for Prollyglot to react.
3. Confirm the main window remains responsive.
4. Stop if necessary, reopen Edge, play the WAV briefly, press **Refresh audio sources**, reselect Edge, and start a new session.

Expected: the active session reports a clear source-exited failure rather than hanging or silently captioning another application, and a fresh session works after refresh/reselection.

## 12. Test the caption overlay and keyboard path

### WIN-OVERLAY-01 — live preview and every current appearance control

1. Stop captions.
2. Press **Appearance**.
3. Confirm both the in-window preview and the real transparent desktop overlay display the preview sentence.
4. Test every value for **Font**, **Size**, **Width**, **Maximum lines**, and **Position**.
5. Move **Background opacity** to 0%, 50%, and 100%.
6. Choose a visibly different **Text color**.
7. Press **Reset** and confirm the defaults return.

Expected: each change updates immediately, the real overlay remains readable and within the current monitor's work area, and no setting crashes or freezes the window.

### WIN-OVERLAY-02 — click-through and focus

1. Open Notepad and place it underneath the overlay.
2. In Appearance, enable **Click-through**.
3. Click where the overlay overlaps Notepad and type `click-through-pass`.
4. Press **Done**, return to the main window, and start live captions.
5. Return to Notepad and continue typing while the caption overlay updates.

Expected: the click and keystrokes reach Notepad, the overlay never takes focus, and caption updates do not interrupt typing.

### WIN-OVERLAY-03 — dragging and second monitor

If there is no second monitor, perform the drag on the primary monitor and mark only the cross-monitor portion **BLOCKED**.

1. Disable **Click-through**.
2. Drag the actual desktop caption surface to monitor 2.
3. Set each anchored position once.
4. Test on the monitor with a negative desktop coordinate if the Windows display arrangement places one left of or above the primary.
5. Re-enable **Click-through** and verify underlying applications receive clicks again.

Expected: the overlay stays on the chosen monitor, anchors inside that monitor's work area, and does not jump off-screen.

### WIN-OVERLAY-04 — scale and fullscreen observations

1. Record results at the current Windows display scale.
2. If practical, repeat the preview at 100%, 150%, and on mixed-DPI monitors.
3. Confirm the overlay stays above an ordinary maximized window and a borderless-fullscreen application.
4. Optionally test an exclusive-fullscreen game.

Expected: ordinary and borderless-window behavior passes. Record exclusive-fullscreen behavior explicitly; a platform limitation there is evidence to handle, not a reason to hide the result.

### WIN-KEYBOARD-01 — basic keyboard operation

1. Return focus to the main window.
2. Use only `Tab`, `Shift+Tab`, arrow keys, `Space`, `Enter`, and `Escape`.
3. Choose a source, start/stop captions, open/close Transcript, open Appearance, change one control, reset it, and press Done.

Expected: every current control has a visible focus path and can be operated without a mouse. Record inaccessible or invisible-focus controls individually.

## 13. Compare equivalent OBS capture paths

This section tests compatibility; it does not ask OBS or Prollyglot to remove, weaken, or route around protected-media controls. Use sources you are authorized to play. Prollyglot captures whatever decoded PCM the documented Windows path exposes.

### WIN-OBS-01 — configure OBS without feedback

1. Open the current OBS Studio release.
2. In **Settings → Audio**, disable unused global desktop/microphone devices so the test source is unambiguous.
3. In **Sources**, add **Audio Output Capture** and choose the same endpoint Prollyglot is testing.
4. Add **Application Audio Capture** and choose the same application Prollyglot is testing.
5. Do not enable audio monitoring back to the captured endpoint; that can create a feedback loop.
6. Keep the OBS Audio Mixer visible. Recording or streaming is unnecessary.

### WIN-OBS-02 — run the parity matrix

Test at least:

- the local `system-device.wav` fixture;
- ordinary English browser video/media;
- one protected-media source you can lawfully access, if available; and
- one game/call source, if available.

For every source, compare the same route rather than comparing OBS device capture to Prollyglot application capture.

| Source | Route | OBS receives meaningful audio | Prollyglot receives/captions it | Result and notes |
| --- | --- | --- | --- | --- |
| Local fixture | Selected device |  |  |  |
| Local fixture | Selected application |  |  |  |
| Ordinary browser media | Selected device |  |  |  |
| Ordinary browser media | Selected application |  |  |  |
| Protected media | Selected device |  |  |  |
| Protected media | Selected application |  |  |  |
| Game/call | Selected device |  |  |  |
| Game/call | Selected application |  |  |  |

Interpret each row as follows:

- OBS yes / Prollyglot yes: **PASS**.
- OBS yes / Prollyglot no: **FAIL — Prollyglot compatibility defect**.
- OBS no / Prollyglot no: record a Windows/source routing limitation; do not attempt to strip protection.
- OBS no / Prollyglot yes: **PASS**, with the OBS configuration noted.

Do not install a virtual cable merely to make the normal path pass. If one is already installed, it may be tested as an ordinary optional endpoint, but any source that only works through it is recorded as a native-capture compatibility gap.

For a parity failure, preserve:

- endpoint and application names;
- which source and route were used;
- Windows volume-mixer routing;
- a screenshot showing the OBS meter and Prollyglot state;
- the current Prollyglot log; and
- OBS **Help → Log Files → View Current Log** output.

## 14. Measure useful-partial latency

Use at least ten minutes total across three English categories:

1. conversational speech;
2. ordinary media/dialogue; and
3. speech over game/call/background noise.

Use **Everything I hear** for the full measurement and spot-check at least five phrase starts in **Only this application**. For each category, collect at least ten phrase starts.

Recommended measurement method:

1. Put the playing video/application and Prollyglot overlay in the same physical view.
2. Record the screen and audible speaker output with a phone at 120 or 240 frames per second.
3. For one phrase, find the frame where the first recognizable spoken word begins.
4. Find the frame where the first useful matching partial appears.
5. Calculate `latency milliseconds = frame difference / frames per second × 1000`.
6. Repeat for at least ten phrase starts per category.
7. Enter the values in a spreadsheet. Use `=MEDIAN(range)` and `=PERCENTILE.INC(range,0.95)`.

Do not measure from phrase end to final text; the gate is audible word to first useful matching partial.

| Measure | Conversation | Media | Noisy game/call |
| --- | ---: | ---: | ---: |
| Phrase starts sampled |  |  |  |
| Median useful-partial latency (ms) |  |  |  |
| P95 useful-partial latency (ms) |  |  |  |
| Missed phrase openings |  |  |  |
| Material final-text errors |  |  |  |

Record **WIN-LATENCY-01 = PASS** when the lightweight model is at least real-time, median useful-partial latency is below 2,000 ms on the reference machine, and phrase openings are not systematically lost. Keep the raw samples; a median alone can hide severe outliers.

## 15. Run a 30-minute soak and privacy check

### WIN-SOAK-01 — sustained capture

1. Open Task Manager's **Details** tab and show the Prollyglot process CPU and memory columns.
2. Start **Everything I hear** with 30 minutes of mixed English speech.
3. At minutes 1, 10, 20, and 30, record CPU, memory, overlay delay, state, and any visible error.
4. Include several silences and resume speech without restarting.
5. At minute 30, press **Stop Captions** and wait for **Ready**.

| Time | CPU | Memory | Overlay still near live | State/warning |
| ---: | ---: | ---: | --- | --- |
| 1 min |  |  |  |  |
| 10 min |  |  |  |  |
| 20 min |  |  |  |  |
| 30 min |  |  |  |  |

Expected: no crash, memory and latency do not grow continuously, silence/resume works, and Stop completes normally. If inference falls behind, old buffered audio should be discarded and the app should recover near live playback rather than accumulate an ever-growing delay.

### WIN-PRIVACY-01 — inspect logs and local files

Run after stopping captions:

```powershell
$AppDataRoot = Join-Path $env:LOCALAPPDATA "com.prollyglot.desktop"
$LogRoot = Join-Path $AppDataRoot "logs"
$CopiedLogs = Join-Path $EvidenceRoot "prollyglot-logs"

if (Test-Path $LogRoot) {
  Copy-Item $LogRoot -Destination $CopiedLogs -Recurse -Force
  Get-ChildItem $LogRoot -Filter *.log |
    Select-String -Pattern "blue windows|silver lanterns|orange bicycles" -CaseSensitive:$false |
    Tee-Object -FilePath (Join-Path $EvidenceRoot "caption-text-log-search.txt")

  Get-ChildItem $LogRoot -Filter *.log |
    Select-String -Pattern "dropped|backpressure|overflow|error|failed" -CaseSensitive:$false |
    Tee-Object -FilePath (Join-Path $EvidenceRoot "runtime-warning-search.txt")
}

Get-ChildItem $AppDataRoot -Recurse -File -ErrorAction SilentlyContinue |
  Where-Object Extension -In ".wav", ".wave", ".pcm", ".raw", ".mp3", ".flac", ".m4a" |
  Select-Object FullName, Length |
  Tee-Object -FilePath (Join-Path $EvidenceRoot "persisted-audio-search.txt")
```

Expected:

- `caption-text-log-search.txt` is empty;
- `persisted-audio-search.txt` is empty;
- logs contain lifecycle/errors rather than audio samples or transcript text; and
- any drop/backpressure warning is included in the report with its time and whether the overlay recovered.

### WIN-PRIVACY-02 — transcript is session-only

1. Confirm Transcript currently contains finalized text.
2. Close Prollyglot completely and stop `tauri dev`.
3. Relaunch while offline.
4. Open Transcript before starting a new capture.

Expected: the installed model remains available, but the previous transcript is not automatically persisted or restored.

## 16. Run the English model catalog comparison

Record this section as **WIN-BENCH-01**.

This is required to close the full Milestone 2 model decision. If representative, shareable samples are not ready, mark this section **BLOCKED — benchmark samples/reference transcripts needed** and return the rest of the Windows evidence now.

Prepare three mono WAVs, ideally 30–90 seconds each:

- `conversation.wav` — natural conversational speech;
- `media.wav` — dialogue from ordinary media; and
- `noisy.wav` — speech over game/call/background noise.

Use audio you are authorized to process. Write a plain-text verbatim reference transcript for each. Do not commit private recordings or transcripts.

If the source is a video or stereo audio file, install a current FFmpeg build and convert only the selected excerpt:

```powershell
ffmpeg -y -ss 00:00:00 -t 00:01:00 -i "C:\path\to\source.mp4" `
  -vn -ac 1 -ar 16000 -c:a pcm_s16le "C:\path\to\conversation.wav"
```

Repeat with the correct source, start time, duration, and output name. Then run three cached comparisons per sample:

```powershell
$BenchmarkModels = Join-Path $env:LOCALAPPDATA "Prollyglot\benchmark-models"
$Sample = "C:\path\to\conversation.wav"
$ReferenceText = Get-Content "C:\path\to\conversation-transcript.txt" -Raw

1..3 | ForEach-Object {
  cargo run --release --locked -p prollyglot-asr-sherpa --example compare_models -- `
    $BenchmarkModels $Sample $ReferenceText *>&1 |
    Tee-Object -FilePath (Join-Path $EvidenceRoot "conversation-model-run-$_.txt")
}
```

Repeat for `media.wav` and `noisy.wav`. The first invocation downloads/verifies all three pinned product models; exclude that download time when comparing cached preparation. While each command runs, observe peak memory and average CPU in Task Manager if the sample is long enough.

Retain this table:

| Category/model | Download MiB | Cached prepare | Load | RTF | First partial audio | WER | Peak memory | Avg CPU | Transcript notes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Conversation — Small |  |  |  |  |  |  |  |  |  |
| Conversation — Standard |  |  |  |  |  |  |  |  |  |
| Conversation — Enhanced |  |  |  |  |  |  |  |  |  |
| Media — Small |  |  |  |  |  |  |  |  |  |
| Media — Standard |  |  |  |  |  |  |  |  |  |
| Media — Enhanced |  |  |  |  |  |  |  |  |  |
| Noisy — Small |  |  |  |  |  |  |  |  |  |
| Noisy — Standard |  |  |  |  |  |  |  |  |  |
| Noisy — Enhanced |  |  |  |  |  |  |  |  |  |

Every advertised choice must have RTF below 1.0 to be at least real-time on the reference machine. Do not select a larger default from one clean clip alone; compare quality/latency gains against load time, memory, CPU, and download size across all three categories.

## 17. Finish and report the run

1. Stop captions and close Prollyglot.
2. Press `Ctrl+C` in the `tauri dev` terminal.
3. Copy the final Prollyglot logs into `$EvidenceRoot` if section 15 did not already do so.
4. Save the development-terminal output as `tauri-dev.txt` if any warning or failure occurred.
5. Run:

   ```powershell
   git status --short | Tee-Object -FilePath (Join-Path $EvidenceRoot "worktree-after-test.txt")
   Stop-Transcript
   ```

6. Confirm the test did not modify tracked repository files. Keep the installed model unless you specifically want to repeat first-run behavior.

Return this summary together with the relevant evidence files:

```text
Prollyglot Windows validation report

Date/time:
Tested commit:
Windows edition/display version/build:
CPU / RAM / GPU:
Power: AC or battery
Monitors, resolutions, and scales:
Playback endpoints:
Applications used:
Protected-media source tested (yes/no; service/title can remain private):

WIN-BUILD-01:
WIN-LAUNCH-01:
WIN-MODEL-01 through 05:
WIN-ASR-01:
WIN-SOURCES-01:
WIN-SYSTEM-01 through 04:
WIN-DEVICE-01 through 04:
WIN-APP-01 through 05:
WIN-OVERLAY-01 through 04:
WIN-KEYBOARD-01:
WIN-OBS-01 and 02, including parity matrix:
WIN-LATENCY-01 and latency table:
WIN-SOAK-01 and resource table:
WIN-PRIVACY-01 and 02:
WIN-BENCH-01 model-comparison table or BLOCKED reason:

Failures, exact UI messages, reproduction steps, and nearby log lines:
Anything OBS captured that Prollyglot did not:
Any exclusive-fullscreen, DPI, Bluetooth, or device-recovery limitation:
```

## Release-gate interpretation

The current Windows caption slice is ready to move beyond Milestone 2 only when:

- the complete local check passes;
- model install/interruption/offline/remove behavior passes;
- selected-device capture and application isolation pass;
- no equivalent OBS path captures meaningful audio that Prollyglot misses;
- incremental/final captions, silence recovery, and ten restarts pass;
- the ordinary-window overlay and click-through path pass;
- median useful-partial latency is below two seconds on the reference machine;
- the 30-minute run has no crash, unbounded memory growth, or accumulating delay;
- logs contain neither fixture caption text nor audio; and
- representative Fast/Balanced/Enhanced evidence is retained, even if it supports keeping Fast as the default for now.

Hardware-specific conditional cases may remain **BLOCKED** only when the missing hardware is clearly recorded. All reproducible **FAIL** results remain open product work.
