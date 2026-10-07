//! Estado local: qué apps hay instaladas y dónde, más el catálogo de GitHub.

use crate::{desktop, github::{self, Release}};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Installed {
    pub version: String,
    pub path: PathBuf,
}

pub type InstalledMap = BTreeMap<String, Installed>;

fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("artcraft-launcher")
}

fn state_file() -> PathBuf {
    data_dir().join("installed.json")
}

pub fn load_installed() -> InstalledMap {
    fs::read_to_string(state_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_installed(map: &InstalledMap) -> Result<(), String> {
    fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    fs::write(state_file(), json).map_err(|e| e.to_string())
}

/// Una app del catálogo con su última versión instalable en esta máquina.
#[derive(Clone, Debug)]
pub struct CatalogApp {
    pub name: String,
    pub description: String,
    /// `None` si no hay ninguna release con AppImage para esta arquitectura.
    pub latest: Option<Latest>,
}

#[derive(Clone, Debug)]
pub struct Latest {
    pub version: String,
    pub published: String,
    pub notes_url: String,
    pub asset_name: String,
    pub asset_url: String,
    pub size: u64,
    pub sums_url: Option<String>,
}

/// Repos que son aplicaciones de ArtCraft. Se autodetectan por nombre para
/// que las apps nuevas (wordcraft, gridcraft...) aparezcan solas.
fn is_app_repo(name: &str) -> bool {
    name.ends_with("craft") && !name.contains("placeholder")
}

fn pick_latest(releases: &[Release]) -> Option<Latest> {
    let arch = std::env::consts::ARCH; // x86_64 / aarch64
    let needle = format!("linux-{arch}");
    releases.iter().filter(|r| !r.draft && !r.prerelease).find_map(|r| {
        let asset = r
            .assets
            .iter()
            .find(|a| a.name.contains(&needle) && a.name.ends_with(".AppImage"))?;
        Some(Latest {
            version: version_of(&r.tag_name),
            published: r.published_at.clone().unwrap_or_default().chars().take(10).collect(),
            notes_url: r.html_url.clone(),
            asset_name: asset.name.clone(),
            asset_url: asset.browser_download_url.clone(),
            size: asset.size,
            sums_url: r
                .assets
                .iter()
                .find(|a| a.name == "SHA256SUMS.txt")
                .map(|a| a.browser_download_url.clone()),
        })
    })
}

pub fn load_catalog() -> Result<Vec<CatalogApp>, String> {
    let mut apps = Vec::new();
    for repo in github::list_repos()? {
        if repo.archived || !is_app_repo(&repo.name) {
            continue;
        }
        // Un fallo en un repo no debe tumbar todo el catálogo.
        let latest = github::list_releases(&repo.name)
            .ok()
            .and_then(|r| pick_latest(&r));
        apps.push(CatalogApp {
            description: repo.description.unwrap_or_default(),
            name: repo.name,
            latest,
        });
    }
    // Primero las que tienen versión, luego alfabético.
    apps.sort_by(|a, b| {
        b.latest.is_some().cmp(&a.latest.is_some()).then(a.name.cmp(&b.name))
    });
    Ok(apps)
}

/// "artcraft-v0.41.0" / "v0.3.0" -> "0.41.0" / "0.3.0"
pub fn version_of(tag: &str) -> String {
    let start = tag.find(|c: char| c.is_ascii_digit()).unwrap_or(0);
    tag[start..].to_string()
}

pub fn parse_version(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect()
}

pub fn is_newer(latest: &str, installed: &str) -> bool {
    parse_version(latest) > parse_version(installed)
}

/// Busca el hash de `file` en un SHA256SUMS.txt (`<hash>  <archivo>`).
fn expected_hash(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let hash = it.next()?;
        let name = it.next()?.trim_start_matches('*');
        (name == file).then(|| hash.to_lowercase())
    })
}

/// Instala la versión indicada. Devuelve el aviso a mostrar al terminar.
pub fn install(
    app: &str,
    latest: &Latest,
    done: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
) -> Result<String, String> {
    let dir = data_dir().join("apps").join(app);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let final_path = dir.join(&latest.asset_name);
    let part = dir.join(format!("{}.part", latest.asset_name));

    let hash = github::download(&latest.asset_url, &part, |d, t| {
        done.store(d, Ordering::Relaxed);
        total.store(t.max(latest.size), Ordering::Relaxed);
    })
    .inspect_err(|_| {
        let _ = fs::remove_file(&part);
    })?;

    let note = match latest.sums_url.as_deref().map(github::fetch_text) {
        Some(Ok(sums)) => match expected_hash(&sums, &latest.asset_name) {
            Some(exp) if exp == hash => "instalada (SHA256 verificado)".to_string(),
            Some(_) => {
                let _ = fs::remove_file(&part);
                return Err("el SHA256 no coincide; descarga descartada".into());
            }
            None => "instalada (sin hash publicado para este archivo)".to_string(),
        },
        Some(Err(e)) => format!("instalada (no se pudo obtener SHA256SUMS: {e})"),
        None => "instalada (esta release no publica SHA256SUMS)".to_string(),
    };

    fs::set_permissions(&part, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    fs::rename(&part, &final_path).map_err(|e| e.to_string())?;

    // Quitar versiones anteriores.
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.path() != final_path {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    let mut map = load_installed();
    map.insert(
        app.to_string(),
        Installed { version: latest.version.clone(), path: final_path.clone() },
    );
    save_installed(&map)?;
    Ok(match desktop::register(app, &final_path) {
        Ok(()) => format!("{note}; acceso directo creado"),
        Err(e) => format!("{note}; sin acceso directo ({e})"),
    })
}

pub fn uninstall(app: &str) -> Result<(), String> {
    desktop::unregister(app);
    let mut map = load_installed();
    if let Some(i) = map.remove(app) {
        if let Some(parent) = i.path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
    save_installed(&map)
}

pub fn launch(app: &str) -> Result<(), String> {
    let map = load_installed();
    let inst = map.get(app).ok_or("no instalada")?;
    std::process::Command::new(&inst.path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("no se pudo lanzar: {e}"))
}

/// Regenera los accesos directos de todo lo instalado.
pub fn sync_desktop() -> Vec<(String, Result<(), String>)> {
    load_installed()
        .into_iter()
        .map(|(app, i)| {
            let r = desktop::register(&app, &i.path);
            (app, r)
        })
        .collect()
}
