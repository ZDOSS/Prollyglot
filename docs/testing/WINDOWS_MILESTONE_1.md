# Windows Milestone 1 validation

This is the manual acceptance run for Prollyglot's Windows capture foundation. It is intentionally run on a real Windows 11 desktop because WSL and hosted CI cannot prove physical audio routing, process isolation, focus behavior, or multi-monitor overlay placement.

## Prepare and launch

Prerequisites: current stable Rust with Clippy, Node.js, pnpm, and the Windows WebView2 runtime.

From a PowerShell prompt at the repository root:

```powershell
git pull origin main
pnpm --dir apps/desktop install --frozen-lockfile
./scripts/check-windows.ps1
pnpm --dir apps/desktop tauri dev
```

The local check must finish without a Rust, TypeScript, test, or lint failure. It runs on the Windows machine directly and does not consume GitHub Actions minutes.

## Capture checks

1. Compare the Playback device list with Windows Settings. Confirm the current default device is labeled “Default.”
2. Play ordinary speech through that device, choose “Everything I hear,” and start captions. The state should become Live and the mint activity treatment should react to the signal.
3. Pause playback for at least two seconds. The state should become Waiting; resuming speech should return it to Live.
4. Stop and start the same source ten times. No attempt should remain stuck in Starting or Stopping.
5. Pin a second output device. Audio sent only to the first device must not register on the second device's capture.
6. While capturing a device, disconnect or disable it. Prollyglot should remain responsive and show a technical error rather than crash.

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

## Soak and restricted-source checks

1. Run ordinary speech capture for 30 minutes. Record Prollyglot's memory near the beginning and end; it should not grow continuously.
2. If a protected-media source is available, observe and record its behavior. Silence or an unavailable-source error is an acceptable operating-system restriction; Prollyglot must not attempt to strip or bypass protection.
3. Inspect `%LOCALAPPDATA%\com.prollyglot.desktop\logs`. Logs rotate daily, retain at most seven files, and should contain lifecycle/errors only—never audio samples or caption text.

## Report back

Please return:

- Windows 11 version and whether the machine has one or multiple monitors;
- playback devices tested;
- system-output result;
- application pair and isolation result;
- ten-restart result;
- process-exit and device-removal behavior;
- overlay/click-through/multi-monitor result;
- 30-minute starting and ending memory;
- any error text plus the nearby log lines.

Milestone 1 is accepted only after these behaviors pass or any discovered limitation is explicitly recorded and handled.
