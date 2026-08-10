# Windows Milestone 1 validation

This is the manual acceptance run for Prollyglot's Windows capture foundation. It is intentionally run on a real Windows 11 desktop because WSL and hosted CI cannot prove physical audio routing, process isolation, focus behavior, or multi-monitor overlay placement.

> [!NOTE]
> This is a formal Milestone 1 acceptance summary, not the routine development loop. Use the [Windows development smoke test](WINDOWS_SMOKE_TEST.md) for ordinary builds. The [release and hardening plan](WINDOWS_TEST_PLAN.md) contains the exhaustive evidence procedure when the milestone is deliberately being closed.

## Prepare and launch

Prerequisites: current stable Rust with Clippy, Node.js, pnpm, and the Windows WebView2 runtime.

From a PowerShell prompt at the repository root:

```powershell
git pull --ff-only origin main
pnpm --dir apps/desktop install --frozen-lockfile
./scripts/check-windows.ps1
pnpm --dir apps/desktop tauri dev
```

The local check must finish without a Rust, TypeScript, test, or lint failure. It runs on the Windows machine directly and does not consume GitHub Actions minutes.

## Capture checks

1. Compare the Playback device list with Windows Settings. Confirm “Follow system default” names the current default device and that each physical or virtual endpoint can also be pinned explicitly.
2. Play ordinary speech through the default device, choose “Everything I hear” and “Follow system default,” then start captions. The state should become Live and the mint activity treatment should react to the signal.
3. Pause playback for at least two seconds. The state should become Waiting; resuming speech should return it to Live.
4. Stop and start the same source ten times. No attempt should remain stuck in Starting or Stopping.
5. Pin a second output device. Audio sent only to the first device must not register on the second device's capture.
6. While following the default device, change the Windows default to the second endpoint. Prollyglot should enter a brief recovery state and then receive audio from the new default without requiring a new session.
7. While capturing a pinned device, disconnect or disable it. Prollyglot should remain responsive and wait/retry; reconnecting the same endpoint should resume capture without requiring a new session.

## Application-isolation checks

Use two applications that can play speech independently, such as Firefox and VLC.

1. Start both applications before refreshing sources. Each should appear as one user-facing application; browser/Electron child audio processes should be grouped under their application root.
2. Select only the first application. With that application paused and the unrelated application playing, Prollyglot should settle to Waiting with no meaningful level response.
3. Play the selected application. Prollyglot should return to Live and show level activity.
4. Reverse which application is selected and repeat the isolation check.
5. Close the selected application during capture. Prollyglot should report that the source exited, remain usable, and allow a new session after sources are refreshed.

## Overlay checks

1. Open Appearance. The real transparent overlay should show the preview caption above ordinary windows.
2. Change font size, width, maximum lines, opacity, and each anchored position. The overlay should resize and remain inside the monitor work area.
3. With Click-through enabled, clicks should reach the window underneath and the overlay should not take keyboard focus.
4. Disable Click-through, drag the caption surface to a second monitor, and change an appearance control. Positioning should use that monitor's work area, including when the second monitor has negative desktop coordinates.
5. Re-enable Click-through and confirm the underlying application is interactive again.

## OBS parity, media compatibility, and soak checks

1. Install the current OBS Studio release and create an Audio Output Capture for the same endpoint. For application testing, create an Application Audio Capture for the same application. Keep OBS monitoring/output configured so it does not create a feedback path.
2. Test ordinary browser media, then any available protected-media playback, through both device-output paths. Repeat through both application-capture paths where OBS supports the application.
3. If the OBS meter receives meaningful audio while Prollyglot remains silent under the same routing, record this as a Prollyglot defect. Include the source type, endpoint, application, Prollyglot log lines, and OBS log. Do not accept the mismatch merely because the media may be protected.
4. If both applications receive audio, the source passes. If both are silent, record the source/routing compatibility limit; Prollyglot must not attempt to disable or strip protection.
5. If an application works only after the user deliberately routes it through an already-installed virtual cable, record that as a native process-capture compatibility gap. A virtual cable is an optional diagnostic/fallback, not a Prollyglot prerequisite.
6. Run ordinary speech capture for 30 minutes. Record Prollyglot's memory near the beginning and end; it should not grow continuously.
7. Inspect `%LOCALAPPDATA%\com.prollyglot.desktop\logs`. Logs rotate daily, retain at most seven files, and should contain lifecycle/errors only—never audio samples or caption text.

## Report back

Please return:

- Windows 11 version and whether the machine has one or multiple monitors;
- playback devices tested;
- system-output result;
- application pair and isolation result;
- ten-restart result;
- process-exit and device-removal behavior;
- default-device switching and endpoint-reconnection behavior;
- paired OBS device/application results for each media source tested;
- overlay/click-through/multi-monitor result;
- 30-minute starting and ending memory;
- any error text plus the nearby log lines.

Milestone 1 is accepted only after these behaviors pass or any discovered limitation is explicitly recorded and handled.
