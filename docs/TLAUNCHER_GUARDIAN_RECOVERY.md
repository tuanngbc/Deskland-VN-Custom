# Guardian and recovery status

The existing TLauncher taskbar watchdog owns restoration of the Explorer bars it hides. It records shell identity and initial visibility, checks that the launcher is responsive, restores on normal exit, emergency hotkey or watchdog conditions, and does not force termination in the UAT switch. This is the original taskbar recovery protocol; it does not yet recover arbitrary foreign window or system changes.

In FluidWin, `fluidwin.cpp` uses window properties, a separate Guardian and an asynchronous worker for foreign style changes. Its transition rollback unconditionally writes FALSE because the documented attribute is a selector for DwmSetWindowAttribute. TLauncher does not copy that rollback. The new animation code has no lease over foreign HWND attributes: it issues requested native show commands, owns and disposes its proxy, and stops on HWND, DPI or placement mismatch. It does not touch original cursors, Snap settings, notification policies, or Windows DWM preferences.

Remaining verification: exact opacity/style recovery before foreign mutations, termination tree coverage, UI hang, GPU loss, session lock/shutdown and Explorer changes, plus false-positive and HWND identity checks using synthetic windows in Actions. No claim of new Guardian parity or foreign-window recovery is made.
