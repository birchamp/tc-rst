//! This machine's speed against the reference laptop, per class of work, measured by running
//! the laptop's own foldbench here (`.audit/tools/laptop-ratios.py`,
//! `docs/evidence/storage-scale-*/laptop-ratios.json`). Each ratio is the laptop's median
//! divided by this machine's, so a figure measured here, times its ratio, is the laptop's.

use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Ratios {
    pub sha: f64,
    pub open_cache: f64,
    pub open_nocache: f64,
    pub multi: f64,
    pub single: f64,
    pub memory: f64,
    /// The laptop's flush time over this machine's, once the laptop has measured one.
    pub disk: Option<f64>,
    pub source: String,
}

impl Ratios {
    /// No scaling: what CI uses, and a laptop run (which is the laptop).
    pub fn identity(source: &str) -> Self {
        Self {
            sha: 1.0,
            open_cache: 1.0,
            open_nocache: 1.0,
            multi: 1.0,
            single: 1.0,
            memory: 1.0,
            disk: Some(1.0),
            source: source.to_owned(),
        }
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let ratios = json
            .get("ratios")
            .ok_or_else(|| format!("{}: no \"ratios\"", path.display()))?;
        let get = |name: &str| {
            ratios
                .get(name)
                .and_then(serde_json::Value::as_f64)
                .ok_or_else(|| format!("{}: no ratio \"{name}\"", path.display()))
        };
        Ok(Self {
            sha: get("sha")?,
            open_cache: get("open_cache")?,
            open_nocache: get("open_nocache")?,
            multi: get("multi")?,
            single: get("single")?,
            memory: get("memory")?,
            disk: ratios.get("disk").and_then(serde_json::Value::as_f64),
            source: path.display().to_string(),
        })
    }
}
