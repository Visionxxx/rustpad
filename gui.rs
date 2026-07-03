// Rustpad GUI – notisblokk à la Notepad (eframe/egui)
use eframe::egui;
use std::io::Write as _;
use std::path::PathBuf;

fn editor_id() -> egui::Id {
    egui::Id::new("rustpad_editor")
}

// byteindeks for tegn nr. i (samme triks som i main.rs)
fn byte(s: &str, i: usize) -> usize {
    s.char_indices().nth(i).map(|(b, _)| b).unwrap_or(s.len())
}

// liten bokstav for søk uten forskjell på store/små
fn sml(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn main() -> eframe::Result {
    let fil = std::env::args().nth(1).map(PathBuf::from);
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([300.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native("Rustpad", opts, Box::new(|_| Ok(Box::new(Pad::ny(fil)))))
}

struct Pad {
    sti: Option<PathBuf>,
    tekst: String,
    lagret: String, // innholdet slik det sist ble lagret/åpnet
    melding: String,
    lukk_ok: bool,
    // søk og erstatt
    vis_sok: bool,
    vis_erstatt: bool,
    fokus_sok: bool,
    sok: String,
    erstatt: String,
    skill_store: bool,
    // gå til linje
    vis_ga_til: bool,
    ga_til: String,
    // format og visning
    ordbryting: bool,
    vis_status: bool,
    skrift: f32,
    // markdown-visning
    vis_md: bool,
    md_cache: egui_commonmark::CommonMarkCache,
}

impl Pad {
    fn ny(sti: Option<PathBuf>) -> Self {
        let tekst = sti
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        Pad {
            sti,
            lagret: tekst.clone(),
            tekst,
            melding: String::new(),
            lukk_ok: false,
            vis_sok: false,
            vis_erstatt: false,
            fokus_sok: false,
            sok: String::new(),
            erstatt: String::new(),
            skill_store: false,
            vis_ga_til: false,
            ga_til: String::new(),
            ordbryting: true,
            vis_status: true,
            skrift: 14.0,
            vis_md: std::env::var_os("RUSTPAD_MD").is_some(),
            md_cache: Default::default(),
        }
    }

    fn endret(&self) -> bool {
        self.tekst != self.lagret
    }

    fn navn(&self) -> String {
        self.sti
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Uten navn".into())
    }

    // ---------- fil ----------

    fn skriv(&mut self) {
        if let Some(p) = &self.sti {
            match std::fs::write(p, &self.tekst) {
                Ok(_) => {
                    self.lagret = self.tekst.clone();
                    self.melding = "Lagret!".into();
                }
                Err(e) => self.melding = format!("Feil ved lagring: {e}"),
            }
        }
    }

    fn lagre(&mut self) {
        if self.sti.is_some() {
            self.skriv();
        } else {
            self.lagre_som();
        }
    }

    fn lagre_som(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .set_file_name(self.navn())
            .add_filter("Tekstfiler", &["txt", "md", "ini", "toml", "conf", "cfg"])
            .add_filter("Alle filer", &["*"])
            .save_file()
        {
            self.sti = Some(p);
            self.skriv();
        }
    }

    fn last(&mut self, ctx: &egui::Context, sti: Option<PathBuf>) {
        let tekst = sti
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        self.sti = sti;
        self.lagret = tekst.clone();
        self.tekst = tekst;
        self.melding.clear();
        self.velg(ctx, 0, 0);
    }

    fn aapne(&mut self, ctx: &egui::Context) {
        if !self.sjekk_ulagret() {
            return;
        }
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Tekstfiler", &["txt", "md", "ini", "toml", "conf", "cfg"])
            .add_filter("Alle filer", &["*"])
            .pick_file()
        {
            self.last(ctx, Some(p));
        }
    }

    fn ny_fil(&mut self, ctx: &egui::Context) {
        if !self.sjekk_ulagret() {
            return;
        }
        self.last(ctx, None);
    }

    fn nytt_vindu(&mut self) {
        match std::env::current_exe().and_then(|e| std::process::Command::new(e).spawn()) {
            Ok(_) => {}
            Err(e) => self.melding = format!("Kunne ikke åpne nytt vindu: {e}"),
        }
    }

    fn skriv_ut(&mut self) {
        let r = std::process::Command::new("lp")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .and_then(|mut barn| {
                barn.stdin.take().unwrap().write_all(self.tekst.as_bytes())?;
                barn.wait()
            });
        self.melding = match r {
            Ok(s) if s.success() => "Sendt til skriver".into(),
            _ => "Utskrift feilet (er «lp»/CUPS tilgjengelig?)".into(),
        };
    }

    // Spør om å lagre ulagrede endringer. Returnerer false hvis brukeren avbryter.
    fn sjekk_ulagret(&mut self) -> bool {
        if !self.endret() {
            return true;
        }
        match rfd::MessageDialog::new()
            .set_title("Rustpad")
            .set_description(format!("Vil du lagre endringene i {}?", self.navn()))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show()
        {
            rfd::MessageDialogResult::Yes => {
                self.lagre();
                !self.endret() // false hvis lagringen ble avbrutt/feilet
            }
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
    }

    // ---------- markør ----------

    fn markering(&self, ctx: &egui::Context) -> (usize, usize) {
        egui::TextEdit::load_state(ctx, editor_id())
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let (a, b) = (r.primary.index.0, r.secondary.index.0);
                (a.min(b), a.max(b))
            })
            .unwrap_or((0, 0))
    }

    fn velg(&self, ctx: &egui::Context, a: usize, b: usize) {
        let mut s = egui::TextEdit::load_state(ctx, editor_id()).unwrap_or_default();
        s.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(a),
            egui::text::CCursor::new(b),
        )));
        s.store(ctx, editor_id());
        ctx.memory_mut(|m| m.request_focus(editor_id()));
    }

    // send en hendelse til tekstfeltet (angre, klipp ut, lim inn …)
    fn hendelse(&self, ctx: &egui::Context, e: egui::Event) {
        ctx.memory_mut(|m| m.request_focus(editor_id()));
        ctx.input_mut(|i| i.events.push(e));
    }

    fn tast(&self, ctx: &egui::Context, key: egui::Key, modifiers: egui::Modifiers) {
        self.hendelse(
            ctx,
            egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers },
        );
    }

    fn lim_inn(&self, ctx: &egui::Context) {
        if let Ok(t) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            self.hendelse(ctx, egui::Event::Paste(t));
        }
    }

    fn sett_inn(&mut self, ctx: &egui::Context, s: &str) {
        let (a, b) = self.markering(ctx);
        let (ba, bb) = (byte(&self.tekst, a), byte(&self.tekst, b));
        self.tekst.replace_range(ba..bb, s);
        let c = a + s.chars().count();
        self.velg(ctx, c, c);
    }

    // ---------- søk og erstatt ----------

    fn finn(&mut self, ctx: &egui::Context, bakover: bool) {
        if self.sok.is_empty() {
            return;
        }
        let skill = self.skill_store;
        let norm = |c: char| if skill { c } else { sml(c) };
        let t: Vec<char> = self.tekst.chars().map(norm).collect();
        let s: Vec<char> = self.sok.chars().map(norm).collect();
        let n = s.len();
        let borte = format!("Fant ikke «{}»", self.sok);
        if n == 0 || n > t.len() {
            self.melding = borte;
            return;
        }
        let (start, slutt) = self.markering(ctx);
        let siste = t.len() - n; // siste mulige startposisjon
        let treff = |i: usize| t[i..i + n] == s[..];
        let funn = if bakover {
            (0..start.min(siste + 1))
                .rev()
                .find(|&i| treff(i))
                .or_else(|| (start.min(siste + 1)..=siste).rev().find(|&i| treff(i)))
        } else {
            (slutt..=siste)
                .find(|&i| treff(i))
                .or_else(|| (0..slutt.min(siste + 1)).find(|&i| treff(i)))
        };
        match funn {
            Some(i) => {
                self.velg(ctx, i, i + n);
                self.melding.clear();
            }
            None => self.melding = borte,
        }
    }

    fn erstatt_en(&mut self, ctx: &egui::Context) {
        let (a, b) = self.markering(ctx);
        if a != b {
            let valgt: String = self.tekst.chars().skip(a).take(b - a).collect();
            let lik = if self.skill_store {
                valgt == self.sok
            } else {
                valgt.chars().map(sml).eq(self.sok.chars().map(sml))
            };
            if lik {
                let (ba, bb) = (byte(&self.tekst, a), byte(&self.tekst, b));
                self.tekst.replace_range(ba..bb, &self.erstatt);
                let c = a + self.erstatt.chars().count();
                self.velg(ctx, c, c);
            }
        }
        self.finn(ctx, false);
    }

    fn erstatt_alle(&mut self, ctx: &egui::Context) {
        if self.sok.is_empty() {
            return;
        }
        let skill = self.skill_store;
        let norm = |c: char| if skill { c } else { sml(c) };
        let t: Vec<char> = self.tekst.chars().collect();
        let tn: Vec<char> = t.iter().map(|&c| norm(c)).collect();
        let s: Vec<char> = self.sok.chars().map(norm).collect();
        let n = s.len();
        let (mut ny, mut i, mut antall) = (String::new(), 0usize, 0usize);
        while i < t.len() {
            if i + n <= t.len() && tn[i..i + n] == s[..] {
                ny.push_str(&self.erstatt);
                i += n;
                antall += 1;
            } else {
                ny.push(t[i]);
                i += 1;
            }
        }
        if antall > 0 {
            self.tekst = ny;
            self.velg(ctx, 0, 0);
        }
        self.melding = format!("Erstattet {antall} forekomst(er)");
    }

    fn ga_til_linje(&mut self, ctx: &egui::Context) {
        if let Ok(nr) = self.ga_til.trim().parse::<usize>() {
            let maal = nr.max(1);
            let (mut idx, mut linje) = (0usize, 1usize);
            for c in self.tekst.chars() {
                if linje == maal {
                    break;
                }
                idx += 1;
                if c == '\n' {
                    linje += 1;
                }
            }
            if linje < maal {
                self.melding = format!("Dokumentet har bare {linje} linjer");
            }
            self.velg(ctx, idx, idx);
        }
        self.vis_ga_til = false;
    }

    // ---------- hurtigtaster ----------

    fn hurtigtaster(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers as M};
        let sn = egui::KeyboardShortcut::new;
        // Shift-variantene sjekkes først så de ikke slukes av de enkle
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL | M::SHIFT, Key::S))) {
            self.lagre_som();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL | M::SHIFT, Key::N))) {
            self.nytt_vindu();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::SHIFT, Key::F3))) {
            self.finn(ctx, true);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL, Key::S))) {
            self.lagre();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL, Key::N))) {
            self.ny_fil(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL, Key::O))) {
            self.aapne(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL, Key::P))) {
            self.skriv_ut();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL, Key::Q))) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL, Key::F))) {
            self.vis_sok = true;
            self.fokus_sok = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL, Key::H))) {
            self.vis_sok = true;
            self.vis_erstatt = true;
            self.fokus_sok = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL, Key::G))) {
            self.vis_ga_til = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::CTRL, Key::M))) {
            self.vis_md = !self.vis_md;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::NONE, Key::F3))) {
            self.finn(ctx, false);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&sn(M::NONE, Key::F5))) {
            let naa = chrono::Local::now().format("%H:%M %d.%m.%Y").to_string();
            self.sett_inn(ctx, &naa);
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) && !self.vis_ga_til {
            self.vis_sok = false;
            self.vis_erstatt = false;
        }
    }
}

