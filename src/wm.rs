//! Colocación de la ventana "tipo launcher" (arriba a la derecha).
//! En X11 un cliente no puede moverse solo bajo un tiling WM, así que se le
//! pide a i3 por IPC. En cualquier otro entorno no hace nada.

use serde_json::Value;
use std::process::Command;

const MARGIN: i64 = 24;

fn i3_json(kind: &str) -> Option<Value> {
    let out = Command::new("i3-msg").args(["-t", kind]).output().ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

/// i3 serializa coordenadas negativas como u32 (-4 -> 4294967292).
fn coord(v: &Value, key: &str) -> Option<i64> {
    Some(v.get(key)?.as_i64()? as u32 as i32 as i64)
}

/// Mueve la ventana de clase `class` a la esquina superior derecha del área
/// útil del workspace con foco. `width_px` es el ancho real de la ventana.
pub fn place_top_right(class: &str, width_px: i64) {
    let Some(workspaces) = i3_json("get_workspaces") else { return };
    let Some(ws) = workspaces.as_array().and_then(|a| a.iter().find(|w| w["focused"] == true))
    else {
        return;
    };
    let (Some(x), Some(y), Some(w)) =
        (coord(&ws["rect"], "x"), coord(&ws["rect"], "y"), coord(&ws["rect"], "width"))
    else {
        return;
    };
    let mut right = x + w;
    // El rect del workspace puede rebasar el monitor (bordes); limita al output.
    if let Some(outputs) = i3_json("get_outputs") {
        if let Some(o) = outputs
            .as_array()
            .and_then(|a| a.iter().find(|o| o["name"] == ws["output"]))
        {
            if let (Some(ox), Some(ow)) = (coord(&o["rect"], "x"), coord(&o["rect"], "width")) {
                right = right.min(ox + ow);
            }
        }
    }
    let px = right - width_px - MARGIN;
    let py = y + MARGIN;
    let _ = Command::new("i3-msg")
        .arg(format!("[class=\"{class}\"] move absolute position {px} px {py} px"))
        .output();
}
