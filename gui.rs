// Rustpad GUI – a Notepad-style text editor (eframe/egui)
use eframe::egui;
use std::io::Write as _;
use std::path::PathBuf;

fn editor_id() -> egui::Id {
    egui::Id::new("rustpad_editor")
}

// byte index of char number i (same trick as in main.rs)
fn byte_idx(s: &str, i: usize) -> usize {
    s.char_indices().nth(i).map(|(b, _)| b).unwrap_or(s.len())
}

// lowercase for case-insensitive search
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn main() -> eframe::Result {
    let file = std::env::args().nth(1).map(PathBuf::from);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([300.0, 200.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Rustpad",
        opts,
        Box::new(|cc| {
            configure_focus(&cc.egui_ctx);
            Ok(Box::new(Notepad::new(file).restore(cc.storage)))
        }),
    )
}

// A little rust-colored notepad icon (rounded square with text lines),
// drawn procedurally so we don't need an image-decoding dependency.
fn app_icon() -> egui::IconData {
    const S: usize = 64;
    let corner_r = 12.0f32;
    let mut rgba = Vec::with_capacity(S * S * 4);
    for y in 0..S {
        for x in 0..S {
            let (xf, yf) = (x as f32 + 0.5, y as f32 + 0.5);
            // distance to the rounded-square outline
            let cx = xf.clamp(corner_r, S as f32 - corner_r);
            let cy = yf.clamp(corner_r, S as f32 - corner_r);
            let inside = (xf - cx).hypot(yf - cy) <= corner_r;
            let line = matches!(y, 17..=21 | 29..=33 | 41..=45) && (12..=51).contains(&x);
            let short = y >= 41 && x > 38; // last text line is shorter
            let px: [u8; 4] = if !inside {
                [0, 0, 0, 0]
            } else if line && !short {
                [255, 243, 231, 255] // cream text lines
            } else {
                [183, 65, 14, 255] // rust
            };
            rgba.extend_from_slice(&px);
        }
    }
    egui::IconData { rgba, width: S as u32, height: S as u32 }
}

// Read a file as UTF-8, falling back to Latin-1 (ISO-8859-1) so legacy
// files don't silently open as an empty document.
fn read_text(p: &std::path::Path) -> std::io::Result<(String, &'static str)> {
    let bytes = std::fs::read(p)?;
    match String::from_utf8(bytes) {
        Ok(s) => Ok((s, "UTF-8")),
        Err(e) => Ok((e.into_bytes().iter().map(|&b| b as char).collect(), "Latin-1")),
    }
}

fn mtime(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

// All (non-overlapping) matches of `query` in `text`, as
// (char position, byte start, byte end). Skips huge documents to stay snappy.
fn find_matches(text: &str, query: &str, match_case: bool) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    if query.is_empty() || text.len() > 1_000_000 {
        return out;
    }
    let norm = |c: char| if match_case { c } else { lower(c) };
    let t: Vec<(usize, char)> = text.char_indices().map(|(b, c)| (b, norm(c))).collect();
    let q: Vec<char> = query.chars().map(norm).collect();
    let n = q.len();
    if n == 0 || n > t.len() {
        return out;
    }
    let mut i = 0;
    while i + n <= t.len() {
        if t[i..i + n].iter().map(|&(_, c)| c).eq(q.iter().copied()) {
            let end = t.get(i + n).map_or(text.len(), |&(b, _)| b);
            out.push((i, t[i].0, end));
            i += n;
        } else {
            i += 1;
        }
    }
    out
}

// egui normally drops keyboard focus when you click outside the focused widget.
// That made menu actions like Edit → Select All appear to do nothing: the click
// on the menu item stole focus from the editor, and without focus the selection
// is never painted. Keep focus until another widget explicitly takes it.
fn configure_focus(ctx: &egui::Context) {
    ctx.options_mut(|o| {
        o.input_options.surrender_focus_on = egui::SurrenderFocusOn::Never;
    });
}

struct Notepad {
    path: Option<PathBuf>,
    text: String,
    saved: String, // the content as it was last saved/opened
    status_msg: String,
    allow_close: bool,
    // find and replace
    show_find: bool,
    show_replace: bool,
    focus_find: bool,
    query: String,
    replacement: String,
    match_case: bool,
    // go to line
    show_goto: bool,
    goto_input: String,
    // format and view
    word_wrap: bool,
    show_status: bool,
    font_size: f32,
    show_line_numbers: bool,
    // markdown preview
    show_md: bool,
    md_cache: egui_commonmark::CommonMarkCache,
    // scroll direction while drag-selecting past the edge of the view
    drag_scroll: egui::Vec2,
    last_title: String,
    // encoding the file had on disk (we always save UTF-8)
    encoding: &'static str,
    // detecting edits made to the file by other programs
    disk_mtime: Option<std::time::SystemTime>,
    disk_changed: bool,
    recent: Vec<PathBuf>,
    // scroll the view to the cursor on the next frame (set after find/goto)
    scroll_to_cursor: bool,
    // current editor scroll position (observed; used by tests)
    scroll_offset: egui::Vec2,
}

const MAX_RECENT: usize = 8;

impl Notepad {
    fn new(path: Option<PathBuf>) -> Self {
        let mut pad = Notepad {
            path: None,
            saved: String::new(),
            text: String::new(),
            status_msg: String::new(),
            allow_close: false,
            show_find: false,
            show_replace: false,
            focus_find: false,
            query: String::new(),
            replacement: String::new(),
            match_case: false,
            show_goto: false,
            goto_input: String::new(),
            word_wrap: true,
            show_status: true,
            font_size: 14.0,
            show_line_numbers: true,
            show_md: std::env::var_os("RUSTPAD_MD").is_some(),
            md_cache: Default::default(),
            drag_scroll: egui::Vec2::ZERO,
            last_title: String::new(),
            encoding: "UTF-8",
            disk_mtime: None,
            disk_changed: false,
            recent: Vec::new(),
            scroll_to_cursor: false,
            scroll_offset: egui::Vec2::ZERO,
        };
        pad.load_from_disk(path);
        pad
    }

    // restore persisted settings (called once at startup)
    fn restore(mut self, storage: Option<&dyn eframe::Storage>) -> Self {
        if let Some(s) = storage {
            if let Some(v) = s.get_string("font_size").and_then(|v| v.parse().ok()) {
                self.font_size = f32::clamp(v, 8.0, 40.0);
            }
            let get_bool = |key: &str| s.get_string(key).and_then(|v| v.parse::<bool>().ok());
            self.word_wrap = get_bool("word_wrap").unwrap_or(self.word_wrap);
            self.show_status = get_bool("show_status").unwrap_or(self.show_status);
            self.show_line_numbers = get_bool("line_numbers").unwrap_or(self.show_line_numbers);
            if let Some(v) = s.get_string("recent") {
                self.recent = v
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(PathBuf::from)
                    .take(MAX_RECENT)
                    .collect();
            }
        }
        self
    }

    fn is_dirty(&self) -> bool {
        self.text != self.saved
    }

    fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into())
    }

    // ---------- file ----------

    fn write_file(&mut self) {
        if let Some(p) = self.path.clone() {
            match std::fs::write(&p, &self.text) {
                Ok(_) => {
                    self.saved = self.text.clone();
                    self.status_msg = "Saved!".into();
                    self.encoding = "UTF-8";
                    self.disk_mtime = mtime(&p);
                    self.disk_changed = false;
                    self.remember_recent(p);
                }
                Err(e) => self.status_msg = format!("Save failed: {e}"),
            }
        }
    }

    fn remember_recent(&mut self, p: PathBuf) {
        let p = p.canonicalize().unwrap_or(p);
        self.recent.retain(|r| r != &p);
        self.recent.insert(0, p);
        self.recent.truncate(MAX_RECENT);
    }

    fn save(&mut self) {
        if self.path.is_some() {
            self.write_file();
        } else {
            self.save_as();
        }
    }

    fn save_as(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .set_file_name(self.file_name())
            .add_filter("Text files", &["txt", "md", "ini", "toml", "conf", "cfg"])
            .add_filter("All files", &["*"])
            .save_file()
        {
            self.path = Some(p);
            self.write_file();
        }
    }

    // Load a file into the editor. Returns false if reading failed; the
    // current document is then left untouched so nothing can be overwritten.
    fn load_from_disk(&mut self, path: Option<PathBuf>) -> bool {
        let (text, encoding) = match &path {
            Some(p) => match read_text(p) {
                Ok(t) => t,
                // a path that doesn't exist yet is a new file (e.g. `rustpad-gui new.txt`)
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), "UTF-8"),
                Err(e) => {
                    self.status_msg = format!("Could not open {}: {e}", p.display());
                    return false;
                }
            },
            None => (String::new(), "UTF-8"),
        };
        self.disk_mtime = path.as_deref().and_then(mtime);
        if let Some(p) = path.clone() {
            self.remember_recent(p);
        }
        self.path = path;
        self.saved = text.clone();
        self.text = text;
        self.encoding = encoding;
        self.disk_changed = false;
        self.status_msg.clear();
        true
    }

    fn load(&mut self, ctx: &egui::Context, path: Option<PathBuf>) {
        if self.load_from_disk(path) {
            self.select_show(ctx, 0, 0);
        }
    }

    fn open(&mut self, ctx: &egui::Context) {
        if !self.confirm_unsaved() {
            return;
        }
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Text files", &["txt", "md", "ini", "toml", "conf", "cfg"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.load(ctx, Some(p));
        }
    }

    fn new_file(&mut self, ctx: &egui::Context) {
        if !self.confirm_unsaved() {
            return;
        }
        self.load(ctx, None);
    }

    fn new_window(&mut self) {
        match std::env::current_exe().and_then(|e| std::process::Command::new(e).spawn()) {
            Ok(_) => {}
            Err(e) => self.status_msg = format!("Could not open new window: {e}"),
        }
    }

    fn print(&mut self) {
        let r = std::process::Command::new("lp")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .and_then(|mut child| {
                child.stdin.take().unwrap().write_all(self.text.as_bytes())?;
                child.wait()
            });
        self.status_msg = match r {
            Ok(s) if s.success() => "Sent to printer".into(),
            _ => "Print failed (is lp/CUPS available?)".into(),
        };
    }

    // Ask about unsaved changes. Returns false if the user cancels.
    fn confirm_unsaved(&mut self) -> bool {
        if !self.is_dirty() {
            return true;
        }
        match rfd::MessageDialog::new()
            .set_title("Rustpad")
            .set_description(format!("Do you want to save changes to {}?", self.file_name()))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show()
        {
            rfd::MessageDialogResult::Yes => {
                self.save();
                !self.is_dirty() // false if saving was cancelled or failed
            }
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
    }

    // ---------- cursor ----------

    fn selection(&self, ctx: &egui::Context) -> (usize, usize) {
        egui::TextEdit::load_state(ctx, editor_id())
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let (a, b) = (r.primary.index.0, r.secondary.index.0);
                (a.min(b), a.max(b))
            })
            .unwrap_or((0, 0))
    }

    fn select(&self, ctx: &egui::Context, a: usize, b: usize) {
        let mut s = egui::TextEdit::load_state(ctx, editor_id()).unwrap_or_default();
        s.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(a),
            egui::text::CCursor::new(b),
        )));
        s.store(ctx, editor_id());
        ctx.memory_mut(|m| m.request_focus(editor_id()));
    }

    // Select AND bring the cursor into view. egui only auto-scrolls to the
    // cursor on keyboard edits, so a selection set programmatically (find,
    // go-to-line) needs an explicit scroll on the next editor frame.
    fn select_show(&mut self, ctx: &egui::Context, a: usize, b: usize) {
        self.select(ctx, a, b);
        self.scroll_to_cursor = true;
    }

    // send an event to the text field (undo, cut, paste …)
    fn send_event(&self, ctx: &egui::Context, e: egui::Event) {
        ctx.memory_mut(|m| m.request_focus(editor_id()));
        ctx.input_mut(|i| i.events.push(e));
    }

    fn send_key(&self, ctx: &egui::Context, key: egui::Key, modifiers: egui::Modifiers) {
        self.send_event(
            ctx,
            egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers },
        );
    }

    fn paste(&self, ctx: &egui::Context) {
        if let Ok(t) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            self.send_event(ctx, egui::Event::Paste(t));
        }
    }

    fn insert(&mut self, ctx: &egui::Context, s: &str) {
        let (a, b) = self.selection(ctx);
        let (ba, bb) = (byte_idx(&self.text, a), byte_idx(&self.text, b));
        self.text.replace_range(ba..bb, s);
        let c = a + s.chars().count();
        self.select_show(ctx, c, c);
    }

    // ---------- find and replace ----------

    // all (non-overlapping) matches of the current query, as
    // (char position, byte start, byte end)
    fn matches(&self) -> Vec<(usize, usize, usize)> {
        find_matches(&self.text, &self.query, self.match_case)
    }

    fn find(&mut self, ctx: &egui::Context, backwards: bool) {
        if self.query.is_empty() {
            return;
        }
        let case = self.match_case;
        let norm = |c: char| if case { c } else { lower(c) };
        let t: Vec<char> = self.text.chars().map(norm).collect();
        let q: Vec<char> = self.query.chars().map(norm).collect();
        let n = q.len();
        let not_found = format!("Cannot find \"{}\"", self.query);
        if n == 0 || n > t.len() {
            self.status_msg = not_found;
            return;
        }
        let (start, end) = self.selection(ctx);
        let last = t.len() - n; // last possible match position
        let hit = |i: usize| t[i..i + n] == q[..];
        let found = if backwards {
            (0..start.min(last + 1))
                .rev()
                .find(|&i| hit(i))
                .or_else(|| (start.min(last + 1)..=last).rev().find(|&i| hit(i)))
        } else {
            (end..=last)
                .find(|&i| hit(i))
                .or_else(|| (0..end.min(last + 1)).find(|&i| hit(i)))
        };
        match found {
            Some(i) => {
                self.select_show(ctx, i, i + n);
                self.status_msg.clear();
            }
            None => self.status_msg = not_found,
        }
    }

    fn replace_one(&mut self, ctx: &egui::Context) {
        let (a, b) = self.selection(ctx);
        if a != b {
            let selected: String = self.text.chars().skip(a).take(b - a).collect();
            let matches = if self.match_case {
                selected == self.query
            } else {
                selected.chars().map(lower).eq(self.query.chars().map(lower))
            };
            if matches {
                let (ba, bb) = (byte_idx(&self.text, a), byte_idx(&self.text, b));
                self.text.replace_range(ba..bb, &self.replacement);
                let c = a + self.replacement.chars().count();
                self.select_show(ctx, c, c);
            }
        }
        self.find(ctx, false);
    }

    fn replace_all(&mut self, ctx: &egui::Context) {
        if self.query.is_empty() {
            return;
        }
        let case = self.match_case;
        let norm = |c: char| if case { c } else { lower(c) };
        let t: Vec<char> = self.text.chars().collect();
        let tn: Vec<char> = t.iter().map(|&c| norm(c)).collect();
        let q: Vec<char> = self.query.chars().map(norm).collect();
        let n = q.len();
        let (mut out, mut i, mut count) = (String::new(), 0usize, 0usize);
        while i < t.len() {
            if i + n <= t.len() && tn[i..i + n] == q[..] {
                out.push_str(&self.replacement);
                i += n;
                count += 1;
            } else {
                out.push(t[i]);
                i += 1;
            }
        }
        if count > 0 {
            self.text = out;
            self.select(ctx, 0, 0);
        }
        self.status_msg = format!("Replaced {count} occurrence(s)");
    }

    fn go_to_line(&mut self, ctx: &egui::Context) {
        if let Ok(nr) = self.goto_input.trim().parse::<usize>() {
            let target = nr.max(1);
            let (mut idx, mut line) = (0usize, 1usize);
            for c in self.text.chars() {
                if line == target {
                    break;
                }
                idx += 1;
                if c == '\n' {
                    line += 1;
                }
            }
            if line < target {
                self.status_msg = format!("The document only has {line} lines");
            }
            self.select_show(ctx, idx, idx);
        }
        self.show_goto = false;
    }

    // ---------- keyboard shortcuts ----------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers as M};
        let sc = egui::KeyboardShortcut::new;
        // check the Shift variants first so the plain ones don't swallow them
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL | M::SHIFT, Key::S))) {
            self.save_as();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL | M::SHIFT, Key::N))) {
            self.new_window();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::SHIFT, Key::F3))) {
            self.find(ctx, true);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL, Key::S))) {
            self.save();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL, Key::N))) {
            self.new_file(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL, Key::O))) {
            self.open(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL, Key::P))) {
            self.print();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL, Key::Q))) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL, Key::F))) {
            self.show_find = true;
            self.focus_find = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL, Key::H))) {
            self.show_find = true;
            self.show_replace = true;
            self.focus_find = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL, Key::G))) {
            self.show_goto = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::CTRL, Key::M))) {
            self.show_md = !self.show_md;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::NONE, Key::F3))) {
            self.find(ctx, false);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sc(M::NONE, Key::F5))) {
            let now = chrono::Local::now().format("%H:%M %Y-%m-%d").to_string();
            self.insert(ctx, &now);
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) && !self.show_goto {
            self.show_find = false;
            self.show_replace = false;
        }
    }

    // ---------- the text editor itself ----------

    fn editor_panel(&mut self, ui: &mut egui::Ui) {
        let (wrap, font_size) = (self.word_wrap, self.font_size);
        // highlight every match while the find bar is open
        let highlight = (self.show_find && !self.query.is_empty())
            .then(|| (self.query.clone(), self.match_case));
        let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, width: f32| {
            let font = egui::FontId::monospace(font_size);
            let color = ui.visuals().text_color();
            let wrap_width = if wrap { width } else { f32::INFINITY };
            let text = buf.as_str();
            let ranges = highlight
                .as_ref()
                .map(|(q, case)| find_matches(text, q, *case))
                .unwrap_or_default();
            let job = if ranges.is_empty() {
                egui::text::LayoutJob::simple(text.to_owned(), font, color, wrap_width)
            } else {
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = wrap_width;
                let normal = egui::TextFormat::simple(font.clone(), color);
                let marked = egui::TextFormat {
                    background: ui.visuals().selection.bg_fill.gamma_multiply(0.4),
                    ..normal.clone()
                };
                let mut prev = 0;
                for &(_, start, end) in &ranges {
                    job.append(&text[prev..start], 0.0, normal.clone());
                    job.append(&text[start..end], 0.0, marked.clone());
                    prev = end;
                }
                job.append(&text[prev..], 0.0, normal);
                job
            };
            ui.ctx().fonts_mut(|f| f.layout_job(job))
        };
        egui::CentralPanel::default().show(ui, |ui| {
            // TextEdit sizes itself from a row count, so compute how many rows fill the panel
            let row_h = ui.ctx().fonts_mut(|f| f.row_height(&egui::FontId::monospace(font_size)));
            let rows = (ui.available_height() / row_h).ceil().max(1.0) as usize;
            let scroll = if wrap {
                egui::ScrollArea::vertical()
            } else {
                egui::ScrollArea::both()
            };
            let scroll_out = scroll.auto_shrink(false).show(ui, |ui| {
                // the line number gutter reserves space left of the text
                let gutter_w = if self.show_line_numbers {
                    let digits = self.text.lines().count().max(1).to_string().len().max(2);
                    let char_w = ui
                        .ctx()
                        .fonts_mut(|f| f.glyph_width(&egui::FontId::monospace(font_size), '0'));
                    digits as f32 * char_w + 16.0
                } else {
                    0.0
                };
                ui.horizontal_top(|ui| {
                    let gutter_right = ui.cursor().left() + gutter_w;
                    if gutter_w > 0.0 {
                        ui.add_space(gutter_w);
                    }
                    let edit = egui::TextEdit::multiline(&mut self.text)
                        .id(editor_id())
                        .font(egui::FontId::monospace(font_size))
                        .desired_width(f32::INFINITY)
                        .desired_rows(rows)
                        .layouter(&mut layouter);
                    let out = edit.show(ui);
                    let response = out.response.response;
                    // bring a selection made by find/go-to-line into view
                    if self.scroll_to_cursor {
                        self.scroll_to_cursor = false;
                        if let Some(range) = out.state.cursor.char_range() {
                            let rect = out
                                .galley
                                .pos_from_cursor(range.primary)
                                .translate(out.galley_pos.to_vec2() - egui::vec2(out.galley.rect.left(), 0.0));
                            // a little margin so the match isn't glued to the edge
                            ui.scroll_to_rect(rect.expand2(egui::vec2(8.0, 2.0 * row_h)), None);
                        }
                    }
                    if gutter_w > 0.0 {
                        // number the rows where a new logical line starts
                        // (wrapped continuation rows get no number)
                        let painter = ui.painter();
                        let weak = ui.visuals().weak_text_color();
                        let font = egui::FontId::monospace(font_size);
                        let clip = ui.clip_rect();
                        let mut line_no = 1usize;
                        let mut line_start = true;
                        for placed in &out.galley.rows {
                            let y = out.galley_pos.y + placed.pos.y;
                            if y > clip.bottom() {
                                break;
                            }
                            if line_start && y + row_h >= clip.top() {
                                painter.text(
                                    egui::pos2(gutter_right - 8.0, y),
                                    egui::Align2::RIGHT_TOP,
                                    line_no.to_string(),
                                    font.clone(),
                                    weak,
                                );
                            }
                            line_start = placed.ends_with_newline;
                            if placed.ends_with_newline {
                                line_no += 1;
                            }
                        }
                    }
                    self.editor_response(ui, wrap, &response);
                });
            });
            self.scroll_offset = scroll_out.state.offset;
        });
    }

    // shared handling of the editor response (status + drag auto-scroll)
    fn editor_response(&mut self, ui: &egui::Ui, wrap: bool, response: &egui::Response) {
                if response.changed() {
                    self.status_msg.clear();
                }
        // egui never scrolls while drag-selecting past the edge of the
        // view (it only follows the cursor on keyboard input), so keep
        // scrolling ourselves as long as the pointer is outside.
        if response.dragged() {
            if let Some(pos) = ui.ctx().pointer_latest_pos() {
                let visible = ui.clip_rect();
                let past_edge = |lo: f32, hi: f32, p: f32| {
                    (lo - p).max(0.0) + (hi - p).min(0.0) // >0 above/left, <0 below/right
                };
                self.drag_scroll = egui::Vec2::new(
                    if wrap { 0.0 } else { past_edge(visible.left(), visible.right(), pos.x) },
                    past_edge(visible.top(), visible.bottom(), pos.y),
                );
            }
            // when the pointer leaves the window mid-drag some platforms
            // stop sending positions; keep the last direction going
            if self.drag_scroll != egui::Vec2::ZERO {
                let delta = self.drag_scroll * 0.3; // overshoot distance controls the speed
                ui.scroll_with_delta_animation(delta, egui::style::ScrollAnimation::none());
                ui.ctx().request_repaint(); // keep scrolling while the pointer rests
            }
        } else {
            self.drag_scroll = egui::Vec2::ZERO;
        }
    }
}

