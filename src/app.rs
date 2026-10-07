//! Ventana principal: buscador arriba, apps agrupadas por estado, atajos abajo.
//! Se maneja entera con el teclado: escribir filtra, ↑↓ elige, Intro actúa.

use crate::{
    desktop,
    store::{self, CatalogApp, InstalledMap},
    theme::{self, ACCENT, ACCENT_DIM, FAINT, LINE, MUTED, OK, OK_DIM, ON_ACCENT, SURFACE, SURFACE_HI, TEXT},
    wm,
};
use eframe::egui::{
    self, load::SizedTexture, text::LayoutJob, Align, Button, Color32, CornerRadius, FontId, Frame,
    Key, Layout, Margin, Modifiers, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextFormat,
    TextureHandle, Ui, UiBuilder, vec2,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    thread,
};

pub const APP_ID: &str = "artcraft-launcher";
const ROW_H: f32 = 66.0;
const ICON: f32 = 44.0;

enum Msg {
    /// Catálogo leído de la caché local: se muestra ya, mientras llega el de GitHub.
    Cached(Vec<CatalogApp>),
    Catalog(Result<Vec<CatalogApp>, String>),
    Installed { app: String, result: Result<String, String> },
}

struct Job {
    done: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
}

enum Action {
    Open(String),
    Install(String),
    Uninstall(String),
    Notes(String),
}

/// Estado de una app respecto a esta máquina. Decide chip, botones y sección.
enum State {
    Installed(String),
    Update { from: String, to: String },
    Available { version: String, size: u64 },
    NoLinux,
}

fn state_of(app: &CatalogApp, installed: &InstalledMap) -> State {
    match (installed.get(&app.name), &app.latest) {
        (Some(i), Some(l)) if store::is_newer(&l.version, &i.version) => {
            State::Update { from: i.version.clone(), to: l.version.clone() }
        }
        (Some(i), _) => State::Installed(i.version.clone()),
        (None, Some(l)) => State::Available { version: l.version.clone(), size: l.size },
        (None, None) => State::NoLinux,
    }
}

const SECTIONS: [&str; 3] = ["Instaladas", "Disponibles", "Sin versión para Linux"];

fn section_of(state: &State) -> usize {
    match state {
        State::Installed(_) | State::Update { .. } => 0,
        State::Available { .. } => 1,
        State::NoLinux => 2,
    }
}

