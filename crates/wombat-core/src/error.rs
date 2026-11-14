//! Errores tipados del núcleo WOMBAT.
//!
//! Todos los errores son claros y testeables: cada variante documenta la
//! condición que la originó para facilitar tests unitarios y de integración.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum WonbError {
    #[error("JSON inválido: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("campo requerido faltante: {field}")]
    MissingField { field: String },

    #[error("versión no soportada: {got} (esperada {expected})")]
    UnsupportedVersion { expected: String, got: String },

    #[error("tipo de WONB no permitido: {got}")]
    InvalidType { got: String },

    #[error("topic inválido: {topic}")]
    InvalidTopic { topic: String },

    #[error("TTL inválido: {ttl} (debe ser > 0)")]
    InvalidTtl { ttl: u64 },

    #[error("timestamp inválido: {0}")]
    InvalidTimestamp(String),

    #[error("WONB expirado: id={id}, expiró en {expired_at}")]
    Expired { id: String, expired_at: String },

    #[error("WONB duplicado en memoria: id={id}")]
    Duplicate { id: String },

    #[error("firma inválida para WONB id={id}")]
    InvalidSignature { id: String },

    #[error("clave pública no disponible para verificar WONB id={id}")]
    MissingPublicKey { id: String },

    #[error("hash esperado {expected} pero se obtuvo {got}")]
    HashMismatch { expected: String, got: String },

    #[error("error criptográfico: {0}")]
    Crypto(String),

    #[error("error interno: {0}")]
    Internal(String),
}