impl eframe::App for Notepad {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.app_ui(ui);
    }

    // called periodically and on exit; settings survive restarts
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("font_size", self.font_size.to_string());
        storage.set_string("word_wrap", self.word_wrap.to_string());
        storage.set_string("show_status", self.show_status.to_string());
        storage.set_string("line_numbers", self.show_line_numbers.to_string());
        let recent: Vec<String> = self.recent.iter().map(|p| p.display().to_string()).collect();
        storage.set_string("recent", recent.join("\n"));
    }
}

impl Notepad {
    fn app_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);

        // Ctrl+scroll zooms, like in a browser
        let zoom = ctx.input(|i| i.zoom_delta());
        if zoom != 1.0 {
            ctx.set_zoom_factor((ctx.zoom_factor() * zoom).clamp(0.5, 4.0));
        }

        // open files dropped onto the window
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.into_iter().find_map(|f| f.path) {
            if self.confirm_unsaved() {
                self.load(&ctx, Some(path));
            }
        }

        // notice edits made by other programs when the window regains focus
        if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(true)))) {
            if let Some(p) = &self.path {
                if mtime(p) != self.disk_mtime {
                    self.disk_changed = true;
                }
            }
        }

        // ask about saving when the window closes with unsaved changes
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.confirm_unsaved() {
                self.allow_close = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        // window title like Notepad: "file.txt* – Rustpad"
        let star = if self.is_dirty() { "*" } else { "" };
        let title = format!("{}{star} – Rustpad", self.file_name());
        if title != self.last_title {
            // only on change: sending a viewport command forces a repaint,
            // and doing it every frame would keep the app repainting forever
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        // ---------- menu bar ----------
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let btn = |t: &str, s: &str| egui::Button::new(t).shortcut_text(s.to_owned());
                ui.menu_button("File", |ui| {
                    if ui.add(btn("New", "Ctrl+N")).clicked() {
                        self.new_file(&ctx);
                    }
                    if ui.add(btn("New Window", "Ctrl+Shift+N")).clicked() {
                        self.new_window();
                    }
                    if ui.add(btn("Open…", "Ctrl+O")).clicked() {
                        self.open(&ctx);
                    }
                    ui.menu_button("Open Recent", |ui| {
                        if self.recent.is_empty() {
                            ui.weak("(empty)");
                            return;
                        }
                        for p in self.recent.clone() {
                            let name = p
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| p.display().to_string());
                            let entry = ui.button(name).on_hover_text(p.display().to_string());
                            if entry.clicked() && self.confirm_unsaved() {
                                self.load(&ctx, Some(p));
                            }
                        }
                        ui.separator();
                        if ui.button("Clear List").clicked() {
                            self.recent.clear();
                        }
                    });
                    ui.separator();
                    if ui.add(btn("Save", "Ctrl+S")).clicked() {
                        self.save();
                    }
                    if ui.add(btn("Save As…", "Ctrl+Shift+S")).clicked() {
                        self.save_as();
                    }
                    ui.separator();
                    if ui.add(btn("Print", "Ctrl+P")).clicked() {
                        self.print();
                    }
                    ui.separator();
                    if ui.add(btn("Exit", "Ctrl+Q")).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    use egui::{Event, Key, Modifiers as M};
                    if ui.add(btn("Undo", "Ctrl+Z")).clicked() {
                        self.send_key(&ctx, Key::Z, M::CTRL);
                    }
                    if ui.add(btn("Redo", "Ctrl+Shift+Z")).clicked() {
                        self.send_key(&ctx, Key::Z, M::CTRL | M::SHIFT);
                    }
                    ui.separator();
                    if ui.add(btn("Cut", "Ctrl+X")).clicked() {
                        self.send_event(&ctx, Event::Cut);
                    }
                    if ui.add(btn("Copy", "Ctrl+C")).clicked() {
                        self.send_event(&ctx, Event::Copy);
                    }
                    if ui.add(btn("Paste", "Ctrl+V")).clicked() {
                        self.paste(&ctx);
                    }
                    if ui.add(btn("Delete", "Del")).clicked() {
                        self.send_key(&ctx, Key::Delete, M::NONE);
                    }
                    ui.separator();
                    if ui.add(btn("Find…", "Ctrl+F")).clicked() {
                        self.show_find = true;
                        self.focus_find = true;
                    }
                    if ui.add(btn("Find Next", "F3")).clicked() {
                        self.find(&ctx, false);
                    }
                    if ui.add(btn("Find Previous", "Shift+F3")).clicked() {
                        self.find(&ctx, true);
                    }
                    if ui.add(btn("Replace…", "Ctrl+H")).clicked() {
                        self.show_find = true;
                        self.show_replace = true;
                        self.focus_find = true;
                    }
                    if ui.add(btn("Go To…", "Ctrl+G")).clicked() {
                        self.show_goto = true;
                    }
                    ui.separator();
                    if ui.add(btn("Select All", "Ctrl+A")).clicked() {
                        self.select(&ctx, 0, self.text.chars().count());
                    }
                    if ui.add(btn("Time/Date", "F5")).clicked() {
                        let now = chrono::Local::now().format("%H:%M %Y-%m-%d").to_string();
                        self.insert(&ctx, &now);
                    }
                });
                ui.menu_button("Format", |ui| {
                    ui.checkbox(&mut self.word_wrap, "Word Wrap");
                    ui.separator();
                    if ui.button("Larger Font").clicked() {
                        self.font_size = (self.font_size + 2.0).min(40.0);
                    }
                    if ui.button("Smaller Font").clicked() {
                        self.font_size = (self.font_size - 2.0).max(8.0);
                    }
                    if ui.button("Default Font Size").clicked() {
                        self.font_size = 14.0;
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.add(btn("Zoom In", "Ctrl++")).clicked() {
                        ctx.set_zoom_factor(ctx.zoom_factor() * 1.1);
                    }
                    if ui.add(btn("Zoom Out", "Ctrl+-")).clicked() {
                        ctx.set_zoom_factor(ctx.zoom_factor() / 1.1);
                    }
                    if ui.add(btn("Reset Zoom", "Ctrl+0")).clicked() {
                        ctx.set_zoom_factor(1.0);
                    }
                    ui.separator();
                    ui.checkbox(&mut self.show_status, "Status Bar");
                    ui.checkbox(&mut self.show_line_numbers, "Line Numbers");
                    if ui
                        .add(egui::Button::selectable(self.show_md, "Markdown Preview").shortcut_text("Ctrl+M"))
                        .clicked()
                    {
                        self.show_md = !self.show_md;
                    }
                    ui.separator();
                    ui.menu_button("Theme", |ui| {
                        let mut pref = ctx.options(|o| o.theme_preference);
                        let before = pref;
                        ui.radio_value(&mut pref, egui::ThemePreference::System, "System");
                        ui.radio_value(&mut pref, egui::ThemePreference::Light, "Light");
                        ui.radio_value(&mut pref, egui::ThemePreference::Dark, "Dark");
                        if pref != before {
                            ctx.set_theme(pref);
                        }
                    });
                });
            });
        });

        // ---------- the file changed on disk ----------
        if self.disk_changed {
            egui::Panel::top("disk_changed").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("\"{}\" has changed on disk.", self.file_name()),
                    );
                    if self.is_dirty() {
                        ui.label("Reloading discards your unsaved edits!");
                    }
                    if ui.button("Reload").clicked() {
                        let p = self.path.clone();
                        self.load(&ctx, p);
                    }
                    if ui.button("Ignore").clicked() {
                        self.disk_changed = false;
                        self.disk_mtime = self.path.as_deref().and_then(mtime);
                    }
                });
            });
        }

        // ---------- find bar ----------
        if self.show_find {
            egui::Panel::top("find_bar").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Find:");
                    let field = ui.add(egui::TextEdit::singleline(&mut self.query).desired_width(180.0));
                    if self.focus_find {
                        field.request_focus();
                        self.focus_find = false;
                    }
                    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.find(&ctx, false);
                        field.request_focus();
                    }
                    if ui.button("Next").clicked() {
                        self.find(&ctx, false);
                    }
                    if ui.button("Previous").clicked() {
                        self.find(&ctx, true);
                    }
                    ui.checkbox(&mut self.match_case, "Match case");
                    if !self.query.is_empty() {
                        let matches = self.matches();
                        let (a, b) = self.selection(&ctx);
                        let current = matches
                            .iter()
                            .position(|&(pos, ..)| pos == a && pos + self.query.chars().count() == b);
                        ui.weak(match current {
                            Some(i) => format!("{} of {}", i + 1, matches.len()),
                            None => format!("{} found", matches.len()),
                        });
                    }
                    if ui.button("✕").clicked() {
                        self.show_find = false;
                        self.show_replace = false;
                    }
                });
                if self.show_replace {
                    ui.horizontal(|ui| {
                        ui.label("Replace with:");
                        ui.add(egui::TextEdit::singleline(&mut self.replacement).desired_width(180.0));
                        if ui.button("Replace").clicked() {
                            self.replace_one(&ctx);
                        }
                        if ui.button("Replace All").clicked() {
                            self.replace_all(&ctx);
                        }
                    });
                }
            });
        }

        // ---------- go to line ----------
        if self.show_goto {
            egui::Window::new("Go to line")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Line number:");
                        ui.add(egui::TextEdit::singleline(&mut self.goto_input).desired_width(80.0))
                            .request_focus();
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Go").clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            self.go_to_line(&ctx);
                        }
                        if ui.button("Cancel").clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Escape))
                        {
                            self.show_goto = false;
                        }
                    });
                });
        }

        // ---------- status bar ----------
        if self.show_status {
            egui::Panel::bottom("status").show(ui, |ui| {
                let cur = egui::TextEdit::load_state(&ctx, editor_id())
                    .and_then(|s| s.cursor.char_range())
                    .map(|r| r.primary.index.0)
                    .unwrap_or(0);
                let (mut ln, mut col) = (1usize, 1usize);
                for (i, c) in self.text.chars().enumerate() {
                    if i == cur {
                        break;
                    }
                    if c == '\n' {
                        ln += 1;
                        col = 1;
                    } else {
                        col += 1;
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(format!("Ln {ln}, Col {col}"));
                    ui.separator();
                    ui.label(format!(
                        "{} lines  {} chars",
                        self.text.lines().count().max(1),
                        self.text.chars().count()
                    ));
                    ui.separator();
                    ui.label(format!("{:.0} %", ctx.zoom_factor() * 100.0));
                    ui.separator();
                    ui.label(if self.text.contains('\r') { "CRLF" } else { "LF" });
                    ui.separator();
                    ui.label(self.encoding)
                        .on_hover_text("Files are always saved as UTF-8");
                    ui.separator();
                    ui.label(if self.is_dirty() { "Modified" } else { "Saved" });
                    if !self.status_msg.is_empty() {
                        ui.separator();
                        ui.label(&self.status_msg);
                    }
                });
            });
        }

        // ---------- markdown preview ----------
        if self.show_md {
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    egui_commonmark::CommonMarkViewer::new().show(
                        ui,
                        &mut self.md_cache,
                        &self.text,
                    );
                });
            });
            return;
        }

        self.editor_panel(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(text: &str) -> (Notepad, egui::Context) {
        let mut p = Notepad::new(None);
        p.text = text.into();
        (p, egui::Context::default())
    }

    #[test]
    fn find_case_insensitive() {
        let (mut p, ctx) = pad("Hello world, hello again");
        p.query = "HELLO".into();
        p.find(&ctx, false);
        assert_eq!(p.selection(&ctx), (0, 5));
        p.find(&ctx, false); // next match
        assert_eq!(p.selection(&ctx), (13, 18));
        p.find(&ctx, false); // wraps around to the start
        assert_eq!(p.selection(&ctx), (0, 5));
    }

    #[test]
    fn find_backwards_and_case_sensitive() {
        let (mut p, ctx) = pad("abc ABC abc");
        p.query = "ABC".into();
        p.match_case = true;
        p.find(&ctx, false);
        assert_eq!(p.selection(&ctx), (4, 7));
        p.match_case = false;
        p.find(&ctx, true); // backwards from position 4
        assert_eq!(p.selection(&ctx), (0, 3));
    }

    #[test]
    fn find_handles_non_ascii() {
        // Norwegian text exercises multi-byte UTF-8 chars
        let (mut p, ctx) = pad("Blåbærsyltetøy og blåbær");
        p.query = "BLÅBÆR".into();
        p.find(&ctx, false);
        assert_eq!(p.selection(&ctx), (0, 6));
        let (a, b) = p.selection(&ctx);
        let selected: String = p.text.chars().skip(a).take(b - a).collect();
        assert_eq!(selected, "Blåbær");
    }

    #[test]
    fn replace_all_counts_matches() {
        let (mut p, ctx) = pad("apples and Apples and APPLES");
        p.query = "apples".into();
        p.replacement = "pears".into();
        p.replace_all(&ctx);
        assert_eq!(p.text, "pears and pears and pears");
        assert_eq!(p.status_msg, "Replaced 3 occurrence(s)");
    }

    #[test]
    fn replace_one_swaps_selected_match() {
        let (mut p, ctx) = pad("one two one");
        p.query = "one".into();
        p.replacement = "three".into();
        p.find(&ctx, false); // selects the first "one"
        p.replace_one(&ctx);
        assert_eq!(p.text, "three two one");
        assert_eq!(p.selection(&ctx), (10, 13)); // next match selected
    }

    #[test]
    fn go_to_correct_line() {
        let (mut p, ctx) = pad("line1\nline2\nline3");
        p.goto_input = "3".into();
        p.show_goto = true;
        p.go_to_line(&ctx);
        assert_eq!(p.selection(&ctx), (12, 12));
        assert!(!p.show_goto);
    }

    #[test]
    fn insert_replaces_selection() {
        let (mut p, ctx) = pad("good morning");
        p.select(&ctx, 5, 12); // selects "morning"
        p.insert(&ctx, "evening");
        assert_eq!(p.text, "good evening");
        assert_eq!(p.selection(&ctx), (12, 12));
    }

    #[test]
    fn latin1_files_open_with_fallback_instead_of_empty() {
        let f = std::env::temp_dir().join("rustpad_test_latin1.txt");
        // "blåbær" as ISO-8859-1 bytes — invalid UTF-8
        std::fs::write(&f, [b'b', b'l', 0xE5, b'b', 0xE6, b'r']).unwrap();
        let p = Notepad::new(Some(f.clone()));
        std::fs::remove_file(&f).ok();
        assert_eq!(p.text, "blåbær");
        assert_eq!(p.encoding, "Latin-1");
        assert!(!p.is_dirty());
    }

    #[test]
    fn missing_file_is_a_new_empty_document() {
        let f = std::env::temp_dir().join("rustpad_does_not_exist/new.txt");
        let p = Notepad::new(Some(f.clone()));
        assert_eq!(p.text, "");
        assert_eq!(p.path, Some(f));
        assert!(p.status_msg.is_empty());
    }

    #[test]
    fn find_matches_reports_char_and_byte_positions() {
        let text = "Blåbær og blåbær";
        let m = find_matches(text, "BLÅBÆR", false);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].0, 0);
        assert_eq!(&text[m[0].1..m[0].2], "Blåbær");
        assert_eq!(m[1].0, 10); // char position, not byte position
        assert_eq!(&text[m[1].1..m[1].2], "blåbær");
        assert!(find_matches(text, "BLÅBÆR", true).is_empty());
    }
}