pub struct Launcher {
    frames: u32,
    placed: bool,
    catalog: Vec<CatalogApp>,
    installed: InstalledMap,
    jobs: HashMap<String, Job>,
    icons: HashMap<String, Option<TextureHandle>>,
    loading: bool,
    status: String,
    query: String,
    selected: usize,
    scroll_to_selected: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl Launcher {
    pub fn new(ctx: &egui::Context, query: String) -> Self {
        theme::apply(ctx);
        let (tx, rx) = channel();
        let mut l = Self {
            frames: 0,
            placed: false,
            catalog: Vec::new(),
            installed: store::load_installed(),
            jobs: HashMap::new(),
            icons: HashMap::new(),
            loading: false,
            status: String::new(),
            query,
            selected: 0,
            scroll_to_selected: false,
            tx,
            rx,
        };
        l.refresh(ctx);
        l
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.status = "Consultando GitHub…".into();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            if let Ok(cached) = store::load_catalog(false) {
                let _ = tx.send(Msg::Cached(cached));
                ctx.request_repaint();
            }
            let _ = tx.send(Msg::Catalog(store::load_catalog(true)));
            ctx.request_repaint();
        });
    }

    fn start_install(&mut self, ctx: &egui::Context, name: &str) {
        let Some(app) = self.catalog.iter().find(|a| a.name == name) else { return };
        let Some(latest) = app.latest.clone() else { return };
        let job = Job { done: Arc::default(), total: Arc::default() };
        let (done, total) = (job.done.clone(), job.total.clone());
        self.jobs.insert(name.to_string(), job);
        let (tx, ctx, name) = (self.tx.clone(), ctx.clone(), name.to_string());
        thread::spawn(move || {
            let result = store::install(&name, &latest, done, total);
            let _ = tx.send(Msg::Installed { app: name, result });
            ctx.request_repaint();
        });
    }

    fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Cached(c) => {
                    if self.catalog.is_empty() {
                        self.catalog = c;
                    }
                }
                Msg::Catalog(r) => {
                    self.loading = false;
                    match r {
                        Ok(c) => {
                            self.status = format!("{} apps en el catálogo", c.len());
                            self.catalog = c;
                        }
                        Err(e) => self.status = format!("Error: {e}"),
                    }
                }
                Msg::Installed { app, result } => {
                    self.jobs.remove(&app);
                    self.icons.remove(&app);
                    self.installed = store::load_installed();
                    self.status = match result {
                        Ok(note) => format!("{app}: {note}"),
                        Err(e) => format!("{app}: error — {e}"),
                    };
                }
            }
        }
    }

    /// Índices del catálogo que casan con la búsqueda, agrupados por sección.
    fn visible(&self) -> Vec<usize> {
        let q = self.query.trim().to_lowercase();
        let mut v: Vec<usize> = (0..self.catalog.len())
            .filter(|&i| {
                let a = &self.catalog[i];
                q.is_empty()
                    || a.name.to_lowercase().contains(&q)
                    || a.description.to_lowercase().contains(&q)
            })
            .collect();
        v.sort_by_key(|&i| section_of(&state_of(&self.catalog[i], &self.installed)));
        v
    }

    fn icon(&mut self, ctx: &egui::Context, name: &str) -> Option<TextureHandle> {
        if !self.installed.contains_key(name) {
            return None;
        }
        self.icons
            .entry(name.to_string())
            .or_insert_with(|| {
                let bytes = std::fs::read(desktop::icon_file(name)?).ok()?;
                let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                Some(ctx.load_texture(format!("icon-{name}"), color, egui::TextureOptions::LINEAR))
            })
            .clone()
    }

    /// Aplica a la búsqueda lo que se ha tecleado en este frame. Devuelve si cambió.
    fn type_text(&mut self, ctx: &egui::Context) -> bool {
        let events = ctx.input(|i| i.events.clone());
        let before = self.query.clone();
        for ev in events {
            match ev {
                egui::Event::Text(t) => self.query.extend(t.chars().filter(|c| !c.is_control())),
                egui::Event::Paste(t) => {
                    self.query.extend(t.lines().next().unwrap_or("").chars().filter(|c| !c.is_control()))
                }
                egui::Event::Key { key: Key::Backspace, pressed: true, modifiers, .. } => {
                    if modifiers.command {
                        // Ctrl+Retroceso borra la última palabra.
                        let trimmed = self.query.trim_end().len();
                        let start = self.query[..trimmed].rfind(char::is_whitespace).map_or(0, |i| i + 1);
                        self.query.truncate(start);
                    } else {
                        self.query.pop();
                    }
                }
                _ => {}
            }
        }
        self.query != before
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Open(name) => match store::launch(&name) {
                // Como un launcher: tras abrir la app, se cierra.
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => self.status = format!("{name}: {e}"),
            },
            Action::Install(name) => self.start_install(ctx, &name),
            Action::Uninstall(name) => {
                self.status = match store::uninstall(&name) {
                    Ok(()) => format!("{name}: desinstalada"),
                    Err(e) => format!("{name}: error — {e}"),
                };
                self.installed = store::load_installed();
                self.icons.remove(&name);
            }
            Action::Notes(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        }
    }
}

impl eframe::App for Launcher {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain();
        self.frames += 1;

