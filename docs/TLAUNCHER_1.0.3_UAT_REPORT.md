# Owner test — TLauncher 1.0.3

## Superseding owner rejection — 2026-11-26

Both `442861d` and `f3bf1f5` were tested and **REJECTED** by the owner. Motion is still insufficiently smooth, colored window borders persist, and the full effects, Desktop/Explorer menus and utilities are missing. `OWNER_VISUAL_ACCEPTANCE=FAIL`; neither CI success nor this historical suggested-check list constitutes acceptance. See `FLUIDWIN_CORRECTION_EXECUTION.md`. The existing TEST shortcut is historical and is not a recommendation to repeat the rejected trial. A new comprehensive candidate must meet the correction gates first.

This branch has intermediate changes; it is not the release or a claim of FluidWin feature parity. Stable production v1.0.2 remains available. See the feature parity ledger for the scope still open.

Delivered intermediate build: `442861d7ac77beaf247c7df3123bc92cef17ef70`, [Actions run 45524299059](https://github.com/tuanngbc/TLauncher/actions/runs/45524299059). The Windows x64 package SHA-256 is `7b972b2988cc7221a250fe397d78ef5d12ed22db25043474224d21864b12abc8`. The Desktop shortcut and separate profile were staged without running the candidate.

Start from the Desktop shortcut **TLauncher 1.0.3 — TEST** once a verified candidate has been delivered. The shortcut's session requests a normal production exit, waits for it to release the desktop, and starts the candidate with a separate profile. Leave its launcher window open. Exit normally from the TLauncher menu; the launcher restores production if it was previously running. If needed, after TEST has completely exited, reopen the existing production shortcut. The candidate directory contains `uat-launch.log` if switching stops with an error. Run the candidate through the shortcut so it gets the isolated profile.

If a packaged desktop tool redirects `%LOCALAPPDATA%` into its private cache, Explorer may not see a staged candidate. Prepare the same Actions artifact with `-UatRoot 'E:\TLauncher-UAT'` when the owner has selected that folder. Use the Desktop shortcut, or the `START-TEST-<short-sha>.cmd` entry point in that folder. It records errors that happen before the candidate's launch log and leaves the window open if the switch fails.

Suggested checks:

1. In Settings → Motion effects, choose Genie or Drop. Double-click a Dock icon for an open window to minimize. Click while minimizing or when minimized to test restore. Check the most recent requested/actual effect and fallback reason under the motion diagnostics. An unchanged Windows animation should be reported as such.
2. Try on both displays, including a maximized app. Confirm focus, dimensions and taskbar return after cancellation/exit. Do not change Windows animation or other system policy as a prerequisite.
3. In both Spotlight modes, enter `25% x (3,5+0,5)`, choose the result and paste it into a blank document. Search for an indexed file or folder you know. Switch modes with F6 and repeat with the same query.
4. In Settings → Dock & Stage, enable Stage Peek. Hover a card, leave it, and click it. Check that hovering does not take focus. With a minimized source, the preview is available only if a permitted snapshot is cached.
5. In Settings → Dock & Stage → Utilities, enable Clipboard history. Copy sample text and a small image. Open the history in Control Center, copy an entry back and delete it. Turn the setting off to clear the in-memory session.

Known limits: native animation overlap can make Genie and Drop fall back. If it does, record the requested effect, actual effect and fallback reason instead of changing Windows settings. Jelly, Snap, globally animated window lifecycle and expanded Guardian are still in scope and not delivered in this milestone. The existing Ctrl+Alt+Shift+F12 recovery hotkey shows the Windows taskbar in the current session. Owner acceptance, resource measurements, demonstrations and public release are pending.

## Motion and icon follow-up

The first owner trial reported stutter and occasional colored streaks during transitions, soft Stage Peek images, and visible icon loading in Spotlight. The first trial logged median acceptance rates of approximately 37.5 Genie and 36.5 Drop submissions per second; these are not measured display frame rates. One wide window used a 360×166 capture for a 2106×974 transition. Spotlight wrote its icon assets to disk; the delay observed during that session does not establish that the cache was lost.

Follow-up candidate: `f3bf1f5bcbf1b6eb8cbb62bcba1e9d88876de4bd`, [Actions run 45530905106](https://github.com/tuanngbc/TLauncher/actions/runs/45530905106), Windows x64 package SHA-256 `63a5c010b52cc0e41fb2c5f71cc63af1a3c19c5c9630385b5508f626ac0cd0d5`. The hosted runner verified sampled Genie and Drop compositor pixels, minimize/restore/reversal through real Dock handlers, a nonactivating live Stage Peek, and an icon cache hit after a process restart. These checks do not establish visual quality, startup icon latency, or motion cadence on the owner's hardware.

Use the same window and Dock icon on both displays when comparing the follow-up. Check the colored streaks while the window is squeezed near the Dock, and check the restore handoff and reversal. Note `captureWidth`, `captureHeight`, `presentedFrames`, `transitionDurationMs` and `longestSubmitGapUs`. During Stage Peek, compare a live source with the same source minimized using the Dock. Finally, exit and reopen the TEST profile, then open both Spotlight modes and check first-page icons and a known search result. A successful warm-cache check will show disk or RAM hits without extracting that source again. Keep the same profile for that comparison.
