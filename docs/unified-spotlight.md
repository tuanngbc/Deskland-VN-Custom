# Unified Spotlight (1.0.2 candidate)

Unified Spotlight runs on TLauncher's existing UI thread. Compact is the original quick search surface; the full-screen native surface adds a paginated application library and an editable Windows search field. The Dock sends its physical monitor position to the same Spotlight owner. Start retains its own surface and Windows-key policy.

- `catalog.rs` builds shared snapshots from Start Menu entries and Windows AppsFolder (packaged apps). Discovery and scoped file enumeration run off the UI thread. Shell notifications while the library is open, cache expiry, Explorer recovery and manual refresh trigger refreshes.
- Both Spotlight views reuse application identity, the bounded icon service, the launch resolver and ranked Unicode matching. Files use the existing local directories; the library does not scan all drives.
- `fullscreen.rs` owns native overlay geometry, search generations, keyboard navigation and catalog editing. It requests Desktop Acrylic through DWM when the OS supports it and uses a theme-colored fallback. The compositor controls blur; there is no simulated blur setting.
- `library.rs` stores logical ordering and folder membership. The existing preserving config writer saves `spotlight` and `spotlightLibrary` in the active profile. Custom icons and Dock pins reuse their current TLauncher settings. Hiding or removing a library entry never deletes the original file.

## Owner UAT

Only the owner may choose to download and run an accepted GitHub Actions build. The developer must perform no local builds, automated tests, or trial executable launches. Use the Windows x64 candidate with its full directory structure. Exit the current TLauncher from its tray menu before opening the candidate with `--trial-dir` and an empty absolute folder named `tuanpc-desk-trial-…`. The singleton prevents the two editions from running together. Keep `prototype/` and `assets/widget/` beside `TLauncher.exe`. After testing, exit the candidate and open the installed TLauncher again. No release upgrade or startup migration is required for UAT.

1. Press **Alt+Space** for Compact. Type a query, press **F6** to expand without clearing it, and press **F6** again to return. **Esc** closes the overlay, or goes back from a folder.
2. Click the Dock magnifier on each monitor. Only that monitor should be covered. Use **Settings → Shortcuts & Utilities → Unified Spotlight** to choose Dock behavior, theme, sizes and labels.
3. Test an installed Win32 app, an AppsFolder app, a portable EXE and a LNK with arguments. Verify Add, Pin, Hide/Restore, New Folder, drag, remove, and the app-specific menu. No automated test launches the owner's apps.
4. Check real Acrylic over the wallpaper and an open window, focus return, portrait and landscape displays at mixed scales, and behavior during a display change. Report any unreadable areas or missing apps with personal names hidden in captures.

## Evidence boundaries

The Windows workflow distinguishes Rust unit tests from the isolated native Spotlight fixture. Fixture PNGs exercise the native renderer on a private desktop with synthetic labels and a forced solid fallback. They do not prove OS Acrylic, real desktop focus, packaged app discovery, the owner's monitor topology, or performance relative to 1.0.1. The candidate remains separate from the public release until owner acceptance.