        // Tras un par de frames la ventana ya está mapeada: pedirle a i3 que la
        // ponga arriba a la derecha (con el ancho real en píxeles).
        if !self.placed && self.frames >= 3 {
            if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                self.placed = true;
                let width = (rect.width() * ctx.pixels_per_point()).round() as i64;
                thread::spawn(move || wm::place_top_right(APP_ID, width));
            }
        }
        if !self.jobs.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        if self.type_text(&ctx) {
            self.selected = 0;
            self.scroll_to_selected = true;
        }
        let visible = self.visible();
        let mut action: Option<Action> = None;

        // Teclado: se consume antes del campo de texto para que no lo interprete.
        let (down, up, enter, ctrl_enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::COMMAND, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if esc {
            if self.query.is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                self.query.clear();
                self.selected = 0;
            }
        }
        if !visible.is_empty() {
            if down {
                self.selected = (self.selected + 1).min(visible.len() - 1);
                self.scroll_to_selected = true;
            }
            if up {
                self.selected = self.selected.saturating_sub(1);
                self.scroll_to_selected = true;
            }
        }
        self.selected = self.selected.min(visible.len().saturating_sub(1));

        let current = visible.get(self.selected).map(|&i| self.catalog[i].clone());
        let current_state = current.as_ref().map(|a| state_of(a, &self.installed));
        if let (Some(app), Some(state)) = (&current, &current_state) {
            if enter && !self.jobs.contains_key(&app.name) {
                action = match state {
                    State::Installed(_) | State::Update { .. } => Some(Action::Open(app.name.clone())),
                    State::Available { .. } => Some(Action::Install(app.name.clone())),
                    State::NoLinux => None,
                };
            }
            if ctrl_enter && !self.jobs.contains_key(&app.name) {
                if matches!(state, State::Update { .. } | State::Available { .. }) {
                    action = Some(Action::Install(app.name.clone()));
                }
            }
        }

        // --- Buscador ---------------------------------------------------------
        egui::Panel::top("search")
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                Frame::new()
                    .fill(SURFACE)
                    .corner_radius(12)
                    .stroke(Stroke::new(1.0, ACCENT.gamma_multiply(0.45)))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                            let c = r.center() - vec2(1.5, 1.5);
                            let stroke = Stroke::new(1.7, MUTED);
                            ui.painter().circle_stroke(c, 6.0, stroke);
                            ui.painter().line_segment([c + vec2(4.4, 4.4), c + vec2(8.5, 8.5)], stroke);

                            // Buscador dibujado a mano: un TextEdit con foco permanente deja a
                            // winit sin eventos de entrada en X11 (ni teclado ni ratón).
                            let w = ui.available_width() - 40.0;
                            let (rect, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
                            let painter = ui.painter_at(rect);
                            let font = FontId::new(17.0, egui::FontFamily::Proportional);
                            let mid = rect.center().y;
                            let caret_x = if self.query.is_empty() {
                                let g = painter.layout_no_wrap(
                                    "Buscar entre las apps de ArtCraft".into(),
                                    font,
                                    FAINT,
                                );
                                painter.galley(Pos2::new(rect.left() + 4.0, mid - g.size().y / 2.0), g, FAINT);
                                rect.left()
                            } else {
                                let g = painter.layout_no_wrap(self.query.clone(), font, TEXT);
                                // Si no cabe, se ve el final de lo escrito.
                                let x = rect.left() - (g.size().x + 2.0 - rect.width()).max(0.0);
                                let end = x + g.size().x + 1.0;
                                painter.galley(Pos2::new(x, mid - g.size().y / 2.0), g, TEXT);
                                end
                            };
                            if (ctx.input(|i| i.time) * 1.8) as i64 % 2 == 0 {
                                painter.line_segment(
                                    [Pos2::new(caret_x, mid - 10.0), Pos2::new(caret_x, mid + 10.0)],
                                    Stroke::new(1.5, ACCENT),
                                );
                            }
                            ctx.request_repaint_after(std::time::Duration::from_millis(400));

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if self.loading {
                                    ui.add(egui::Spinner::new().size(16.0).color(MUTED));
                                } else if ui
                                    .add(Button::new(RichText::new("⟳").size(17.0).color(MUTED)).frame(false))
                                    .on_hover_text("Buscar versiones nuevas")
                                    .clicked()
                                {
                                    self.refresh(&ctx);
                                }
                            });
                        });
                    });
            });

        // --- Pie: estado a la izquierda, atajos a la derecha -----------------
        egui::Panel::bottom("footer")
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::symmetric(18, 9)))
            .show(ui, |ui| {
                let y = ui.max_rect().top() - 9.0;
                ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(1.0, LINE));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.status).size(12.0).color(FAINT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let enter_label = match &current_state {
                            Some(State::Installed(_)) | Some(State::Update { .. }) => "Abrir",
                            Some(State::Available { .. }) => "Instalar",
                            _ => "",
                        };
                        let mut hints = vec![("Esc", if self.query.is_empty() { "cerrar" } else { "limpiar" })];
                        if matches!(current_state, Some(State::Update { .. })) {
                            hints.push(("Ctrl+Intro", "actualizar"));
                        }
                        if !enter_label.is_empty() {
                            hints.push(("Intro", enter_label));
                        }
                        hints.push(("Flechas", "elegir"));
                        for (key, label) in hints {
                            ui.label(RichText::new(label).size(12.0).color(FAINT));
                            ui.label(RichText::new(key).size(12.0).color(MUTED));
                            ui.add_space(6.0);
                        }
                    });
                });
            });

        // --- Lista -----------------------------------------------------------
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::symmetric(10, 0)))
            .show(ui, |ui| {
                if visible.is_empty() {
                    ui.add_space(60.0);
                    ui.vertical_centered(|ui| {
                        if self.loading && self.catalog.is_empty() {
                            ui.add(egui::Spinner::new().size(22.0).color(MUTED));
                            ui.add_space(10.0);
                            ui.label(RichText::new("Consultando GitHub…").color(MUTED));
                        } else if self.catalog.is_empty() {
                            ui.label(RichText::new("No se pudo cargar el catálogo").color(MUTED));
                            ui.label(RichText::new("Pulsa ⟳ para reintentarlo").size(12.0).color(FAINT));
                        } else {
                            ui.label(RichText::new(format!("Nada coincide con «{}»", self.query.trim())).color(MUTED));
                        }
                    });
                    return;
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut last_section = usize::MAX;
                    for (pos, &idx) in visible.iter().enumerate() {
                        let app = self.catalog[idx].clone();
                        let state = state_of(&app, &self.installed);
                        let section = section_of(&state);
                        if section != last_section {
                            last_section = section;
                            ui.add_space(if pos == 0 { 2.0 } else { 12.0 });
                            ui.label(
                                RichText::new(SECTIONS[section].to_uppercase())
                                    .size(11.0)
                                    .color(FAINT)
                                    .extra_letter_spacing(1.2),
                            );
                            ui.add_space(2.0);
                        }
                        let icon = self.icon(&ctx, &app.name);
                        let progress = self.jobs.get(&app.name).map(|j| {
                            (j.done.load(Ordering::Relaxed), j.total.load(Ordering::Relaxed))
                        });
                        let out = row(ui, &app, &state, progress, pos == self.selected, icon.as_ref());
                        if pos == self.selected && self.scroll_to_selected {
                            out.response.scroll_to_me(None);
                            self.scroll_to_selected = false;
                        }
                        if out.response.clicked() {
                            self.selected = pos;
                        }
                        if out.response.double_clicked() && progress.is_none() {
                            action = match state {
                                State::Installed(_) | State::Update { .. } => Some(Action::Open(app.name.clone())),
                                State::Available { .. } => Some(Action::Install(app.name.clone())),
                                State::NoLinux => None,
                            };
                        }
                        if out.action.is_some() {
                            action = out.action;
                        }
                        ui.add_space(2.0);
                    }
                });
            });

        if let Some(a) = action {
            self.apply(&ctx, a);
        }
    }
}

