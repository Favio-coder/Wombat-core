//! Lo único de este crate que toca el disco.
//!
//! Está aparte a propósito. [`crate::evaluate`] es una función pura: sin reloj,
//! sin azar, sin red y sin I/O. Así el ranking es reproducible por cualquiera
//! que tenga los mismos bytes de entrada, que es lo que permite que un juez
//! vuelva a calcularlo meses después y obtenga lo mismo.

use std::path::Path;

use crate::config::RiskConfig;
use crate::error::RiskError;
use crate::inputs::Inputs;

/// Lee `risk-config.json`.
pub fn load_config(path: impl AsRef<Path>) -> Result<RiskConfig, RiskError> {
    RiskConfig::from_json(&read(path)?)
}

/// Lee `cells.geojson`.
pub fn load_inputs(aoi_id: impl Into<String>, path: impl AsRef<Path>) -> Result<Inputs, RiskError> {
    Inputs::from_cells_geojson(aoi_id, &read(path)?)
}

fn read(path: impl AsRef<Path>) -> Result<String, RiskError> {
    let path = path.as_ref();
    std::fs::read_to_string(path).map_err(|source| RiskError::Io {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_says_which_one() {
        let err = load_config("no/existe/risk-config.json").unwrap_err();
        match err {
            RiskError::Io { path, .. } => assert!(path.contains("risk-config.json"), "{path}"),
            other => panic!("se esperaba Io, llegó {other}"),
        }
    }
}
