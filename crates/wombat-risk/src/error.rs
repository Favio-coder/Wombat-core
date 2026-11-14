//! Errores de `wombat-risk`.

use thiserror::Error;

/// Razón por la que no se pudo calcular un ranking.
#[derive(Error, Debug)]
pub enum RiskError {
    #[error("aritmética exacta: {0}")]
    Arithmetic(String),

    #[error("{bp} puntos básicos está fuera de 0..=10000")]
    BpOutOfRange { bp: i128 },

    #[error("config fuera de contrato: {0}")]
    Config(String),

    #[error("entrada fuera de contrato: {0}")]
    Input(String),

    #[error("celda duplicada: {cell_id}")]
    DuplicateCell { cell_id: String },

    #[error("JSON inválido: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("no se pudo leer {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("WONB: {0}")]
    Wonb(#[from] wombat_core::v02::WonbError),
}