struct RowOut {
    response: egui::Response,
    action: Option<Action>,
}

/// Color estable por nombre para el icono de una app que aún no tiene uno.
fn placeholder_color(name: &str) -> (Color32, Color32) {
    let hash = name.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let hue = (hash % 360) as f32 / 360.0;
    (
        egui::ecolor::Hsva::new(hue, 0.35, 0.30, 1.0).into(),
        egui::ecolor::Hsva::new(hue, 0.28, 0.92, 1.0).into(),
    )
}

fn pill(ui: &mut Ui, text: &str, filled: bool) -> egui::Response {
    let button = if filled {
        Button::new(RichText::new(text).color(ON_ACCENT)).fill(ACCENT)
    } else {
        Button::new(RichText::new(text).color(TEXT))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, Color32::from_rgb(0x42, 0x3f, 0x4e)))
    };
    ui.add(button.corner_radius(8).min_size(vec2(0.0, 30.0)))
}

enum Glyph {
    Trash,
    Info,
}

fn icon_button(ui: &mut Ui, glyph: Glyph, tip: &str, visible: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    if !visible {
        return response;
    }
    let color = if response.hovered() { TEXT } else { MUTED };
    let stroke = Stroke::new(1.4, color);
    let (p, c) = (ui.painter(), rect.center());
    if response.hovered() {
        p.rect_filled(rect, 8, SURFACE_HI);
    }
    match glyph {
        Glyph::Info => {
            p.circle_stroke(c, 8.0, stroke);
            p.line_segment([c + vec2(0.0, -0.5), c + vec2(0.0, 3.8)], stroke);
            p.circle_filled(c + vec2(0.0, -3.4), 1.0, color);
        }
        Glyph::Trash => {
            p.line_segment([c + vec2(-6.0, -4.5), c + vec2(6.0, -4.5)], stroke);
            p.line_segment([c + vec2(-2.0, -4.5), c + vec2(-2.0, -6.5)], stroke);
            p.line_segment([c + vec2(2.0, -4.5), c + vec2(2.0, -6.5)], stroke);
            p.line_segment([c + vec2(-2.0, -6.5), c + vec2(2.0, -6.5)], stroke);
            p.rect_stroke(
                Rect::from_min_max(c + vec2(-4.5, -4.5), c + vec2(4.5, 6.5)),
                2,
                stroke,
                StrokeKind::Middle,
            );
        }
    }
    response.on_hover_text(tip)
}

