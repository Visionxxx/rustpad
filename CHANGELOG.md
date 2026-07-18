# Changelog

All notable changes to Rustpad are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.3] – 2026-07-19

### Added

- **Right-click context menu** in the editor with Undo, Redo, Cut, Copy,
  Paste, Delete and Select All. The selection survives the right-click
  (egui normally moves the cursor on any press), so Copy/Cut act on what
  you selected — like in GTK editors.

## [0.2.2] – 2026-07-19

### Fixed

- **The scroll wheel did nothing while selecting text with the mouse.**
  egui disables wheel scrolling whenever a widget is being dragged, so
  holding a selection and spinning the wheel was ignored. Wheel input is
  now forwarded during drag-selection, and the selection follows along —
  like in other editors.

## [0.2.1] – 2026-07-11

### Fixed

- **Find Next did not scroll to matches outside the visible area.** egui
  only follows the cursor on keyboard edits, so selections set by
  find/go-to-line/replace now explicitly scroll the view to the match
  (including back up when the search wraps around). Opening a file also
  resets the view to the top instead of keeping the old scroll position.

### Added

- **MIT license** and package metadata.
- **Linux packages**: a Debian/Ubuntu `.deb` (via `cargo-deb`) and a
  portable AppImage, built automatically by a GitHub Actions workflow on
  every version tag and attached to the GitHub release.
- Stress tests: 1000-line and 10,000-line Markdown fixtures
  (`testdata/stress.md`, `testdata/stress10k.md`) driven through the real
  app — editing view, scrolling, search highlighting, select-all and
  Markdown preview — with per-frame timing assertions.

## [0.2.0] – 2026-07-10

### Fixed

- **Select All from the Edit menu did nothing.** egui drops keyboard focus
  when you click outside the text field, and the menu click counted as
  "outside" — so the selection was set but never painted. The editor now
  keeps focus while you use menus (like classic Notepad), which also fixes
  Cut/Copy/Paste from the menu leaving the editor unfocused.
- **Drag-selecting past the window edge never scrolled.** egui only follows
  the cursor on keyboard input, so the view now auto-scrolls itself while
  you drag a selection beyond the visible area (speed grows with how far
  past the edge you drag). Scrolling keeps going if the pointer briefly
  leaves the window.
- **Legacy (non-UTF-8) files opened as an empty document.** A Latin-1 file
  — common for older Norwegian text files with æøå — would silently show
  as empty, and saving would overwrite the original with nothing. Files now
  fall back to Latin-1 decoding, read errors leave the current document
  untouched, and the status bar shows the detected encoding. Files are
  always saved as UTF-8.
- The window title was re-sent every frame, forcing a continuous repaint
  (wasted CPU). It is now only sent when it actually changes.

### Added

- **Persistent settings** — font size, word wrap, status bar, line numbers,
  recent files, window size/position, zoom and theme survive restarts.
- **File → Open Recent** with the last 8 files, and a Clear List entry.
- **Drag-and-drop**: drop a file on the window to open it.
- **Change-on-disk detection**: if another program modifies the open file,
  a warning bar offers Reload / Ignore when the window regains focus.
- **Line numbers** in a gutter (View → Line Numbers). Word-wrap aware:
  wrapped continuation rows are not numbered.
- **Theme choice** (View → Theme): System, Light or Dark.
- **Ctrl+scroll zoom**, clamped to 50–400 %.
- **Find improvements**: the find bar shows a match counter ("2 of 17")
  and every match is highlighted in the text while the bar is open.
- **Application icon** — a rust-colored notepad, used both as the window
  icon and installed as SVG/PNG for the desktop launcher (`assets/`).
- **UI regression tests** using egui_kittest that drive the real menus and
  editor with simulated mouse input.

## [0.1.0] – 2026-07-04

### Added

- Initial release: a Notepad-style editor in Rust with a terminal version
  (`rustpad`) and a GUI version (`rustpad-gui`, eframe/egui).
- Open/save with unsaved-changes confirmation, find & replace
  (case-insensitive, wrap-around), go to line, word wrap, font size,
  zoom, status bar, print via `lp`, time/date insert, Markdown preview,
  new-window support and the usual keyboard shortcuts.
