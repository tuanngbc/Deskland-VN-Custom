# Start visual rebuild

The new native component tree is a search/control header, a segment strip, a responsive app canvas, an optional Recent strip and a command footer. The same logical layout determines pixels, hit areas and page capacity; a narrower monitor reduces grid columns or rows before it reduces the DPI scale. Search uses a vertical result list. The app catalog uses the same icon grid as pins and adds a jump menu by initial. Empty Recents use zero height.

Start retains the native EDIT control for IME, shared Shell catalog/icons/launching, and the existing command arguments, work directory, pin, menu and power confirmation handlers. The power handler still blocks actions in isolated trials. Arrow/keyboard navigation stays separate from the search caret. Folder creation and cross-page drag are not implemented.

The Windows renderer uses the existing BGRA/GDI surface, Segoe UI Variable (12 and 14 logical px), system popup corners, and antialiased shared vector shapes. Native fixture images use a neutral light/dark gradient to avoid capturing the runner desktop. Production uses the existing in-memory frosted backdrop. A DirectWrite comparison and perceived text quality on the owner's displays remain open.

At nominal 544 dp: inner width 500, five columns, 95 dp tiles with 5 dp gaps, 77 dp pitch, 46 dp icon, 36 dp search header and 46 dp command footer. The fixture logs final dimensions and visible counts. Labels use Windows' measured ellipsis; truncation on long names is expected. Screenshot review and two-monitor interaction remain separate checks.
