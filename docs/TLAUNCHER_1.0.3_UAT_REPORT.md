# Owner test — TLauncher 1.0.3

This branch has intermediate changes; it is not the release or a claim of FluidWin feature parity. Stable production v1.0.2 remains available. See the feature parity ledger for the scope still open.

Start from the Desktop shortcut **TLauncher 1.0.3 — TEST** once a verified candidate has been delivered. It opens the packaged PowerShell launcher with the isolated profile and preserves the installed v1.0.2. Do not run the candidate EXE directly. Exit normally from the launcher menu. The shortcut's session requests a normal production exit, waits for the singleton and restores production if it was running before. If needed, after TEST has exited, reopen the existing production shortcut.

Suggested checks:

1. In Settings → Motion effects, choose Genie or Drop. Double-click a Dock icon for an open window to minimize. Click while minimizing or when minimized to test restore. Check the most recent requested/actual effect and fallback reason under the motion diagnostics. An unchanged Windows animation should be reported as such.
2. Try on both displays, including a maximized app. Confirm focus, dimensions and taskbar return after cancellation/exit. Do not change Windows animation or other system policy as a prerequisite.
3. In both Spotlight modes, enter `25% x (3,5+0,5)`, choose the result and paste it into a blank document. Search for an indexed file or folder you know. Switch modes with F6 and repeat with the same query.
4. In Settings → Dock & Stage, enable Stage Peek. Hover a card, leave it, and click it. Check that hovering does not take focus. With a minimized source, the preview is available only if a permitted snapshot is cached.
5. In Settings → Dock & Stage → Utilities, enable Clipboard history. Copy sample text and a small image. Open the history in Control Center, copy an entry back and delete it. Turn the setting off to clear the in-memory session.

Known limits: native animation overlap can make Genie and Drop fall back; no full animated foreign-window lifecycle or expanded Guardian yet. Owner acceptance, resource measurements, demonstrations and public release are pending.
