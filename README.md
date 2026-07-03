# Rustpad

A simple notepad written in Rust – in two flavors:

- **`rustpad`** – terminal version (crossterm), minimal and fast
- **`rustpad-gui`** – graphical Notepad-style version (eframe/egui)

## Features (GUI)

- File: New, New Window, Open, Save, Save As, Print (via `lp`/CUPS)
- Edit: Undo/Redo, Cut/Copy/Paste, Find/Replace, Go To line, Time/Date
- Format: Word wrap, font size
- View: Zoom, status bar, **Markdown preview** (Ctrl+M) with fully rendered text
- Status bar with line/column, zoom, line-ending type and UTF-8
- Prompts to save when closing with unsaved changes

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| Ctrl+N / Ctrl+Shift+N | New file / new window |
| Ctrl+O | Open |
| Ctrl+S / Ctrl+Shift+S | Save / save as |
| Ctrl+P | Print |
| Ctrl+F / F3 / Shift+F3 | Find / find next / previous |
| Ctrl+H | Replace |
| Ctrl+G | Go to line |
| F5 | Insert time/date |
| Ctrl+M | Toggle markdown preview |
| Ctrl+Q | Exit |

The terminal version uses ^S=save, ^P=markdown preview, ^Q=quit.

## Build and install

```sh
cargo install --path .
```

Installs both `rustpad` and `rustpad-gui` to `~/.cargo/bin`.

Run with an optional file name: `rustpad-gui notes.md`

## Tests

```sh
cargo test --bin rustpad-gui
```
