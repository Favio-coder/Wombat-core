//! # wombat-core
//!
//! Núcleo del protocolo WOMBAT. Define el modelo de datos WONB (Wombat Observation
//! Notation Block), su serialización JSON, validación de campos requeridos,
//! cálculo de hash SHA-256 canónico, firmado/verificación con Ed25519 y
//! detección de duplicados en memoria.

pub mod error;
pub mod wonb;
pub mod validation;
pub mod crypto;

// Re-export commonly used types.
pub use wonb::{Wonb, WonbSource, WonbProvenance, WonbRouting, WonbType};
pub use crypto::sign_wonb;
pub use validation::{validate_wonb, WonbValidator, ExpiryPolicy};
