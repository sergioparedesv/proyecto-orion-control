//! Catálogo de relays de Valve (Steam Datagram Relay) para Dota 2 (§6.1.5, §16.2).
//!
//! Orden de preferencia: caché < 24 h → descarga → caché vieja → `assets/relays.json` empaquetado.
//! Ninguna IP se escribe a mano en el código.

use std::collections::{BTreeMap, HashMap};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ProbeError;

pub const SDR_URL: &str = "https://api.steampowered.com/ISteamApps/GetSDRConfig/v1/?appid=570";
pub const CACHE_FILE: &str = "sdr_config.json";
pub const CACHE_TTL: Duration = Duration::from_secs(24 * 3600);
const BUNDLED: &str = include_str!("../assets/relays.json");

/// POP vigilados por defecto: LATAM y la costa este/sur de EE. UU.
pub const DEFAULT_WATCH: &[&str] = &["lim", "scl", "eze", "gru", "iad", "atl", "dfw"];

#[derive(Debug, Deserialize)]
struct SdrConfig {
    revision: Option<u64>,
    pops: HashMap<String, SdrPop>,
}

#[derive(Debug, Deserialize)]
struct SdrPop {
    desc: Option<String>,
    geo: Option<Vec<f64>>,
    relays: Option<Vec<SdrRelay>>,
}

#[derive(Debug, Deserialize)]
struct SdrRelay {
    ipv4: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CatalogSource {
    Remote,
    Cache,
    StaleCache,
    Bundled,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PopInfo {
    pub code: String,
    pub desc: String,
    pub geo: Option<Vec<f64>>,
    #[ts(type = "string[]")]
    pub relays: Vec<Ipv4Addr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Catalog {
    pub source: CatalogSource,
    #[ts(type = "number | null")]
    pub revision: Option<u64>,
    pub pops: BTreeMap<String, PopInfo>,
}

impl Catalog {
    pub fn parse(json: &str, source: CatalogSource) -> Result<Self, ProbeError> {
        let cfg: SdrConfig = serde_json::from_str(json)?;
        let pops = cfg
            .pops
            .into_iter()
            .filter_map(|(code, p)| {
                let relays: Vec<Ipv4Addr> = p
                    .relays
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|r| r.ipv4?.parse().ok())
                    .collect();
                (!relays.is_empty()).then(|| {
                    let desc = p.desc.unwrap_or_else(|| code.clone());
                    (code.clone(), PopInfo { code, desc, geo: p.geo, relays })
                })
            })
            .collect::<BTreeMap<_, _>>();
        if pops.is_empty() {
            return Err(ProbeError::System("el catálogo SDR no trae ningún POP con relays".into()));
        }
        Ok(Self { source, revision: cfg.revision, pops })
    }

    pub fn bundled() -> Self {
        Self::parse(BUNDLED, CatalogSource::Bundled).expect("assets/relays.json es válido (se prueba en tests)")
    }

    pub fn pop(&self, code: &str) -> Option<&PopInfo> {
        self.pops.get(code)
    }

    /// Carga el catálogo con la política de caché. Nunca falla: en el peor caso usa el empaquetado.
    pub async fn load(cache_dir: Option<&Path>, http: &reqwest::Client) -> (Self, Vec<String>) {
        let mut notes = Vec::new();
        let cache_path: Option<PathBuf> = cache_dir.map(|d| d.join(CACHE_FILE));

        if let Some(p) = &cache_path {
            if let Some(age) = file_age(p) {
                if age < CACHE_TTL {
                    match std::fs::read_to_string(p).map_err(ProbeError::from).and_then(|s| Self::parse(&s, CatalogSource::Cache)) {
                        Ok(c) => return (c, notes),
                        Err(e) => notes.push(format!("caché del catálogo inválida: {e}")),
                    }
                }
            }
        }

        match fetch(http).await {
            Ok((text, c)) => {
                if let Some(p) = &cache_path {
                    if let Err(e) = std::fs::create_dir_all(p.parent().unwrap_or(Path::new("."))).and_then(|_| std::fs::write(p, &text)) {
                        notes.push(format!("no se pudo guardar la caché del catálogo: {e}"));
                    }
                }
                return (c, notes);
            }
            Err(e) => notes.push(format!("no se pudo descargar el catálogo SDR: {e}")),
        }

        if let Some(p) = &cache_path {
            if let Ok(s) = std::fs::read_to_string(p) {
                if let Ok(c) = Self::parse(&s, CatalogSource::StaleCache) {
                    notes.push("usando una caché del catálogo de más de 24 h".into());
                    return (c, notes);
                }
            }
        }
        notes.push("usando el catálogo empaquetado con la app (puede estar desactualizado)".into());
        (Self::bundled(), notes)
    }
}

async fn fetch(http: &reqwest::Client) -> Result<(String, Catalog), ProbeError> {
    let text = http.get(SDR_URL).timeout(Duration::from_secs(10)).send().await?.error_for_status()?.text().await?;
    let c = Catalog::parse(&text, CatalogSource::Remote)?;
    Ok((text, c))
}

fn file_age(p: &Path) -> Option<Duration> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    SystemTime::now().duration_since(m).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_has_latam() {
        let c = Catalog::bundled();
        for code in DEFAULT_WATCH {
            assert!(c.pop(code).is_some(), "falta {code}");
        }
        assert!(c.pop("lim").unwrap().desc.contains("Lima"));
    }

    #[test]
    fn real_sample_parses_and_skips_empty_pops() {
        let json = include_str!("../../../docs/samples/sdr_config.json");
        let c = Catalog::parse(json, CatalogSource::Remote).unwrap();
        assert!(c.pop("gru").is_some());
        assert!(c.pop("mas1").is_none(), "POP sin relays públicos");
    }

    #[test]
    fn tolerates_unknown_fields() {
        let json = r#"{"pops":{"x":{"relays":[{"ipv4":"1.2.3.4","foo":1}],"bar":true}},"other":1}"#;
        let c = Catalog::parse(json, CatalogSource::Remote).unwrap();
        assert_eq!(c.pop("x").unwrap().relays.len(), 1);
    }
}