// UI tests: drive the real app (menus and all) with simulated mouse input via
// egui_kittest, so regressions in focus/selection/scrolling show up.
#[cfg(test)]
mod ui_tests {
    use super::*;
    use egui::Pos2;
    use egui_kittest::kittest::Queryable;
    use egui_kittest::Harness;

    fn harness(text: &str) -> Harness<'static, Notepad> {
        let mut p = Notepad::new(None);
        p.text = text.into();
        let harness = Harness::new_ui_state(|ui, p: &mut Notepad| p.app_ui(ui), p);
        configure_focus(&harness.ctx);
        harness
    }

    // a realistic click: hover, press and release on separate frames
    fn click_at(h: &mut Harness<'_, Notepad>, pos: Pos2) {
        h.hover_at(pos);
        h.step();
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
            h.step();
        }
        h.run_steps(2);
    }

    fn click_label(h: &mut Harness<'_, Notepad>, label: &str) {
        let pos = h.get_by_label(label).rect().center();
        click_at(h, pos);
    }

    // Without SurrenderFocusOn::Never, the click on the menu item steals focus
    // from the editor and the Select All highlight is never painted.
    #[test]
    fn select_all_from_the_edit_menu() {
        let mut h = harness("hello\nworld\nfoo");
        h.run();

        // click into the editor, like a user placing the cursor
        click_at(&mut h, Pos2::new(400.0, 300.0));
        assert_eq!(h.ctx.memory(|m| m.focused()), Some(editor_id()), "clicking the text should focus it");

        // menu item labels include the shortcut text
        click_label(&mut h, "Edit");
        click_label(&mut h, "Select All Ctrl+A");

        let ctx = h.ctx.clone();
        assert_eq!(h.state().selection(&ctx), (0, 15), "everything should be selected");
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(editor_id()),
            "the editor must keep focus or the selection is not painted"
        );
    }

    // Find Next must scroll the view to matches beyond the visible area
    // (egui only follows the cursor on keyboard edits, so the app does it).
    #[test]
    fn find_next_scrolls_to_offscreen_matches() {
        let text: String = (1..=300)
            .map(|i| if i % 100 == 0 { format!("needle {i}\n") } else { format!("line {i}\n") })
            .collect();
        let mut h = harness(&text);
        h.set_size(egui::Vec2::new(500.0, 300.0));
        h.run();

        h.state_mut().show_find = true;
        h.state_mut().query = "needle".into();
        h.run();

        // first match is on line 100, far below the ~15 visible rows
        click_label(&mut h, "Next");
        let first = h.state().scroll_offset.y;
        assert!(first > 0.0, "view should scroll down to the match, offset={first}");

        // next match on line 200: further down
        click_label(&mut h, "Next");
        let second = h.state().scroll_offset.y;
        assert!(second > first, "view should follow to the next match ({first} -> {second})");

        // after line 300 the search wraps to line 100: the view must jump back up
        click_label(&mut h, "Next");
        click_label(&mut h, "Next");
        let wrapped = h.state().scroll_offset.y;
        assert!(wrapped < second, "wrap-around should scroll back up ({second} -> {wrapped})");
    }

    // Stress test: run the real app over a large Markdown document and
    // time the expensive paths (layout with line numbers, search highlight,
    // select all, scrolling, markdown preview).
    #[test]
    fn stress_1000_line_markdown() {
        stress_markdown("testdata/stress.md", 1000, 500.0);
    }

    #[test]
    fn stress_10k_line_markdown() {
        stress_markdown("testdata/stress10k.md", 10_000, 2000.0);
    }

    fn stress_markdown(path: &str, expected_lines: usize, budget_ms: f64) {
        let path = format!("{}/{path}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(text.lines().count(), expected_lines);

        let mut h = harness(&text);
        h.set_size(egui::Vec2::new(900.0, 600.0));
        let time = |h: &mut Harness<'_, Notepad>, steps: usize| {
            let t = std::time::Instant::now();
            h.run_steps(steps);
            t.elapsed().as_secs_f64() * 1000.0 / steps as f64
        };

        // plain editing view (layout + line number gutter)
        let editor_ms = time(&mut h, 20);

        // scroll through the whole document
        h.hover_at(Pos2::new(450.0, 300.0));
        let t = std::time::Instant::now();
        for _ in 0..40 {
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::Vec2::new(0.0, -20.0),
                phase: egui::TouchPhase::Move,
                modifiers: Default::default(),
            });
            h.step();
        }
        let scroll_ms = t.elapsed().as_secs_f64() * 1000.0 / 40.0;

        // find bar open: every match gets highlighted while laying out
        h.state_mut().show_find = true;
        h.state_mut().query = "Rust".into();
        let find_ms = time(&mut h, 20);
        let matches = h.state().matches().len();
        assert!(matches > 50, "expected plenty of matches, got {matches}");

        // select the whole document
        let ctx = h.ctx.clone();
        let n = h.state().text.chars().count();
        h.state_mut().select(&ctx, 0, n);
        let select_ms = time(&mut h, 10);
        assert_eq!(h.state().selection(&ctx), (0, n));

        // markdown preview (egui_commonmark renders all 1000 lines)
        h.state_mut().show_find = false;
        h.state_mut().show_md = true;
        let md_ms = time(&mut h, 10);

        println!(
            "stress {expected_lines} lines (ms/frame): editor {editor_ms:.1}, scroll {scroll_ms:.1}, \
             find+highlight {find_ms:.1} ({matches} matches), select-all {select_ms:.1}, \
             markdown {md_ms:.1}"
        );
        // generous sanity bounds — catches runaway regressions, not jitter
        for (name, ms) in [
            ("editor", editor_ms),
            ("scroll", scroll_ms),
            ("find", find_ms),
            ("select-all", select_ms),
            ("markdown", md_ms),
        ] {
            assert!(ms < budget_ms, "{name} took {ms:.1} ms/frame — way too slow");
        }
    }

    // Dragging a selection past the bottom edge must scroll the view along.
    #[test]
    fn drag_select_scrolls_past_the_edge() {
        let text: String = (1..=500).map(|i| format!("line number {i}\n")).collect();
        let mut h = harness(&text);
        h.set_size(egui::Vec2::new(400.0, 200.0));
        h.run();

        h.hover_at(Pos2::new(100.0, 50.0));
        h.step();
        h.drag_at(Pos2::new(100.0, 50.0)); // press and hold
        h.step();
        h.hover_at(Pos2::new(100.0, 250.0)); // drag below the window edge
        h.run_steps(10);
        let ctx = h.ctx.clone();
        let (_, halfway) = h.state().selection(&ctx);
        h.run_steps(10);
        let (_, sel_end) = h.state().selection(&ctx);

        // without auto-scroll the selection stalls at the last visible row;
        // with it, the view follows and the selection keeps growing
        assert!(sel_end > 300, "view should scroll while dragging, got {sel_end}");
        assert!(sel_end > halfway, "selection should keep growing while the pointer rests");
    }
}
