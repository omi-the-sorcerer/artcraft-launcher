mod desktop;
mod github;
mod store;
mod wm;

use eframe::egui;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    thread,
};
use store::{CatalogApp, InstalledMap};

enum Msg {
    Catalog(Result<Vec<CatalogApp>, String>),
    Installed { app: String, result: Result<String, String> },
}

struct Job {
    done: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
}

const APP_ID: &str = "artcraft-launcher";

struct Launcher {
    frames: u32,
    placed: bool,
    catalog: Vec<CatalogApp>,
    installed: InstalledMap,
    jobs: HashMap<String, Job>,
    loading: bool,
    status: String,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl Launcher {
    fn new(ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let mut l = Self {
            frames: 0,
            placed: false,
            catalog: Vec::new(),
            installed: store::load_installed(),
            jobs: HashMap::new(),
            loading: false,
            status: String::new(),
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
            let _ = tx.send(Msg::Catalog(store::load_catalog()));
            ctx.request_repaint();
        });
    }

    fn start_install(&mut self, ctx: &egui::Context, app: &CatalogApp) {
        let Some(latest) = app.latest.clone() else { return };
        let job = Job { done: Arc::default(), total: Arc::default() };
        let (done, total) = (job.done.clone(), job.total.clone());
        self.jobs.insert(app.name.clone(), job);
        let (tx, ctx, name) = (self.tx.clone(), ctx.clone(), app.name.clone());
        thread::spawn(move || {
            let result = store::install(&name, &latest, done, total);
            let _ = tx.send(Msg::Installed { app: name, result });
            ctx.request_repaint();
        });
    }

    fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
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
                    self.installed = store::load_installed();
                    self.status = match result {
                        Ok(note) => format!("{app}: {note}"),
                        Err(e) => format!("{app}: error — {e}"),
                    };
                }
            }
        }
    }
}

impl eframe::App for Launcher {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain();
        self.frames += 1;

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
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

        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("ArtCraft Launcher");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!self.loading, egui::Button::new("⟳ Actualizar lista")).clicked() {
                        self.refresh(&ctx);
                    }
                    if self.loading {
                        ui.spinner();
                    }
                });
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.label(&self.status);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let catalog = self.catalog.clone();
                for app in &catalog {
                    self.app_card(ui, &ctx, app);
                    ui.add_space(6.0);
                }
            });
        });
    }
}

impl Launcher {
    fn app_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, app: &CatalogApp) {
        let installed = self.installed.get(&app.name).cloned();
        let update_available = match (&installed, &app.latest) {
            (Some(i), Some(l)) => store::is_newer(&l.version, &i.version),
            _ => false,
        };

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Fila 1: nombre a la izquierda, acciones a la derecha.
            ui.horizontal(|ui| {
                ui.strong(&app.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(job) = self.jobs.get(&app.name) {
                        let total = job.total.load(Ordering::Relaxed);
                        let done = job.done.load(Ordering::Relaxed);
                        let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                        ui.add(
                            egui::ProgressBar::new(frac)
                                .desired_width(180.0)
                                .text(format!("{:.0} / {:.0} MB", done as f64 / 1e6, total as f64 / 1e6)),
                        );
                        return;
                    }
                    if app.latest.is_some() && (installed.is_none() || update_available) {
                        let label = if installed.is_some() { "Actualizar" } else { "Instalar" };
                        if ui.button(label).clicked() {
                            self.start_install(ctx, app);
                        }
                    }
                    if installed.is_some() {
                        if ui.button("🗑").on_hover_text("Desinstalar").clicked() {
                            self.status = match store::uninstall(&app.name) {
                                Ok(()) => format!("{}: desinstalada", app.name),
                                Err(e) => format!("{}: error — {e}", app.name),
                            };
                            self.installed = store::load_installed();
                        }
                        if ui.button("▶ Abrir").clicked() {
                            match store::launch(&app.name) {
                                // Como un launcher: tras abrir la app, se cierra.
                                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                                Err(e) => self.status = format!("{}: {e}", app.name),
                            }
                        }
                    }
                });
            });

            // Fila 2: descripción, con salto de línea a ancho completo.
            if !app.description.is_empty() {
                ui.label(egui::RichText::new(&app.description).weak().small());
            }

            // Fila 3: versión y enlace a las notas, que si no caben pasan a otra línea.
            ui.horizontal_wrapped(|ui| {
                let version_line = match (&installed, &app.latest) {
                    (Some(i), Some(l)) if update_available => {
                        format!("Instalada {} → disponible {} ({})", i.version, l.version, l.published)
                    }
                    (Some(i), _) => format!("Instalada {} (al día)", i.version),
                    (None, Some(l)) => format!(
                        "Disponible {} · {} · {:.0} MB",
                        l.version,
                        l.published,
                        l.size as f64 / 1_048_576.0
                    ),
                    (None, None) => "Sin versión para Linux todavía".into(),
                };
                ui.label(version_line);
                if let Some(l) = &app.latest {
                    ui.label("·");
                    ui.hyperlink_to("notas de la versión", &l.notes_url);
                }
            });
        });
    }
}

/// Modo sin ventana: `--list`, `--install <app>` o `--sync-desktop`.
fn cli(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("--list") => {
            let installed = store::load_installed();
            match store::load_catalog() {
                Ok(apps) => apps.iter().for_each(|a| {
                    let inst = installed.get(&a.name).map(|i| i.version.as_str()).unwrap_or("-");
                    let lat = a.latest.as_ref().map(|l| l.version.as_str()).unwrap_or("sin Linux");
                    println!("{:<14} instalada={:<8} última={}", a.name, inst, lat);
                }),
                Err(e) => eprintln!("error: {e}"),
            }
            true
        }
        Some("--sync-desktop") => {
            for (app, r) in store::sync_desktop() {
                println!("{app}: {r:?}");
            }
            true
        }
        Some("--install") => {
            let name = args.get(1).cloned().unwrap_or_default();
            let apps = store::load_catalog().unwrap_or_default();
            match apps.iter().find(|a| a.name == name).and_then(|a| a.latest.clone()) {
                Some(l) => {
                    let (d, t) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
                    println!("{name}: {:?}", store::install(&name, &l, d, t));
                }
                None => eprintln!("{name}: no encontrada o sin versión para Linux"),
            }
            true
        }
        _ => false,
    }
}

fn main() -> eframe::Result {
    if cli(&std::env::args().skip(1).collect::<Vec<_>>()) {
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 640.0])
            .with_title("ArtCraft Launcher")
            .with_app_id(APP_ID)
            // i3 abre flotantes las ventanas de tipo diálogo, sin reglas extra.
            .with_window_type(egui::X11WindowType::Dialog),
        ..Default::default()
    };
    eframe::run_native(
        "ArtCraft Launcher",
        options,
        Box::new(|cc| Ok(Box::new(Launcher::new(&cc.egui_ctx)))),
    )
}