/// Las descripciones de los repos son casi idénticas ("clean-room reimplementation
/// of Adobe X built in pure Rust"). Se resume lo que distingue a cada app.
fn short_description(desc: &str) -> String {
    let lower = desc.to_lowercase();
    if let Some(i) = lower.find("reimplementation of ") {
        let rest = &desc[i + "reimplementation of ".len()..];
        let end = [" built in", " in pure Rust", ", built", " — ", "."]
            .iter()
            .filter_map(|m| rest.find(m))
            .min()
            .unwrap_or(rest.len());
        return format!("Reimplementación libre de {}", rest[..end].trim());
    }
    desc.to_string()
}

fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat::simple(font, color));
    job.wrap.max_width = width.max(10.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    ui.fonts_mut(|f| f.layout_job(job))
}

fn row(
    ui: &mut Ui,
    app: &CatalogApp,
    state: &State,
    progress: Option<(u64, u64)>,
    selected: bool,
    icon: Option<&TextureHandle>,
) -> RowOut {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let active = selected || response.hovered();
    let radius = CornerRadius::same(12);
    if selected {
        ui.painter().rect_filled(rect, radius, SURFACE_HI);
        ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, ACCENT.gamma_multiply(0.55)), StrokeKind::Inside);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, radius, SURFACE);
    }

    let dim = matches!(state, State::NoLinux);

    // Icono.
    let icon_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 14.0, rect.center().y - ICON / 2.0),
        vec2(ICON, ICON),
    );
    match icon {
        Some(tex) => {
            let img = egui::Image::from_texture(SizedTexture::new(tex.id(), vec2(ICON, ICON)))
                .corner_radius(10);
            ui.put(icon_rect, img);
        }
        None => {
            let (bg, fg) = placeholder_color(&app.name);
            let bg = if dim { bg.gamma_multiply(0.6) } else { bg };
            ui.painter().rect_filled(icon_rect, 10, bg);
            let letter = app.name.chars().next().unwrap_or('?').to_uppercase().to_string();
            let g = ui.painter().layout_no_wrap(letter, FontId::new(20.0, theme::medium()), fg);
            ui.painter().galley(icon_rect.center() - g.size() / 2.0, g, fg);
        }
    }

    // Controles a la derecha. Se dibujan primero para saber cuánto sitio queda al texto.
    let mut action = None;
    let controls_rect = Rect::from_min_max(
        Pos2::new(icon_rect.right() + 12.0, rect.top()),
        Pos2::new(rect.right() - 12.0, rect.bottom()),
    );
    let mut controls = ui.new_child(
        UiBuilder::new().max_rect(controls_rect).layout(Layout::right_to_left(Align::Center)),
    );
    if let Some((done, total)) = progress {
        let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
        controls.add(
            egui::ProgressBar::new(frac)
                .desired_width(170.0)
                .fill(ACCENT)
                .text(RichText::new(format!("{:.0} / {:.0} MB", done as f64 / 1e6, total as f64 / 1e6)).size(12.0)),
        );
    } else {
        match state {
            State::Installed(_) => {
                if pill(&mut controls, "Abrir", false).clicked() {
                    action = Some(Action::Open(app.name.clone()));
                }
            }
            State::Update { .. } => {
                if pill(&mut controls, "Actualizar", true).clicked() {
                    action = Some(Action::Install(app.name.clone()));
                }
                if pill(&mut controls, "Abrir", false).clicked() {
                    action = Some(Action::Open(app.name.clone()));
                }
            }
            State::Available { .. } => {
                if pill(&mut controls, "Instalar", true).clicked() {
                    action = Some(Action::Install(app.name.clone()));
                }
            }
            State::NoLinux => {}
        }
        if matches!(state, State::Installed(_) | State::Update { .. })
            && icon_button(&mut controls, Glyph::Trash, "Desinstalar", active).clicked()
        {
            action = Some(Action::Uninstall(app.name.clone()));
        }
        if let Some(l) = &app.latest {
            if icon_button(&mut controls, Glyph::Info, "Notas de la versión", active).clicked() {
                action = Some(Action::Notes(l.notes_url.clone()));
            }
        }
    }

    // Texto: nombre + chip de estado, y descripción en una línea.
    let text_left = controls_rect.left();
    let text_right = controls.min_rect().left().min(controls_rect.right()) - 12.0;
    let desc_text = short_description(&app.description);
    let top = rect.center().y - if desc_text.is_empty() { 10.0 } else { 19.0 };
    let name_color = if dim { MUTED } else { TEXT };
    let name = ui.painter().layout_no_wrap(app.name.clone(), FontId::new(15.0, theme::medium()), name_color);
    let name_w = name.size().x;
    ui.painter().galley(Pos2::new(text_left, top), name, name_color);

    let (chip_text, chip_fg, chip_bg) = match state {
        State::Installed(v) => (format!("Instalada {v}"), OK, Some(OK_DIM)),
        State::Update { from, to } => (format!("{from} · nueva {to}"), ACCENT, Some(ACCENT_DIM)),
        State::Available { version, size } => {
            (format!("{version} · {:.0} MB", *size as f64 / 1_048_576.0), MUTED, Some(SURFACE_HI))
        }
        State::NoLinux => ("Sin build para Linux".to_string(), FAINT, None),
    };
    let chip_x = text_left + name_w + 8.0;
    let chip_room = text_right - chip_x;
    let chip = one_line(ui, &chip_text, FontId::new(11.5, theme::medium()), chip_fg, chip_room - 16.0);
    if chip_room > 40.0 {
        let chip_rect = Rect::from_min_size(
            Pos2::new(chip_x, top + 1.0),
            vec2(chip.size().x + 16.0, 20.0),
        );
        if let Some(bg) = chip_bg {
            ui.painter().rect_filled(chip_rect, 6, bg);
        }
        let pad = if chip_bg.is_some() { 8.0 } else { 0.0 };
        ui.painter().galley(
            Pos2::new(chip_rect.left() + pad, chip_rect.center().y - chip.size().y / 2.0),
            chip,
            chip_fg,
        );
    }

    if !desc_text.is_empty() {
        let desc = one_line(ui, &desc_text, FontId::new(12.5, egui::FontFamily::Proportional), MUTED, text_right - text_left);
        ui.painter().galley(Pos2::new(text_left, top + 23.0), desc, MUTED);
    }

    let response = match &app.latest {
        Some(l) => response.on_hover_text(format!(
            "{} {} · publicada el {}",
            app.name, l.version, l.published
        )),
        None => response,
    };
    RowOut { response, action }
}

#[cfg(test)]
mod tests {
    use super::short_description;

    #[test]
    fn resume_las_reimplementaciones() {
        assert_eq!(
            short_description("An open-source, clean-room reimplementation of Adobe Premiere Pro built in pure Rust."),
            "Reimplementación libre de Adobe Premiere Pro"
        );
        assert_eq!(
            short_description("An open-source, clean-room reimplementation of Adobe Illustrator, built in pure Rust."),
            "Reimplementación libre de Adobe Illustrator"
        );
        assert_eq!(
            short_description("An open-source, clean-room reimplementation of Adobe Lightroom in pure Rust."),
            "Reimplementación libre de Adobe Lightroom"
        );
    }

    #[test]
    fn deja_igual_el_resto() {
        assert_eq!(short_description(""), "");
        assert_eq!(short_description("Fonts for the Crafting Apps"), "Fonts for the Crafting Apps");
    }
}
