//! This machine's speed against the reference laptop, per class of work, measured by running
//! the laptop's own foldbench here (`.audit/tools/laptop-ratios.py`,
//! `docs/evidence/storage-scale-*/laptop-ratios.json`). Each ratio is the laptop's median
//! divided by this machine's, so a figure measured here, times its ratio, is the laptop's. A
//! class has a band: the smallest and the largest ratio of its rows, because work of one
//! class does not scale by one number (a hash, a projection and a conflict list differ).

use std::path::Path;

/// The smallest and the largest ratio of a class's rows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub low: f64,
    pub high: f64,
}

impl Band {
    pub fn one() -> Self {
        Self {
            low: 1.0,
            high: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ratios {
    pub sha: Band,
    pub open_cache: Band,
    pub open_nocache: Band,
    pub multi: Band,
    pub single: Band,
    pub memory: Band,
    /// The laptop's flush time over this machine's, once the laptop has measured one.
    pub disk: Option<Band>,
    pub source: String,
}

impl Ratios {
    /// No scaling: what CI uses, and a laptop run (which is the laptop).
    pub fn identity(source: &str) -> Self {
        Self {
            sha: Band::one(),
            open_cache: Band::one(),
            open_nocache: Band::one(),
            multi: Band::one(),
            single: Band::one(),
            memory: Band::one(),
            disk: Some(Band::one()),
            source: source.to_owned(),
        }
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let section = |name: &str| {
            json.get(name)
                .ok_or_else(|| format!("{}: no \"{name}\"", path.display()))
        };
        let (high, low) = (section("ratios")?, section("ratios_low")?);
        let band = |name: &str| -> Result<Band, String> {
            let get = |from: &serde_json::Value| {
                from.get(name)
                    .and_then(serde_json::Value::as_f64)
                    .ok_or_else(|| format!("{}: no ratio \"{name}\"", path.display()))
            };
            Ok(Band {
                low: get(low)?,
                high: get(high)?,
            })
        };
        let disk = match (
            high.get("disk").and_then(serde_json::Value::as_f64),
            low.get("disk").and_then(serde_json::Value::as_f64),
        ) {
            (Some(high), Some(low)) => Some(Band { low, high }),
            _ => None,
        };
        Ok(Self {
            sha: band("sha")?,
            open_cache: band("open_cache")?,
            open_nocache: band("open_nocache")?,
            multi: band("multi")?,
            single: band("single")?,
            memory: band("memory")?,
            disk,
            source: path.display().to_string(),
        })
    }
}
