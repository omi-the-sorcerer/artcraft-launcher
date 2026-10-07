mod app;
mod desktop;
mod github;
mod store;
mod theme;
mod wm;

use app::{Launcher, APP_ID};
use eframe::egui;
use std::sync::{
    atomic::AtomicU64,
    Arc,
};

/// Modo sin ventana: `--list`, `--install <app>` o `--sync-desktop`.
fn cli(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("--list") => {
            let installed = store::load_installed();
            match store::load_catalog(true) {
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
            let apps = store::load_catalog(true).unwrap_or_default();
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if cli(&args) {
        return Ok(());
    }
    // `artcraft-launcher photo` abre la ventana con la búsqueda ya escrita.
    let query = args.join(" ");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 560.0])
            .with_decorations(false)
            .with_title("ArtCraft Launcher")
            .with_app_id(APP_ID)
            // i3 abre flotantes las ventanas de tipo diálogo, sin reglas extra.
            .with_window_type(egui::X11WindowType::Dialog),
        ..Default::default()
    };
    eframe::run_native(
        "ArtCraft Launcher",
        options,
        Box::new(move |cc| Ok(Box::new(Launcher::new(&cc.egui_ctx, query)))),
    )
}
