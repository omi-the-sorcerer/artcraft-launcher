//! Accesos directos (.desktop + iconos) para que las apps instaladas aparezcan
//! en rofi, ulauncher y demás menús. Se reutilizan el `.desktop` y los iconos
//! que ya trae cada AppImage, reescribiendo `Exec` para apuntar a la ruta real.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MARKER: &str = "X-ArtCraft-Managed=true";

fn data_home() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn applications_dir() -> PathBuf {
    data_home().join("applications")
}

fn icons_dir() -> PathBuf {
    data_home().join("icons").join("hicolor")
}

fn desktop_path(app: &str) -> PathBuf {
    applications_dir().join(format!("artcraft-{app}.desktop"))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

fn refresh_caches() {
    let quiet = |mut c: Command| {
        let _ = c.stdout(Stdio::null()).stderr(Stdio::null()).status();
    };
    let mut c = Command::new("update-desktop-database");
    c.arg(applications_dir());
    quiet(c);
    if icons_dir().join("index.theme").exists() {
        let mut c = Command::new("gtk-update-icon-cache");
        c.args(["-f", "-t"]).arg(icons_dir());
        quiet(c);
    }
}

/// Reescribe `Exec` con la ruta real del AppImage y marca el fichero como nuestro.
fn rewrite(original: &str, appimage: &Path) -> String {
    let exe = format!("\"{}\"", appimage.display());
    let mut out = Vec::new();
    for line in original.lines() {
        if line.starts_with("TryExec=") || line.starts_with(MARKER) {
            continue;
        }
        if let Some(cmd) = line.strip_prefix("Exec=") {
            // Conserva los argumentos (%F, %U...) del Exec original.
            let args = cmd.split_once(char::is_whitespace).map(|(_, a)| a).unwrap_or("");
            out.push(format!("Exec={exe} {args}").trim_end().to_string());
        } else {
            out.push(line.to_string());
        }
        if line == "[Desktop Entry]" {
            out.push(MARKER.to_string());
        }
    }
    out.join("\n") + "\n"
}

/// Crea (o actualiza) el acceso directo de `app` a partir de su AppImage.
pub fn register(app: &str, appimage: &Path) -> Result<(), String> {
    let tmp = data_home().join("artcraft-launcher").join("tmp-extract");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let result = register_from(app, appimage, &tmp);
    let _ = fs::remove_dir_all(&tmp);
    result
}

fn register_from(app: &str, appimage: &Path, tmp: &Path) -> Result<(), String> {
    let status = Command::new(appimage)
        .arg("--appimage-extract")
        .current_dir(tmp)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("no se pudo extraer el AppImage: {e}"))?;
    if !status.success() {
        return Err("falló la extracción del AppImage".into());
    }
    let root = tmp.join("squashfs-root");

    let desktop_src = fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "desktop"))
        .ok_or("el AppImage no incluye .desktop")?;
    let original = fs::read_to_string(&desktop_src).map_err(|e| e.to_string())?;
    let icon_name = original
        .lines()
        .find_map(|l| l.strip_prefix("Icon="))
        .map(|s| s.trim().to_string());

    // Iconos: todo el árbol hicolor del AppImage; si no hay, el PNG de la raíz.
    let mut copied = false;
    let mut files = Vec::new();
    walk(&root.join("usr/share/icons/hicolor"), &mut files);
    for f in files {
        let rel = f.strip_prefix(root.join("usr/share/icons/hicolor")).unwrap();
        let dest = icons_dir().join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&f, &dest).map_err(|e| e.to_string())?;
        copied = true;
    }
    if let (false, Some(name)) = (copied, &icon_name) {
        let png = root.join(format!("{name}.png"));
        if png.exists() {
            let dest = icons_dir().join("256x256/apps");
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            fs::copy(&png, dest.join(format!("{name}.png"))).map_err(|e| e.to_string())?;
        }
    }

    fs::create_dir_all(applications_dir()).map_err(|e| e.to_string())?;
    fs::write(desktop_path(app), rewrite(&original, appimage)).map_err(|e| e.to_string())?;
    refresh_caches();
    Ok(())
}

/// Borra el acceso directo y los iconos de `app`. Solo toca lo que creamos nosotros.
pub fn unregister(app: &str) {
    let path = desktop_path(app);
    let Ok(content) = fs::read_to_string(&path) else { return };
    if !content.contains(MARKER) {
        return;
    }
    if let Some(icon) = content.lines().find_map(|l| l.strip_prefix("Icon=")) {
        let icon = icon.trim();
        let mut files = Vec::new();
        walk(&icons_dir(), &mut files);
        for f in files {
            let stem = f.file_stem().and_then(|s| s.to_str());
            if stem == Some(icon) {
                let _ = fs::remove_file(f);
            }
        }
    }
    let _ = fs::remove_file(path);
    refresh_caches();
}
