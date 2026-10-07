//! Cliente mínimo de la API de GitHub con caché en disco por ETag.
//! Una respuesta 304 no cuenta contra el límite de 60 peticiones/hora.

use serde::Deserialize;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub const ORG: &str = "storytold";

#[derive(Deserialize, Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Release {
    pub tag_name: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub draft: bool,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Repo {
    pub name: String,
    pub description: Option<String>,
    pub archived: bool,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(6)))
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent("artcraft-launcher")
        .build()
        .into()
}

fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("artcraft-launcher")
}

fn cache_key(url: &str) -> String {
    url.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

fn token() -> Option<String> {
    std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")).ok()
}

/// GET con caché ETag. Si el límite de la API se agota o la red falla, usa la
/// copia en caché. Con `network = false` no sale a internet: solo lee la caché.
fn get_json(url: &str, network: bool) -> Result<String, String> {
    let dir = cache_dir();
    let _ = fs::create_dir_all(&dir);
    let body_path = dir.join(format!("{}.json", cache_key(url)));
    let etag_path = dir.join(format!("{}.etag", cache_key(url)));
    let cached = fs::read_to_string(&body_path).ok();
    let etag = fs::read_to_string(&etag_path).ok();

    if !network {
        return cached.ok_or_else(|| "sin caché local".into());
    }

    let mut req = agent().get(url).header("Accept", "application/vnd.github+json");
    if let Some(t) = token() {
        req = req.header("Authorization", &format!("Bearer {t}"));
    }
    if let (Some(e), Some(_)) = (&etag, &cached) {
        req = req.header("If-None-Match", e.trim());
    }
    let mut resp = match req.call() {
        Ok(r) => r,
        Err(e) => return cached.ok_or_else(|| format!("red: {e}")),
    };
    match resp.status().as_u16() {
        200 => {
            let new_etag = resp
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
            let _ = fs::write(&body_path, &body);
            if let Some(e) = new_etag {
                let _ = fs::write(&etag_path, e);
            }
            Ok(body)
        }
        304 => cached.ok_or_else(|| "304 sin caché local".into()),
        403 | 429 => cached.ok_or_else(|| {
            "límite de la API de GitHub agotado (define GITHUB_TOKEN para ampliarlo)".into()
        }),
        s => Err(format!("GitHub respondió {s} para {url}")),
    }
}

pub fn list_repos(network: bool) -> Result<Vec<Repo>, String> {
    let body = get_json(&format!("https://api.github.com/orgs/{ORG}/repos?per_page=100"), network)?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

pub fn list_releases(repo: &str, network: bool) -> Result<Vec<Release>, String> {
    let body = get_json(
        &format!("https://api.github.com/repos/{ORG}/{repo}/releases?per_page=5"),
        network,
    )?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

/// Descarga `url` a `dest` llamando a `progress(descargados, total)`.
/// Devuelve el SHA256 en hexadecimal.
pub fn download(
    url: &str,
    dest: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let mut resp = agent().get(url).call().map_err(|e| format!("red: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("descarga falló: HTTP {}", resp.status()));
    }
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress(done, total);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn fetch_text(url: &str) -> Result<String, String> {
    let mut resp = agent().get(url).call().map_err(|e| format!("red: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.body_mut().read_to_string().map_err(|e| e.to_string())
}
