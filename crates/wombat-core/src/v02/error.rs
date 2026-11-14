//! Errores tipados de WONB 0.2.
//!
//! Separado de [`crate::error::WonbError`] (WONB 0.1): el contrato 0.2 falla por
//! razones distintas (canonicalización JCS, emisores de confianza, cadena de
//! padres) y mezclar ambos haría imposible distinguir qué versión rechazó un
//! bloque.

use thiserror::Error;

/// Razón por la que un bloque WONB 0.2 no es válido.
#[derive(Error, Debug)]
pub enum WonbError {
    #[error("id no coincide: esperado {expected}, declarado {got}")]
    IdMismatch { expected: String, got: String },

    #[error("firma Ed25519 inválida para el bloque {id}")]
    BadSignature { id: String },

    #[error("emisor no confiable para un bloque de tipo {block_type}: {issuer}")]
    UntrustedIssuer { issuer: String, block_type: String },

    #[error("bloque {id} expirado: válido hasta {valid_until}, evaluado a {as_of}")]
    Expired {
        id: String,
        valid_until: String,
        as_of: String,
    },

    #[error("el bloque {id} declara el padre {parent}, que no existe en el conjunto")]
    MissingParent { id: String, parent: String },

    #[error("float en un bloque firmado (R16), en {path}")]
    FloatInPayload { path: String },

    /// El mensaje ya describe el problema por sí solo: estos errores viajan
    /// anidados dentro de serde (ver `BlockBody`), y un prefijo fijo se
    /// duplicaría en cada capa.
    #[error("{0}")]
    Schema(String),

    #[error("fecha no es RFC 3339: {0}")]
    InvalidTimestamp(String),

    #[error("JSON inválido: {0}")]
    InvalidJson(#[from] serde_json::Error),
}