impl eframe::App for Pad {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.hurtigtaster(&ctx);

        // Spør om lagring når vinduet lukkes med ulagrede endringer
        if ctx.input(|i| i.viewport().close_requested()) && !self.lukk_ok {
            if self.sjekk_ulagret() {
                self.lukk_ok = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        // Vindustittel som i Notepad: "fil.txt* – Rustpad"
        let stjerne = if self.endret() { "*" } else { "" };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{}{stjerne} – Rustpad",
            self.navn()
        )));

        // ---------- menylinje ----------
        egui::Panel::top("meny").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let knapp = |t: &str, s: &str| egui::Button::new(t).shortcut_text(s.to_owned());
                ui.menu_button("Fil", |ui| {
                    if ui.add(knapp("Ny", "Ctrl+N")).clicked() {
                        self.ny_fil(&ctx);
                    }
                    if ui.add(knapp("Nytt vindu", "Ctrl+Shift+N")).clicked() {
                        self.nytt_vindu();
                    }
                    if ui.add(knapp("Åpne…", "Ctrl+O")).clicked() {
                        self.aapne(&ctx);
                    }
                    ui.separator();
                    if ui.add(knapp("Lagre", "Ctrl+S")).clicked() {
                        self.lagre();
                    }
                    if ui.add(knapp("Lagre som…", "Ctrl+Shift+S")).clicked() {
                        self.lagre_som();
                    }
                    ui.separator();
                    if ui.add(knapp("Skriv ut", "Ctrl+P")).clicked() {
                        self.skriv_ut();
                    }
                    ui.separator();
                    if ui.add(knapp("Avslutt", "Ctrl+Q")).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Rediger", |ui| {
                    use egui::{Event, Key, Modifiers as M};
                    if ui.add(knapp("Angre", "Ctrl+Z")).clicked() {
                        self.tast(&ctx, Key::Z, M::CTRL);
                    }
                    if ui.add(knapp("Gjør om", "Ctrl+Shift+Z")).clicked() {
                        self.tast(&ctx, Key::Z, M::CTRL | M::SHIFT);
                    }
                    ui.separator();
                    if ui.add(knapp("Klipp ut", "Ctrl+X")).clicked() {
                        self.hendelse(&ctx, Event::Cut);
                    }
                    if ui.add(knapp("Kopier", "Ctrl+C")).clicked() {
                        self.hendelse(&ctx, Event::Copy);
                    }
                    if ui.add(knapp("Lim inn", "Ctrl+V")).clicked() {
                        self.lim_inn(&ctx);
                    }
                    if ui.add(knapp("Slett", "Del")).clicked() {
                        self.tast(&ctx, Key::Delete, M::NONE);
                    }
                    ui.separator();
                    if ui.add(knapp("Finn…", "Ctrl+F")).clicked() {
                        self.vis_sok = true;
                        self.fokus_sok = true;
                    }
                    if ui.add(knapp("Finn neste", "F3")).clicked() {
                        self.finn(&ctx, false);
                    }
                    if ui.add(knapp("Finn forrige", "Shift+F3")).clicked() {
                        self.finn(&ctx, true);
                    }
                    if ui.add(knapp("Erstatt…", "Ctrl+H")).clicked() {
                        self.vis_sok = true;
                        self.vis_erstatt = true;
                        self.fokus_sok = true;
                    }
                    if ui.add(knapp("Gå til…", "Ctrl+G")).clicked() {
                        self.vis_ga_til = true;
                    }
                    ui.separator();
                    if ui.add(knapp("Merk alt", "Ctrl+A")).clicked() {
                        self.velg(&ctx, 0, self.tekst.chars().count());
                    }
                    if ui.add(knapp("Klokkeslett/dato", "F5")).clicked() {
                        let naa = chrono::Local::now().format("%H:%M %d.%m.%Y").to_string();
                        self.sett_inn(&ctx, &naa);
                    }
                });
                ui.menu_button("Format", |ui| {
                    ui.checkbox(&mut self.ordbryting, "Ordbryting");
                    ui.separator();
                    if ui.button("Større skrift").clicked() {
                        self.skrift = (self.skrift + 2.0).min(40.0);
                    }
                    if ui.button("Mindre skrift").clicked() {
                        self.skrift = (self.skrift - 2.0).max(8.0);
                    }
                    if ui.button("Standard skrift").clicked() {
                        self.skrift = 14.0;
                    }
                });
                ui.menu_button("Vis", |ui| {
                    if ui.add(knapp("Zoom inn", "Ctrl++")).clicked() {
                        ctx.set_zoom_factor(ctx.zoom_factor() * 1.1);
                    }
                    if ui.add(knapp("Zoom ut", "Ctrl+-")).clicked() {
                        ctx.set_zoom_factor(ctx.zoom_factor() / 1.1);
                    }
                    if ui.add(knapp("Standard zoom", "Ctrl+0")).clicked() {
                        ctx.set_zoom_factor(1.0);
                    }
                    ui.separator();
                    ui.checkbox(&mut self.vis_status, "Statuslinje");
                    if ui
                        .add(egui::Button::selectable(self.vis_md, "Markdown-visning").shortcut_text("Ctrl+M"))
                        .clicked()
                    {
                        self.vis_md = !self.vis_md;
                    }
                });
            });
        });

        // ---------- søkefelt ----------
        if self.vis_sok {
            egui::Panel::top("sokefelt").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Finn:");
                    let felt = ui.add(egui::TextEdit::singleline(&mut self.sok).desired_width(180.0));
                    if self.fokus_sok {
                        felt.request_focus();
                        self.fokus_sok = false;
                    }
                    if felt.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.finn(&ctx, false);
                        felt.request_focus();
                    }
                    if ui.button("Neste").clicked() {
                        self.finn(&ctx, false);
                    }
                    if ui.button("Forrige").clicked() {
                        self.finn(&ctx, true);
                    }
                    ui.checkbox(&mut self.skill_store, "Skill store/små");
                    if ui.button("✕").clicked() {
                        self.vis_sok = false;
                        self.vis_erstatt = false;
                    }
                });
                if self.vis_erstatt {
                    ui.horizontal(|ui| {
                        ui.label("Erstatt med:");
                        ui.add(egui::TextEdit::singleline(&mut self.erstatt).desired_width(180.0));
                        if ui.button("Erstatt").clicked() {
                            self.erstatt_en(&ctx);
                        }
                        if ui.button("Erstatt alle").clicked() {
                            self.erstatt_alle(&ctx);
                        }
                    });
                }
            });
        }

        // ---------- gå til linje ----------
        if self.vis_ga_til {
            egui::Window::new("Gå til linje")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Linjenummer:");
                        ui.add(egui::TextEdit::singleline(&mut self.ga_til).desired_width(80.0))
                            .request_focus();
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Gå til").clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            self.ga_til_linje(&ctx);
                        }
                        if ui.button("Avbryt").clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Escape))
                        {
                            self.vis_ga_til = false;
                        }
                    });
                });
        }

        // ---------- statuslinje ----------
        if self.vis_status {
            egui::Panel::bottom("status").show(ui, |ui| {
                let kur = egui::TextEdit::load_state(&ctx, editor_id())
                    .and_then(|s| s.cursor.char_range())
                    .map(|r| r.primary.index.0)
                    .unwrap_or(0);
                let (mut ln, mut kol) = (1usize, 1usize);
                for (i, c) in self.tekst.chars().enumerate() {
                    if i == kur {
                        break;
                    }
                    if c == '\n' {
                        ln += 1;
                        kol = 1;
                    } else {
                        kol += 1;
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(format!("Ln {ln}, Kol {kol}"));
                    ui.separator();
                    ui.label(format!(
                        "{} linjer  {} tegn",
                        self.tekst.lines().count().max(1),
                        self.tekst.chars().count()
                    ));
                    ui.separator();
                    ui.label(format!("{:.0} %", ctx.zoom_factor() * 100.0));
                    ui.separator();
                    ui.label(if self.tekst.contains('\r') { "CRLF" } else { "LF" });
                    ui.separator();
                    ui.label("UTF-8");
                    ui.separator();
                    ui.label(if self.endret() { "Endret" } else { "Lagret" });
                    if !self.melding.is_empty() {
                        ui.separator();
                        ui.label(&self.melding);
                    }
                });
            });
        }

        // ---------- selve tekstfeltet ----------
        let (ordbryt, skrift) = (self.ordbryting, self.skrift);
        let mut oppsett = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, bredde: f32| {
            let jobb = egui::text::LayoutJob::simple(
                buf.as_str().to_owned(),
                egui::FontId::monospace(skrift),
                ui.visuals().text_color(),
                if ordbryt { bredde } else { f32::INFINITY },
            );
            ui.ctx().fonts_mut(|f| f.layout_job(jobb))
        };
        if self.vis_md {
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    egui_commonmark::CommonMarkViewer::new().show(
                        ui,
                        &mut self.md_cache,
                        &self.tekst,
                    );
                });
            });
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let h = ui.available_height();
            let rulle = if ordbryt {
                egui::ScrollArea::vertical()
            } else {
                egui::ScrollArea::both()
            };
            rulle.auto_shrink(false).show(ui, |ui| {
                let felt = egui::TextEdit::multiline(&mut self.tekst)
                    .id(editor_id())
                    .desired_width(f32::INFINITY)
                    .min_size(egui::vec2(0.0, h))
                    .layouter(&mut oppsett);
                if ui.add(felt).changed() {
                    self.melding.clear();
                }
            });
        });
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    fn pad(tekst: &str) -> (Pad, egui::Context) {
        let mut p = Pad::ny(None);
        p.tekst = tekst.into();
        (p, egui::Context::default())
    }

    #[test]
    fn finn_uten_skille_paa_store_smaa() {
        let (mut p, ctx) = pad("Hei verden, hei igjen");
        p.sok = "HEI".into();
        p.finn(&ctx, false);
        assert_eq!(p.markering(&ctx), (0, 3));
        p.finn(&ctx, false); // neste treff
        assert_eq!(p.markering(&ctx), (12, 15));
        p.finn(&ctx, false); // rundt til starten igjen
        assert_eq!(p.markering(&ctx), (0, 3));
    }

    #[test]
    fn finn_bakover_og_med_skille() {
        let (mut p, ctx) = pad("abc ABC abc");
        p.sok = "ABC".into();
        p.skill_store = true;
        p.finn(&ctx, false);
        assert_eq!(p.markering(&ctx), (4, 7));
        p.skill_store = false;
        p.finn(&ctx, true); // bakover fra posisjon 4
        assert_eq!(p.markering(&ctx), (0, 3));
    }

    #[test]
    fn finn_haandterer_norske_tegn() {
        let (mut p, ctx) = pad("Blåbærsyltetøy og blåbær");
        p.sok = "BLÅBÆR".into();
        p.finn(&ctx, false);
        assert_eq!(p.markering(&ctx), (0, 6));
        let (a, b) = p.markering(&ctx);
        let valgt: String = p.tekst.chars().skip(a).take(b - a).collect();
        assert_eq!(valgt, "Blåbær");
    }

    #[test]
    fn erstatt_alle_teller_riktig() {
        let (mut p, ctx) = pad("epler og Epler og EPLER");
        p.sok = "epler".into();
        p.erstatt = "pærer".into();
        p.erstatt_alle(&ctx);
        assert_eq!(p.tekst, "pærer og pærer og pærer");
        assert_eq!(p.melding, "Erstattet 3 forekomst(er)");
    }

    #[test]
    fn erstatt_en_bytter_markert_treff() {
        let (mut p, ctx) = pad("en to en");
        p.sok = "en".into();
        p.erstatt = "tre".into();
        p.finn(&ctx, false); // markerer første "en"
        p.erstatt_en(&ctx);
        assert_eq!(p.tekst, "tre to en");
        assert_eq!(p.markering(&ctx), (7, 9)); // neste treff markert
    }

    #[test]
    fn ga_til_riktig_linje() {
        let (mut p, ctx) = pad("linje1\nlinje2\nlinje3");
        p.ga_til = "3".into();
        p.vis_ga_til = true;
        p.ga_til_linje(&ctx);
        assert_eq!(p.markering(&ctx), (14, 14));
        assert!(!p.vis_ga_til);
    }

    #[test]
    fn sett_inn_erstatter_markering() {
        let (mut p, ctx) = pad("god morgen");
        p.velg(&ctx, 4, 10); // markerer "morgen"
        p.sett_inn(&ctx, "kveld");
        assert_eq!(p.tekst, "god kveld");
        assert_eq!(p.markering(&ctx), (9, 9));
    }
}
