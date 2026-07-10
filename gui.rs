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
            .with_min_inner_size([300.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Rustpad",
        opts,
        Box::new(|cc| {
            configure_focus(&cc.egui_ctx);
            Ok(Box::new(Notepad::new(file)))
        }),
    )
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
    // markdown preview
    show_md: bool,
    md_cache: egui_commonmark::CommonMarkCache,
    // scroll direction while drag-selecting past the edge of the view
    drag_scroll: egui::Vec2,
    last_title: String,
}

impl Notepad {
    fn new(path: Option<PathBuf>) -> Self {
        let text = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        Notepad {
            path,
            saved: text.clone(),
            text,
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
            show_md: std::env::var_os("RUSTPAD_MD").is_some(),
            md_cache: Default::default(),
            drag_scroll: egui::Vec2::ZERO,
            last_title: String::new(),
        }
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
        if let Some(p) = &self.path {
            match std::fs::write(p, &self.text) {
                Ok(_) => {
                    self.saved = self.text.clone();
                    self.status_msg = "Saved!".into();
                }
                Err(e) => self.status_msg = format!("Save failed: {e}"),
            }
        }
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

    fn load(&mut self, ctx: &egui::Context, path: Option<PathBuf>) {
        let text = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        self.path = path;
        self.saved = text.clone();
        self.text = text;
        self.status_msg.clear();
        self.select(ctx, 0, 0);
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
        self.select(ctx, c, c);
    }

    // ---------- find and replace ----------

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
                self.select(ctx, i, i + n);
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
                self.select(ctx, c, c);
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
            self.select(ctx, idx, idx);
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
        let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, width: f32| {
            let job = egui::text::LayoutJob::simple(
                buf.as_str().to_owned(),
                egui::FontId::monospace(font_size),
                ui.visuals().text_color(),
                if wrap { width } else { f32::INFINITY },
            );
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
            scroll.auto_shrink(false).show(ui, |ui| {
                let edit = egui::TextEdit::multiline(&mut self.text)
                    .id(editor_id())
                    .font(egui::FontId::monospace(font_size))
                    .desired_width(f32::INFINITY)
                    .desired_rows(rows)
                    .layouter(&mut layouter);
                let response = ui.add(edit);
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
            });
        });
    }
}

impl eframe::App for Notepad {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.app_ui(ui);
    }
}

impl Notepad {
    fn app_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);

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
                    if ui
                        .add(egui::Button::selectable(self.show_md, "Markdown Preview").shortcut_text("Ctrl+M"))
                        .clicked()
                    {
                        self.show_md = !self.show_md;
                    }
                });
            });
        });

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
                    ui.label("UTF-8");
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

    // Dragging a selection past the bottom edge must scroll the view along.
    #[test]
    fn drag_select_scrolls_past_the_edge() {
        let text: String = (1..=500).map(|i| format!("line number {i}\n")).collect();
        let mut h = harness(&text);
        h.set_size(egui::Vec2::new(400.0, 200.0));
        h.run();

        h.hover_at(Pos2::new(50.0, 50.0));
        h.step();
        h.drag_at(Pos2::new(50.0, 50.0)); // press and hold
        h.step();
        h.hover_at(Pos2::new(50.0, 250.0)); // drag below the window edge
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
