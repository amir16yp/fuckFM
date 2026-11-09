# fuckFM — Feature Tracker

**fuckFM** = fuck file management.

Rust-based file manager aimed at replacing:

- **Linux:** Thunar (feature parity) + optional **fuckWM** as a full window manager
- **Windows:** File Explorer / `explorer.exe` (file management first); later **`fuckwm.exe`** as Win7-style shell host (Explorer replacement — not Microsoft’s DWM compositor)

Status legend:

| Mark | Meaning |
|------|---------|
| `[ ]` | Not started |
| `[~]` | In progress |
| `[x]` | Done |
| `[-]` | Deferred / out of current scope |
| `L` | Linux / Thunar-oriented |
| `W` | Windows / Explorer-oriented |
| `B` | Both platforms |

Update this file as work lands. Prefer checking items only when the behavior is usable end-to-end on the target platform(s).

---

## 0. Project scaffolding

- [x] Rust workspace / crate layout — lib `fuckfm` + bins `fuckfm` / `fuckwm`
- [~] Cross-platform GUI shell (window, menus, basic layout) — SDL2 window + themed chrome/list MVP
- [~] Platform abstraction for filesystem + desktop integration — `src/platform` with `cfg(unix)` / `cfg(windows)`
- [x] Target-gated dependencies — `libc` only on unix; `winreg` only on windows (excluded from the other OS build)
- [x] Config / preferences store — `.ffini` binary INI (`~/.config/fuckfm/settings.ffini` / `%APPDATA%\fuckFM\`)
- [ ] Logging + crash-friendly error surfaces
- [ ] CI for Linux and Windows builds
- [x] Build scripts — `scripts/build.sh` (+ `--release` / `build-release.sh`); `install-shell.cmd` / `restore-shell.cmd` for HKCU Winlogon Shell ↔ `fuckwm.exe`
- [x] SDL2 rendering backend
- [~] Theme engine — built-ins: Win95/98, XP Luna, Win7 Aero, Modern Light/Dark + effect library; `.fficons` packs; planned: Luna variants + `.ffmtheme` (see §18)
- [x] Directory listing + folder navigation (double-click / Enter, Back, Up)

---

## 1. Window chrome & navigation UI

- [x] Optional classic framed chrome vs OS-frame-only — `B` — Settings → uncheck “Classic framed chrome” to avoid window-in-window
- [x] Settings button (theme / icon size / chrome / defaults) — `B` — theme moved off toolbar

### 1.1 Tabs (required)

- [x] New tab (`Ctrl+T`) — `B`
- [x] Close tab (`Ctrl+W`) — `B`
- [x] Open folder in new tab (context menu / middle-click) — `B` — “Open in New Tab”; middle-click folder; middle-click tab still closes
- [x] Switch tabs (`Ctrl+Tab` / `Ctrl+Shift+Tab`) — `B`
- [x] Jump to tab by number (`Ctrl+1`…`9`) — `B` — switches to tab N when it exists
- [x] Reorder tabs by drag — `B` — drag tab along strip to new position
- [x] Tab context menu: close others / close to the right — `B` — right-click tab: Close / Close Other / Close to the Right
- [x] Drag files onto a tab to switch + drop into that folder — `B` — drop onto tab; Trash tab = bin; switches to that tab
- [x] Persist last session tabs (optional preference) — `B` — Settings “Restore last session tabs”; saved on tab change / exit

### 1.2 Windows & panes

- [x] New window (`Ctrl+N`) — `B` — spawns another fuckFM at current location
- [x] Split / dual pane view (`F4`) — `B` — F4 toggles; click pane to focus; nav/ops use focused pane (F3 remains view cycle)
- [x] Side / navigation pane — `B` — Places: Home, Desktop, Trash, Root + bookmarks
- [x] Toggle navigation pane — `B` — F9 / Settings; persisted
- [x] Preview pane — `B` — Prev button / Alt+P; images + text/hex (ffmpeg media later)
- [x] Details / properties pane — `B` — Det button / Alt+Shift+P; bottom strip tracks selection (modal Properties still Alt+Enter)
- [x] Status bar (item count, selection, free space) — `B` — selection/counts + humanized selected size; free space on path folders
- [x] Compact view / denser list spacing — `B` — Settings “Compact / dense list spacing”; tighter rows + slightly smaller UI font

### 1.3 Location & history

- [x] Address / location bar (editable path) — `B` — click / Ctrl+L; Enter goes; Esc cancels; `~` / This PC / Trash
- [x] Pathbar / breadcrumb style location selector — `B` — under toolbar; click segment to navigate (This PC / Trash / path components)
- [x] Back / Up as arrow toolbar buttons — `B`
- [x] Forward arrow — `B`
- [x] History dropdown on Back/Forward — `B` — ▾ next to Back/Forward; right-click arrows; jump N steps
- [x] Go to Home / Desktop / parent / This PC — `B`
- [x] Open location by typed path / URI — `B` — via editable address bar

### 1.4 View modes (required)

Different view types are a core goal — users must be able to switch layouts like Thunar/Explorer.

- [x] Icons (grid) — `B` — icon grid + image thumbs; 1×–4× size in Settings
- [x] List — `B`
- [x] Details / columns (name, size, date) — `B` — Name / Type / Size / Modified
- [ ] Tiles / content-style views — `W`
- [x] Compact / dense spacing option — `B` — same as §1.2 Settings toggle
- [x] Sort by name, size, date modified — `B` — click Details headers (incl. Type)
- [ ] Group / arrange by (type, date, …) — `W`
- [x] Show / hide hidden files — `B` — Ctrl+H
- [x] Remember view mode per folder — `B` — saved in `.ffini` `[views]`; restored on navigate
- [x] Column chooser / reorder / resize in details view — `B` — drag dividers to resize; right-click header to show/hide Type/Size/Modified + move left/right
- [x] View mode toolbar / menu switcher — `B` — View button / F3 cycles List→Details→Icons

---

## 2. Navigation pane / places

- [x] This PC (Windows 7-style computer page) — `B`
- [x] Home — `B`
- [x] Desktop — `B`
- [x] Trash / Recycle Bin — `B` — side-pane place; Linux XDG Trash; Windows Recycle Bin listing
- [x] Mounted drives listed on This PC (label + free/total) — `B` (Linux mounts MVP)
- [x] Bookmarks / favorites / Quick access pins — `B` — side-pane ★ pins; Ctrl+B / context Add Bookmark
- [x] Drag folder to create bookmark/pin — `B` — drop folder onto empty side-pane space
- [x] Rename / remove / reorder bookmarks — `B` — right-click Rename/Remove; drag reorder; labels in `.ffini`
- [ ] Recent files / frequently used folders (Home/Quick access style) — `W` (optional `L`)
- [ ] Tree view side pane — `L` (optional `W`)
- [ ] Expand tree to current folder — `B`
- [~] Devices / volumes section — `B` — shown via This PC; side-pane device tree still open
- [x] Network browse entry — `B` — side pane + address `fuckfm:network` / Network crumbs
- [ ] Libraries (Documents, Music, Pictures, Videos aggregation) — `W`
- [ ] Cloud provider nodes (e.g. OneDrive) — `W`
- [ ] Gallery / media-centric entry — `W` (optional)
- [x] Up from filesystem root returns to This PC — `B` — Backspace / Up toolbar; trash → This PC too

---

## 3. Core file & folder operations

- [x] Open file with default app — `B` — Linux `xdg-open`/`gio`; Windows `Start-Process` / `cmd start`
- [x] Open with… / choose application — `B` — context Open with listed apps + Other (see §7)
- [x] Set default application for type — `B` — context “Set default: …”; Linux `xdg-mime default`; Windows HKCU ProgID (UAC elevate on access denied)
- [x] Open folder — `B` — Enter / double-click / context Open
- [x] Create new folder — `B` — context menu + Ctrl+Shift+N, then rename
- [x] Create new empty file / from templates — `B` — context “New Empty File” then rename; XDG/`New` templates later
- [x] Rename (dialog + F2 + context menu) — `B`
- [x] Cut / Copy / Paste — `B` — Ctrl+X / Ctrl+C / Ctrl+V
- [x] Duplicate — `B` — context + Ctrl+D; auto-renames `name (N)`
- [x] Undo / Redo of file operations — `B` — Ctrl+Z / Ctrl+Y (Ctrl+Shift+Z); rename, move, copy/duplicate, trash (XDG)
- [x] Move to Trash / Recycle Bin — `B` — Delete key + context “Send to Trash” / “Send to Recycle Bin”
- [x] Permanent delete (bypass trash) — `B` — Shift+Delete; context “Delete Permanently”; Delete while in trash
- [x] Restore from Trash / Recycle Bin — `B` — context “Restore” in trash view
- [x] Empty Trash / Recycle Bin — `B` — context on trash background
- [x] Drag & drop move / copy (with modifiers / prompts) — `B` — internal DnD onto folders / side places; Ctrl=copy; drop on Trash sends to bin
- [x] Conflict resolution on copy/move (skip, replace, rename) — `B` — Settings policy Ask/Rename/Replace/Skip; Ask shows Skip/Replace/Rename (+ Apply to all)
- [x] Progress UI for long copy/move/delete — `B` — themed progress dialog (classic blocks / Luna / Aero / modern); Cancel; per-item pump so the bar paints; delete still confirm-then-sync
- [x] Properties dialog (size, dates, type, location) — `B` — Alt+Enter / context; multi-select summary
- [x] Folder size calculation — `B` — Properties async full walk (live progress; capped at 500k entries)
- [x] Permissions / ownership editing — `L` — context Permissions (octal) + Take Ownership via pkexec; Windows Take Ownership (Admin) via UAC
- [x] Attributes / read-only / hidden flags — `W` — context toggles; `SetFileAttributes` with UAC elevate on access denied
- [x] Create symlink / shortcut — `B` — context “Create Link” (`name - link`); Windows elevates on access denied (or use Developer Mode)
- [x] Create hardlink — `B` — context “Create Hard Link” for a single file (`name - hardlink`; same FS)
- [x] Send to… menu — `B` — context Send to Desktop / Home / Documents / Downloads / …

---

## 4. Selection

- [x] Single click / double click open preference — `B` — Settings “Single-click to open”
- [x] Multi-select with Ctrl / Shift — `B`
- [x] Rubber-band / marquee select — `B` — drag empty list space; Ctrl adds to selection
- [x] Select all / invert selection — `B` — Ctrl+A select all; Ctrl+I invert
- [x] Select by pattern / glob (`Ctrl+S` Thunar-style) — `B` — `*` / `?`; bare text becomes `*text*`
- [x] Keyboard focus navigation in file list — `B` — arrows (+ grid in Icons), Home/End, PgUp/PgDn, Space toggle, type-ahead

---

## 5. Search & storage indexing (required)

Hybrid of **Everything**-style instant find and **WizTree**-style size indexing so people can locate files by name, type, size, etc.

### 5.1 Everything-like search

- [x] Toolbar search bar filters current location by name — `B` — click / Ctrl+F; substring or `*`/`?` glob; Esc clears
- [ ] Instant/indexed filename search across drives — `B`
- [x] Search by name (substring / glob / regex) — `B` — substring + glob; `regex:`/`re:` or `/pat/` (`/pat/c` case-sensitive)
- [x] Search by type / extension / MIME — `B` — `type:folder|file|image|…` / `ext:png` (MIME still open)
- [x] Search by size range — `B` — `size:>1M` / `size:<=512k` / `size:1M-10M`
- [x] Search by date modified — `B` — `mtime:today|week|7d` / `mtime:>3d` / `modified:…`
- [x] Combined filters (name + type + size + date) — `B` — whitespace tokens; name + `type:`/`ext:`/`size:`/`mtime:`
- [x] Incremental / as-you-type results — `B` — toolbar filter updates live
- [x] Search in current folder only (+ subfolders) — `B` — Ctrl+F folder; Ctrl+Shift+F recursive (capped)
- [ ] Advanced query syntax — `B`
- [ ] Background index updater (watch + rescan) — `B`

### 5.2 WizTree-like storage map

- [ ] Per-folder / per-drive size index — `B`
- [ ] Largest folders / files views — `B`
- [ ] Treemap or percentage breakdown UI — `B`
- [x] Free/used space rollup on This PC — `B` — status bar used/total/free (deduped mounts)
- [ ] Re-index / pause / exclude paths — `B`

---

## 6. Previews, thumbnails & metadata (required)

Preview many formats via **ffmpeg** and other existing tools/libraries (not a custom decoder stack).

- [x] Preview pane (show/hide) — `B`
- [x] Image preview (common formats) — `B` — png/jpeg/gif/webp/bmp/qoi via `image` crate
- [x] Text / code preview — `B` — content-sniffed text; first 64KB / 200 lines
- [x] Hex preview for binary non-image files — `B` — classic offset/hex/ASCII dump (capped)
- [ ] PDF / document preview (via existing helpers) — `B`
- [ ] Audio/video preview + poster frames via **ffmpeg** / ffprobe — `B`
- [~] Thumbnail generation (images, video posters, etc.) — `B` — Icons-view image thumbs cached; video posters still open
- [ ] Thumbnail Managing Standard compliance — `L`
- [ ] Optional Cargo features so preview backends are excluded when not compiled in — `B`
- [ ] Details pane metadata — `W`
- [ ] Emblems / folder highlighting — `L`
- [ ] Media tags view/edit (ID3 etc.) — `L` (plugin parity)
- [ ] Checksums / hash helpers — `L` (optional plugin parity)

---

## 7. Context menus & extensibility

- [~] File/folder context menu with standard actions — `B` — Open / Rename / cut / copy / paste / Send to Trash (+ trash Restore/Empty)
- [x] Open (default app / enter folder) from context menu — `B`
- [x] Open with (listed apps + Other/system chooser) — `B` — single-file selection; Linux via `gio mime` / `.desktop`; Windows OpenWithList + OpenAs
- [x] Rename from context menu — `B`
- [x] Send to Trash / Recycle Bin from context menu — `B`
- [x] Restore / Empty / Delete Permanently in trash context menu — `B`
- [x] Open Terminal Here (Linux) — `L` — right-click; uses `$TERMINAL` / xdg-terminal-exec / DE terminals
- [x] Open shell here (Windows builtins) — `W` — PowerShell / PowerShell Admin / CMD / CMD Admin (+ HKCU verb registration)
- [~] Windows registry-based shell verbs in context menu — `W` — reads HKCU/HKCR `Directory` / `Directory\Background` / `*\shell`
- [x] Ensure “Open with CMD” + “Open with CMD (Admin)” exist — `W` — registers under HKCU if missing
- [ ] Custom actions (command templates, mime/pattern filters) — `L`
- [ ] Plugin / extension API — `B`
- [ ] Archive create / extract from context menu — `B`
- [ ] Share / send via installed apps — `W` (optional `L`)
- [ ] Samba / folder sharing quick action — `L`
- [ ] VCS actions (git/svn) in context menu — `L` (optional)
- [ ] Full Windows shell extension COM hosting — `W` (later; hard; registry verbs are the near-term path)

---

## 8. Bulk rename

- [ ] Bulk rename UI — `B`
- [ ] Pattern / numbering / search-replace rename — `B`
- [ ] Preview of rename results before apply — `B`

---

## 9. Removable media & volumes

- [x] List mounted / unmounted volumes — `B` — This PC shows mounted + unmounted via lsblk/udisks / Win drives
- [x] Mount / unmount / eject — `B` — context menu + activate unmounted `/dev` (gio/udisksctl / net use)
- [x] Auto-mount on insert (configurable) — `B` — Settings → Removable media; polled insert detection
- [x] Auto-open / auto-run policy for media — `B` — off / open / ask (confirm dialog)
- [x] Drive free-space indicators — `B` — usage bars in Details + Icons for drives
- [x] Format / volume label helpers (where safe/supported) — `B` (careful UX) — confirm before format; label prompt

---

## 10. Network & remote locations

- [x] Browse network neighborhood / SMB — `B` — Network place + side pane; avahi/`smbclient` / Windows net view
- [x] Open `smb://`, `ftp://`, `sftp://`, `mtp://` (or platform equivalents) — `B` — gio mount / Windows UNC
- [x] Recent / bookmarked remote locations — `B` — settings + Network list + context bookmark
- [x] Credential prompts for remote shares — `B` — user:password prompt on auth failure

---

## 11. Clipboard, DnD & desktop protocols

- [x] Internal clipboard for file URIs — `B` — cut/copy paths + `file://` URI-list mirrored to system clipboard
- [ ] Interop with system clipboard file transfers — `B`
- [ ] Cross-app drag and drop — `B`
- [ ] X Direct Save (XDS) — `L`
- [x] File URI / path copy helpers — `B` — context Copy Path / Copy as URI; Ctrl+Shift+C / Ctrl+Shift+U

---

## 12. Keyboard shortcuts & accessibility

- [~] Full keyboard navigation — `B` — list focus nav done; remapping / a11y still open
- [ ] Shortcut editor / remapping — `L` (optional `W`)
- [~] Standard Explorer / Thunar shortcut parity where sensible — `B` — F5/Ctrl+R; Alt nav; Ctrl+Z/Y undo; Ctrl+Shift+F; more still open
- [ ] Screen reader / a11y hooks — `B`
- [ ] High-contrast / theme integration — `B`

---

## 13. Preferences & customization

- [x] Preferences dialog — `B` — Settings: theme, icon size, classic chrome, visual effects, hidden/preview/side defaults; saved as `.ffini`
- [ ] Customizable toolbar / command bar — `B`
- [x] Show/hide UI chrome toggles — `B` — classic framed chrome on/off (nested title bar)
- [x] Disable visual effects — `B` — Settings toggle gates gloss / soft shadow / modal dim / button sheen / glass
- [x] Default startup location — `B` — Settings cycle This PC / Home / Current folder (when session restore off)
- [x] Confirm-on-delete / confirm-on-drag preferences — `B` — Settings toggles; drag confirm for move/copy/trash (bookmarks skipped)
- [ ] Icon theme / emblem theme follow system — `L`
- [ ] Dark / light appearance follow system — `B`

---

## 14. Standards & platform integration (Linux)

Goal: fuckFM can be set as the **default file manager** via normal XDG conventions.

- [ ] XDG Base Directory — `L`
- [ ] Shared MIME database — `L`
- [ ] Icon Theme Specification — `L`
- [ ] Desktop Trash Can Specification — `L`
- [ ] File URI Specification — `L`
- [ ] `.desktop` app entry (`inode/directory` handler, `%U`/`%F`) — `L`
- [ ] `xdg-mime default … inode/directory` / DE default-apps integration — `L`
- [ ] DBus `org.freedesktop.FileManager1` (ShowFolders / ShowItems / ShowItemProperties) — `L`
- [ ] Single-instance activation when opened as default FM — `L`
- [ ] Trash panel / desktop environment hooks (optional) — `L`
- [ ] Daemon / single-instance mode for volume events — `L`

---

## 15. Windows file-management specifics

Goal: fuckFM can replace Explorer as the **default folder handler** (file management); full `explorer.exe` shell is section 17.

- [x] Recycle Bin semantics — `W` — send / list / restore / empty / permanent delete via Shell
- [ ] Libraries (`.library-ms` style aggregation) — `W`
- [ ] Quick access pin / unpin / frequent+recent — `W`
- [ ] Home search (local + cloud accounts) — `W`
- [ ] OneDrive / Files On-Demand indicators — `W`
- [ ] ZIP folder browsing (compressed folder) — `W`
- [ ] File type associations via system defaults — `W`
- [x] “This PC” / Devices and drives view — `W` — known folders + drive letters via GetDriveType / GetDiskFreeSpaceEx / volume labels
- [x] Known folders (Documents, Downloads, …) — `W` — Shell Folders registry + side pane places
- [x] Register as default folder/file browser (user choice / HKCU classes) — `W` — `fuckfm --register-folder-handler` / `scripts/register-fm.cmd` (Directory + Drive + Background)
- [x] Compact view — `B` — Settings compact spacing
- [ ] Share ribbon/button integration — `W`
- [ ] Optional classic menu bar — `W`

---

## 16. Performance & reliability

- [ ] Fast startup
- [ ] Async directory listing (UI stays responsive)
- [ ] Lazy thumbnail loading / cache
- [ ] Large-directory handling (virtualized list)
- [ ] Cancellation of in-flight I/O / searches / copies
- [ ] Safe handling of permission errors / locked files
- [ ] Crash isolation for preview / thumbnail workers

---

## 17. fuckWM — WM + Windows shell replacement

> One shell/WM host binary: **`fuckwm`** on Linux and Windows. Phase 0 is a nested SDL **shell preview** plus a Win32 desktop/taskbar host. On Windows this replaces **Explorer as shell**, not Microsoft’s Desktop Window Manager compositor.

### 17.0 Binary / packaging

- [x] Separate WM binary (not the FM process) — `B` — `cargo run --bin fuckwm`; shared lib with fuckFM
- [~] Windows: `fuckwm.exe` as the shell host — `W` — Win32 desktop icons + Start menu + clock taskbar + WGL; Explorer shell install via `install-shell.cmd`
- [~] Linux: installable **fuckWM** WM/compositor session — `L` — X11 WM + EGL/GLES2; `--xephyr` uses `extra/xorg-server-xephyr` + `-glamor`; Wayland later
- [x] Hardware-accelerated chrome (native mode) — `B` — EGL/XCB (Linux) / WGL (Windows); software fallback if GPU init fails; SDL mode unchanged
- [~] Shared chrome/theme/cursor pipeline with fuckFM (`.ffmtheme`) — `B` — shared theme engine now; `.ffmtheme` / cursors later
- [~] Session handoff: FM as folder handler vs fuckWM as shell/WM — `B` — Start/taskbar launches fuckFM; `--sdl` preview fallback

### 17.1 Desktop shell

- [~] Desktop surface (wallpaper host, icon layer) — theme defaults; Windows registry wallpaper+style; Linux gsettings (GNOME/MATE); SDL / Win32 GDI / X11 PutImage paint
- [x] Desktop icon arrange / auto-arrange / align to grid — `B` — SDL: Auto / Free / Align to grid via desktop context menu
- [x] Desktop context menu (display settings, personalize, new item, …) — `B` — SDL + Win32: Refresh / Open fuckFM / New Folder / arrange / Personalize / Show Desktop / Exit
- [x] Show desktop / peek desktop — `B` — taskbar right strip + Win/Alt+D; peek hover on strip (SDL)

### 17.2 Window manager (Win7-class shell + Linux WM)

- [~] Window decoration / titlebars using active theme — `B` — X11 framed clients + titlebar/close (Win32 client framing later)
- [~] Move / resize / snap / maximize / minimize / close — `B` — X11 move + close; resize/snap later
- [x] Alt-Tab / window switcher — `B` — SDL themed overlay (cycle / click / Enter); Task View reuses it
- [ ] Virtual desktops / workspaces — `B`
- [~] Focus policy + stacking — `B` — X11 enter-focus + raise; refine later
- [~] Windows: Win7-class shell host session (Explorer replacement) where feasible — `W` — `fuckwm.exe` via Winlogon Shell; cannot replace Microsoft’s DWM compositor
- [ ] Linux: full WM session (start from display manager / `.desktop` session) — `L`
- [ ] Multi-monitor layout — `B`

### 17.3 Taskbar

- [~] Taskbar host — X11/Win32 native strips + SDL preview (theme-accurate face per ThemeId)
- [~] Pinned apps — fuckFM pin only (MVP)
- [x] Running app buttons / grouping / badges — `B` — SDL: per-theme buttons + badge chip; minimize/restore toggle
- [x] Jump lists — `B` — right-click pin/running → Open fuckFM / This PC / Close
- [x] Task View / virtual desktops entry — `B` — TV button + flyout; opens Alt-Tab overview (single desktop MVP)
- [x] System tray / notification area — `B` — chevron + net/vol/bat chips
- [x] Clock / calendar flyout — `B` — themed clock; click opens month calendar flyout (SDL)
- [x] Network / volume / battery quick status — `B` — tray probes (Linux sysfs net/battery; stubs elsewhere)
- [x] Quick Settings / Notification Center equivalents — `B` — tray flyout with status + notifications stub
- [x] Widgets entry (optional parity) — `B` — Wid button + clock/net/battery widgets panel

### 17.4 Start & search shell

- [~] Start menu / Start button (logo from active `.ffmtheme`) — themed Start (classic flag / Luna green / Aero pearl / modern flat) + caption-gradient brand strip; places + Cycle theme + power + Exit
- [~] Pinned / recommended apps — fuckFM + places in Start
- [~] Power actions (sleep, shutdown, restart) — Win32 Start: Log off / Restart / Shut down via `shutdown.exe`
- [ ] Shell search (apps, settings, files)
- [ ] Win+X / quick link menu — `W`

### 17.5 Shell namespace & system UX

- [-] Shell namespace extensions hosting — `W`
- [-] AutoPlay handler — `W`
- [-] Control Panel / Settings deep links from shell — `W`
- [~] Set `fuckwm.exe` as Windows shell (replacement for `explorer.exe` / Win7 shell session) — `W` — `scripts/install-shell.cmd` + `restore-shell.cmd` (HKCU Winlogon Shell); not a full session stack yet
- [-] Recovery path if shell / WM fails to start — `B`

---

## 18. Themes & binary theme packs

> Built-in palettes already cover classic Win9x + modern light/dark. Expand to Aero/Luna and a first-party binary theme format shared by the FM and fuckWM.

### 18.1 Built-in looks

- [x] Windows 95 — `B`
- [x] Windows 98 — `B`
- [x] Windows 7 Aero — `B` — soft glass title gradient + pearl chrome (opaque approx; real blur later)
- [x] Modern Light — `B`
- [x] Modern Dark — `B`
- [x] Windows XP Luna (blue) — `B` — olive/silver variants still open

### 18.1b Effect library

- [x] `EffectSet` (title gradient/gloss, soft shadow, modal dim, button sheen, glass chrome) — `B`
- [x] Theme presets: classic / luna / aero / modern / none — `B`
- [x] Runtime gate via Settings → Visual effects — `B`

### 18.1c Binary icon packs (`.fficons`)

- [x] Format: `FFICONS\0` + zlib body of purpose / len / QOI — `B`
- [x] Loader/writer + resolve by purpose@size — `B`
- [x] `b00merang2fficons` converter for B00merang / freedesktop themes — `B` — size/context + context/size layouts
- [x] Bundled packs in `assets/icons/` (XP / 7 / 10; Win95/98 share XP until a 9x set exists) — `B`
- [x] Theme switch reloads matching pack — `B`
- [x] FM + fuckWM + Win32 desktop/taskbar draw from pack — `B`

### 18.2 Binary theme file format (`.ffmtheme`)

Own binary theme pack format (not MS `.msstyles`). Embeds chrome styles **and** shell assets so FM + fuckWM load one pack.

- [-] Spec + loader/writer for `.ffmtheme` — `B`
- [-] Pack sections: chrome/colors/metrics, fonts, Start logo (bitmap), cursor set, optional sounds — `B`
- [-] Default pack = Win95-esque theme with embedded classic Start logo + Win95 `.cur` cursors — `B`
- [-] Theme browser / picker loads built-ins + user `.ffmtheme` files — `B`
- [-] Hot-swap theme without restart (FM; fuckWM where safe) — `B`

### 18.3 Cursor packs (`.cur` / related)

- [-] Embed cursors inside `.ffmtheme` (default: Win95 cursors in the default Win95-esque pack) — `B`
- [-] Parse / use classic Windows `.cur` (and common variants from Win95 through Win7/modern where practical) — `B`
- [-] Per-theme cursor map: arrow, wait, size NESW/NWSE/NS/EW, hand, text I-beam, etc. — `B`
- [-] Apply cursors in FM window; fuckWM applies system/session cursors when acting as shell/WM — `B`

---

## Suggested build order

1. Scaffolding + single-window browser (list dir, open folders)
2. Core ops (copy/move/rename/trash) + selection + DnD
3. **Tabs** + navigation history + side pane/bookmarks + This PC
4. Context menus (Linux terminal; Windows registry shell verbs)
5. View modes + preview pane (ffmpeg-gated feature)
6. Everything-like index search + WizTree-like size index
7. Default FM registration (XDG / Windows folder handler)
8. Volumes / network / Open With / preferences
9. Bulk rename + custom actions / plugins
10. Theme packs: Aero + Luna + `.ffmtheme` + `.cur` embedding (section 18)
11. Only then: fuckWM WM + Windows Explorer shell replacement (section 17)

---

## Notes

- Parity target is **behavioral**, not pixel-perfect cloning of Thunar or Explorer.
- Where Thunar and Explorer disagree, prefer the clearer UX and keep a preference if both are common.
- Items marked `L` or `W` may still ship on the other OS when the concept maps cleanly.
