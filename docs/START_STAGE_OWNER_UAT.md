# Start and Stage owner UAT

Use only the verified Windows x64 trial for this commit and its own profile. Keep the app, assets and prototype settings together. The desktop trial launcher is the intended entry point; programmatic tests do not switch the owner's current session. Keep the stable e:\deskland installation and profile intact.

Try Start from both monitors, search for Paint, press Enter and Esc, open All Apps, jump to a letter, reorder a pin and open its context menu. Compare light/dark, portrait/landscape, and 100/125/150% DPI. Check start anchors on bottom, left and right. Try only safe Settings and system controls; trial mode refuses power/session actions.

Try Stage with 1, 2, 3, 5 and 8 fixture windows; check rear clicks and the member picker beyond five. Compare close and dissolve with disposable windows. In this product candidate the DWM previews are still flat; the separate perspective fixture is evidence only, not a preview setting.

Use the paired rollback shortcut to return to production. Ctrl+Alt+Shift+F12 is the emergency taskbar restore. Do not merge PR #1 or mark owner acceptance before the owner has actually tested the candidate.
