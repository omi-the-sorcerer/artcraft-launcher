//! Identidad visual: tinta oscura con matiz cálido y el naranja del icono como
//! único acento. El verde y el ámbar solo expresan estado (instalada / hay
//! actualización), nunca decoran.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};
use std::{fs, sync::Arc};

pub const BG: Color32 = Color32::from_rgb(0x14, 0x13, 0x18);
pub const SURFACE: Color32 = Color32::from_rgb(0x1d, 0x1c, 0x23);
pub const SURFACE_HI: Color32 = Color32::from_rgb(0x27, 0x25, 0x2e);
pub const LINE: Color32 = Color32::from_rgb(0x2c, 0x2a, 0x34);

pub const TEXT: Color32 = Color32::from_rgb(0xec, 0xea, 0xf0);
pub const MUTED: Color32 = Color32::from_rgb(0x9a, 0x97, 0xa6);
pub const FAINT: Color32 = Color32::from_rgb(0x66, 0x63, 0x72);

pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0x7a, 0x45);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x3b, 0x24, 0x1a);
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x1a, 0x0f, 0x0a);

pub const OK: Color32 = Color32::from_rgb(0x74, 0xd0, 0x9a);
pub const OK_DIM: Color32 = Color32::from_rgb(0x1c, 0x32, 0x28);

/// Familia con el peso intermedio, para nombres y botones.
pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

fn load_fonts(ctx: &egui::Context) {
    let read = |name: &str| fs::read(format!("/usr/share/fonts/noto/{name}")).ok();
    let (Some(regular), Some(med)) = (read("NotoSans-Regular.ttf"), read("NotoSans-Medium.ttf"))
    else {
        // Sin Noto Sans: se usa la fuente de egui y "medium" cae en la normal.
        let mut fonts = FontDefinitions::default();
        let base = fonts.families[&FontFamily::Proportional].clone();
        fonts.families.insert(medium(), base);
        ctx.set_fonts(fonts);
        return;
    };
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("noto-regular".into(), Arc::new(FontData::from_owned(regular)));
    fonts.font_data.insert("noto-medium".into(), Arc::new(FontData::from_owned(med)));
    let prop = fonts.families.get_mut(&FontFamily::Proportional).unwrap();
    prop.insert(0, "noto-regular".into());
    let mut with_medium = prop.clone();
    with_medium.insert(0, "noto-medium".into());
    fonts.families.insert(medium(), with_medium);
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context) {
    load_fonts(ctx);

    let mut style = (*ctx.global_style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::new(20.0, medium())),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.0, medium())),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(14.0, 6.0);

    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = SURFACE;
    v.faint_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.window_stroke = Stroke::new(1.0, LINE);

    let r = CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.inactive.bg_fill = SURFACE_HI;
    v.widgets.inactive.weak_bg_fill = SURFACE_HI;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x34, 0x31, 0x3d);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x34, 0x31, 0x3d);
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);

    ctx.set_global_style(style);
}
